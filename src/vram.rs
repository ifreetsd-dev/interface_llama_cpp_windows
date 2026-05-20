use crate::error::Result;
use nvml_wrapper::Nvml;
use std::sync::Mutex;
use std::time::{Duration, Instant};

static VRAM_CACHE: Mutex<Option<(VramInfo, Instant)>> = Mutex::new(None);
static NVML: Mutex<Option<Nvml>> = Mutex::new(None);
const VRAM_CACHE_DURATION: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
pub struct VramInfo {
    pub used_mb: u64,
    pub free_mb: u64,
    pub total_mb: u64,
    pub gpu_name: String,
}

impl Default for VramInfo {
    fn default() -> Self {
        Self { 
            used_mb: 0, 
            free_mb: 0,
            total_mb: 0,
            gpu_name: "NVIDIA GPU".into() 
        }
    }
}

#[cfg(target_os = "windows")]
fn query_vram() -> Result<VramInfo> {
    let mut nvml_guard = NVML.lock().unwrap();
    
    if nvml_guard.is_none() {
        match Nvml::init() {
            Ok(nvml) => *nvml_guard = Some(nvml),
            Err(_) => return Ok(VramInfo::default()),
        }
    }
    
    if let Some(ref nvml) = *nvml_guard {
        match nvml.device_count() {
            Ok(count) if count > 0 => {
                if let Ok(device) = nvml.device_by_index(0) {
                    let gpu_name = device.name().unwrap_or("GPU".to_string());
                    let memory = device.memory_info().ok();
                    
                    let used_mb = memory.as_ref().map(|m| m.used / (1024 * 1024)).unwrap_or(0);
                    let free_mb = memory.as_ref().map(|m| m.free / (1024 * 1024)).unwrap_or(0);
                    let total_mb = memory.as_ref().map(|m| m.total / (1024 * 1024)).unwrap_or(0);
                    
                    return Ok(VramInfo {
                        used_mb,
                        free_mb,
                        total_mb,
                        gpu_name,
                    });
                }
            }
            _ => {}
        }
    }
    
    Ok(VramInfo::default())
}

#[cfg(not(target_os = "windows"))]
fn query_vram() -> Result<VramInfo> {
    Ok(VramInfo::default())
}

pub fn get_vram_usage() -> Result<VramInfo> {
    let mut cache = VRAM_CACHE.lock().unwrap();
    
    if let Some((info, time)) = cache.as_ref() {
        if time.elapsed() < VRAM_CACHE_DURATION {
            return Ok(info.clone());
        }
    }
    
    let info = query_vram()?;
    *cache = Some((info.clone(), Instant::now()));
    Ok(info)
}

