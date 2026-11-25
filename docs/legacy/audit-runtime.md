# Audit 1.7 — runtime insertion points for the legacy backend (step 1.7, taken by TGT)

Where the legacy chain (Dispatch → UAV barrier → scan-CS → recon-CS → [cull] →
ExecuteIndirect(DRAW)) plugs into the D3D12.X backend, what each hook does today,
and where `groups` and `split` become known during command-list recording. Claims
carry `file:symbol`; the companion audits are 1.1 (the record), 1.2 (the reference
call list), 1.3 (the exports).

## The central fact

**The runtime two-pass path does not exist today.** Three independent facts pin it:

| # | Claim | Source | Verified |
|---|---|---|---|
| 0.1 | There is **no DispatchMesh hook anywhere** in the HLE — the only mention of the word is a comment | grep over `hle/d3d12x/*` (one hit: comment `real.rs:5104`) | code read |
| 0.2 | A two-pass compute PSO is **not** entered into `TRANSLATED_COMPUTE` (only `note_translated_compute`, the classic `translated_compute_pso` path, inserts there) — so a title's `DispatchX` with it is **skipped** as "no translated console shader" | `real.rs:5365` (only `TWO_PASS` insert) vs `real.rs:5042-5046, 1700-1702`, skip at `bind.rs:806-814` | code read |
| 0.3 | A two-pass mesh PSO is not in `TRANSLATED_GRAPHICS` either (only `TWO_PASS`, `real.rs:5365`), so `graphics_bound` treats its draws as PC pipelines | `real.rs:1677-1679` vs `:5365`; `bind.rs:499-523` | code read |

On a mesh GPU the two-pass pipeline object exists but nothing drives it from title
code; on the 1080 the mesh PSO creation itself fails (`finish_two_pass` → driver
rejects → `note_translation_error` + E_INVALIDARG, `real.rs:5308-5311` — the
0.2a/0.6 analysis). The legacy backend replaces exactly this missing runtime leg.

## Insertion points

| # | Point | What it does today | Legacy insertion |
|---|---|---|---|
| 1 | `real.rs::finish_two_pass` (5296-5371) | builds the mesh PSO through the driver (`finish_pso`, 5308), then the compute PSO with the driver-blob cache (5313-5355), registers `TwoPassPso { compute, compute_rs, mesh_rs, record, mesh_groups_per_record, kind }` in `TWO_PASS` keyed by the **mesh** PSO pointer (5365) | the branch point for stage 4.2: when legacy is effective, build the graphics PSO (VS fetch + PS, legacy RS) instead of the mesh PSO; `record`/`mesh_groups_per_record`/`compute` are exactly the legacy chain's parameters. Note: the registry is keyed by mesh PSO — the dispatch side needs a reverse index **compute PSO → TwoPassPso** (today's key cannot answer the dispatch's lookup) |
| 2 | `real.rs::translated_mesh_stream_once` (5379-5450) | CreatePipelineState stream with a console MS: parses subobjects, refuses AS-slot MS (5395-5412), translates (`console_mesh`), rewrites RT/DS formats via `device_x::pc_target_format` (5437-5445), calls `finish_two_pass` | the same branch begins here (2.3 will log `PSO_LEGACY_PENDING` at this point on a mesh-less GPU) |
| 3 | `real.rs::translated_gs_pso` (5455+) | classic-desc VS+GS → `console_geometry` → the same `finish_two_pass` ("GS") | inherits the same branch |
| 4 | `bind.rs::graphics_bound` (495-541) | before every draw: `is_translated_graphics` → `bind_abi` (ABI RS + kept graphics args) else the PC path (`rs_gfx` rebind, 519-522); skips draw on `bind_abi` error | the legacy draw path binds the legacy graphics PSO + IB/args here (4.4); a two-pass PSO is *not* recognized today (0.3) |
| 5 | `bind.rs::draw_instanced` / `draw_indexed_instanced` / `prepare_indexed_draw` / `emit_prepared_indexed_draw` (444-490) | title draws: `graphics_bound` → rect-list CPU expansion (447) → `sync_title_buffers` → the PC draw call | not the NGG leg (a title does not draw NGG records this way), but the pattern `draw_rects` (bind.rs:384+, CPU-expanded `DrawInstanced(rects*6)`) is the working precedent of "synthesize geometry into a draw" that the legacy chain generalizes; `prepare_indexed_draw`'s prepare/emit split is the shape 4.4's indirect draw will reuse. `draw_indexed_x` (list_x.rs:3630) routes here |
| 6 | `bind.rs::dispatch` (803-826) + `list_x.rs::dispatch_x` (2766-2769) | the console's `DispatchX(x,y,z,flags)` funnels into `dispatch`, which requires `is_translated_compute` (skips otherwise — today's fate of a two-pass compute PSO, 0.2) then `translated_dispatch` | **the main insertion point**: with a two-pass PSO resolved, record `Dispatch(x,y,z)` [the NGG compute pass] → UAV barrier on `res` (the only barrier the reference needs, audit 1.2 B6) → scan-CS → recon-CS → [cull] → `ExecuteIndirect(DRAW)`. **`groups = x` (the argument), `split = TwoPassPso.mesh_groups_per_record`** — known right here, at record time, as D8 requires; `y/z ≠ 1` never occurs for the console's NGG dispatches (the record is one per group along x) |
| 7 | `bind.rs::execute_indirect` (253-313) | title compute-indirect on a translated pipeline: root-signature-changing signatures skipped (292-297), `bind_abi` then the PC call wrapped in `fresh_indirect_arguments` (332-340): a **COMMON→INDIRECT_ARGUMENT→COMMON transition of an XCL-owned buffer around the call** — the NVIDIA fix so the driver waits for argument writes | graphics indirect is untranslated today; the legacy `ExecuteIndirect(DRAW)` must wrap itself the same way (the same driver ordering bug applies to XCL's own args buffer) |
| 8 | `real.rs::dev_create_command_signature` (3237) | `creator!` macro — passes straight to the driver (`Kind::CommandSignature`), no translation | the legacy chain uses **XCL's own** DRAW signature (D6: 16-byte VertexCount/InstanceCount/StartVertex/StartInstance, written by scan-CS), not a title signature — no change to the title's signatures (6.2 then only has to not break them) |
| 9 | `hle/d3d12x/frame.rs` | console frame clock: `SetFrameIntervalX` / `ScheduleFrameEventX` / `WaitFrameEventX` (frame origin token) / `PresentX` / `GetFrameStatisticsX`; no per-frame allocator reset exists (nothing to reset today) | D8's `StoreRing` (N=3 frames in flight) resets its current frame's region at the frame origin — the natural hook is the frame event / `PresentX` tracking already here |

## Assertions table

| # | Assertion | Source | How verified |
|---|---|---|---|
| 1 | No DispatchMesh hook exists; two-pass execution lives only in `runner::mesh_draw_split` | grep `hle/d3d12x` | code read |
| 2 | A title's DispatchX with a two-pass compute PSO is skipped today (not in TRANSLATED_COMPUTE) | `bind.rs:806-814`, `real.rs:5365` vs `:5044` | code read |
| 3 | `TWO_PASS` is keyed by the mesh PSO pointer; `TwoPassPso` carries `compute`, `compute_rs`, `mesh_rs`, `record`, `mesh_groups_per_record`, `kind` | `real.rs:5117, 5365`, `two_pass_of` `:5120` | code read |
| 4 | The dispatch side has no way today to map a bound compute PSO back to its `TwoPassPso` | `real.rs:5120-5123` | code read — the reverse index is new work for 4.2/4.4 |
| 5 | `groups` is known at record time (the DispatchX argument) and `split` from the registry | `list_x.rs:2767`, `real.rs:5365` | code read |
| 6 | The only inter-pass ordering the reference needs is one UAV barrier on `res` | audit 1.2 B6, `runner.rs:1402` | code read |
| 7 | ExecuteIndirect needs XCL's COMMON→INDIRECT_ARGUMENT wrap against the driver's stale-argument fetch | `bind.rs:318-340` (the #59 comment) | code read; measured by `execute_indirect_reads_the_arguments_written_before_it` (test name in the comment) |
| 8 | CommandSignature creation is pass-through; no title signature is transformed | `real.rs:3237` | code read |
| 9 | The frame clock gives a per-frame origin hook with no allocator state today | `frame.rs` module docs (1-8) | code read |

## What this means for the stage-2..4 steps

- 2.3's `PSO_LEGACY_PENDING` logs at points 1-3 (the `finish_two_pass` entry) — no new
  hook needed.
- 4.4's runtime chain lives in point 6 (`dispatch`/`dispatch_x`) plus point 4
  (`graphics_bound`) for the pipeline/RS state; the ExecuteIndirect wrap of point 7 is
  copied for XCL's own args.
- The reverse index (compute PSO → `TwoPassPso`) is the one structural addition to
  `TWO_PASS` (point 3) the chain needs; everything else already exists.
