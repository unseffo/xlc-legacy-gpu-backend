//! Step 1.4 evidence: aggregate NGG-program statistics from a retail shader pack.
//!
//! Read-only: scans the pack, inflates only the mesh records and reads their
//! declared interfaces. Prints AGGREGATES ONLY - counts, maxima, histograms;
//! no record names, no file names, no game data (the pack path stays in argv
//! and is never printed).
//!
//! Usage: rpf_survey <path-to-shaders.rpf>

use std::collections::BTreeMap;

use xcl::gpu::pack;
use xcl::gpu::recomp::{interface, ranked, Mode, MsInterface};

/// The split formula of audit 1.1 §C, reimplemented here (emit is private to xcl):
/// cap = 32768 / (16*(1+elems)) clipped to 256 and rounded down to 32-vertex chunks;
/// max_vertices <= cap -> 1; else ppg = max(cap/3, 1) triangles a run,
/// split = ceil(max_primitives / ppg).
fn split_estimate(max_vertices: u32, max_primitives: u32, elems: u32) -> u32 {
    let vbytes = 16 * (1 + elems);
    let cap = (32768 / vbytes).min(256) / 32 * 32;
    if max_vertices <= cap {
        return 1;
    }
    let ppg = (cap / 3).max(1);
    max_primitives.max(1).div_ceil(ppg)
}

fn bucket(v: u32, edges: &[u32]) -> usize {
    edges.iter().position(|&e| v <= e).unwrap_or(edges.len())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: rpf_survey <shaders.rpf>")?;
    let data = std::fs::read(&path)?;
    let records = pack::scan(&data, 0x40);
    println!("pack records: {}", records.len());

    // stage histogram by the container's own declared stage (inflating everything;
    // no names stored or printed - the pack's shader names are game data)
    let t0 = std::time::Instant::now();
    let mut stages: BTreeMap<String, usize> = BTreeMap::new();
    let mut parse_fail = 0usize;
    let mut ms_records = 0usize;
    for r in &records {
        let stage = match pack::inflate(&data, r).as_deref().and_then(xcl::gpu::xbox::parse) {
            Some(sh) => sh.stage,
            None => {
                parse_fail += 1;
                continue;
            }
        };
        if stage == "MS" {
            ms_records += 1;
        }
        *stages.entry(stage).or_default() += 1;
    }
    println!("stage histogram by container stage (all records inflated in {:.1}s, parse failures: {parse_fail}): {:?}", t0.elapsed().as_secs_f64(), stages);
    println!("mesh records: {ms_records}");

    // interfaces of the mesh records (the metrics above already inflated them once;
    // this second pass is small next to the stage sweep)
    let t0 = std::time::Instant::now();
    let mut iface_fail = 0usize;
    let mut not_mesh = 0usize;
    let mut mv = Vec::new();
    let mut mp = Vec::new();
    let mut slots = Vec::new();
    let mut rd = Vec::new();
    let mut splits: BTreeMap<u32, usize> = BTreeMap::new();
    let mut threads = Vec::new();
    let mut ib_bytes_per_record = Vec::new();
    for r in records.iter().filter(|r| r.name.starts_with("MS")) {
        let Some(blob) = pack::inflate(&data, r) else {
            iface_fail += 1;
            continue;
        };
        let Some(sh) = xcl::gpu::xbox::parse(&blob) else {
            iface_fail += 1;
            continue;
        };
        let f = sh.fssm.iter().find(|f| f.stages.first().is_some_and(|s| s.hw == 7));
        let Some(f) = f else {
            not_mesh += 1;
            continue;
        };
        let ms = match interface(&blob, &sh, f) {
            Some(Mode::Mesh(m)) => m,
            _ => {
                iface_fail += 1;
                continue;
            }
        };
        let MsInterface { max_vertices, max_primitives, threads: th, .. } = &ms;
        let s = ranked(&ms.outputs, true).len() as u32;
        let split = split_estimate(*max_vertices, *max_primitives, s);
        mv.push(*max_vertices);
        mp.push(*max_primitives);
        slots.push(s);
        rd.push(ms.record_dwords() as u32);
        threads.push(*th);
        *splits.entry(split).or_default() += 1;
        ib_bytes_per_record.push(3 * *max_primitives as u64 * 4);
    }
    println!("mesh interfaces read in {:.1}s (parse failures: {iface_fail}, no NGG hw variant: {not_mesh})", t0.elapsed().as_secs_f64());
    let n = mv.len();
    println!("NGG (MS) programs with an interface: {n}");

    let stats = |name: &str, v: &[u32]| {
        if v.is_empty() {
            return;
        }
        let mut s = v.to_vec();
        s.sort_unstable();
        let at = |f: f64| s[((s.len() - 1) as f64 * f) as usize];
        println!(
            "{name}: min {} p50 {} p90 {} p99 {} max {}",
            s[0], at(0.50), at(0.90), at(0.99), s[s.len() - 1]
        );
    };
    stats("max_vertices", &mv);
    stats("max_primitives", &mp);
    stats("output slots (ranked incl. clip/cull)", &slots);
    stats("record_dwords", &rd);
    stats("numthreads", &threads);

    // histograms
    let he = [16u32, 32, 64, 96, 128, 192, 256];
    let mut h: BTreeMap<usize, usize> = BTreeMap::new();
    for v in &mv {
        *h.entry(bucket(*v, &he)).or_default() += 1;
    }
    let names = ["<=16", "17-32", "33-64", "65-96", "97-128", "129-192", "193-256", ">256"];
    println!("max_vertices histogram: {}", h.iter().map(|(k, c)| format!("{}:{}", names[*k.min(&he.len())], c)).collect::<Vec<_>>().join(" "));
    let mut h: BTreeMap<usize, usize> = BTreeMap::new();
    for v in &mp {
        *h.entry(bucket(*v, &[16, 32, 64, 128, 256])).or_default() += 1;
    }
    let names = ["<=16", "17-32", "33-64", "65-128", "129-256", ">256"];
    println!("max_primitives histogram: {}", h.iter().map(|(k, c)| format!("{}:{}", names[*k.min(&5)], c)).collect::<Vec<_>>().join(" "));
    let mut h: BTreeMap<usize, usize> = BTreeMap::new();
    for v in &slots {
        *h.entry(bucket(*v, &[1, 2, 4, 8, 16])).or_default() += 1;
    }
    let names = ["1", "2", "3-4", "5-8", "9-16", ">16"];
    println!("output slots histogram: {}", h.iter().map(|(k, c)| format!("{}:{}", names[*k.min(&5)], c)).collect::<Vec<_>>().join(" "));

    // split share (upper-bound estimate: with a real PS the element count can only
    // shrink, so a split found here stays; a split missed here can appear with a PS
    // only via clip/cull additions, which the record already carries)
    let need_split: usize = splits.iter().filter(|(&k, _)| k > 1).map(|(_, &v)| v).sum();
    println!("passthrough split distribution (no PS bound): {:?}", splits);
    let pct = |x: usize| format!("{:.1}%", 100.0 * x as f64 / n.max(1) as f64);
    println!("programs needing a split: {need_split} of {n} ({})", pct(need_split));

    // capacity inputs for 1.5 (per record; groups multiply at runtime)
    let mut ib = ib_bytes_per_record.clone();
    ib.sort_unstable();
    if !ib.is_empty() {
        let at = |f: f64| ib[((ib.len() - 1) as f64 * f) as usize];
        println!("IB bytes per record (u32 indices, 3/mp): min {} p50 {} p90 {} max {}", ib[0], at(0.5), at(0.9), ib[ib.len() - 1]);
        let sum: u64 = ib.iter().sum();
        println!("total IB bytes for one of each record: {sum} ({} KiB)", sum / 1024);
    }
    Ok(())
}
