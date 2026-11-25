use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_12_0;
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

#[derive(serde::Serialize)]
struct ProbeOutput {
    adapters: Vec<AdapterInfo>,
    device_created: bool,
    device_adapter_index: Option<u32>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut adapters = Vec::new();
        let mut device_created = false;
        let mut device_adapter_index = None;

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
                let mut device: Option<ID3D12Device> = None;
                let hr = D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_12_0, &mut device);
                match hr {
                    Ok(()) => {
                        println!("  -> D3D12 device created successfully (FL 12_0)");
                        device_created = true;
                        device_adapter_index = Some(i);
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