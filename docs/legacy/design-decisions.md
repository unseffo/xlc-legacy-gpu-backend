# Design decisions 1.5 — D2 / D3 / D7 / D8 and the capacity estimate (step 1.5, taken by TGT)

Inputs: the audits 1.1 (record layout, G1 contract), 1.2 (the reference call list,
the one-barrier rule), 1.3 (D5/D7 verdicts, B9), 1.7 (insertion points) and 1.4
(retail NGG statistics: 9 282 programs, all 128 vertices × 128 primitives × 128
threads, 0-22 output slots, record_dwords p50 4 228 / p90 8 324 / max 11 908,
9.8% mesh-split — informational for the legacy chain).

**Owner's constraints for this step:** capacity is computed worst-case (128×128);
record counts per frame are ASSUMPTIONS, not measurements (verify at 4.4/6.1);
D3 and the D8 overflow behaviour each get two options with pros/cons and a
recommendation but NO final pick — the owner chooses; IB is u32 by default, u16 is
an optimisation to re-check against real peaks at 4.4. Nothing here changes code.

**RESOLVED BY THE OWNER (2026-11-25):** **D3 = A** (vertices stay in `res`, the VS
fetches; re-open trigger unchanged: measured fill < 15% AND VS traffic > 2× B's
writes at 4.4 → revisit at 7.5) and **D8 overflow = (a)** (skip the whole draw +
`LEGACY_OVERFLOW` counter + `xlog_warn`, never silent; **the overflow counter
goes into `report.json` — carry it into step 8.2**). The recommendations below
are therefore the final decisions; the alternatives stay on record for the
re-open trigger.

Notation: a draw carries `G` thread groups (records), a record is 128 vertices ×
128 primitives, `s` output slots → `vd = 4(1+s)` vertex dwords, `rd = 4 + 128·vd +
128` record dwords (all from audit 1.1).

## D2 — deterministic order, the scan scheme (no open choice; recorded)

**Order = (record index, primitive slot).** The IB position of record `r`'s slot `j`
is `(prefix[r] + j)·3 + k`, where `prefix[r]` is the exclusive prefix sum of the
records' `primitive_count` (record dword 1, audit 1.1 B2) in record order. No
atomics anywhere; the scheme is a pure function of the record headers, hence
deterministic and golden-comparable bitwise (G1's reference generator implements
the same formulas independently, audit 1.1 §F).

Two-level scan, both levels without wave intrinsics (groupshared + plain loops —
identical on wave32/64, which the 1080 and WARP both run):

- **scan-1**: 128 threads, one group per **1 024 records**; each thread reads 8
  header dwords, the group computes the block's exclusive prefixes in groupshared
  and writes them (`local_prefix[r]`, 4 B/record) plus the block's total to a sums
  buffer.
- **scan-2**: one group of 128 threads scans the block sums (≤ 1 024 blocks =
  **support up to 1 048 576 records per draw**), then one thread per record adds
  `block_prefix[r/1024] + local_prefix[r]` into `prefix[r]` and one thread writes
  the DRAW args `[Σ primitive_count, 1, 0, 0]` (16 B, D6). For `G ≤ 1024` scan-1
  alone suffices and scan-2 reduces to the args write.
- **recon**: one group per record, one thread per primitive slot (128 = threads),
  exactly the compute pass's own shape (audit 1.2 A-grouping): unpack the 10-10-10
  dword (`p & 0x3FF`, `>>10 & 0x3FF`, `>>20 & 0x3FF`), write the three global
  vertex ids `r·128 + local` as u32 (u16: see the optimisation note) into
  `IB[(prefix[r]+j)·3 + k]`. Bit-31 nulls and out-of-range indices keep their slot
  as data (B9, audit 1.3 (д) — degenerate triangles, not compaction); if 1.5's
  final B9 call (owner) is "compact", recon additionally writes a 0-mask and the
  cull pass's stable compaction handles it — recorded under D8-cull.

Dispatch count per draw: compute (existing) → UAV barrier on `res` (the only
inter-pass barrier, audit 1.2 B6) → **scan-1, scan-2, recon = 3 dispatches** (+
2 more with optional cull: mask pass + stable compaction scan). Barriers between
the scan/recon stages: one UAV barrier each on the buffer just written (scan writes
prefix+args, recon writes IB) — same minimal rule as the reference.

## D3 — vertex storage: A (no VB) vs B (VB copy) — RESOLVED BY THE OWNER: A

The unknown that decides this is the **record fill ratio** `vertex_count / 128` —
a runtime value the static pack cannot give (the program writes the header when it
runs, audit 1.1 B8); it is NOT measurable before 4.4/3.5. Both options below are
therefore priced worst-case (fill 100%) with the sparse case called out.

**Option A — vertices stay in `res`, VS fetches (the playbook default).**
IB carries global vertex ids `r·128 + local`; the VS computes the address
`r·rd + 4 + local·vd` (for the uniform 128 shape: `r = id >> 7`,
`local = id & 127` — shifts, no division) and loads `4(1+s)` dwords per vertex,
plus a UAV→SRV transition of `res` before the draw.

- *Bytes written per record:* IB 1 536 B (u32) — nothing else.
- *VS read traffic per record (worst):* 128 unique vertices × `4(1+s)` B = at
  p50 (`s=7`, 32 B stride) **4 KB**, at max (`s=22`, 92 B stride) 11.8 KB — but
  the 384 vertex references of a record's 128 triangles hit at most 128 distinct
  vertices, and consecutive vertices sit `vd·4` B apart (32 B p50 — two cache
  lines), so the repeats are L1/L2 hits; effective reads ≈ unique vertices once.
- *Sparse records (fill f):* A reads only the vertices the IB references — the
  sparse case costs nothing extra in reads; the record's unused tail is simply
  never touched.
- *Complexity:* the least — no extra pass, no compaction scan, `res` stays the
  single geometry source; the UAV→SRV transition of `res` is one extra barrier
  pair per draw chain (it already ends as UAV after the compute pass).
- *Risks:* scattered `res` records (p90 33 KB stride-jumps between records of the
  same draw) make the VS's per-vertex reads jump `rd·4` B apart — a TLB/cache
  pressure point at large `G`; Pascal's 2.5 MB L2 holds ~75 p50-records' worth of
  working set at a time, so only very large single draws thrash.

**Option B — recon copies vertices into a compact VB (second scan by
`vertex_count`).**

- *Extra writes per record (worst, fill 100%):* the VB copy itself — 128 ×
  `4(1+s)` B = 4 KB p50 (equal to A's reads), plus the compaction scan's prefix
  (4 B) plus IB in local ids (768 B u16-friendly).
- *Wins when sparse:* writes scale with `vertex_count` (fill f → f·4 KB), IB ids
  are dense (0..Σcount-1) and small (u16 legal without the groups<512 bound), and
  the VS reads a tight linear VB — the best possible cache story.
- *Costs:* a second scan level (by `vertex_count`, +1 dispatch), a second copy of
  every vertex byte on the GPU (write+read), a bigger transient allocation
  (VB+IB ≈ 4.7 KB/record p50 vs 1.5 KB for A), and two more failure surfaces
  (compaction + copy) in the chain that G1 must then golden.
- *Risks:* at high fill B strictly loses — it writes ≥2.7× A's bytes at p50 with
  no read-side gain (A's reads are cache-hit-dominated).

**Decision (resolved by the owner): A.** The retail shape (uniform 128-vertex records,
32 B p50 stride) is cache-friendly for A, A adds no writes and no passes, and the
golden chain stays minimal. **Re-open trigger (per the playbook, at 7.5 or earlier
at 4.4):** measured mean fill < 15% AND measured VS read traffic > 2× B's write
traffic — then B pays for itself; otherwise A stands. The fill ratio and per-draw
`G` peaks are logged from 4.4 on (the parameters below).

## D7 — provoking vertex, per-primitive attributes (no open choice; recorded)

From audit 1.3: **provoking = v0** (first index in the 10-10-10 bit order — the
dword is the console program's own, stored verbatim); flat rides as
`nointerpolation` (v_interp_mov) with the D3D provoking vertex — the golden of 3.5
proves console-P0 ≡ PC-flat bit for bit. **Per-primitive outputs do not exist**
(no record payload, none of the 9 282 retail programs requests them — 1.4), so no
`prim_src` buffer; **SV_PrimitiveID is unsupported** and unused by the corpus —
the legacy chain feeds the same translated PS and inherits both limits for free.
Clip/cull distances ride as ordinary vertex slots (`ranked(…, true)`), carried by
whichever D3 option is chosen.

## D8 — capacity (worst-case) and the overflow behaviour

**Worst-case capacity per draw** (`G` records, u32 IB, 256-B-aligned regions):

| Region | Bytes | Formula |
|---|---|---|
| IB | 1 536·G | 3 · 128 · 4 · G |
| scan (prefix + block sums + final) | ≈ 9·G + 16 | 4·G (local) + 4·(G/1024) + 4·G (final) + args 16 |
| optional cull (IB2 + compaction) | ≤ 1 536·G + 9·G | stable compaction, D5 opt-in |
| **total (no cull)** | **≈ 1 545·G + 16** | ≈ 1.51 KB per record |

**u16 IB as an optimisation (default stays u32):** halves the IB to 768·G and is
legal while the largest global vertex id `G·128 + 127` stays < 65 536, i.e.
`G < 512` per draw — **verify the real per-draw G peaks at 4.4** before ever
switching; a mixed path (u16 only for draws that fit) is possible but not proposed.

**Frame-level — ASSUMPTIONS, not measurements (verify at 4.4/6.1):** suppose
**A100k** records per frame (100 000; no measurement exists — the console's real
per-frame NGG dispatch volume is unknown until the title runs with the chain, and
4.4's logging / 6.1's multi-frame harness measure it): IB ≈ **147 MB u32**
(74 MB u16), scan ≈ 0.9 MB, args ≈ 16 B × draws. Under A100k the whole legacy
transient footprint is ≈ **148 MB**, ~6.9× inside the proposed
`legacy_budget_mb = 1024`; even A1M (the scan's structural ceiling per draw is
1 048 576 records) stays at 1.48 GB — above the 1024 MB budget, i.e. the overflow
path is reachable only at volumes ≥ 660k records/frame (assumption-space; the
real margin comes from 4.4).

**Allocator:** bump regions per draw from a per-frame ring of N = 3 frames in
flight, reset at the frame origin (audit 1.7 point 9 — `frame.rs`); every region
256-B aligned; the peak per frame is logged (D8's counters).

**Overflow behaviour — RESOLVED BY THE OWNER: (a) skip the draw (counter + warning, never silent; the counter lands in `report.json` for step 8.2).**

- **(a) Skip the whole draw** (+ `LEGACY_OVERFLOW` counter + one `xlog_warn`,
  never silent). Pros: honest semantics (no half-drawn geometry), the simplest
  accounting, goldens unaffected (overflow never produces partial IBs). Cons: the
  draw's geometry vanishes for that frame — a visible hole if it ever fires for
  real. *(This is the playbook's original default and my recommendation: with the
  measured A100k footprint at ~15% of budget it should never fire; if it does,
  the warning names the draw volume and the 4.4 logs say why.)*
- **(b) Truncate to the budget** (fill the regions that fit, clamp the DRAW count
  to the allocated prefix so the chain stays consistent). Pros: something is
  drawn. Cons: non-deterministic w.r.t. draw order and frame history, partial
  triangles/geometry seams, and G1/G2 goldens break whenever it triggers — the
  gate would have to special-case overflow. Not recommended.

(A third shape — fail the call / panic — is rejected outright: nothing in the
title's draw path may fault.)

## What 4.4 must measure (the parameters this document defers)

1. Per-draw `G` (records) — histogram and p99, per pipeline (`TwoPassPso`).
2. Record fill ratio `vertex_count / 128` — mean and p10 (decides D3's re-open
   trigger).
3. Frame peak: Σ IB+scan bytes, draws with args, overflow count (should stay 0).
4. VS read bytes per draw (A's traffic, against the 2× re-open threshold).

All four come out of the D8 counters + `XCL_FRAME_DUMP`-style logging at 4.4;
the re-check points are 4.4 (first title frames) and 7.5 (optimisation pass).
