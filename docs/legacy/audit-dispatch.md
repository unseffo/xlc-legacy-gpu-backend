# Audit 1.2 — two-pass execution and the split (step 1.2, taken by TGT)

What the mesh pass of `TwoPass` receives and returns, how the split changes the
dispatch arithmetic, and the reference implementation `runner::mesh_draw_split`
written out call by call — the exact sequence a legacy chain replaces with
scan/recon/ExecuteIndirect. Companion of audit 1.1 (the record) and 1.3 (the
exports); claims carry `file:symbol`.

## (а) What the mesh pass receives and returns

| # | Claim | Source | Verified |
|---|---|---|---|
| A1 | Inputs bound on the graphics stage: the same ABI root signature, with `u0 = io`, `u1 = res` (the records), `u2 = mem` as root UAVs — the pass-through HLSL itself declares `io : register(u0)` and `res : register(u1)` and reads **only** `res` | `runner.rs:1405-1408`, `emit.rs:5223` | code read |
| A2 | The mesh PSO is a subobject stream: root signature, MS (pass-through), PS, blend (mask ALL on RT0), sample mask `0xFFFFFFFF`, rasterizer (`SOLID`, **`CullMode_NONE`**, depth clip on), default depth-stencil (no depth buffer), one RT `R32G32B32A32_FLOAT`, 1 sample | `runner.rs:1310-1332`, `MeshStream` `runner.rs:1256-1264` | code read; `CullMode_NONE` matches audit 1.3 (б): no culling in the reference either |
| A3 | Outputs: the record triangles rasterized with the PS into a `size × size` RGBA32F render target, cleared to `clear`; the runner reads the pixels back row by row (RowPitch-aware) and returns `(pixels, records)` — the records go through the compute pass and come back unchanged | `runner.rs:1334-1356, 1428-1439` | code read; the mesh filter (27/0/0 on WARP, F4) exercises the whole path incl. pixel asserts |
| A4 | `mesh_draw(cs, ms, ps, …)` is `mesh_draw_split` with `split = 1` | `runner.rs:1282` | code read |

## (б) How the split changes the dispatch arithmetic

| # | Claim | Source | Verified |
|---|---|---|---|
| B1 | Compute pass: `Dispatch(groups, 1, 1)` — exactly `groups` thread groups, one record of `record_dwords` each; the res buffer is sized `record_dwords * groups` | `runner.rs:1368, 1401` | code read; dump 1.1 saw one group's record |
| B2 | Mesh pass: `DispatchMesh(groups * split, 1, 1)` — the split multiplies the **mesh** group count, never the compute one | `runner.rs:1414` | code read |
| B3 | `split = passthrough_split(record, passthrough_elements(record, ps_inputs))`: `cap = (32768 / (16*(1+elems)))` clipped to 256 and rounded **down to whole 32-vertex chunks**; `max_vertices ≤ cap` → split 1; else `ppg = (cap/3).max(1)` triangles per run and `split = ceil(max_primitives / ppg)` | `emit.rs:5316-5326, 5333-5341`, `pso.rs:839` | code read; the chunk accounting is the fix proven by `mesh_split_accounts_vertex_chunks` (green on WARP, F4) |
| B4 | Group `gid.x` maps to `rec = gid.x / split`, run `first = (gid.x % split) * ppg`; `n = min(np - first, ppg)` triangles; `SetMeshOutputCounts(3*n, n)` with `verts = 3*ppg` declared; each run re-copies its triangles' three vertices (`verts[3j+c]`), so the run's triangle indices are consecutive `(3j, 3j+1, 3j+2)` — the split **rewires the vertex↔primitive relation** (record vertex ids are only inside the copies) | `emit.rs:5293-5311` | code read; audit 1.1 §C5 |
| B5 | The split form is pixel-identical to the single group: `gpu_mesh_mode_two_passes` asserts `px16 == px` between `mesh_draw` and `mesh_draw_split` — green on WARP (27/0/0) | `tests.rs:3422-3423`, F4 | test (WARP run, `test_warp_mesh.log`) |
| B6 | A single barrier sits between the passes: one **UAV barrier on `res` only** (io/mem need none for this path) — the minimal ordering a legacy chain (scan/recon) must reproduce between compute→scan→recon→draw | `runner.rs:1402` | code read |

## (в) The reference, call by call (`runner::mesh_draw_split`, `runner.rs:1287-1441`)

```
setup (CPU):
  1  CreateComputePipelineState(cs PSO)                 — the translated NGG program,
       root = ABI signature, CS = cs                     runner.rs:1303-1309
  2  build the mesh PSO as a subobject STREAM             runner.rs:1310-1332
       (rs, MS, PS, blend mask ALL, sample mask, rast: SOLID / CullMode_NONE /
        DepthClip, depth: default, RT: R32G32B32A32_FLOAT ×1, 1 sample)
     via ID3D12Device2::CreatePipelineState(stream)
  3  CreateCommittedResource: RT texture size×size R32G32B32A32,
       RENDER_TARGET state, optimized clear value       runner.rs:1334-1356
  4  CreateDescriptorHeap(RTV, 1) + CreateRenderTargetView
                                                          runner.rs:1357-1361
  5  upload buffer (io sgprs + flat mem) Map/Copy/Unmap   runner.rs:1363-1377
  6  default buffers io(u0), mem(u2), res(u1) sized
       io = KERNEL_SGPRS, res = record_dwords * groups    runner.rs:1378-1380
  7  GetCopyableFootprints(RT) + readback buffer
       (RT bytes + res bytes, COPY_DEST)                  runner.rs:1381-1384
  8  CreateCommandAllocator + CreateCommandList(record into the CS PSO)  runner.rs:1386-1387
recorded list:
  9  CopyBufferRegion upload→io, upload→mem               runner.rs:1388-1389
 10  ResourceBarrier: io,mem COPY_DEST→UAV; res COMMON→UAV runner.rs:1390-1394
 11  bind_resources(&[], &[]) — empty shader-visible CBV/SRV/UAV and sampler
       heaps (the heap-indexed SM 6.6 form needs them present)  runner.rs:1396, 188-217
 12  SetComputeRootSignature(ABI rs)                      runner.rs:1397
 13  SetComputeRootUnorderedAccessView 0=io, 1=res, 2=mem runner.rs:1398-1400
 14  Dispatch(groups, 1, 1)          ← pass 1: records into res  runner.rs:1401
 15  ResourceBarrier: UAV barrier on res (nothing else)   runner.rs:1402
 16  SetPipelineState(mesh PSO)                            runner.rs:1404
 17  SetGraphicsRootSignature(ABI rs)                      runner.rs:1405
 18  SetGraphicsRootUnorderedAccessView 0=io, 1=res, 2=mem runner.rs:1406-1408
 19  OMSetRenderTargets(rtv) + ClearRenderTargetView(clear) runner.rs:1409-1410
 20  RSSetViewports(size×size) + RSSetScissorRects(0,0,size,size)  runner.rs:1411-1412
 21  DispatchMesh(groups * split, 1, 1) ← pass 2 (ID3D12GraphicsCommandList6)  runner.rs:1413-1414
 22  ResourceBarrier: rt RT→COPY_SOURCE, res UAV→COPY_SOURCE  runner.rs:1415-1418
 23  CopyTextureRegion rt→readback (placed footprint)
     CopyBufferRegion  res→readback+rt_bytes              runner.rs:1419-1422
 24  Close + ExecuteCommandLists + fence wait             runner.rs:1423-1426
 25  Map(readback): pixels by row (RowPitch stride),
     records = [rt_bytes .. +res_bytes]                   runner.rs:1427-1439
```

Note for the legacy backend: steps 14–21 are the part replaced — `Dispatch` stays,
the UAV barrier stays, and `DispatchMesh` becomes scan-CS → recon-CS →
`ExecuteIndirect(DRAW)` of a plain graphics PSO (VS fetch + the same PS); steps
1–13 and 22–25 are reusable as-is by a golden harness (exactly what G2's
both-paths-on-one-WARP comparison will drive, `single-machine-plan.md` §3).

## Assertions table

| # | Assertion | Source | How verified |
|---|---|---|---|
| 1 | One compute group per record; res sized `record_dwords*groups` | `runner.rs:1368, 1401` | code read |
| 2 | Only `res` gets a UAV barrier between the passes | `runner.rs:1402` | code read |
| 3 | `DispatchMesh(groups*split)`; the compute group count is never scaled | `runner.rs:1414` | code read |
| 4 | The mesh PSO rasterizes with no cull mode, no depth, one RGBA32F RT, 1 sample | `runner.rs:1310-1332` | code read |
| 5 | Split runs re-copy vertices; triangle indices become consecutive per run | `emit.rs:5293-5311` | code read (audit 1.1 C5) |
| 6 | Split and single-group renders are pixel-identical | `tests.rs:3422-3423` | test green on WARP (F4) |
| 7 | The reference's cull-free rasterizer matches the no-culling verdict of 1.3 (б) | `runner.rs:1312` + `emit.rs:5286-5288` | code read |
| 8 | Records return untouched; pixels return row-by-row with RowPitch | `runner.rs:1428-1439` | code read; the 1.1 dump read the same record channel |
