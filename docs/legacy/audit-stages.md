# Audit 1.4 — stage combinations and retail NGG statistics (step 1.4, taken by TGT)

What the retail shader pack `<RPF>` actually contains, as the capacity input for 1.5.
Read-only: `tools/rpf_survey` (this step) inflates every record and reads the
declared interfaces through XCL's public API (`pack::scan`/`pack::inflate`,
`xbox::parse`, `recomp::interface`) — the game never runs, the pack is only read,
and everything below is aggregates. No record names, file names or game data appear
here; the split formula is reimplemented locally from audit 1.1 §C (`emit` is
private to xcl, so the tool cannot call it — which also makes this an independent
check of the formula).

## Stage combinations (the pack, 26 503 records, 0 parse failures)

| Stage (container) | records | share | XCL path |
|---|---|---|---|
| MS (NGG) | 9 282 | 35.0% | two-pass: `console_mesh` → compute + pass-through mesh (the legacy backend's target) |
| PS | 12 101 | 45.7% | classic `translated_graphics_pso` / `two_pass_ps` |
| VS | 2 236 | 8.4% | classic `translated_graphics_pso`; the ES+GS variant of a VS feeds `console_geometry` |
| CS | 2 689 | 10.1% | `translated_compute_pso` |
| GS | 195 | 0.7% | two-pass as a mesh pipeline: `console_geometry(vs, gs)` → `translated_gs_pso` |

The PSO-level pairings (which VS goes with which PS, which VS+GS forms a pipeline)
are the title's runtime data, not in the shader pack — the counts above are record
counts, and the pairings are made at PSO creation (`real.rs::translated_gs_pso`
5455, `translated_graphics_pso` 4595). The three two-pass shapes the legacy backend
must carry: **MS (35% of the pack)**, **VS+GS (the 195 GS containers)**, and the
rect-list GS XCL generates itself (`rect_list_gs`, real.rs:5170 — not in the pack).

## NGG (MS) record statistics — 9 282 programs, every interface parsed

| Metric | min | p50 | p90 | p99 | max |
|---|---|---|---|---|---|
| max_vertices | 128 | 128 | 128 | 128 | 128 |
| max_primitives | 128 | 128 | 128 | 128 | 128 |
| numthreads | 128 | 128 | 128 | 128 | 128 |
| output slots (ranked, incl. clip/cull) | 0 | 7 | 15 | 20 | 22 |
| record_dwords (= 4 + 128·vd + 128) | 644 | 4 228 | 8 324 | 10 884 | 11 908 |

Output-slot histogram: 1 slot 1 924 · 2 slots 1 535 · 3-4 244 · 5-8 1 874 ·
9-16 3 138 · >16 567 (and a handful with 0 slots — position-only records,
`record_dwords` 644). The uniform 128/128/128 shape is exactly the pattern the
playbook predicted for retail NGG transform programs.

Per-primitive outputs: none of the 9 282 interfaces requests any — consistent with
audit 1.3 (г): the record has no per-primitive payload, and the pack's programs do
not use what does not exist.

## Split (the 32 KB mesh output limit)

`passthrough_split` distribution (upper bound, no PS bound — a real PS can only
read fewer elements): **split 1: 8 371 · split 4: 853 · split 7: 58** —
**911 programs (9.8%) need a split**.

For the legacy backend this number is **informational, not a requirement**: the
32 KB limit is a *mesh-shader output* limit (`emit.rs:5316`). A legacy chain that
rasterizes through a plain graphics PSO + `ExecuteIndirect` has no per-group output
budget — one indexed draw carries a whole record's triangles regardless of vertex
count, so `mesh_groups_per_record` (and the split's vertex rewiring, audit 1.2 B4)
never enters the legacy path. It still matters for the **golden comparisons**
(3.5/3.7/G1): the mesh reference runs *are* split for 9.8% of the programs, and
their IB-equivalent (the per-run consecutive triangle lists) must be compared
against the legacy form with the split unwired — the reference generator
(audit 1.1 §F) already builds the unsplit index order, which is the right target.

## Capacity inputs for 1.5 (D8)

With the uniform 128×128 record shape:

- **IB per record**: 3 · 128 · 4 = **1 536 B** (u32 indices); **768 B** as u16 —
  legal whenever the largest global vertex id `groups · 128 + 127` stays < 65 536,
  i.e. groups < 512 per draw (true for the draw sizes seen in practice; verify at
  4.4 with the logged peaks).
- **scan**: one prefix dword per record (+ a second level above 1 024 records per
  draw, audit D2's block scan) ≈ 4 B/record.
- **args**: 16 B per draw (D6).
- **Example peaks**: a draw of 1 000 groups costs 1 536 KB u32 IB (768 KB u16);
  a whole-frame pile of 100 000 records across draws is 150 MB u32 / 75 MB u16 —
  inside the proposed `legacy_budget_mb` (1 024) but worth the peak logging D8
  already plans.
- **`res` records themselves** (4 228 dword p50, 8 324 p90) are the existing
  compute pass's output — allocated today by the two-pass path, not a legacy cost.

## Assertions table

| # | Assertion | Source | How verified |
|---|---|---|---|
| 1 | The pack holds 26 503 records; every one inflates and parses (0 failures) | `tools/rpf_survey` run (aggregates in `artifacts_tgt/rpf_survey.txt`, outside git) | run (read-only, 246 s stage sweep + 50 s MS pass) |
| 2 | Stage split: MS 9 282, PS 12 101, VS 2 236, CS 2 689, GS 195 | same run | run |
| 3 | Every NGG interface parses: 9 282/9 282; per-primitive outputs requested by none | same run + `recomp::interface` | run |
| 4 | All NGG programs are 128 max vertices / 128 max primitives / 128 threads; slots 0-22 (p50 7, p99 20) | same run | run |
| 5 | 9.8% (911) of NGG programs exceed the 32 KB mesh output limit (split 4 or 7 without a PS) | same run, formula of audit 1.1 §C reimplemented independently | run |
| 6 | The split does not constrain the legacy chain (no mesh output budget in VS+ExecuteIndirect) | `emit.rs:5316-5341` (the limit is a mesh-stage limit) | code read |
| 7 | PSO pairings are title runtime data, not pack data | `real.rs:5455, 4595` | code read |
| 8 | u16 IB legal below 512 groups per draw | global id = `rec·128 + local` (audit 1.1 §F) | derived |

## What the legacy path must support (the 1.5 input)

MS(NGG) records — 9 282 programs, uniform 128/128/128, up to 22 output slots
(record_dwords up to 11 908) — through scan/recon/ExecuteIndirect with u32 (or
sub-512-group u16) indices; VS+GS two-pass (195 GS containers) through the same
record shape (`console_geometry` writes the same records, audit 1.1 B/GS notes);
the generated rect-list GS needs nothing new. No per-primitive payloads, no
SV_PrimitiveID (1.3), and no split handling in the legacy chain itself — only in
the mesh reference side of the goldens.
