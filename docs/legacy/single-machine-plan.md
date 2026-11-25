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

## 1. What WARP is and is not (F3–F6)

**WARP (package 1.0.22 + Agility) IS a usable mesh-path reference** for exactly the
things the legacy backend needs to be compared against: the two-pass NGG pipeline —
compute pass writing the `res` records, the pass-through mesh shader, `DispatchMesh`,
`mesh_draw_split` and its pixel output. The whole mesh-filter family runs with zero
skips.

**WARP is NOT a reference for the compute semantics suite.** On the full recomp filter
it disagrees on a lane-read value and then loses the device (F5); a software renderer
diverging on cross-lane semantics cannot arbitrate recompiler correctness. Nothing in
the plan below uses WARP that way. Any "WARP as reference" line below means the mesh
path only, and each such use carries a device-removed risk note.

The `no heap windows` GPU test also skips on WARP ("no GPU" — shared-heap windows),
consistent with the 1080 note in 0.6.

## 2. Step-by-step replacement table (DEV steps and their dependents)

| Playbook step | Was | Replacement on one machine | Acceptance change (stated openly) |
|---|---|---|---|
| 0.4 baseline on DEV | mesh-GPU baseline: release build, full suite, title run + smoke, report artifacts | **GTX 1080**: already delivered by 0.6 (release build green, full suite 1154/0/46, e2e title ok, logs in `docs/legacy/0.6-full-test-run.txt`) + 0.2a (G0). Propose closing 0.4 as "satisfied by the TGT baseline" (owner's word needed; status untouched here) | **Weaker in one aspect**: the DEV baseline would have shown the mesh path working; the TGT baseline shows the mesh path *not drawing* (that is the project's starting point, not a regression). All numbers are real, from the target machine |
| 0.7 compute on WARP (wave32) | DEV, early proof that wave64→wave32 rendezvous works on 32-lane adapters | **Done as part of this revision's step 2** (F4/F5): the mesh-filter's compute passes (incl. the wave64 rendezvous) run on WARP; the deeper compute suite fails there (F5), which on DEV would have been measured on WARP the same way. Propose closing 0.7 with the Этап-2 artifacts | The 0.7 acceptance "differences with the DEV-native run explained" becomes "differences with the 1080 run explained" — same shape, different device |
| 1.4 audit stages/GS + NGG statistics | DEV + title dump | **GTX 1080 + `<RPF>`** (F7/F8): the pack is readable and its records decode; statistics (`max_vertices`, `max_primitives`, per-primitive outputs, MS/ES+GS/VS+PS shares) are collected read-only from `<RPF>` on this machine. `xcl shaders <RPF> …` survey runs on TGT | none — same commands, same data |
| 2.3 flag into PSO hooks (log-only) | DEV, testtitle | **GTX 1080**: the test title runs on Pascal (F2); `PSO_LEGACY_PENDING` logging and the "off == baseline" log diff are done against the 0.6 TGT baseline | baseline is the TGT log, not a DEV log — no practical weakening |
| 2.4 smoke "legacy off == baseline" | DEV, smoke.ps1 on a title | **GTX 1080 + `<GAME_DIR>`** (title run is now possible — F7; the run itself needs the owner's go, this revision did not launch the game) | baseline = the same machine's earlier smoke, so the comparison is *tighter* than DEV-vs-TGT would have been |
| 3.5 golden scan+recon == mesh/reference | DEV mesh-GPU: legacy output vs `mesh_draw`/`mesh_draw_split` | **WARP as the mesh reference + GTX 1080 as the legacy side** (F4): records `res` and index buffers are compared bitwise where the compute is deterministic (res/IB are pure compute — device-independent), pixels via a tolerance | **Stated weakening**: pixels are now cross-device (WARP rasterizer vs NVIDIA). For `res`/IB no weakening (bitwise). Pixel comparisons need a threshold fixed BEFORE the run (PSNR/epsilon) |
| 3.6 correctness on wave32 | DEV (WARP wave32 vs native wave64) | **Same machine does both**: WARP has native 64-lane waves (F3) and the 1080 is wave32 — run the same golden set on both and compare | none — this check gets *stronger* (both sides on one host) |
| 3.7 G1 retail golden | DEV mesh-GPU, retail programs | **WARP + `<RPF>`**: retail NGG programs through `mesh_draw_split` on WARP vs legacy on the 1080; `res`/IB bitwise, pixels with threshold | **Weaker**: G1 was "bitwise/PSNR + a mesh-GPU as oracle"; the oracle is now a software mesh renderer with a known device-removed risk (F5). Fallback if WARP drops the device on retail shaders: **CPU reference** — rebuild the expected index list straight from the `res` records (their format is fixed by audit 1.1 and the compute pass is deterministic) and compare the legacy IB against it; golden pixels then deferred to G2's tolerance check |
| 4.2 legacy RS + graphics PSO | DEV | **GTX 1080** (heap RS + PSO creation proven, F1) | none |
| 4.3 CommandSignature + args | DEV | **GTX 1080** | none |
| 4.4 runtime draw chain (indirect) | DEV | **GTX 1080** — the target machine itself | none — this is the goal, not a compromise |
| 4.5 frame-diff tool (imgdiff, FRAME_SAVE) | DEV | **any/TGT** — pure tooling | none |
| 4.6 G2 "picture" | DEV mesh-GPU golden + owner's eyes | **WARP reference frame vs 1080 legacy frame** + the owner's visual confirmation (required by the playbook, unchanged — the owner is the same person) | **Weaker**: cross-device pixel comparison (WARP vs NVIDIA rasterization differences are real). Mitigation: fixed PSNR/epsilon tolerance written down BEFORE the run; structural comparison (non-black pixel counts, triangle coverage) alongside PSNR |
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
| G1 retail golden (3.7) | DEV mesh-GPU, retail golden | **WARP mesh reference + `<RPF>`**, `res`/IB bitwise; pixels thresholded; **CPU reference as fallback** when WARP loses the device on retail shaders (F5 risk) | golden oracle is software, not hardware; if WARP fails on the retail set the pixel part of G1 falls back to structural checks — that must be decided and written down BEFORE the first G1 run |
| G2 picture (4.6) | DEV mesh-GPU frame vs legacy frame, bitwise/PSNR + owner's visual pass | **WARP reference frame vs 1080 legacy frame**, PSNR/epsilon fixed in advance + structural stats + owner's visual confirmation (still mandatory) | cross-device pixels: WARP and NVIDIA rasterize slightly differently; a pure bitwise comparison is impossible — the tolerance must be set BEFORE the run and justified once |
| G3 cull on == off (5.5) | DEV | **GTX 1080** only | none |
| G4 title run + tests on TGT (7.2/7.3) | TGT | **unchanged** — and now concretely possible: the retail dump `<GAME_DIR>` exists (F7), so 7.2's "run the title" is no longer hypothetical | none (the gate was always TGT; the missing piece — a title — is now present) |

## 4. The 0.4 dependency

0.4 was the DEV baseline that 0.5 formally depended on (a playbook ordering deviation
already recorded in PROGRESS.md «Решения»). On one machine the baseline role is
fulfilled by the TGT facts F1/F2 (0.2a + 0.6). Proposal: mark 0.4 "satisfied by the TGT
baseline (0.2a + 0.6)" and note that its DEV-only content (a mesh-GPU baseline of what
graphics worked) is **not reproducible on this machine and never was needed again** —
every later step compares against the same machine's own baseline. Status change only
with the owner's approval; nothing was edited in the step table.

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

1. **WARP device-removed on retail shaders (highest risk).** Already measured on the
   synthetic suite (F5: 126 failures after 0x887A0005). If WARP cannot carry the retail
   NGG set through `mesh_draw_split`, the mesh reference shrinks to "what WARP
   survives" and G1/G2 lean on the CPU reference. Mitigation: CPU reference (records →
   expected IB is fully determined by the 1.1 format; the compute pass is
   deterministic), structural pixel checks, and a WARP-survivor list recorded per run.
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

1. Close 0.4 as "satisfied by the TGT baseline (0.2a + 0.6)" and 0.7 with the Этап-2
   artifacts (both need the owner's word; statuses untouched until then).
2. Stage 1 audits (1.1–1.3, 1.5–1.7) on TGT as ANY-steps — unchanged.
3. 1.4 with `<RPF>` on TGT — unchanged commands, real retail statistics.
4. From stage 2 on: TGT for everything; WARP (package DLL + Agility) as the mesh
   reference in 3.5/3.7/4.6/7.6 with the tolerances fixed before each golden run;
   CPU reference ready as the G1 fallback.
5. 2.4/6.2/7.2 title runs with `<GAME_DIR>` — each individually approved by the owner
   before the first launch of the game.
