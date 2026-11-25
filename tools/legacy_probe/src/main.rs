//! legacy_probe — TGT (GTX 1080) D3D12 capability probe: inbox runtime vs Agility SDK.
//!
//! Step 0.2a of AGENT_PLAYBOOK.md. One run = one process: the runtime is chosen once at
//! startup from `XCL_D3D12_DIR` (set by the caller for the Agility run, unset for inbox),
//! and the actually loaded `D3D12Core.dll` is verified afterwards: in Agility mode a
//! system DLL is an ERROR (exit code 2), not a result.
//!
//! Unlike XCL's loader (crates/xcl/src/gpu/agility.rs), which also searches the exe
//! directory and %LOCALAPPDATA%, this probe deliberately honours `XCL_D3D12_DIR` only,
//! so the inbox run cannot pick up a stray local SDK copy.
//!
//! Output: JSON report to argv[1] (default `out_tgt.json`) + the same facts on stdout.
//! Exit codes: 0 = complete; 1 = fatal (no device / init failure); 2 = Agility requested
//! but the system D3D12Core.dll was loaded.

use std::ffi::c_void;
use std::ffi::CString;
use std::path::Path;
use std::path::PathBuf;

use windows::core::GUID;
use windows::core::HRESULT;
use windows::core::Interface;
use windows::core::PCSTR;
use windows::core::PCWSTR;
use windows::Win32::Foundation::E_INVALIDARG;
use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_12_0;
use windows::Win32::Graphics::Direct3D::ID3DBlob;
use windows::Win32::Graphics::Direct3D::Dxc::DxcBuffer;
use windows::Win32::Graphics::Direct3D::Dxc::DxcValidatorFlags_InPlaceEdit;
use windows::Win32::Graphics::Direct3D::Dxc::DXC_CP_UTF8;
use windows::Win32::Graphics::Direct3D::Dxc::CLSID_DxcCompiler;
use windows::Win32::Graphics::Direct3D::Dxc::CLSID_DxcUtils;
use windows::Win32::Graphics::Direct3D::Dxc::CLSID_DxcValidator;
use windows::Win32::Graphics::Direct3D::Dxc::IDxcBlob;
use windows::Win32::Graphics::Direct3D::Dxc::IDxcCompiler3;
use windows::Win32::Graphics::Direct3D::Dxc::IDxcOperationResult;
use windows::Win32::Graphics::Direct3D::Dxc::IDxcResult;
use windows::Win32::Graphics::Direct3D::Dxc::IDxcUtils;
use windows::Win32::Graphics::Direct3D::Dxc::IDxcValidator;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::LibraryLoader::GetProcAddress;
use windows::Win32::System::LibraryLoader::LoadLibraryExW;
use windows::Win32::System::LibraryLoader::LoadLibraryW;
use windows::Win32::System::LibraryLoader::LOAD_LIBRARY_SEARCH_SYSTEM32;
use windows::Win32::System::Registry::RegCloseKey;
use windows::Win32::System::Registry::RegOpenKeyExW;
use windows::Win32::System::Registry::RegQueryValueExW;
use windows::Win32::System::Registry::HKEY_LOCAL_MACHINE;
use windows::Win32::System::Registry::KEY_READ;

const ENV_D3D12: &str = "XCL_D3D12_DIR";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

fn hr_string(hr: HRESULT) -> String {
    let name = if hr == E_INVALIDARG {
        " E_INVALIDARG"
    } else if hr.0 == 0 {
        " S_OK"
    } else {
        ""
    };
    format!("{:#010x}{}", hr.0 as u32, name)
}

// ---------- Windows version / driver version (registry, in-proc) ----------

unsafe fn reg_open(subkey: &str, out: &mut windows::Win32::System::Registry::HKEY) -> bool {
    let sk = wide(subkey);
    RegOpenKeyExW(HKEY_LOCAL_MACHINE, PCWSTR(sk.as_ptr()), None, KEY_READ, out).0 == 0
}

unsafe fn reg_value_raw(hk: windows::Win32::System::Registry::HKEY, value: &str) -> Option<Vec<u8>> {
    let name = wide(value);
    let mut size = 0u32;
    if RegQueryValueExW(hk, PCWSTR(name.as_ptr()), None, None, None, Some(&mut size)).0 != 0 || size == 0 {
        return None;
    }
    let mut data = vec![0u8; size as usize];
    if RegQueryValueExW(hk, PCWSTR(name.as_ptr()), None, None, Some(data.as_mut_ptr()), Some(&mut size)).0 != 0 {
        return None;
    }
    Some(data)
}

fn reg_string(subkey: &str, value: &str) -> Option<String> {
    unsafe {
        let mut hk = std::mem::zeroed();
        if !reg_open(subkey, &mut hk) {
            return None;
        }
        let raw = reg_value_raw(hk, value);
        let _ = RegCloseKey(hk);
        let raw = raw?;
        let units: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        Some(String::from_utf16_lossy(&units).trim_end_matches('\0').trim().to_string())
    }
}

fn reg_dword(subkey: &str, value: &str) -> Option<u32> {
    unsafe {
        let mut hk = std::mem::zeroed();
        if !reg_open(subkey, &mut hk) {
            return None;
        }
        let raw = reg_value_raw(hk, value);
        let _ = RegCloseKey(hk);
        let raw = raw?;
        raw.get(0..4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

#[derive(serde::Serialize)]
struct WindowsInfo {
    product_name: String,
    display_version: String,
    build: String,
    ubr: Option<u32>,
}

fn windows_info() -> WindowsInfo {
    const CV: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    WindowsInfo {
        product_name: reg_string(CV, "ProductName").unwrap_or_default(),
        display_version: reg_string(CV, "DisplayVersion").unwrap_or_default(),
        build: reg_string(CV, "CurrentBuild").unwrap_or_default(),
        ubr: reg_dword(CV, "UBR"),
    }
}

/// Driver registry entries of the display adapter class that match a name.
fn driver_versions(adapter_desc: &str) -> Vec<(String, String)> {
    const CLASS: &str = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
    let want = adapter_desc.trim().to_lowercase();
    let mut out = Vec::new();
    for k in 0..100u32 {
        let sub = format!("{CLASS}\\{k:04}");
        let Some(desc) = reg_string(&sub, "DriverDesc") else { continue };
        if desc.trim().to_lowercase() != want {
            continue;
        }
        let ver = reg_string(&sub, "DriverVersion").unwrap_or_else(|| "n/a".into());
        out.push((desc, ver));
    }
    out
}

// ---------- file version resource ----------

fn file_version(path: &Path) -> Option<String> {
    use windows::Win32::Storage::FileSystem::GetFileVersionInfoSizeW;
    use windows::Win32::Storage::FileSystem::GetFileVersionInfoW;
    use windows::Win32::Storage::FileSystem::VerQueryValueW;
    unsafe {
        let name = wide(&path.to_string_lossy());
        let n = GetFileVersionInfoSizeW(PCWSTR(name.as_ptr()), None);
        if n == 0 {
            return None;
        }
        let mut data = vec![0u8; n as usize];
        GetFileVersionInfoW(PCWSTR(name.as_ptr()), None, n, data.as_mut_ptr() as *mut c_void).ok()?;
        let mut p: *mut c_void = std::ptr::null_mut();
        let mut len = 0u32;
        if VerQueryValueW(data.as_ptr() as *const c_void, PCWSTR(wide("\\").as_ptr()), &mut p, &mut len).0 == 0 || p.is_null() || len < 16 {
            return None;
        }
        let q = p as *const u32;
        let ms = std::ptr::read_unaligned(q.add(2));
        let ls = std::ptr::read_unaligned(q.add(3));
        Some(format!("{}.{}.{}.{}", ms >> 16, ms & 0xFFFF, ls >> 16, ls & 0xFFFF))
    }
}

// ---------- minimal PE export reader: D3D12SDKVersion from a D3D12Core.dll on disk ----------

/// The `D3D12SDKVersion` u32 export of a PE DLL, read from file bytes without loading it.
fn read_sdk_version_export(dll: &Path) -> Result<u32, String> {
    let tag = format!("{}", dll.display());
    let b = std::fs::read(dll).map_err(|e| format!("{tag}: {e}"))?;
    let u16_at = |o: usize| b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]));
    let u32_at = |o: usize| b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]));
    if b.get(0..2) != Some(&[b'M', b'Z']) {
        return Err(format!("{tag}: not an MZ image"));
    }
    let pe_off = u32_at(0x3C).ok_or_else(|| format!("{tag}: no PE offset"))? as usize;
    if b.get(pe_off..pe_off + 4) != Some(&[b'P', b'E', 0, 0]) {
        return Err(format!("{tag}: no PE signature"));
    }
    let n_sections = u16_at(pe_off + 6).ok_or_else(|| format!("{tag}: short file header"))? as usize;
    let opt_size = u16_at(pe_off + 20).ok_or_else(|| format!("{tag}: short file header"))? as usize;
    let opt_off = pe_off + 24;
    if u16_at(opt_off) != Some(0x20B) {
        return Err(format!("{tag}: not PE32+"));
    }
    // PE32+ data directories start at optional header + 112; export table is index 0.
    let dd = opt_off + 112;
    let exp_rva = u32_at(dd).ok_or_else(|| format!("{tag}: short data directory"))?;
    if exp_rva == 0 {
        return Err(format!("{tag}: no export table"));
    }
    let sec_off = opt_off + opt_size;
    let rva_to_off = |rva: u32| -> Option<usize> {
        for s in 0..n_sections {
            let h = sec_off + s * 40;
            let v_size = u32_at(h + 8)? as u64;
            let v_addr = u32_at(h + 12)? as u64;
            let raw_size = u32_at(h + 16)? as u64;
            let raw_off = u32_at(h + 20)? as u64;
            let r = rva as u64;
            if r >= v_addr && r < v_addr + v_size.max(raw_size) {
                let off = (raw_off + (r - v_addr)) as usize;
                return (off < b.len()).then_some(off);
            }
        }
        None
    };
    let e = rva_to_off(exp_rva).ok_or_else(|| format!("{tag}: export dir outside sections"))?;
    let n_funcs = u32_at(e + 20).ok_or_else(|| format!("{tag}: short export dir"))? as usize;
    let n_names = u32_at(e + 24).ok_or_else(|| format!("{tag}: short export dir"))? as usize;
    let funcs_off = rva_to_off(u32_at(e + 28).ok_or_else(|| format!("{tag}: short export dir"))?).ok_or_else(|| format!("{tag}: functions outside sections"))?;
    let names_off = rva_to_off(u32_at(e + 32).ok_or_else(|| format!("{tag}: short export dir"))?).ok_or_else(|| format!("{tag}: names outside sections"))?;
    let ords_off = rva_to_off(u32_at(e + 36).ok_or_else(|| format!("{tag}: short export dir"))?).ok_or_else(|| format!("{tag}: ordinals outside sections"))?;
    for i in 0..n_names {
        let name_rva = u32_at(names_off + i * 4).ok_or_else(|| format!("{tag}: short names array"))?;
        let name_off = rva_to_off(name_rva).ok_or_else(|| format!("{tag}: name outside sections"))?;
        let end = b[name_off..].iter().position(|&c| c == 0).map(|p| name_off + p).ok_or_else(|| format!("{tag}: unterminated export name"))?;
        if &b[name_off..end] == b"D3D12SDKVersion" {
            let ord = u16_at(ords_off + i * 2).ok_or_else(|| format!("{tag}: short ordinals array"))? as usize;
            if ord >= n_funcs {
                return Err(format!("{tag}: export ordinal out of range"));
            }
            let frva = u32_at(funcs_off + ord * 4).ok_or_else(|| format!("{tag}: short functions array"))?;
            let foff = rva_to_off(frva).ok_or_else(|| format!("{tag}: D3D12SDKVersion outside sections"))?;
            return u32_at(foff).filter(|&v| v != 0).ok_or_else(|| format!("{tag}: D3D12SDKVersion invalid"));
        }
    }
    Err(format!("{tag}: D3D12SDKVersion not exported"))
}

// ---------- Agility SDK (XCL_D3D12_DIR only; mirrors crates/xcl/src/gpu/agility.rs) ----------

type CreateInstanceFn = unsafe extern "system" fn(*const GUID, *const GUID, *mut *mut c_void) -> HRESULT;

unsafe fn d3d12_get_interface() -> Result<CreateInstanceFn, String> {
    let w = wide("d3d12.dll");
    let h = LoadLibraryExW(PCWSTR(w.as_ptr()), None, LOAD_LIBRARY_SEARCH_SYSTEM32)
        .map_err(|e| format!("LoadLibraryExW d3d12.dll: {e}"))?;
    let f = GetProcAddress(h, PCSTR(b"D3D12GetInterface\0".as_ptr()));
    if f.is_none() {
        return Err("D3D12GetInterface not exported by d3d12.dll (Windows 10 22H2 KB5029331+)".into());
    }
    Ok(std::mem::transmute::<usize, CreateInstanceFn>(f.unwrap() as usize))
}

/// Activate the Agility SDK from `XCL_D3D12_DIR`; None when the variable is unset (inbox run).
fn init_agility() -> Result<Option<(ID3D12DeviceFactory, PathBuf, u32)>, String> {
    let Some(dir) = std::env::var_os(ENV_D3D12).map(PathBuf::from) else {
        return Ok(None);
    };
    if dir.as_os_str().is_empty() {
        return Ok(None);
    }
    let dll = dir.join("D3D12Core.dll");
    if !dll.is_file() {
        return Err(format!("{ENV_D3D12}={} has no D3D12Core.dll", dir.display()));
    }
    let version = read_sdk_version_export(&dll)?;
    let abs = std::path::absolute(&dir).map_err(|e| format!("{ENV_D3D12}: {e}"))?;
    // ID3D12SDKConfiguration1::CreateDeviceFactory takes an ANSI path with a trailing
    // backslash (XCL encodes it in the active code page; this probe requires ASCII — the
    // repository path is).
    let mut s = abs.to_string_lossy().into_owned();
    if s.ends_with('/') {
        s.pop();
        s.push('\\');
    } else if !s.ends_with('\\') {
        s.push('\\');
    }
    if !s.is_ascii() {
        return Err(format!("{ENV_D3D12} must be ASCII for this probe: {}", abs.display()));
    }
    let path_cstr = CString::new(s.as_bytes()).map_err(|_| format!("{ENV_D3D12}: interior NUL"))?;
    unsafe {
        let get_iface = d3d12_get_interface()?;
        let mut cfg_raw: *mut c_void = std::ptr::null_mut();
        get_iface(&CLSID_D3D12SDKConfiguration, &ID3D12SDKConfiguration1::IID, &mut cfg_raw)
            .ok()
            .map_err(|e| format!("ID3D12SDKConfiguration1 unavailable (Windows 10 22H2 KB5029331+): {e}"))?;
        if cfg_raw.is_null() {
            return Err("D3D12GetInterface returned null ID3D12SDKConfiguration1".into());
        }
        let cfg = ID3D12SDKConfiguration1::from_raw(cfg_raw);
        let factory: ID3D12DeviceFactory = cfg
            .CreateDeviceFactory(version, PCSTR(path_cstr.as_ptr().cast()))
            .map_err(|e| format!("CreateDeviceFactory(v{version}, {}): {e}", abs.display()))?;
        factory
            .SetFlags(D3D12_DEVICE_FACTORY_FLAG_ALLOW_RETURNING_EXISTING_DEVICE)
            .map_err(|e| format!("ID3D12DeviceFactory::SetFlags: {e}"))?;
        factory
            .ApplyToGlobalState()
            .map_err(|e| format!("ID3D12DeviceFactory::ApplyToGlobalState: {e}"))?;
        Ok(Some((factory, abs, version)))
    }
}

// ---------- DXC (dxil.dll + dxcompiler.dll), as XCL loads them ----------

struct DxcToolchain {
    dir: String,
    dxcompiler_version: Option<String>,
    dxil_version: Option<String>,
    create: usize,
    sign: Option<usize>,
}

fn dxc_candidates() -> Vec<PathBuf> {
    // Order matters: the newest DXC first (XCL's dxc.rs does the same — an SDK's
    // dxcompiler.dll carries either a DXC release number (1.7.2308) or its Windows build
    // (10.0.19041 = DXC 1.5); 19041's compiler predates IDxcCompiler3).
    let mut front = Vec::new();
    if let Some(d) = std::env::var_os("XCL_DXC_DIR") {
        front.push(PathBuf::from(d));
    }
    if let Some(d) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
        front.push(d);
    }
    let mut sdks: Vec<(Option<Vec<u32>>, PathBuf)> = Vec::new();
    for root in [r"C:\Program Files (x86)\Windows Kits\10\bin", r"C:\Program Files\Windows Kits\10\bin"] {
        if let Ok(rd) = std::fs::read_dir(root) {
            for e in rd.flatten() {
                let p = e.path().join("x64");
                let compiler = p.join("dxcompiler.dll");
                if !compiler.is_file() {
                    continue;
                }
                let ver: Option<Vec<u32>> = file_version(&compiler)
                    .and_then(|s| s.split('.').map(|x| x.parse::<u32>().ok()).collect());
                sdks.push((ver, p));
            }
        }
    }
    // A DXC release starts below 10 (1.7.2308); a file version that starts with 10 is the
    // Windows build number of an old SDK's compiler (10.0.19041 = DXC 1.5) — those and
    // unknown versions come last (XCL dxc.rs::dxc_release does the same).
    let release = |v: &Option<Vec<u32>>| match v {
        Some(v) if !v.is_empty() && v[0] < 10 => v.clone(),
        _ => vec![0, 0, 0, 0],
    };
    sdks.sort_by(|a, b| release(&b.0).cmp(&release(&a.0)));
    let mut v = front;
    v.extend(sdks.into_iter().map(|(_, p)| p));
    v
}

fn load_dxc() -> Result<DxcToolchain, String> {
    for dir in dxc_candidates() {
        let compiler = dir.join("dxcompiler.dll");
        if !compiler.is_file() {
            continue;
        }
        let dxil_path = dir.join("dxil.dll");
        unsafe {
            // dxil.dll first (full path), as XCL does, so DXC can sign the DXIL it builds.
            let sign_handle = if dxil_path.is_file() {
                LoadLibraryW(PCWSTR(wide(&dxil_path.to_string_lossy()).as_ptr())).ok()
            } else {
                None
            };
            let Ok(h) = LoadLibraryW(PCWSTR(wide(&compiler.to_string_lossy()).as_ptr())) else { continue };
            let Some(create) = GetProcAddress(h, PCSTR(b"DxcCreateInstance\0".as_ptr())) else { continue };
            let sign = sign_handle.and_then(|sh| GetProcAddress(sh, PCSTR(b"DxcCreateInstance\0".as_ptr())));
            return Ok(DxcToolchain {
                dir: dir.display().to_string(),
                dxcompiler_version: file_version(&compiler),
                dxil_version: if dxil_path.is_file() { file_version(&dxil_path) } else { None },
                create: create as usize,
                sign: sign.map(|f| f as usize),
            });
        }
    }
    Err("dxcompiler.dll not found (XCL_DXC_DIR / probe exe dir / Windows SDK)".into())
}

unsafe fn dxc_compile(create: usize, src: &str, profile: &str) -> Result<Vec<u8>, String> {
    let create: CreateInstanceFn = std::mem::transmute::<usize, _>(create);
    let mut p = std::ptr::null_mut();
    create(&CLSID_DxcCompiler, &IDxcCompiler3::IID, &mut p)
        .ok()
        .map_err(|e| format!("DxcCreateInstance IDxcCompiler3: {e}"))?;
    let c = IDxcCompiler3::from_raw(p);
    let args: Vec<Vec<u16>> = ["-E", "main", "-T", profile, "-HV", "2021"].iter().map(|a| wide(a)).collect();
    let ptrs: Vec<PCWSTR> = args.iter().map(|a| PCWSTR(a.as_ptr())).collect();
    let buf = DxcBuffer { Ptr: src.as_ptr() as *const c_void, Size: src.len(), Encoding: DXC_CP_UTF8.0 };
    let r: IDxcResult = c.Compile(&buf, Some(&ptrs), None).map_err(|e| format!("Compile: {e}"))?;
    let status = r.GetStatus().map_err(|e| format!("GetStatus: {e}"))?;
    if status.is_err() {
        let msg = r
            .GetErrorBuffer()
            .ok()
            .map(|b| {
                let s = std::slice::from_raw_parts(b.GetBufferPointer() as *const u8, b.GetBufferSize());
                String::from_utf8_lossy(s).trim_end_matches('\0').to_string()
            })
            .unwrap_or_default();
        return Err(msg);
    }
    let blob: IDxcBlob = r.GetResult().map_err(|e| format!("GetResult: {e}"))?;
    Ok(std::slice::from_raw_parts(blob.GetBufferPointer() as *const u8, blob.GetBufferSize()).to_vec())
}

/// Validate (and sign) DXIL through `dxil.dll`'s `IDxcValidator`. `create` is
/// dxcompiler.dll's DxcCreateInstance (IDxcUtils lives there, XCL assemble.rs does the
/// same); `sign` is dxil.dll's.
unsafe fn dxc_validate(create: usize, sign: usize, dxil: &[u8]) -> Result<(), String> {
    let create: CreateInstanceFn = std::mem::transmute::<usize, _>(create);
    let sign: CreateInstanceFn = std::mem::transmute::<usize, _>(sign);
    let mut q = std::ptr::null_mut();
    create(&CLSID_DxcUtils, &IDxcUtils::IID, &mut q).ok().map_err(|e| format!("DxcCreateInstance IDxcUtils: {e}"))?;
    let utils = IDxcUtils::from_raw(q);
    let blob = utils
        .CreateBlob(dxil.as_ptr() as *const c_void, dxil.len() as u32, DXC_CP_UTF8)
        .map_err(|e| format!("CreateBlob: {e}"))?;
    let blob: IDxcBlob = blob.cast::<IDxcBlob>().map_err(|e| format!("cast IDxcBlob: {e}"))?;
    let mut v_raw = std::ptr::null_mut();
    sign(&CLSID_DxcValidator, &IDxcValidator::IID, &mut v_raw)
        .ok()
        .map_err(|e| format!("DxcCreateInstance IDxcValidator: {e}"))?;
    let v = IDxcValidator::from_raw(v_raw);
    let r: IDxcOperationResult = v.Validate(&blob, DxcValidatorFlags_InPlaceEdit).map_err(|e| format!("Validate: {e}"))?;
    let status = r.GetStatus().map_err(|e| format!("GetStatus: {e}"))?;
    if status.is_err() {
        let msg = r
            .GetErrorBuffer()
            .ok()
            .map(|b| {
                let s = std::slice::from_raw_parts(b.GetBufferPointer() as *const u8, b.GetBufferSize());
                String::from_utf8_lossy(s).trim_end_matches('\0').to_string()
            })
            .unwrap_or_default();
        return Err(msg);
    }
    Ok(())
}

// ---------- reports ----------

#[derive(serde::Serialize)]
struct AdapterInfo {
    index: u32,
    description: String,
    dedicated_video_memory: u64,
    shared_system_memory: u64,
    adapter_luid: String,
}

#[derive(serde::Serialize)]
struct RuntimeInfo {
    mode: String,
    agility_dir: Option<String>,
    agility_requested_sdk_version: Option<u32>,
    d3d12core_path: Option<String>,
    d3d12core_file_version: Option<String>,
    d3d12core_sdk_version_export: Option<u32>,
    /// Some(false) in Agility mode = the system D3D12Core.dll was loaded (probe error).
    loaded_matches_request: Option<bool>,
}

#[derive(serde::Serialize)]
struct Features {
    mesh_shader_tier: i32,
    sampler_feedback_tier: i32,
    wave_lane_count_min: u32,
    wave_lane_count_max: u32,
    /// which SM the first successful CheckFeatureSupport call asked for
    shader_model_asked: String,
    /// what the runtime reported in HighestShaderModel
    highest_shader_model: String,
    resource_binding_tier: i32,
    tiled_resources_tier: i32,
    view_instancing_tier: i32,
}

#[derive(serde::Serialize, Clone)]
struct ShaderProbe {
    compile: String,
    validate: String,
    pso: String,
}

#[derive(serde::Serialize)]
struct G0Report {
    root_signature: String,
    sm66: ShaderProbe,
    sm60_wave: ShaderProbe,
}

#[derive(serde::Serialize)]
struct Report {
    windows: WindowsInfo,
    gpu_name: Option<String>,
    gpu_luid: Option<String>,
    gpu_driver_candidates: Vec<(String, String)>,
    adapters: Vec<AdapterInfo>,
    runtime: RuntimeInfo,
    dxc: Option<serde_json::Value>,
    features: Option<Features>,
    g0: Option<G0Report>,
    fatal: Option<String>,
    notes: Vec<String>,
}

fn main() {
    let out_path = std::env::args().nth(1).unwrap_or_else(|| "out_tgt.json".into());
    let mut report = Report {
        windows: windows_info(),
        gpu_name: None,
        gpu_luid: None,
        gpu_driver_candidates: Vec::new(),
        adapters: Vec::new(),
        runtime: RuntimeInfo {
            mode: "inbox".into(),
            agility_dir: None,
            agility_requested_sdk_version: None,
            d3d12core_path: None,
            d3d12core_file_version: None,
            d3d12core_sdk_version_export: None,
            loaded_matches_request: None,
        },
        dxc: None,
        features: None,
        g0: None,
        fatal: None,
        notes: Vec::new(),
    };
    let mut exit_code = 0;

    // DXC first: it is independent of the D3D12 runtime choice.
    let dxc = match load_dxc() {
        Ok(t) => {
            report.dxc = Some(serde_json::json!({
                "dir": t.dir,
                "dxcompiler_file_version": t.dxcompiler_version,
                "dxil_file_version": t.dxil_version,
            }));
            println!("DXC: dxcompiler {}, dxil {}, from {}", t.dxcompiler_version.clone().unwrap_or_default(), t.dxil_version.clone().unwrap_or_default(), t.dir);
            Some(t)
        }
        Err(e) => {
            report.dxc = Some(serde_json::json!({ "found": false, "error": e }));
            println!("DXC: NOT FOUND ({e})");
            None
        }
    };

    // Runtime choice: XCL_D3D12_DIR set → Agility; unset → inbox.
    let agility = match init_agility() {
        Ok(a) => a,
        Err(e) => {
            report.fatal = Some(format!("agility init: {e}"));
            report.runtime.mode = "agility".into();
            report.runtime.agility_dir = std::env::var_os(ENV_D3D12).map(|v| v.to_string_lossy().into_owned());
            finish(&out_path, &report, 1);
            return;
        }
    };
    let factory = agility.as_ref().map(|(f, dir, version)| {
        report.runtime.mode = "agility".into();
        report.runtime.agility_dir = Some(dir.display().to_string());
        report.runtime.agility_requested_sdk_version = Some(*version);
        f.clone()
    });

    // Adapter enumeration + device creation on the first adapter that works.
    let mut device: Option<ID3D12Device> = None;
    let mut gpu_desc: Option<(String, String)> = None;
    unsafe {
        let factory_dxgi: IDXGIFactory1 = match CreateDXGIFactory1() {
            Ok(f) => f,
            Err(e) => {
                report.fatal = Some(format!("CreateDXGIFactory1: {e}"));
                finish(&out_path, &report, 1);
                return;
            }
        };
        for i in 0.. {
            let Ok(adapter) = factory_dxgi.EnumAdapters1(i) else { break };
            let Ok(desc) = adapter.GetDesc1() else { continue };
            let name = String::from_utf16_lossy(&desc.Description).split('\0').next().unwrap_or("").to_string();
            report.adapters.push(AdapterInfo {
                index: i,
                description: name.clone(),
                dedicated_video_memory: desc.DedicatedVideoMemory as u64,
                shared_system_memory: desc.SharedSystemMemory as u64,
                adapter_luid: format!("{:08X}:{:08X}", desc.AdapterLuid.HighPart, desc.AdapterLuid.LowPart as u32),
            });
            let mut dev: Option<ID3D12Device> = None;
            let created = match &factory {
                Some(f) => f.CreateDevice(&adapter, D3D_FEATURE_LEVEL_12_0, &mut dev),
                None => D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_12_0, &mut dev),
            };
            if created.is_ok() && dev.is_some() {
                device = dev;
                gpu_desc = Some((name.clone(), report.adapters.last().unwrap().adapter_luid.clone()));
                println!("Adapter {i}: {name} — D3D12 device created ({})", if factory.is_some() { "Agility factory" } else { "inbox D3D12CreateDevice" });
                break;
            }
        }
    }
    let Some(dev) = device else {
        report.fatal = Some("no adapter could create a D3D12 device".into());
        finish(&out_path, &report, 1);
        return;
    };
    let (gpu_name, gpu_luid) = gpu_desc.unwrap();
    report.gpu_name = Some(gpu_name.clone());
    report.gpu_luid = Some(gpu_luid.clone());
    report.gpu_driver_candidates = driver_versions(&gpu_name);
    if let Some((_, v)) = report.gpu_driver_candidates.first() {
        println!("Driver (registry, DriverDesc == {gpu_name}): {v} ({} candidate(s))", report.gpu_driver_candidates.len());
    } else {
        println!("Driver (registry): no DriverDesc match for {gpu_name}");
    }

    // Which D3D12Core.dll is actually loaded?
    unsafe {
        if let Ok(h) = GetModuleHandleW(PCWSTR(wide("D3D12Core.dll").as_ptr())) {
            let mut buf = vec![0u16; 1024];
            let n = GetModuleFileNameW(Some(h), &mut buf) as usize;
            let path = String::from_utf16_lossy(&buf[..n.min(buf.len())]);
            report.runtime.d3d12core_path = Some(path.clone());
            report.runtime.d3d12core_file_version = file_version(Path::new(&path));
            report.runtime.d3d12core_sdk_version_export = read_sdk_version_export(Path::new(&path)).ok();
            println!("Loaded D3D12Core.dll: {path} (file version {}, D3D12SDKVersion export {:?})", report.runtime.d3d12core_file_version.clone().unwrap_or_else(|| "n/a".into()), report.runtime.d3d12core_sdk_version_export);
            if let (Some(dir), false) = (report.runtime.agility_dir.clone(), factory.is_none()) {
                let _ = dir; // inbox mode: nothing to compare
            }
            if let Some(dir) = report.runtime.agility_dir.clone() {
                let pl = path.to_lowercase();
                let dl = dir.trim_end_matches('\\').to_lowercase();
                let inside = pl.starts_with(&dl) && (pl.len() == dl.len() || pl.as_bytes().get(dl.len()) == Some(&b'\\'));
                report.runtime.loaded_matches_request = Some(inside);
                if !inside {
                    report.notes.push(format!("AGILITY RUN LOADED THE SYSTEM D3D12Core.dll: requested {} but loaded {}", dir, path));
                    println!("ERROR: Agility run loaded the system D3D12Core.dll: requested {dir} but loaded {path}");
                    exit_code = 2;
                }
            }
        } else {
            report.notes.push("D3D12Core.dll is not loaded in this process".into());
            println!("NOTE: D3D12Core.dll is not loaded in this process");
            if factory.is_some() {
                report.runtime.loaded_matches_request = Some(false);
                report.notes.push("Agility run: D3D12Core.dll not even loaded".into());
                exit_code = 2;
            }
        }
    }

    // Features.
    unsafe {
        let mut mesh_shader_tier = 0i32;
        let mut sampler_feedback_tier = 0i32;
        let mut o7 = D3D12_FEATURE_DATA_D3D12_OPTIONS7::default();
        if dev.CheckFeatureSupport(D3D12_FEATURE_D3D12_OPTIONS7, &mut o7 as *mut _ as _, std::mem::size_of_val(&o7) as u32).is_ok() {
            mesh_shader_tier = o7.MeshShaderTier.0;
            sampler_feedback_tier = o7.SamplerFeedbackTier.0;
        }
        let mut wave_min = 0u32;
        let mut wave_max = 0u32;
        let mut o1 = D3D12_FEATURE_DATA_D3D12_OPTIONS1::default();
        if dev.CheckFeatureSupport(D3D12_FEATURE_D3D12_OPTIONS1, &mut o1 as *mut _ as _, std::mem::size_of_val(&o1) as u32).is_ok() {
            wave_min = o1.WaveLaneCountMin;
            wave_max = o1.WaveLaneCountMax;
        }
        let mut resource_binding_tier = 0i32;
        let mut tiled_resources_tier = 0i32;
        let mut o = D3D12_FEATURE_DATA_D3D12_OPTIONS::default();
        if dev.CheckFeatureSupport(D3D12_FEATURE_D3D12_OPTIONS, &mut o as *mut _ as _, std::mem::size_of_val(&o) as u32).is_ok() {
            resource_binding_tier = o.ResourceBindingTier.0;
            tiled_resources_tier = o.TiledResourcesTier.0;
        }
        let mut view_instancing_tier = 0i32;
        let mut o3 = D3D12_FEATURE_DATA_D3D12_OPTIONS3::default();
        if dev.CheckFeatureSupport(D3D12_FEATURE_D3D12_OPTIONS3, &mut o3 as *mut _ as _, std::mem::size_of_val(&o3) as u32).is_ok() {
            view_instancing_tier = o3.ViewInstancingTier.0;
        }
        // Shader model: ask from the highest known version down; E_INVALIDARG means the
        // runtime does not know the request — try the next lower one. The first successful
        // call reports the runtime's own maximum, which may be lower than asked.
        let mut asked = String::from("none");
        let mut highest = String::from("unknown");
        for m in [D3D_SHADER_MODEL_6_8, D3D_SHADER_MODEL_6_7, D3D_SHADER_MODEL_6_6, D3D_SHADER_MODEL_6_5, D3D_SHADER_MODEL_6_4, D3D_SHADER_MODEL_6_3, D3D_SHADER_MODEL_6_2, D3D_SHADER_MODEL_6_1, D3D_SHADER_MODEL_6_0] {
            let mut sm = D3D12_FEATURE_DATA_SHADER_MODEL { HighestShaderModel: m };
            let r = dev.CheckFeatureSupport(D3D12_FEATURE_SHADER_MODEL, &mut sm as *mut _ as _, std::mem::size_of_val(&sm) as u32);
            match r {
                Ok(()) => {
                    let x = sm.HighestShaderModel.0 as u32;
                    asked = format!("6_{}", m.0 - 96);
                    highest = format!("{}.{}", x >> 4, x & 15);
                    break;
                }
                Err(e) if e.code() == E_INVALIDARG => continue,
                Err(e) => {
                    report.notes.push(format!("ShaderModel probe at {} failed: {} (trying lower)", format!("6_{}", m.0 - 96), hr_string(e.code())));
                    continue;
                }
            }
        }
        report.features = Some(Features {
            mesh_shader_tier,
            sampler_feedback_tier,
            wave_lane_count_min: wave_min,
            wave_lane_count_max: wave_max,
            shader_model_asked: asked.clone(),
            highest_shader_model: highest.clone(),
            resource_binding_tier,
            tiled_resources_tier,
            view_instancing_tier,
        });
        println!("MeshShaderTier: {mesh_shader_tier}, SamplerFeedbackTier: {sampler_feedback_tier}");
        println!("WaveLaneCountMin/Max: {wave_min}/{wave_max}");
        println!("ShaderModel: asked {asked}, runtime reports {highest}");
        println!("ResourceBindingTier: {resource_binding_tier}, TiledResourcesTier: {tiled_resources_tier}, ViewInstancingTier: {view_instancing_tier}");
    }

    // G0: compile minimal compute shaders with DXC and try to build PSOs.
    let g0 = g0_probe(&dev, dxc.as_ref());
    if let Some(g0) = &g0 {
        report.g0 = Some(G0Report {
            root_signature: g0.root_signature.clone(),
            sm66: g0.sm66.clone(),
            sm60_wave: g0.sm60_wave.clone(),
        });
        println!("G0 root signature: {}", g0.root_signature);
        println!("G0 cs_6_6: compile {}, validate {}, PSO {}", g0.sm66.compile, g0.sm66.validate, g0.sm66.pso);
        println!("G0 cs_6_0 wave: compile {}, validate {}, PSO {}", g0.sm60_wave.compile, g0.sm60_wave.validate, g0.sm60_wave.pso);
    }

    finish(&out_path, &report, exit_code);
}

struct G0Out {
    root_signature: String,
    sm66: ShaderProbe,
    sm60_wave: ShaderProbe,
}

/// Serialize + create an RS 1.1 with `flags`; the failure text as the second element.
unsafe fn make_rs(dev: &ID3D12Device, params: &[D3D12_ROOT_PARAMETER1], flags: D3D12_ROOT_SIGNATURE_FLAGS) -> (Option<ID3D12RootSignature>, String) {
    let desc = D3D12_VERSIONED_ROOT_SIGNATURE_DESC {
        Version: D3D_ROOT_SIGNATURE_VERSION_1_1,
        Anonymous: D3D12_VERSIONED_ROOT_SIGNATURE_DESC_0 {
            Desc_1_1: D3D12_ROOT_SIGNATURE_DESC1 {
                NumParameters: params.len() as u32,
                pParameters: params.as_ptr(),
                NumStaticSamplers: 0,
                pStaticSamplers: std::ptr::null(),
                Flags: flags,
            },
        },
    };
    let mut blob: Option<ID3DBlob> = None;
    let bytes = match D3D12SerializeVersionedRootSignature(&desc, &mut blob, None) {
        Err(e) => Err(format!("serialize {}", hr_string(e.code()))),
        Ok(()) => blob
            .map(|b| std::slice::from_raw_parts(b.GetBufferPointer() as *const u8, b.GetBufferSize()).to_vec())
            .ok_or_else(|| "serialize returned no blob".to_string()),
    };
    match bytes {
        Err(msg) => (None, msg),
        Ok(bytes) => match dev.CreateRootSignature(0, &bytes) {
            Ok(rs) => (Some(rs), "ok".into()),
            Err(e) => (None, hr_string(e.code())),
        },
    }
}

fn g0_probe(dev: &ID3D12Device, dxc: Option<&DxcToolchain>) -> Option<G0Out> {
    // Root signature: one UAV root descriptor (u0) + the heap-directly-indexed flags, the
    // shape XCL's own compute pass uses (pso.rs::abi_root_signature_with). If the runtime
    // rejects the SM 6.6 heap flags (Windows 10's inbox D3D12Core does not know them),
    // fall back to the same RS without them and record both results.
    let params = [D3D12_ROOT_PARAMETER1 {
        ParameterType: D3D12_ROOT_PARAMETER_TYPE_UAV,
        Anonymous: D3D12_ROOT_PARAMETER1_0 {
            Descriptor: D3D12_ROOT_DESCRIPTOR1 { ShaderRegister: 0, RegisterSpace: 0, Flags: D3D12_ROOT_DESCRIPTOR_FLAG_DATA_VOLATILE },
        },
        ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL,
    }];
    let (rs, rs_hr): (Option<ID3D12RootSignature>, String) = unsafe {
        let heap = D3D12_ROOT_SIGNATURE_FLAG_CBV_SRV_UAV_HEAP_DIRECTLY_INDEXED | D3D12_ROOT_SIGNATURE_FLAG_SAMPLER_HEAP_DIRECTLY_INDEXED;
        let (rs, hr) = make_rs(dev, &params, heap);
        if rs.is_some() {
            (rs, hr)
        } else {
            let (rs2, hr2) = make_rs(dev, &params, D3D12_ROOT_SIGNATURE_FLAG_NONE);
            (rs2, format!("{hr}; without heap flags: {hr2}"))
        }
    };

    let Some(tool) = dxc else {
        return Some(G0Out {
            root_signature: rs_hr,
            sm66: ShaderProbe { compile: "skipped: no DXC".into(), validate: "skipped".into(), pso: "skipped".into() },
            sm60_wave: ShaderProbe { compile: "skipped: no DXC".into(), validate: "skipped".into(), pso: "skipped".into() },
        });
    };

    // cs_6_6: descriptor-heap direct indexing (SM 6.6's feature XCL relies on).
    const SM66: &str = r#"
[numthreads(1,1,1)]
void main(uint tid : SV_DispatchThreadID) {
  RWByteAddressBuffer b = ResourceDescriptorHeap[0];
  b.Store(0, tid);
}
"#;
    // cs_6_0 with a wave intrinsic, bound the classic way (register u0).
    const SM60: &str = r#"
RWByteAddressBuffer b : register(u0);
[numthreads(32,1,1)]
void main(uint tid : SV_DispatchThreadID) {
  b.Store(0, WaveActiveSum(tid));
}
"#;

    let probe_shader = |src: &str, profile: &str| -> ShaderProbe {
        let dxil = unsafe { dxc_compile(tool.create, src, profile) };
        let dxil = match dxil {
            Ok(d) => d,
            Err(e) => {
                return ShaderProbe { compile: format!("dxc error: {e}"), validate: "skipped: compile failed".into(), pso: "skipped: compile failed".into() };
            }
        };
        let validate = match tool.sign {
            Some(sign) => match unsafe { dxc_validate(tool.create, sign, &dxil) } {
                Ok(()) => "ok".to_string(),
                Err(e) => format!("dxil.dll error: {e}"),
            },
            None => "skipped: no dxil.dll".to_string(),
        };
        let pso = match &rs {
            None => "skipped: no root signature".to_string(),
            Some(rs) => unsafe {
                let probe = std::mem::transmute_copy(&Some(rs.clone()));
                match dev.CreateComputePipelineState::<ID3D12PipelineState>(&D3D12_COMPUTE_PIPELINE_STATE_DESC {
                    pRootSignature: probe,
                    CS: D3D12_SHADER_BYTECODE { pShaderBytecode: dxil.as_ptr() as *const c_void, BytecodeLength: dxil.len() },
                    ..Default::default()
                }) {
                    Ok(_) => "ok".to_string(),
                    Err(e) => hr_string(e.code()),
                }
            },
        };
        ShaderProbe { compile: "ok".into(), validate, pso }
    };

    Some(G0Out {
        root_signature: rs_hr,
        sm66: probe_shader(SM66, "cs_6_6"),
        sm60_wave: probe_shader(SM60, "cs_6_0"),
    })
}

fn finish(out_path: &str, report: &Report, code: i32) {
    let json = serde_json::to_string_pretty(report).unwrap_or_else(|e| format!("{{\"fatal\": \"json: {e}\"}}"));
    let _ = std::fs::write(out_path, json);
    println!("\nWritten {out_path}");
    if let Some(f) = &report.fatal {
        println!("FATAL: {f}");
    }
    std::process::exit(code);
}
