# Audit 1.3 — NGG outputs, primitive attributes and the PS link (step 1.3, taken by TGT)

How the console NGG program's exports land in the record (audit 1.1's layout), what
the pixel shader expects on the PC side, and what does not exist at all — the inputs
for design decisions D5/D7 (and the B9 degenerate-primitive semantics the owner
asked to resolve here). Claims carry `file:symbol`; **[DUMP]** marks confirmation by
the record dump of step 1.1 (`tools/record_dump`, GTX 1080, test-synthetic replay).

## (a) What the PS expects on its inputs

| # | Claim | Source | Verified |
|---|---|---|---|
| A1 | The translated PS takes a `struct XclIn`: `SV_Position` first, then one field per element of `ranked(ps.inputs, false)` — `a{k} : {semantic}` (non-system inputs, sorted by register, i.e. the PC linkage is by semantic while slots are register order) | `emit.rs:5065-5073` | code read; the PS entry declares them, the body reads `xi.a{slot}[{chan}]` |
| A2 | Interpolation modifier per attribute comes from the console program itself: `v_interp_mov_f32` → `nointerpolation` (flat); `v_interp_p1/p2` with IJ matching an enabled SPI PS input → `""`, `centroid`, `sample`, `noperspective[ …]`; non-float inputs are always `nointerpolation` | `emit.rs:5370-5387` (`ps_attr_modes`), `emit.rs:5069-5072`, comment `emit.rs:5165` | code read |
| A3 | System inputs beyond position: `SV_IsFrontFace` (SPI ENA bit 12) and `SV_Coverage` (bit 14) only; conservative-depth PS declares the position `noperspective centroid` | `emit.rs:5063-5064, 5075-5080` | code read |
| A4 | `v_interp attrN chan` maps to a slot via the console's own PS input table (`ps.attrs`) else by distinct register; a channel is read from the element at that register that declares the channel | `mod.rs:425-429` (`ps_attribute_slot`), `:415-420` | code read; `Err("attribute outside ISG1")` otherwise (`emit.rs:2388`) |
| A5 | The pass-through MS declares **exactly the elements the PS reads** (same semantics, order, types) and links by semantic; outputs the PS does not read are dropped (the 32 KB mesh output limit); PS inputs the program never writes read 0; the PS's clip/cull inputs always ride along from the program's clip/cull output slots | `emit.rs:5213-5239`, `:5254-5256` | code read; the mesh-filter family (incl. `gpu_mesh_mode_two_passes` 16-input split variant) is green on WARP (27/0/0) |

## (б) VERDICT: does the console NGG program itself do backface/clip-culling?

**No culling is performed anywhere in XCL's translation or pass-through.**

- The compute pass only moves the console program's exports into the record
  (`emit.rs:2326-2353` — the whole `Mode::Mesh` export table; there is no cull logic).
- The pass-through filters **only** two things: the bit-31 null marker and
  out-of-range vertex indices — and it does NOT remove such primitives from the
  count (see (д)); real backface/frustum culling happens in the **D3D12 rasterizer
  per the PSO's cull mode**, identically for the mesh path and any legacy
  VS/rasterizer path (`emit.rs:5286-5288` only degrades bad triangles).
- A console program that culls on its own (the NGG "prim shader culling" pattern)
  does it in its own code — XCL sees only its result: killed primitives arrive as
  bit-31 null dwords, or never arrive. **The legacy backend must preserve exactly
  this: bit 31 from the record is data, not a hint.**

Consequence for D5: the mesh path never culls before the rasterizer, so a legacy
`cull_enable = false` default loses nothing; frustum culling (5.1) stays opt-in and
must use stable compaction so the surviving order is unchanged. Backface (5.2) as a
flag is additive — the PSO already backface-culls at raster time in both paths.

## (в) The primitive dword: who packs it, how it is read, vertex order

| # | Claim | Source | Verified |
|---|---|---|---|
| V1 | **The console program packs the dword itself.** In mesh mode the entire dword arrives as `VSRC0` of `EXP TGT 20` and XCL stores it verbatim: `res.Store(xcl_prm * 4u, VSRC0)` — no masking, no repacking, no index bias | `emit.rs:2351` (`(Mode::Mesh(_), 20)`), addresses `emit.rs:4901` | **[DUMP]** — the synthetic's `V_MOV v21 = 1<<10\|2<<20` arrived bit-exact as primitive dword `0x00200400` |
| V2 | Three indices read out as `v0 = p & 0x3FF`, `v1 = (p >> 10) & 0x3FF`, `v2 = (p >> 20) & 0x3FF` (10-10-10, low bits first); bit 31 is the null marker | `emit.rs:5286`, `5305` | **[DUMP]** — `0x00200400` → (0, 1, 2) |
| V3 | Vertex order is whatever bit order the console program wrote (V1/V2): XCL never reorders a mesh-mode primitive. **Provoking vertex = v0** — the first index in bit order — matches the D3D convention and what the PC rasterizer takes for `nointerpolation` inputs | V1+V2, D3D12 flat semantics; `emit.rs:5071` flat inputs | code read — see the D7 caveat below |
| V4 | Contrast — the *legacy GS* path (`Mode::Geometry`) DOES repack: it re-bases indices by the run's vertex base `+xcl_vb` when it copies triangles out of LDS (`emit.rs:4972-4977`). Mesh mode does not; a legacy backend over mesh records must not introduce a bias either | `emit.rs:4975` vs `emit.rs:2351` | code read |

**D7 caveat (to check on 3.5):** the console's flat reads (`v_interp_mov` P0) take the
provoking vertex's value; the PC's `nointerpolation` takes the D3D provoking vertex.
Both are "vertex 0 of the primitive" by the bit order above, so they should agree
bit for bit — the golden of 3.5 (flat attribute over a triangle) is the check that
proves it on real programs.

## (г) Table: output → where it lives in the record → what the PS gets

| Console output | Export | Record slot | PC side |
|---|---|---|---|
| Vertex position | `EXP TGT 12` | `vert[i][0..4]` (dword 0..4 of the vertex) | `XclIn.xcl_pos : SV_Position` (`emit.rs:5065`) — **[DUMP]**: (-1,-1,0,1) arrived verbatim |
| Per-vertex param N (a **register**) | `EXP TGT 32..63` | `vert[i][4*(j+1)..]`, slot j = every OSG1 element at that register | `XclIn.a{k}` with the interpolation modifier (A2); elements the PS drops disappear, unread ones read 0 (A5) — **[DUMP]**: the TEXCOORD register landed at dword 4..8 |
| Clip/cull distance | param of its register (`ranked(…, true)` keeps system_value 2/3) | a normal vertex slot | PS clip/cull inputs; the pass-through always carries them (`emit.rs:5236`) |
| Primitive indices | `EXP TGT 20` | `prim[j]` | the rasterizer (no PS input) — **[DUMP]** |
| TGT 13-15 (misc vector) | ignored (`"misc vector / clip distances (from their params)"`) | nowhere | — (`emit.rs:2333`) |
| TGT 9 | ignored | nowhere | — (`emit.rs:2352`) |
| Anything else | `Err("export target")` → translation incomplete → the pipeline is refused | — | — (`emit.rs:2353`) |

**What does NOT exist:**

- **Per-primitive outputs / prim attributes: not supported, not present in the
  record.** The export table has no primitive-attribute target (only TGT 20's index
  dword); `record_dwords` (mod.rs:176-178) has no per-primitive payload beyond the
  index dword. A console program exporting them would fail the table and be refused
  — and the retail corpus carries none (the retail `gpu_pso_e2e`/`retail_vs_ps`
  tests are green, 0.6/F8). No `prim_src[(rec,slot)]` buffer is needed for the
  legacy backend.
- **`SV_PrimitiveID`: not supported anywhere in the mesh path.** The PS input struct
  has only position, `a{k}`, IsFrontFace and Coverage (`emit.rs:5065-5080`); the SPI
  ENA bits have no primitive-id slot (`mod.rs:448-451`, `PS_INPUT_SIZES`); the name
  appears only in the direct-DXIL semantic table (`gpu/dxil/graphics.rs:32`,
  `SemKind::PrimitiveId`) — the HLSL path never reads it. If a retail PS required
  it, translation would fail (`attribute outside ISG1`, `emit.rs:2388`) — none does
  so far (F8). The legacy backend inherits this for free: it feeds the same PS.
- **Flat interpolation exists and is carried correctly**: `v_interp_mov_f32` →
  `nointerpolation` (A2), non-float inputs are always flat — so integer attributes
  are provoking-vertex values on both consoles and PC.

## (д) Degenerate/null primitives — B9 resolved

`np = min(res[1], max_primitives)` counts **every** dword the program exported,
including bit-31 nulls and out-of-range indices (`emit.rs:5278, 5300-5301`). The
pass-through keeps them in the count and in their index slots:

- single group: `bad = (p >> 31) != 0 || any(idx >= nv)` → `tris[j] = (0,0,0)` — a
  degenerate triangle that rasterizes to nothing (`emit.rs:5286-5288`);
- split runs: the same `bad`, with the three vertex copies taken from vertex 0 when
  bad (`select(bad, 0u, x[c])`, `emit.rs:5306-5308`).

So the mesh path's observable semantics: **null/bad primitives occupy their slot,
keep the primitive order and the count, and draw nothing.** The legacy backend should
do the same — write them into the IB as degenerate (0,0,0) triangles rather than
compacting them away — for two reasons: (1) G1's bitwise golden against
`mesh_draw_split` requires the same index count and order; (2) compacting would
change `SV_PrimitiveID`-adjacent ordering semantics if it ever mattered. Final
call belongs to 1.5/D2; recorded as the recommended default.

## Verdicts (recorded in PROGRESS.md «Решения»)

- **D5 confirmed with teeth:** no culling in translation; PSO/rasterizer culling is
  shared by both paths; the console's bit-31 null is *data* the legacy chain must
  preserve; frustum cull stays opt-in behind stable compaction; backface as a flag
  only.
- **D7 confirmed with two "not supported" marks:** provoking vertex = v0 of the
  bit order; flat via `nointerpolation` (golden check on 3.5); per-primitive
  outputs and `SV_PrimitiveID` do not exist in the mesh path (and the legacy chain
  feeds the same PS, so it inherits the limits); clip/cull ride as normal vertex
  slots.
