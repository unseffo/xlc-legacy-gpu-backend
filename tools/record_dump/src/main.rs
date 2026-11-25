//! Step 1.1 evidence: dump one real NGG mesh record, dword by dword.
//!
//! Reproduces the synthetic program of `gpu::recomp::tests::gpu_mesh_mode_two_passes`
//! (same instruction table, same MsInterface) through the same code path the test
//! uses — `translate_mesh` → DXC cs_6_6 → `runner::run_in_arena` — on whatever
//! adapter the runner picks (XCL_D3D12_DIR/XCL_ADAPTER apply as usual). The record
//! lands in `u1` and comes back in `KernelResult::raw`; this tool prints it.
//!
//! Output: a compact summary on stdout and the full dwords in `record_dump.txt`.

use xcl::gpu::recomp::{dxc, runner, translate_mesh, Mode, MsInterface};
use xcl::gpu::rdna2::encode;

const V: u64 = 256; // VALU source code of v0

fn sig(name: &str, sv: u32, reg: u32) -> xcl::gpu::dxbc::SigElement {
    xcl::gpu::dxbc::SigElement {
        name: name.into(),
        index: 0,
        system_value: sv,
        component_type: 3,
        register: reg,
        mask: 0xF,
        rw_mask: 0xF,
        stream: 0,
    }
}

fn exp(tgt: u64, en: u64, v: [u64; 4], done: u64) -> (&'static str, &'static str, Vec<(&'static str, u64)>, Option<u32>) {
    ("EXP", "ENC_EXP", vec![("TGT", tgt), ("EN", en), ("VSRC0", v[0]), ("VSRC1", v[1]), ("VSRC2", v[2]), ("VSRC3", v[3]), ("DONE", done)], None)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // MsInterface straight out of gpu_mesh_mode_two_passes (tests.rs:3348)
    let ms = MsInterface {
        outputs: vec![sig("SV_Position", 1, 0), sig("TEXCOORD", 0, 1)],
        threads: 128,
        max_vertices: 128,
        max_primitives: 128,
    };
    // Its instruction table (tests.rs:3352-3376)
    let mut list: Vec<(&str, &str, Vec<(&str, u64)>, Option<u32>)> = vec![
        ("S_BFE_U32", "ENC_SOP2", vec![("SDST", 0), ("SSRC0", 3), ("SSRC1", 255)], Some(0x40018)),
        ("V_MBCNT_LO_U32_B32", "ENC_VOP3", vec![("VDST", 0), ("SRC0", 193), ("SRC1", 128)], None),
        ("V_MBCNT_HI_U32_B32", "ENC_VOP3", vec![("VDST", 0), ("SRC0", 193), ("SRC1", V)], None),
        ("V_LSHL_ADD_U32", "ENC_VOP3", vec![("VDST", 0), ("SRC0", 0), ("SRC1", 134), ("SRC2", V)], None),
        ("V_READFIRSTLANE_B32", "ENC_VOP1", vec![("VDST", 1), ("SRC0", V + 5)], None),
        ("S_LSHR_B32", "ENC_SOP2", vec![("SDST", 1), ("SSRC0", 1), ("SSRC1", 135)], None),
        ("S_MOV_B32", "ENC_SOP1", vec![("SDST", 124), ("SSRC0", 255)], Some(1 << 12 | 3)),
        ("S_SENDMSG", "ENC_SOPP", vec![("SIMM16", 9)], None),
        ("S_BARRIER", "ENC_SOPP", vec![], None),
        ("V_CMPX_GT_U32", "ENC_VOPC", vec![("SRC0", 131), ("VSRC1", 0)], None),
        ("BUFFER_LOAD_DWORDX4", "ENC_MUBUF", vec![("VDATA", 10), ("VADDR", 0), ("SRSRC", 4), ("SOFFSET", 128), ("IDXEN", 1)], None),
        ("V_CVT_F32_U32", "ENC_VOP1", vec![("VDST", 20), ("SRC0", 1)], None),
        ("V_ADD_F32", "ENC_VOP2", vec![("VDST", 10), ("SRC0", V + 20), ("VSRC1", 10)], None),
        ("V_MOV_B32", "ENC_VOP1", vec![("VDST", 14), ("SRC0", V + 20)], None),
        ("V_CVT_F32_U32", "ENC_VOP1", vec![("VDST", 15), ("SRC0", V)], None),
        ("V_MOV_B32", "ENC_VOP1", vec![("VDST", 16), ("SRC0", 128)], None),
        ("V_MOV_B32", "ENC_VOP1", vec![("VDST", 17), ("SRC0", 242)], None),
    ];
    list.push(exp(12, 0xF, [10, 11, 12, 13], 1));
    list.push(exp(32, 0xF, [14, 15, 16, 17], 0));
    list.push(("S_MOV_B64", "ENC_SOP1", vec![("SDST", 126), ("SSRC0", 193)], None));
    list.push(("V_CMPX_EQ_U32", "ENC_VOPC", vec![("SRC0", 128), ("VSRC1", 0)], None));
    list.push(("V_MOV_B32", "ENC_VOP1", vec![("VDST", 21), ("SRC0", 255)], Some(1 << 10 | 2 << 20)));
    list.push(exp(20, 1, [21, 0, 0, 0], 1));

    let mut code = Vec::new();
    for (n, e, f, l) in &list {
        code.extend(encode(n, e, f, *l).map_err(|x| format!("{n}: {x}"))?);
    }
    code.extend(encode("S_ENDPGM", "ENC_SOPP", &[], None)?);

    // Flat memory the way gpu_mesh_mode_reads_memory_windows sets it up: a UMA arena,
    // the vertex table at byte 0x100 of window 0, V# in s[16:19] (base, stride 16, 3).
    let arena = xcl::gpu::uma::Uma::with_windows(1, 1 << 20).ok_or("Uma::with_windows")?;
    let w0 = arena.window(0).0 as u64;
    let word = |a: u64| unsafe { &mut *(a as *mut u32) };
    for (i, (x, y)) in [(-1.0f32, -1.0f32), (-1.0, 1.0), (0.0, -1.0)].iter().enumerate() {
        for (k, v) in [*x, *y, 0.0, 1.0].iter().enumerate() {
            *word(w0 + 0x100 + 16 * i as u64 + 4 * k as u64) = v.to_bits();
        }
    }
    let mut st = runner::KernelState::default();
    st.sgpr[16..20].copy_from_slice(&[0x100, 16 << 16, 3, 0]);

    let split = runner::split_for(&Mode::Mesh(ms.clone()));
    let t = translate_mesh(&code, ms.clone(), true, split, 1)?;
    if !t.complete() {
        return Err(format!("incomplete translation: {:?}", t.unsupported_why).into());
    }
    let cs = dxc::compile(&t.hlsl, "main", "cs_6_6", &[])?;
    let r = runner::run_in_arena(&cs, &st, &arena)?;

    let (vd, rd) = (ms.vert_dwords(), ms.record_dwords());
    let hdr = &r.raw[0..4];
    let vertex = |v: usize| &r.raw[4 + v * vd..4 + v * vd + vd];
    let prim_base = 4 + ms.max_vertices as usize * vd;
    let prims: Vec<u32> = r.raw[prim_base..prim_base + ms.max_primitives as usize].to_vec();
    let live: Vec<usize> = (0..ms.max_primitives as usize).filter(|&j| prims[j] != 0).collect();

    let wave64 = runner::wave64_supported()?;
    println!("adapter: {} (wave64={wave64}, split={split})", runner::adapter()?);
    println!("MsInterface: outputs={} ranked(with_clip)={} threads={} max_vertices={} max_primitives={}", ms.outputs.len(), xcl::gpu::recomp::ranked(&ms.outputs, true).len(), ms.threads, ms.max_vertices, ms.max_primitives);
    println!("vert_dwords = 4*(1+ranked) = {vd};  record_dwords = 4 + max_vertices*vd + max_primitives = {rd}");
    println!("header dwords [0..4]: {:?}", hdr);
    for v in 0..3 {
        let w = vertex(v);
        println!("vertex {v}: pos={:?} params={:?}", [f32::from_bits(w[0]), f32::from_bits(w[1]), f32::from_bits(w[2]), f32::from_bits(w[3])], &w[4..vd]);
    }
    let prim_str: Vec<String> = live.iter().map(|&j| format!("[{j}] {:#010x} ({},{},{})", prims[j], prims[j] & 0x3FF, (prims[j] >> 10) & 0x3FF, (prims[j] >> 20) & 0x3FF)).collect();
    println!("primitive dwords live: {}", prim_str.join(" "));

    let mut out = String::new();
    out.push_str(&format!("record dwords ({rd}), one group, dword per line:\n"));
    for (i, d) in r.raw[..rd].iter().enumerate() {
        out.push_str(&format!("{i:5} {d:#010x} {d:10}\n"));
    }
    std::fs::write("record_dump.txt", out)?;
    println!("full dump written to record_dump.txt");
    Ok(())
}
