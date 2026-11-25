# XCL Legacy GPU Backend — Contract (Gate Document for Stages 2–8)

Consolidates audits 1.1–1.4, 1.7 and design-decisions 1.5 into a single specification.
Every claim below carries a `file:symbol` or audit reference.

---

## 0. Premise: the two-pass runtime does not exist

Three independent facts (audit 1.7 §0):

- **No `DispatchMesh` hook** in HLE — only a comment at `real.rs:5104` (audit 1.7 #0.1).
- **Two-pass compute PSO is not in `TRANSLATED_COMPUTE`** — only `translated_compute_pso` inserts there (`real.rs:5044`); `TWO_PASS` is a separate registry (`real.rs:5365`). A title `DispatchX` with a two-pass PSO is skipped as "no translated console shader" (`bind.rs:806-814`) (audit 1.7 #0.2).
- **Two-pass mesh PSO is not in `TRANSLATED_GRAPHICS`** — `graphics_bound` treats its draws as PC pipelines (`real.rs:1677-1679`, `bind.rs:499-523`) (audit 1.7 #0.3).

**Consequence:** Stage 4 (especially 4.4) builds the legacy draw chain **from scratch**. The insertion points are documented in audit 1.7 §1–9.

---

## 1. Compute pass → `res` records (unchanged, audited)

**Source:** audit 1.1 (A–E), audit 1.2 (а), audit 1.3 (а)–(в), design-decisions D2/D3-A.

- Console NGG program → HLSL → DXC `cs_6_6` (`pso.rs:885`, `pso.rs:848`).
- One record per thread group, written to root UAV `u1` (`res`) (`runner.rs:1266-1268`, `emit.rs:5223`).
- **Record layout (dword-by-dword, confirmed by dump on GTX 1080 — audit 1.1 [DUMP]):**

  | Region | Dwords | Formula / Content |
  |---|---|---|
  | Header | 4 | `[vertex_count, primitive_count, spare, spare]` (audit 1.1 B2) |
  | Vertices | `max_vertices × vert_dwords` | `vert_dwords = 4 × (1 + ranked(outputs, true).len())` — pos float4 + 1 float4 per ranked output (audit 1.1 B3, B6) |
  | Primitives | `max_primitives` | Packed 10-10-10-1: `v0=bits 0-9, v1=bits 10-19, v2=bits 20-29, bit31=null` (audit 1.1 B7, audit 1.3 V1–V2) |

- `record_dwords = 4 + max_vertices × vert_dwords + max_primitives` (audit 1.1 B4). Retail: all 128/128/128, `vert_dwords` 8–92, `record_dwords` 644–11908 (audit 1.4).
- Root signature: 6 root UAVs `u0..u5` — `u0=io`, `u1=res`, `u2..u5=UMA windows`, `DATA_VOLATILE`, `HEAP_DIRECTLY_INDEXED` (`pso.rs:1046-1080`, audit 1.1 D1).
- `mesh_groups_per_record` (split) decided at PSO finish from PS inputs (`pso.rs:839`, `emit.rs:5333-5341`, audit 1.1 C). **Legacy chain does not use split** (no mesh output budget in VS+ExecuteIndirect, audit 1.4 #6).

---

## 2. What the legacy backend must produce (from `res` records)

**Inputs:** `res` buffer (records concatenated per group), `groups` (from `DispatchX` argument), `split` (from `TwoPassPso.mesh_groups_per_record`), `record` (`MsInterface`), `max_vertices`, `max_primitives`, `vert_dwords`, `record_dwords`.

**Outputs (per draw):**

1. **Index Buffer (IB)** — u32 indices (u16 optimisation: legal when `groups < 512`, verify at 4.4). Layout:
   - Deterministic order = `(record_index, primitive_slot)` (design D2).
   - Exclusive prefix sum `prefix[r]` of `primitive_count` (record header dword 1) over records in dispatch order.
   - Primitive `j` of record `r` writes three indices at `IB[(prefix[r] + j) × 3 + k]`.
   - Indices = global vertex id `r × max_vertices + local` (D3-A: vertices stay in `res`).
   - Null/bad primitives (bit31 or index ≥ `vertex_count`) keep their slot as degenerate `(0,0,0)` — **no compaction** (audit 1.3 B9, design D2/B9).
   - Size: `3 × Σ primitive_count × 4` bytes (u32).

2. **Scan buffers** — prefix sums + block sums + final args.
   - Two-level scan (D2): scan-1 (1 group per 1024 records, 128 threads, groupshared, no wave intrinsics) → scan-2 (1 group scans block sums, ≤1024 blocks = **≤1 048 576 records/draw**).
   - Output: `prefix[r]` per record (4 B) + DRAW args `[total_primitives, 1, 0, 0]` (16 B, D6).

3. **Optional cull outputs** (opt-in, D5): second IB (IB2) + compaction scan + mask buffer. Stable compaction preserves order.

4. **Optional `prim_src` buffer** — **not needed** (per-primitive outputs and `SV_PrimitiveID` do not exist, audit 1.3 (г), design D7).

---

## 3. Graphics pipeline the legacy chain feeds

**Source:** audit 1.2 (а), audit 1.3 (а), audit 1.3 (г), design D1/D3-A/D4/D7.

- **Root signature:** same ABI 6 UAVs (`u0=io, u1=res, u2..u5=UMA`) + IB/args as root SRV/indirect args.
- **Vertex Shader (passthrough):** fetch from `res` by global vertex id.
  - `r = id >> 7` (since `max_vertices = 128`), `local = id & 127`.
  - Record base = `r × record_dwords + 4` (skip header).
  - Vertex stride = `vert_dwords × 4` bytes.
  - Loads: `pos` (float4) + each ranked output (float4) → `XclIn` struct.
  - `res` transitions `UAV → NON_PIXEL_SHADER_RESOURCE` before draw (one barrier pair).
- **Pixel Shader:** the translated console PS (unchanged).
  - Inputs: `SV_Position` + `a{k}` from `ranked(ps.inputs, false)` with interpolation modifiers (`v_interp_mov_f32` → `nointerpolation`, non-float always flat) (audit 1.3 A2).
  - System: `SV_IsFrontFace` (SPI ENA bit 12), `SV_Coverage` (bit 14) (audit 1.3 A3).
- **Rasterizer state:** `SOLID`, `CullMode_NONE`, `DepthClipEnable` — **identical to mesh reference** (audit 1.2 A2, audit 1.3 (б): no culling in translation; rasterizer culling shared by both paths).
- **Render targets / blend / sample mask:** same as mesh PSO (audit 1.2 A2).
- **Provoking vertex = v0** (first index in 10-10-10 bit order) — matches D3D `nointerpolation` semantics (audit 1.3 V3, design D7).
- **Flat attributes** → `nointerpolation` (golden check at 3.5: console P0 ≡ PC-flat bitwise) (audit 1.3 D7 caveat, design D7).

---

## 4. Dispatch chain (per draw) — replaces `DispatchMesh`

**Source:** audit 1.2 (в) steps 14–21, audit 1.7 #6, design D2/D6/D8.

```
[Existing] Dispatch(groups) → compute pass writes res
                ↓
         UAV barrier on res (only barrier, audit 1.2 B6)
                ↓
[Legacy]   scan-1 CS (⌈groups/1024⌉ groups) → prefix + block_sums
                ↓ UAV barrier on prefix/block_sums
         scan-2 CS (1 group) → final prefix + DRAW args [Σ,1,0,0]
                ↓ UAV barrier on prefix/args
         recon CS (groups groups, 128 threads) → IB (global vertex ids)
                ↓ UAV barrier on IB
         [optional] cull CS → IB2 + new args (stable compaction)
                ↓ UAV barrier on IB2/args
         ExecuteIndirect(DRAW, 1, args_buffer, 0, NULL, 0)
                ↓
         graphics PSO (VS fetch + PS) draws
```

- **Dispatch count:** 3 (+2 with cull) per draw. Barriers: one UAV per written buffer (minimal, matches reference).
- **`groups` known at record time** = `DispatchX` argument `x` (audit 1.7 #6).
- **`split` known** = `TwoPassPso.mesh_groups_per_record` (registry). **Needed: reverse index compute PSO → TwoPassPso** (audit 1.7 #4).
- **Indirect args** written by scan-2 (or cull final scan) — 16 B `DRAW` signature (D6).
- **ExecuteIndirect wrap:** `COMMON → INDIRECT_ARGUMENT → COMMON` on XCL-owned args buffer (NVIDIA fix #59, `bind.rs:318-340`, audit 1.7 #7).
- **CommandSignature:** XCL's own DRAW signature (creator! pass-through, `real.rs:3237`, audit 1.7 #8) — title signatures untouched (6.2 later).

---

## 5. Capacity & Allocator (D8)

**Source:** design-decisions D8, audit 1.4 §Capacity, audit 1.7 #9.

- **Per-record worst-case (u32 IB):** ≈1545 B = IB 1536 + scan ~9 + args 16.
- **u16 IB optimisation:** 768 B/record, legal when `groups < 512` (global id `< 65536`). Verify real peaks at 4.4.
- **Per-frame ring allocator:** bump regions per draw, N=3 frames in flight, 256-B alignment, reset at frame origin (`frame.rs` frame clock, audit 1.7 #9).
- **Budget:** `legacy_budget_mb` default 1024 (config). A100k records/frame ≈148 MB (×6.9 headroom).
- **Overflow behaviour (owner choice = a):** skip entire draw + `LEGACY_OVERFLOW` counter + `xlog_warn!` (never silent). Counter → `report.json` (step 8.2). No partial draws, goldens unaffected.

---

## 6. Culling (D5) — opt-in, default off

**Source:** audit 1.3 (б), design D5.

- **Frustum test:** clip-space, cull only if **all 3 vertices** outside same plane (`x<-w, x>w, y<-w, y>w, z<0, z>w`; `w≤0` → same-plane rule). Doubtful → **do not cull**.
- **Stable compaction** — order preserved.
- **Backface:** flag `cull_backface` (default off); rasterizer already culls backfaces in both paths.
- **Hi-Z:** deferred.
- **Integration:** `cull_enable` config → extra mask pass + compaction scan + IB2/args2. Only if enabled and G3 passed.

---

## 7. Decisions D1–D8 (final, with owner picks)

| ID | Decision | Status | Reference |
|---|---|---|---|
| D1 | Fetch in VS (non-indexed Draw, `SV_VertexID` → IB → `res`) | Default | Playbook §4 |
| D2 | Deterministic order `(record, slot)`; 2-level scan 1024/grp, no wave intrinsics, 3 dispatches/draw | **Fixed** | Design D2, audit 1.1 §F |
| D3 | **A: vertices stay in `res`** (IB = global ids; VS fetches; UAV→SRV transition) | **Owner picked A** | Design D3, audit 1.3 (в) |
| D4 | `float4 pos` + `float4` per ranked output (as in `res` or VB) | Follows D3 | Design D4 |
| D5 | Culling opt-in, default false; frustum all-3-vertices; stable; backface flag | Default | Audit 1.3 (б), Design D5 |
| D6 | `ExecuteIndirect` DRAW (16 B); args from scan-CS (or cull); from first draw | Default | Design D6, audit 1.7 #7 |
| D7 | **Provoking = v0; flat = nointerpolation; per-prim = none; SV_PrimitiveID = none** | **Fixed** | Audit 1.3 (г)/(д), Design D7 |
| D8 | **≈1545 B/record (u32); u16 IB optimisation (G<512); ring N=3; budget 1024 MB; overflow = (a) skip draw + counter + warn** | **Owner picked (a)** | Design D8, audit 1.4 |

**Re-open triggers (recorded in design-decisions):**
- D3: if measured mean fill < 15% **AND** VS read traffic > 2× B's writes at 4.4/7.5 → revisit to VB copy.
- D8 u16: if real per-draw `G` peaks ≥ 512 at 4.4 → stay u32.

---

## 8. Runtime insertion points (audit 1.7 §1–9)

| # | Point | File:Symbol | Legacy Action |
|---|---|---|---|
| 1 | PSO creation branch | `real.rs::finish_two_pass` (5296) | Build graphics PSO (VS+PS) instead of mesh PSO; register `TwoPassPso` with `kind=Legacy` |
| 2 | Mesh stream entry | `real.rs::translated_mesh_stream_once` (5379) | Same branch; log `PSO_LEGACY_PENDING` (2.3) |
| 3 | GS two-pass | `real.rs::translated_gs_pso` (5455) | Inherits branch |
| 4 | Draw binding | `bind.rs::graphics_bound` (495) | Bind legacy graphics PSO + IB/args for legacy draws |
| 5 | Draw helpers | `bind.rs::draw_rects` (384) | Precedent: CPU-synthesized geometry → draw |
| 6 | **Main: Dispatch** | `bind.rs::dispatch` (803), `list_x.rs::dispatch_x` (2766) | **Insert legacy chain here**: compute pass → UAV barrier → scan → recon → [cull] → ExecuteIndirect(DRAW). `groups=x`, `split=TwoPassPso.mesh_groups_per_record`. **Needs reverse index compute PSO → TwoPassPso.** |
| 7 | ExecuteIndirect | `bind.rs::execute_indirect` (253) | Copy COMMON→INDIRECT_ARGUMENT wrap for XCL's own args buffer |
| 8 | CommandSignature | `real.rs::dev_create_command_signature` (3237) | Pass-through; legacy uses XCL's own DRAW signature |
| 9 | Frame reset | `hle/d3d12x/frame.rs` | `StoreRing` reset at frame origin (WaitFrameEventX/PresentX) |

---

## 9. Gates G1–G4 (single-machine plan)

**Source:** `single-machine-plan.md` §3, owner-approved modifications.

| Gate | Primary Check | Secondary / Fallback | Owner Confirmation |
|---|---|---|---|
| **G0** | SM 6.6 PSO on TGT via Agility | — | **PASS** (0.2a/0.3) |
| **G1** (3.7) | **1080: legacy IB/args vs independent reference generator** (bitwise). Generator = 2nd impl of D2 from dumped `res` records (audit 1.1 §F). | WARP mesh reference (`mesh_draw_split`) where WARP survives retail set. CPU reference fallback if WARP drops device. | Not required |
| **G2** (4.6) | **WARP: both paths on same device, bitwise** — `mesh_draw_split` reference frame vs legacy chain frame, same WARP, same program. | WARP reference vs 1080 legacy (cross-device): PSNR ≥ 40 dB + ≤1 LSB on ≥99.9% covered pixels (proposal, not fixed). | **Required** (visual) |
| **G3** (5.5) | **1080: cull on == cull off** (same-device, bitwise IB/args + pixels) | — | Not required |
| **G4** (7.2/7.3) | **TGT: title run + legacy/compute tests on wave32** | — | **Required** |

**Pre-G2 check (owner-approved):** retail NGG programs from `<RPF>` must run on WARP through **main HLSL path** (not direct-DXIL) — all 5 WARP device removals (F9) are in direct-DXIL path. Survivor list recorded; bitwise retail G2 conditional on it.

---

## 10. Open questions / Blockers (from audits)

1. **Reverse index compute PSO → TwoPassPso** (audit 1.7 #4) — new structure for 4.2/4.4.
2. **Record fill ratio `vertex_count/128`** — runtime value, not in static pack. Decides D3 re-open. Measured at 4.4.
3. **Per-draw `G` peaks** — decides u16 IB viability and overflow reachability. Measured at 4.4.
4. **WARP device removal on retail shaders** — risk 1 in single-machine-plan. Mitigation: G1 primary on 1080 (no WARP), WARP secondary + survivor list.
5. **Cross-device pixel tolerance** — PSNR/epsilon must be fixed **before first G2 run** (playbook 4.6). Owner decision pending.

---

## 11. Scope boundaries (what legacy backend does NOT change)

- Mesh shader path (`mesh_passthrough*`, `mesh_dispatch.rs`, `mesh_wave.rs`, existing mesh tests) — untouched.
- Classic VS+PS path (`translated_graphics_pso`, `TRANSLATED_GRAPHICS`) — untouched.
- Compute path (`translated_compute_pso`, `TRANSLATED_COMPUTE`) — untouched.
- Title's own graphics-indirect (6.2) — only must not break.
- Generated files (`*_spec.rs`, `tiling_tables.rs`) — via `scripts/` only.
- `HANDOFF.md`, `docs/specs/` — read-only.

---

*End of contract. This document is the gate for stages 2–8. All implementation steps must trace to claims above.*