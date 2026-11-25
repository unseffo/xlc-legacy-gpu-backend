use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::*;

#[derive(serde::Serialize)]
struct AdapterInfo {
    index: u32,
    description: String,
    dedicated_video_memory: u64,
    dedicated_system_memory: u64,
    shared_system_memory: u64,
    adapter_luid: String,
    driver_version: String,
}

#[derive(serde::Serialize, Clone)]
struct FeatureProbe {
    mesh_shader_tier: u32,
    sampler_feedback_tier: u32,
    wave_lane_count_min: u32,
    wave_lane_count_max: u32,
    max_shader_model: String,
    binding_tier: u32,
    tiled_resources: bool,
    cross_adapter: bool,
}

#[derive(serde::Serialize)]
struct ProbeOutput {
    adapters: Vec<AdapterInfo>,
    device_created: bool,
    device_adapter_index: Option<u32>,
    features: Option<FeatureProbe>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut adapters = Vec::new();
        let mut device_created = false;
        let mut device_adapter_index = None;
        let mut features: Option<FeatureProbe> = None;

        for i in 0.. {
            let adapter_result = factory.EnumAdapters1(i);
            let adapter: IDXGIAdapter1 = match adapter_result {
                Ok(a) => a,
                Err(_) => break,
            };

            let desc = adapter.GetDesc1()?;
            let description = String::from_utf16_lossy(&desc.Description);
            let luid = format!("{:08X}:{:08X}", desc.AdapterLuid.HighPart, desc.AdapterLuid.LowPart as u32);

            let driver_version = get_driver_version(&description);
            let driver_version_print = driver_version.clone();

            let info = AdapterInfo {
                index: i,
                description: description.clone(),
                dedicated_video_memory: desc.DedicatedVideoMemory as u64,
                dedicated_system_memory: desc.DedicatedSystemMemory as u64,
                shared_system_memory: desc.SharedSystemMemory as u64,
                adapter_luid: luid,
                driver_version,
            };

            println!("Adapter {}: {}", i, description);
            println!("  Dedicated Video Memory: {} MB", desc.DedicatedVideoMemory / (1024 * 1024));
            println!("  Dedicated System Memory: {} MB", desc.DedicatedSystemMemory / (1024 * 1024));
            println!("  Shared System Memory: {} MB", desc.SharedSystemMemory / (1024 * 1024));
            println!("  Adapter LUID: {}", info.adapter_luid);
            println!("  Driver Version: {}", driver_version_print);

            adapters.push(info);

            if !device_created {
                let mut dev: Option<ID3D12Device> = None;
                let hr = D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_12_0, &mut dev);
                match hr {
                    Ok(()) => {
                        let d = dev.unwrap();
                        println!("  -> D3D12 device created successfully (FL 12_0)");
                        device_created = true;
                        device_adapter_index = Some(i);

                        let fp = probe_features(&d);
                        features = Some(fp.clone());
                        println!("  MeshShaderTier: {}", fp.mesh_shader_tier);
                        println!("  SamplerFeedbackTier: {}", fp.sampler_feedback_tier);
                        println!("  WaveLaneCountMin: {}", fp.wave_lane_count_min);
                        println!("  WaveLaneCountMax: {}", fp.wave_lane_count_max);
                        println!("  MaxShaderModel: {}", fp.max_shader_model);
                        println!("  BindingTier: {}", fp.binding_tier);
                        println!("  TiledResources: {}", fp.tiled_resources);
                        println!("  ViewInstancingTier: {}", fp.cross_adapter);
                    }
                    Err(e) => {
                        println!("  -> D3D12 device creation failed: {:?}", e);
                    }
                }
            }
        }

        let output = ProbeOutput {
            adapters,
            device_created,
            device_adapter_index,
            features,
        };

        let json = serde_json::to_string_pretty(&output)?;
        std::fs::write("out_tgt.json", json)?;
        println!("\nWritten out_tgt.json");

        if device_created {
            println!("\nSUCCESS: Device created on adapter {}", device_adapter_index.unwrap());
        } else {
            println!("\nFAILURE: No adapter could create D3D12 device");
            std::process::exit(1);
        }
    }

    Ok(())
}

fn probe_features(dev: &ID3D12Device) -> FeatureProbe {
    unsafe {
        let mut mesh_shader_tier = 0u32;
        let mut sampler_feedback_tier = 0u32;
        let mut wave_lane_count_min = 0u32;
        let mut wave_lane_count_max = 0u32;
        let mut max_shader_model = "none".to_string();
        let mut binding_tier = 0u32;
        let mut tiled_resources = false;
        let mut cross_adapter = false;

        // OPTIONS7: MeshShaderTier, SamplerFeedbackTier
        let mut o7 = D3D12_FEATURE_DATA_D3D12_OPTIONS7::default();
        if dev.CheckFeatureSupport(D3D12_FEATURE_D3D12_OPTIONS7, &mut o7 as *mut _ as _, std::mem::size_of_val(&o7) as u32).is_ok() {
            mesh_shader_tier = o7.MeshShaderTier.0 as u32;
            sampler_feedback_tier = o7.SamplerFeedbackTier.0 as u32;
        }

        // OPTIONS1: WaveLaneCountMin/Max
        let mut o1 = D3D12_FEATURE_DATA_D3D12_OPTIONS1::default();
        if dev.CheckFeatureSupport(D3D12_FEATURE_D3D12_OPTIONS1, &mut o1 as *mut _ as _, std::mem::size_of_val(&o1) as u32).is_ok() {
            wave_lane_count_min = o1.WaveLaneCountMin;
            wave_lane_count_max = o1.WaveLaneCountMax;
        }

        // SM probe: 6_6 / 6_5 / 6_0
        for m in [D3D_SHADER_MODEL_6_6, D3D_SHADER_MODEL_6_5, D3D_SHADER_MODEL_6_0] {
            let mut sm = D3D12_FEATURE_DATA_SHADER_MODEL { HighestShaderModel: m };
            if dev.CheckFeatureSupport(D3D12_FEATURE_SHADER_MODEL, &mut sm as *mut _ as _, std::mem::size_of_val(&sm) as u32).is_ok() {
                let x = sm.HighestShaderModel.0 as u32;
                let major = (x >> 4) as u8;
                let minor = (x & 15) as u8;
                max_shader_model = format!("{major}.{minor}");
                break;
            }
        }

        // OPTIONS: ResourceBindingTier, TiledResourcesTier
        let mut o = D3D12_FEATURE_DATA_D3D12_OPTIONS::default();
        if dev.CheckFeatureSupport(D3D12_FEATURE_D3D12_OPTIONS, &mut o as *mut _ as _, std::mem::size_of_val(&o) as u32).is_ok() {
            binding_tier = o.ResourceBindingTier.0 as u32;
            tiled_resources = o.TiledResourcesTier.0 != 0;
        }

        // OPTIONS3: ViewInstancingTier (or other useful flags)
        let mut o3 = D3D12_FEATURE_DATA_D3D12_OPTIONS3::default();
        if dev.CheckFeatureSupport(D3D12_FEATURE_D3D12_OPTIONS3, &mut o3 as *mut _ as _, std::mem::size_of_val(&o3) as u32).is_ok() {
            cross_adapter = o3.ViewInstancingTier.0 != 0;
        }

        FeatureProbe {
            mesh_shader_tier,
            sampler_feedback_tier,
            wave_lane_count_min,
            wave_lane_count_max,
            max_shader_model,
            binding_tier,
            tiled_resources,
            cross_adapter,
        }
    }
}

fn get_driver_version(adapter_desc: &str) -> String {
    use std::process::Command;
    let output = Command::new("wmic")
        .args(["path", "win32_VideoController", "where", &format!("Name='{}'", adapter_desc.replace("'", "''")), "get", "DriverVersion", "/value"])
        .output();
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                if line.starts_with("DriverVersion=") {
                    return line["DriverVersion=".len()..].trim().to_string();
                }
            }
            "n/a".to_string()
        }
        Err(_) => "n/a".to_string(),
    }
}