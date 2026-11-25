# Audit 1.1 — PSO / record format (step 1.1, taken by TGT)

What a console NGG (MS) program becomes in XCL's two-pass pipeline, and the exact
dword layout of the record its compute pass writes — the input the legacy backend
replaces the mesh pass for. Every claim below names its source; the layout claims
marked **[DUMP]** are confirmed against a real record dumped on the GTX 1080 by
`tools/record_dump` (see "The dump" at the end).

## A. The two-pass shape

The console MS container becomes a `TwoPass` (`gpu/recomp/pso.rs:820`):

- `cs` — the console program translated to HLSL and compiled by DXC as **`cs_6_6`**
  (`pso.rs:885` via `compile_console_mesh`, and `pso.rs:848` via `finish_two_pass`;
  with `XCL_DIRECT_DXIL=1` the direct DXIL translator replaces the HLSL route,
  `pso.rs:861-879`). The compute pass writes **one record per thread group into
  root UAV `u1`** (`runner.rs:1266-1268`).
- `ms` — a generated pass-through mesh shader compiled as **`ms_6_6`**
  (`pso.rs:838`, `emit.rs:5209/5220`).
- `record` — the `MsInterface` the console container declared.
- `mesh_groups_per_record` — see C.
- `samplers` — static samplers of the compute pass.

`console_mesh` (`pso.rs:929`) is the entry: it parses the container
(`console_mesh_with`, `pso.rs:889-925`): `xbox::parse` → the FSSM fragment with
`hw == 7` (the NGG variant) → `interface` → `Mode::Mesh(ms)`; `wave64 =
detect_wave64(code)`; `split = wave64 && split_wave64`; the compute pass goes
through the shader cache keyed by `fnv64(blob)`, stage, `windows`, `split`
(`pso.rs:910-923`).

## B. The record layout (dword by dword)

`MsInterface` (`gpu/recomp/mod.rs:158-165`): the console's per-vertex outputs
(`OSG1`), `threads` (numthreads.x), `max_vertices`, `max_primitives`.

| # | Claim | Source | Verified |
|---|---|---|---|
| B1 | One record per thread group, in root UAV `u1` (`res`); the runner reads them back concatenated per group | `runner.rs:1266-1268`, `emit.rs:5223` (`res : register(u1)`) | **[DUMP]** — `run_in_arena` returned the record at `raw[0..]` |
| B2 | `MESH_HEADER = 4` dwords before the vertices: **vertex count, primitive count, 2 spare** | `mod.rs:167-168` | **[DUMP]** — header `[3, 1, 0, 0]`: vertex_count=3, primitive_count=1, both spare dwords are 0 |
| B3 | `vert_dwords = 4 * (1 + ranked(outputs, true).len())` — `pos` float4 + one float4 per ranked output, raw bits | `mod.rs:172-174` | **[DUMP]** — outputs [SV_Position, TEXCOORD] → ranked(with clip) = 1 → `vd = 8`; vertex 0 reads `pos = (-1, -1, 0, 1)`, `param = (0, 0, 0, 1.0f)` |
| B4 | `record_dwords = MESH_HEADER + max_vertices * vert_dwords + max_primitives` | `mod.rs:176-178` | **[DUMP]** — `4 + 128*8 + 128 = 1156` dwords; the dump has exactly 1156 lines |
| B5 | `ranked(sig, with_clip)`: non-system elements (`system_value == 0`, name not `SV_*`) sorted by register; `with_clip` appends clip/cull (`system_value` 2 or 3) after them, sorted by register | `mod.rs:310-319` | code read; `ranked` order is what B3's slot numbering uses |
| B6 | The per-vertex slot order is the ranked order: slot k lives at record dword `MESH_HEADER + v*vert_dwords + 4*(1+k)` (pos is slot −1, i.e. the first float4 of the vertex) | `emit.rs:5250-5256` (`Load4(b + 16*(slot+1))`, `b = (rec + 4 + i*vd)*4`) | **[DUMP]** — the TEXCOORD slot (k=0) is dword 4..8 of each vertex, values match the test's `(group, vertex, ·, 1.0)` expectation |
| B7 | One dword per primitive after the vertices, at `MESH_HEADER + max_vertices*vert_dwords + j`; it packs three 10-bit indices (v0 = bits 0-9, v1 = bits 10-19, v2 = bits 20-29) and bit 31 marks a null primitive | `emit.rs:5265`, `5285-5288` (`p & 0x3FF`, `>>10 & 0x3FF`, `>>20 & 0x3FF`, `(p >> 31) != 0`) | **[DUMP]** — primitive [0] = `0x00200400` = `1<<10 \| 2<<20` → indices (0, 1, 2), bit 31 = 0 |
| B8 | The header counts come from the console program's GS_ALLOC_REQ (S_SENDMSG, message 9): `s124 = num_vertices | num_primitives << 12` in the synthetic program; the record header is `res[0] = vertex_count`, `res[1] = primitive_count` | test `gpu_mesh_mode_two_passes` (`tests.rs:3359-3362`, asserts `r[0], r[1] == (3, 1)`), runner reads them back as `nv/np` (`emit.rs:5278`) | **[DUMP]** + the same test green on WARP (27/0/0, `test_warp_mesh.log`) |
| B9 | The pass-through clamps `nv = min(res[0], max_vertices)`, `np = min(res[1], max_primitives)`; **`np` counts null and out-of-range primitives too** — a bad primitive is emitted as a degenerate triangle `(0,0,0)`, not dropped from the count | `emit.rs:5278, 5287-5288, 5306-5311` | code read — a semantics point the legacy backend must decide (kept for 1.5 / D2) |

## C. `mesh_groups_per_record` and the split

| # | Claim | Source | Verified |
|---|---|---|---|
| C1 | `mesh_groups_per_record = passthrough_split(record, passthrough_elements(record, ps_inputs))` — decided when the TwoPass is finished, from the PS's input signature | `pso.rs:839` | code read |
| C2 | `passthrough_elements`: without a PS → `ranked(outputs, true).len()`; with a PS → `ranked(ps_inputs, false).len()` plus the program's clip/cull outputs (they always ride along) | `emit.rs:5344-5350` | code read |
| C3 | D3D12 mesh output limit: 32768 B per group; `passthrough_max_vertices(vbytes) = (32768/vbytes).min(256) / 32 * 32` — DXC accounts whole chunks of 32 vertices (`vbytes = 16*(1+elements)`) | `emit.rs:5316-5326` | code read; the chunk accounting is the fix of `mesh_split_accounts_vertex_chunks` (`tests.rs:3432`, green on WARP) |
| C4 | If `max_vertices <= cap` → **1** mesh group per record (the whole record, one group). Otherwise the triangles go out in **runs**: `ppg = (cap/3).max(1)` triangles per group, `mesh_groups_per_record = ceil(max_primitives / ppg)` — each run re-copies its triangles' three vertices (`verts = 3*ppg`) | `emit.rs:5291-5311`, `passthrough_split` `emit.rs:5333-5341` | code read; the split pass is pixel-identical to the single group (`gpu_mesh_mode_two_passes` asserts `px16 == px`, green on WARP) |
| C5 | Split dispatch math: group `gid.x` → record `gid.x / split`, run `gid.x % split`, first primitive `(gid.x % split) * ppg`; `n = min(np - first, ppg)`; `SetMeshOutputCounts(3n, n)`; the re-copied vertex index is `3j+c`, triangle indices become consecutive `(3j, 3j+1, 3j+2)` | `emit.rs:5299-5311` | code read — **the split changes the primitive→vertex wiring**, so a legacy golden must state which form produced it |

## D. Root parameters of the compute pass

| # | Claim | Source | Verified |
|---|---|---|---|
| D1 | ABI root signature: **6 root UAV descriptors, registers `u0..u5`, space 0** — `u0 = io` (SGPR/VGPR block), `u1 = res` (the records), `u2..u5 = the UMA memory windows`; `D3D12_ROOT_DESCRIPTOR_FLAG_DATA_VOLATILE`; `CBV_SRV_UAV_HEAP_DIRECTLY_INDEXED \| SAMPLER_HEAP_DIRECTLY_INDEXED` (SM 6.6 bindless); graphics adds the input-assembler flag | `pso.rs:1046-1080` (`abi_root_signature_with`) | code read; the runner builds the same signature for its GPU (`runner.rs:119, 127-129` → `abi_root_signature`) |
| D2 | With `heap_windows` (`XCL_HEAP_WINDOWS`), the windows also sit in a shader-visible descriptor heap and the HLSL addresses them by heap index instead of a root-UAV `switch` | `pso.rs:849` region (`mem_windows`), `runner.rs:1525-1527` | code read |
| D3 | The record buffer `res` holds `KERNEL_RES_DWORDS` dwords in the runner (`io + 64*lanes*4 split words + 1`), i.e. all groups' records concatenated | `mod.rs:107-109`, `runner.rs:1505-1509` | **[DUMP]** — `run_in_arena` read the first record from `raw[0..1156]` |

## E. Where `Mode::Mesh` becomes `cs_6_6`

`pso.rs:885` (`compile_console_mesh` → `dxc::compile_cached(hlsl, "main", "cs_6_6")`)
and `pso.rs:848` (`finish_two_pass`); the direct-DXIL alternative (`XCL_DIRECT_DXIL`)
is `pso.rs:861-879`. The pass-through MS is `ms_6_6` at `pso.rs:838`. The runner
requires SM 6.6 + wave ops before any of this (`runner.rs:101-110`).

## F. G1 reference contract (for `single-machine-plan.md` §3)

The legacy backend's golden input is exactly this record stream. Contract for the
reference generator and the dump tooling:

1. **Dump**: one run of the compute pass (the unchanged first pass) on the 1080; the
   records are read back per group as `raw[g*rd .. (g+1)*rd]` (`rd = record_dwords`)
   — `tools/record_dump` already does this for one group through the same public
   path the tests use (`translate_mesh` → `dxc::compile(cs_6_6)` → `run_in_arena`);
   the same tool scales to `groups` by dispatching more groups.
2. **Reference generator** (independent implementation of D2, shares no code with
   scan-CS/recon-CS): for each record `rec` in order — `np = raw[rec*rd + 1]`;
   exclusive prefix sum over the records gives each record's first primitive slot;
   for `slot = 0..np-1`: `p = raw[rec*rd + 4 + max_vertices*vd + slot]`; skip the
   `p >> 31` null-primitive bit **only if** the legacy chain decides to cull (B9 —
   the mesh pass does NOT drop them from the count); indices
   `(p & 0x3FF, p>>10 & 0x3FF, p>>20 & 0x3FF)`; vertex global id
   `rec*max_vertices + local` (D3-A); args `[Σnp, 1, 0, 0]` (D6).
3. The legacy chain's IB/args are compared bitwise against this output on the 1080.

## The dump

`tools/record_dump` (this step) — the synthetic program of `gpu_mesh_mode_two_passes`
through the real path on the **GTX 1080** (Agility SDK, split-wave64 form):

```
adapter: NVIDIA GeForce GTX 1080 (wave64=false, split=true)
MsInterface: outputs=2 ranked(with_clip)=1 threads=128 max_vertices=128 max_primitives=128
vert_dwords = 4*(1+ranked) = 8;  record_dwords = 4 + 128*8 + 128 = 1156
header dwords [0..4]: [3, 1, 0, 0]
vertex 0: pos=[-1.0, -1.0, 0.0, 1.0] params=[0, 0, 0, 1065353216]        // (g, v, ·, 1.0)
vertex 1: pos=[-1.0,  1.0, 0.0, 1.0] params=[0, 1065353216, 0, 1065353216]
vertex 2: pos=[ 0.0, -1.0, 0.0, 1.0] params=[0, 1073741824, 0, 1065353216]
primitive dwords live: [0] 0x00200400 (0,1,2)
```

Full 1156 dwords: `artifacts_tgt/record_dump.txt` (outside the repo, `*.txt` under
`artifacts_tgt/` is not tracked). 1065353216 = 1.0f, 1073741824 = 2.0f — the params
are the test's `(group, vertex index, ·, 1.0)`; the primitive dword is the test's
`1<<10 | 2<<20` (indices 0,1,2; bit 31 clear).

**Open points for later steps**: B9 (do null/bad primitives keep their index slots
in the legacy IB — the mesh pass keeps them as degenerate triangles, `emit.rs:5287`);
per-primitive outputs / `SV_PrimitiveID` (the record format above has **no**
per-primitive payload — anything the console program emits per primitive goes
through `EXP` targets the audit of 1.3 must locate); `2 spare` header dwords are
written as 0 by the current translation (B2) — the legacy scan must not read them.
