# Single-machine plan — XCL legacy GPU backend on one GTX 1080

Status: PROPOSAL (2026-11-25). Nothing here changes `AGENT_PLAYBOOK.md`, XCL code or any
step's status by itself; every status change needs the owner's word. The second machine
(DEV, mesh-GPU) does not exist and will not: the only hardware is TGT (GTX 1080, Pascal).
This document re-derives every DEV-machine step of the playbook and each gate for the
single-machine reality, based on measured facts only.

## 0. Measured facts this plan rests on (all from this machine)

| # | Fact | Where measured |
|---|---|---|
| F1 | GTX 1080 (driver 32.0.15.8278) through the Agility SDK 1.620.2: ShaderModel **6.8**, heap-directly-indexed RS ok, cs_6_6 + cs_6_0-wave PSO ok; MeshShaderTier **0**; WaveLaneCount **32/32**; ResourceHeapTier 1; TiledResourcesTier 4 (3 on inbox) | `tools/legacy_probe/out_tgt_agility.json`, `out_tgt_inbox.json` (steps 0.2a/0.3, G0 = pass) |
| F2 | Full workspace test suite on the 1080 + Agility: **1154 passed / 0 failed / 46 ignored / 35 GPU-skip**; the e2e test title runs to completion on Pascal (`xcl_testtitle: done, failures: 0`) | step 0.6, `docs/legacy/0.6-full-test-run.txt` |
| F3 | WARP from the **NuGet package** `Microsoft.Direct3D.WARP 1.0.22` (x64 d3d10warp.dll 1.0.22.0, Authenticode valid; copied next to the exe, loaded INSTEAD of System32 — confirmed by the loaded path) + the same Agility SDK: **MeshShaderTier 10 (≥ tier 1), WaveLaneCount 4/128 (native 64-lane waves!), ShaderModel 6.8**, ResourceHeapTier 2 | step 2.1, `tools/legacy_probe/out_tgt_warp.json` |
| F4 | `cargo test -p xcl --lib mesh` with `XCL_ADAPTER=warp` (+package DLL +Agility): **27 passed / 0 failed / 0 skipped** — `gpu_mesh_mode_two_passes`, `passthrough_mesh_stream_accepts_a_triangle_driver_blob`, `gpu_mesh_mode_reads_memory_windows` and the split half (`mesh_draw_split`, incl. the `px16 == px` pixel comparison) all REALLY execute (1.61 s of real dispatches vs 0.16 s on the system WARP) | step 2.2, `artifacts_tgt/test_warp_mesh.log` |
| F5 | `cargo test -p xcl --lib recomp` on the same WARP: **227 passed / 126 failed / 19 ignored** — first a real semantic mismatch (`lane reads, wave32: lane 8 v6: left 21, right 45`, lower.rs:4151), then **D3D12 device removed (0x887A0005)** and a cascade of failures in that one process | step 2.2, `artifacts_tgt/test_warp_recomp.log` |
| F6 | System WARP (no package DLL): MeshShaderTier 0 — the mesh tests skip "no mesh shaders". The package d3d10warp.dll next to the exe is MANDATORY for any WARP mesh work | step 2.2 first run |
| F7 | Retail game dump `<GAME_DIR>` (a folder, not an archive): the game exe reads fine (`xcl inspect`: console title, imports d3d12_xs.dll, 587 functions / 68 DLLs, **0 unimplemented**), and `<RPF>` = `<GAME_DIR>\shaders.rpf` (≈393 MB, all records `xbox-native`, 0 inflate errors, 0 undecoded dwords) is a valid shader pack | step 5 |
| F8 | With `XCL_SAMPLE_RPF=<RPF>` the retail lib tests REALLY run on the 1080: `retail_pack_sample` and `gpu_retail_vs_blit_gen_amb_cube` (a real console VS translated, compiled and dispatched on Pascal) both pass (7 passed / 0 failed / 0 skipped in the `retail` filter) | step 5, `artifacts_tgt/test_retail_rpf.log` |
| F9 | The 20 "no wave64" skips of the 1080 (13 unique tests) re-run on WARP **one test per process** (no cascade possible): **6 pass, 2 fail with value mismatches on WARP, 5 fail on their own device removal**. The mismatches (`cross_lane_reads_agree_with_the_hlsl_path_on_the_gpu`, lane 8 v6: 21 vs 45, lower.rs:4151; `gpu_geometry_wave32_prefix_count_over_a_mask`, prefix counts wrong for lanes ≥ 32, tests.rs:4517) are **on WARP with the cause not established**: WARP has a variable wave size (4–128), and it is not proven to be a recompiler bug — both tests sit in the native-wave64 area the 1080 never executes, so no hardware cross-check exists. The 5 device-removal failures are all in the direct-DXIL path (`lower_dxil`), including with a single test in the process | step 2 of the owner's corrections, `artifacts_tgt/warp_iso.log` |

## 1. What WARP is and is not (F3–F6)

**WARP (package 1.0.22 + Agility) IS a usable mesh-path reference** for exactly the
things the legacy backend needs to be compared against: the two-pass NGG pipeline —
compute pass writing the `res` records, the pass-through mesh shader, `DispatchMesh`,
`mesh_draw_split` and its pixel output. The whole mesh-filter family runs with zero
skips.

**WARP is NOT a reference for the compute semantics suite.** Isolated single-test runs
(F9) show two value mismatches on WARP in the native-wave64 area (cause not
established — WARP's wave size is variable, a recompiler bug is not proven; the 1080
never executes that area) and five self-contained device removals in the direct-DXIL
path — a software renderer that disagrees on cross-lane/prefix semantics and drops
the device cannot arbitrate
recompiler correctness. Nothing in the plan below uses WARP that way: WARP's roles are
(a) the mesh-path reference for golden comparisons — always rendered on the SAME WARP
device as the legacy chain when the comparison is bitwise (G2 primary, G1 secondary),
and (b) nothing else.

The `no heap windows` GPU test also skips on WARP ("no GPU" — shared-heap windows),
consistent with the 1080 note in 0.6.

## 2. Step-by-step replacement table (DEV steps and their dependents)

| Playbook step | Was | Replacement on one machine | Acceptance change (stated openly) |
|---|---|---|---|
| 0.4 baseline on DEV | mesh-GPU baseline: release build, full suite, title run + smoke, report artifacts | **GTX 1080**: already delivered by 0.6 (release build green, full suite 1154/0/46, e2e title ok, logs in `docs/legacy/0.6-full-test-run.txt`) + 0.2a (G0). Propose closing 0.4 as "satisfied by the TGT baseline" (owner's word needed; status untouched here) | **Weaker in one aspect**: the DEV baseline would have shown the mesh path working; the TGT baseline shows the mesh path *not drawing* (that is the project's starting point, not a regression). All numbers are real, from the target machine |
| 0.7 compute on WARP (wave32) | DEV, early proof that wave64→wave32 rendezvous works on 32-lane adapters | **NOT accepted on WARP** (owner's correction): WARP's WaveLaneCount is 4/128 — that is not a wave32 adapter. The step is **superseded by the 0.6 fact**: real wave32 proven on the 1080 itself (`--lib recomp` 353 passed / 0 failed, split-wave64 rendezvous included). Propose marking 0.7 `superseded` in the journal (owner's word; status untouched here) | none — the 0.6 numbers are the evidence, measured on real wave32 hardware |
| 1.4 audit stages/GS + NGG statistics | DEV + title dump | **GTX 1080 + `<RPF>`** (F7/F8): the pack is readable and its records decode; statistics (`max_vertices`, `max_primitives`, per-primitive outputs, MS/ES+GS/VS+PS shares) are collected read-only from `<RPF>` on this machine. `xcl shaders <RPF> …` survey runs on TGT | none — same commands, same data |
| 2.3 flag into PSO hooks (log-only) | DEV, testtitle | **GTX 1080**: the test title runs on Pascal (F2); `PSO_LEGACY_PENDING` logging and the "off == baseline" log diff are done against the 0.6 TGT baseline | baseline is the TGT log, not a DEV log — no practical weakening |
| 2.4 smoke "legacy off == baseline" | DEV, smoke.ps1 on a title | **GTX 1080 + `<GAME_DIR>`** (title run is now possible — F7; the run itself needs the owner's go, this revision did not launch the game) | baseline = the same machine's earlier smoke, so the comparison is *tighter* than DEV-vs-TGT would have been |
| 3.5 golden scan+recon == mesh/reference | DEV mesh-GPU: legacy output vs `mesh_draw`/`mesh_draw_split` | **Primary: self-consistent on the 1080** — the compute pass writes the `res` records; an **independent reference generator** rebuilds the expected IB/args from those records (see "G1 reference mechanism" below); legacy IB/args compared **bitwise**. **Secondary: WARP mesh reference** (`mesh_draw_split` from the same records) for the mesh-pass half of the chain | **Stated weakening**: the primary check is an independent second implementation, not an independent device; it catches implementation bugs, not contract misunderstandings — the contract itself is pinned by audit 1.1/1.6 and the mesh-path (WARP) secondary |
| 3.6 correctness on wave32 | DEV (WARP wave32 vs native wave64) | **GTX 1080** (owner's correction): the 1080 IS the real wave32 machine — the golden set runs there in the split-wave64 form (already proven: recomp 353/0 on Pascal). WARP is not used for this step (its 4/128 lane range is not wave32) | none — the check runs on real wave32 hardware, exactly what the step wanted |
| 3.7 G1 retail golden | DEV mesh-GPU, retail programs | **WARP + `<RPF>`**: retail NGG programs through `mesh_draw_split` on WARP vs legacy on the 1080; `res`/IB bitwise, pixels with threshold | **Weaker**: G1 was "bitwise/PSNR + a mesh-GPU as oracle"; the oracle is now a software mesh renderer with a known device-removed risk (F5). Fallback if WARP drops the device on retail shaders: **CPU reference** — rebuild the expected index list straight from the `res` records (their format is fixed by audit 1.1 and the compute pass is deterministic) and compare the legacy IB against it; golden pixels then deferred to G2's tolerance check |
| 4.2 legacy RS + graphics PSO | DEV | **GTX 1080** (heap RS + PSO creation proven, F1) | none |
| 4.3 CommandSignature + args | DEV | **GTX 1080** | none |
| 4.4 runtime draw chain (indirect) | DEV | **GTX 1080** — the target machine itself | none — this is the goal, not a compromise |
| 4.5 frame-diff tool (imgdiff, FRAME_SAVE) | DEV | **any/TGT** — pure tooling | none |
| 4.6 G2 "picture" | DEV mesh-GPU golden + owner's eyes | **Primary: BOTH paths on ONE device (WARP)** — `mesh_draw_split` (reference) and the full legacy chain (scan/recon/ExecuteIndirect) render the same frame/program on the same WARP; **bitwise** comparison, no tolerance needed. **Secondary metric: WARP reference vs 1080 legacy frame** (cross-device, PSNR/epsilon fixed in advance). Owner's visual confirmation stays mandatory (same person) | **Stated openly**: the primary G2 comparison is bitwise but on a software renderer; the cross-device metric is a secondary, tolerance-based check. If WARP drops the device on the chosen retail frame (see risk 1), G2 falls back to the secondary metric alone |
| 5.1 cull CS (ANY/DEV) | — | GTX 1080 | none |
| 5.2 backface flag | DEV | **GTX 1080** (cull on == off is compared on the SAME device — no reference needed) | none |
| 5.3 Hi-Z occlusion (optional) | DEV | GTX 1080, or **deferred** (already optional in the playbook) | none |
| 5.4 cull integration | DEV | GTX 1080 | none |
| 5.5 G3 cull on == off | DEV | **GTX 1080** — same-device comparison, no weakening | none (this gate does not need the mesh-GPU at all) |
| 6.1 allocator stability | DEV | GTX 1080 | none |
| 6.2 title's own graphics-indirect | DEV | GTX 1080 + `<GAME_DIR>` (title run — owner's go needed) | none |
| 6.3 barrier audit (debug layer) | DEV | **GTX 1080 + Graphics Tools** (d3d12SDKLayers.dll present; the e2e debug-layer test currently skips only on the missing System32\xgameruntime.dll — see risks) | needs Gaming Services installed to run the debug-layer e2e test; that is an installable, not a hardware gap |
| 6.4/6.5 final isolation/flags | ANY | unchanged | none |
| 7.5 optimisations | TGT/DEV | GTX 1080 | none |
| 7.6 regression on DEV | DEV mesh-GPU | **GTX 1080 full suite (0.6 command) + WARP mesh-filter** as the two regression legs; the DEV "does the mesh path still work" leg becomes the WARP mesh-filter leg (F4) | **Weaker**: no hardware mesh regression — WARP is software. Stated plainly: if a change breaks mesh-PSO *creation* the WARP leg catches it; driver-level mesh regressions on real hardware are out of reach on this machine |
| 8.2 report.json counters | DEV/TGT | GTX 1080 | none |

## 3. Gates G1–G4 on one machine

| Gate | Was | Now | Weakening (explicit) |
|---|---|---|---|
| G0 SM 6.6 PSO | TGT | **PASS already** (F1) | — |
| G1 retail golden (3.7) | DEV mesh-GPU, retail golden | **Primary: 1080 vs its own records** — retail NGG programs run their compute pass on the 1080; the expected IB/args come from an **independent reference generator** built from the dumped records (below); comparison **bitwise**. **Secondary: WARP mesh reference** (`mesh_draw_split`) where WARP survives the retail set | the golden oracle is an independent implementation on the same device, not a mesh GPU; the WARP leg is secondary and partial (see risk 1) |

**Proposed tolerance for the secondary WARP-vs-1080 metric (NOT fixed — awaiting the
owner's word):**

- **Coverage mask: exact (bitwise).** D3D12 rasterization rules (top-left fill) are
  part of the spec, the vertex positions come from the same `res` records on both
  devices, so the set of covered pixels must match exactly; any difference is a real
  defect, not noise.
- **Colour: PSNR ≥ 40 dB over the frame's covered pixels**, counted on the 8-bit
  render target. Justification: the same fp32 attribute values quantised to 8 bits
  differ by at most a few LSB (PSNR of ±1 LSB on 8-bit ≈ 48–52 dB); WARP computes
  attribute interpolation in software with its own fp32/fp64 rounding, so 40 dB leaves
  an order-of-magnitude margin for renderer rounding differences while still failing
  on any structural colour error (a wrong triangle or attribute is typically < 20 dB).
  A per-pixel companion — at most 1 LSB (1/255) difference on ≥ 99.9 % of covered
  pixels — catches thin bright-edge errors that PSNR can average out.
- These numbers are a proposal only; nothing is fixed until the owner approves, and
  the decision must be written down before the first secondary-metric run (playbook
  4.6).

**G1 reference mechanism — how the reference is written independently of the code under test:**

1. The compute pass (the existing, unchanged first pass — console NGG code translated to cs_6_6) runs on the 1080 and its `res` records are **dumped once** to a file outside the repo (the runner already reads records back in the mesh tests; the audit-1.1 dump is the same readback). The records are data, not code.
2. A **reference generator** — a small standalone tool (in `tools/`, written for audit 1.1/goldens) reads that dump and produces the expected IB and args **straight from the record layout pinned in audit 1.1 and decision D2**: exclusive prefix sum of `primitive_count` over the record headers (the deterministic `(record, slot)` order), each primitive's three indices unpacked from the 10-10-10-1 dword, global vertex ids as `rec*max_vertices + local`, args as `[total, 1, 0, 0]`. It shares **no code** with the scan-CS/recon-CS emitters or the runtime chain: a second implementation of the same formulas, reviewed against the audit document.
3. The legacy chain (scan-CS, recon-CS, ExecuteIndirect) runs on the 1080; its IB and args buffers are read back and compared **bitwise** against the generator's output.
4. If the generator and the chain disagree, the tie-breaker is the WARP mesh reference (the same records through `mesh_draw_split` give the reference IB the mesh path itself consumes) and the audit-1.1 layout — a three-way disagreement means the contract document is wrong, which is a finding, not a failure of the check.
| G2 picture (4.6) | DEV mesh-GPU frame vs legacy frame, bitwise/PSNR + owner's visual pass | **Primary: one device (WARP) for both paths, BITWISE** — `mesh_draw_split` reference frame vs the legacy chain's frame, same WARP, same program; no tolerance. **Secondary: WARP reference vs 1080 legacy** (cross-device, PSNR/epsilon fixed in advance). Owner's visual confirmation still mandatory. On retail shaders the primary leg is conditional on the pre-G2 check (retail NGG through the main HLSL path on WARP, §7.4) | primary is bitwise (stronger than a tolerance) but on a software renderer; the hardware check is the tolerance-based secondary |
| G3 cull on == off (5.5) | DEV | **GTX 1080** only | none |
| G4 title run + tests on TGT (7.2/7.3) | TGT | **unchanged** — and now concretely possible: the retail dump `<GAME_DIR>` exists (F7), so 7.2's "run the title" is no longer hypothetical | none (the gate was always TGT; the missing piece — a title — is now present) |

## 4. The 0.4 dependency

0.4 was the DEV baseline that 0.5 formally depended on (a playbook ordering deviation
already recorded in PROGRESS.md «Решения»). On one machine the baseline role is
fulfilled by the TGT facts F1/F2 (0.2a + 0.6) **plus a smoke run of the title**
(`.\scripts\smoke.ps1 -Game <GAME_DIR>`) — without that smoke leg step 2.4 ("legacy off
== baseline") has no baseline to diff against and stays blocked. The smoke run needs
the owner's explicit go each time (the game launches); this revision did not launch
it. Proposal: mark 0.4 "satisfied by the TGT baseline (0.2a + 0.6 + the title smoke)"
only after that smoke exists, and only with the owner's word; statuses untouched here.
The DEV-only content of 0.4 (a mesh-GPU baseline of what graphics worked) is not
reproducible on this machine and is not needed again: every later comparison is
same-machine.

## 5. What can be verified without the retail game, and what the dump adds

Without any retail data (already proven): config, PSO hooks, scan/recon emitters and
their unit tests, `gpu_mesh_mode_two_passes`-shaped synthetic goldens on WARP (F4), the
whole recomp semantics suite on the 1080 (F2), the e2e test title, allocator/layout
logic, barriers, culling math.

The dump adds (F7/F8): real NGG-program statistics for 1.4/1.5; retail shader goldens
for G1 (via WARP mesh + `<RPF>`, CPU-reference fallback); retail PS/GS surveys; a real
title for 2.4/6.2/7.2 smoke and run. The game itself is NOT run in this revision
(read-only inspection only); every future run needs the owner's go, and no game file
ever enters the repository (`XCL_SAMPLE_RPF`/`XCL_ADAPTER`/`XCL_D3D12_DIR` are set only
in the environment of a run; frames and shader dumps go to a folder outside the repo).

## 6. Principal technical risks on a single GTX 1080 (and what kills the goal)

1. **WARP device-removal (highest risk).** Measured precisely (F9): five of the
   direct-DXIL-path tests drop the WARP device even alone in a process, and the mesh
   reference leg inherits that risk on retail shaders. Mitigation: G1's primary check
   is the independent reference generator on the 1080 (no WARP involved); WARP is the
   secondary leg; a WARP-survivor list is recorded per golden run.
2. **Cross-device golden pixels.** WARP vs NVIDIA rasterization/interpolation differ;
   G2's tolerance must be chosen once, before the first run, or the gate becomes
   either always-green or always-red. Mitigation: PSNR + coverage stats + the owner's
   eyes (already required by the playbook).
3. **Single point of failure.** One machine builds, tests, references and measures;
   an OS/driver/Agility update can change WARP and the 1080 behaviour at once with no
   second host to cross-check. Mitigation: pin the Agility package (1.620.2) and the
   WARP package (1.0.22) in `third_party/` (done), record versions in every report.
4. **No hardware mesh regression (7.6).** A change that breaks mesh-PSO creation is
   caught by the WARP leg; driver-level mesh behaviour is unverifiable here. Accepted
   consciously: the legacy backend's whole point is that the target machine has no
   mesh.
5. **Wave32 only on the 1080.** Split-wave64 programs run (proven) but slower; 7.4's
   performance numbers will reflect Pascal + split — that is the real target
   performance anyway, but expectations for other GPUs cannot be derived from it.
6. **Retail pack size vs a 2016-class CPU.** `<RPF>` is ≈393 MB; full precompile/golden
   sweeps will be slow. Mitigation: sampled runs first (`XCL_MS_EVERY`-style), full
   sweeps overnight.
7. **Missing system bits (installable, not hardware):** no System32\xgameruntime.dll
   (Gaming Services) — the e2e debug-layer test and some XGameRuntime paths skip; no
   Windows Developer Mode — the driver-cache-off test skips. Both are installs the
   owner can make; neither blocks the legacy backend itself.
8. **What would make the goal unachievable:** if (a) WARP drops the device on MOST
   retail NGG programs AND (b) the CPU reference cannot be built from the records
   (it can, unless the 1.1 audit finds the record stream is not self-contained) AND
   (c) the owner's visual pass is the only acceptance left — then G1/G2 would rest on
   eyes alone, which the playbook forbids. Judging by F4/F8 this is unlikely: the
   mesh path and the retail VS already run. The goal itself — geometry on the 1080
   via scan/recon/ExecuteIndirect — needs no mesh GPU anywhere and no second machine.

## 7. Proposed immediate sequence (for the owner to approve)

1. Mark 0.4 "satisfied by the TGT baseline (0.2a + 0.6 **+ the title smoke**)" — only
   after the owner's go for the smoke run of `<GAME_DIR>`; until then 2.4 stays blocked
   on its baseline. Mark 0.7 **superseded by 0.6** (real wave32 on the 1080:
   recomp 353/0; WARP's 4/128 is not wave32). Both need the owner's word; statuses
   untouched until then.
2. Stage 1 audits (1.1–1.3, 1.5–1.7) on TGT as ANY-steps — unchanged. 1.1 additionally
   produces the record-dump + reference-generator contract used by G1 (§3).
3. 1.4 with `<RPF>` on TGT — unchanged commands, real retail statistics.
4. From stage 2 on: TGT for everything. G1 primary = the independent reference
   generator vs the legacy chain on the 1080 (bitwise IB/args); G2 primary = mesh
   reference and legacy chain both on WARP, bitwise; the WARP-vs-1080 cross-device
   metric secondary with its tolerance fixed before the first run.
   **Pre-G2 check (added with the owner's approval of the plan):** retail NGG programs
   from `<RPF>` must run on WARP through the **main HLSL path** (not direct-DXIL,
   `XCL_DIRECT_DXIL` unset) — all five WARP device removals (F9) are in the direct-DXIL
   path, and the bitwise retail G2 needs the mesh half (`mesh_draw_split`) and the
   legacy chain to survive the same retail set on WARP. This check runs as soon as the
   golden tooling exists (3.5/3.7 setup): a sampled retail set through the mesh path on
   WARP, survivor list recorded; the tolerance-free G2 on retail is conditional on it.
5. 2.4/6.2/7.2 title runs with `<GAME_DIR>` — each individually approved by the owner
   before the first launch of the game.
