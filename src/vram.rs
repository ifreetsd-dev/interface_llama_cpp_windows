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

pub fn check_vram_for_config(ctx_size: i32, ngl: i32, cache_k: &str, cache_v: &str) -> (bool, u64, u64) {
    let vram = match get_vram_usage() {
        Ok(v) => v,
        Err(_) => return (true, 0, 0),
    };
    
    let quant_k = match cache_k.to_lowercase().as_str() {
        "q4_0" | "q4_k" => 0.5,
        "q5_0" | "q5_k" => 0.7,
        "q8_0" | "q8_k" => 1.0,
        "f16" => 2.0,
        "f32" => 4.0,
        _ => 1.0,
    };
    
    let quant_v = match cache_v.to_lowercase().as_str() {
        "q4_0" | "q4_k" => 0.5,
        "q5_0" | "q5_k" => 0.7,
        "q8_0" | "q8_k" => 1.0,
        "f16" => 2.0,
        "f32" => 4.0,
        _ => 1.0,
    };
    
    let ctx_k_mb = (ctx_size as f64 * quant_k / 1024.0 / 1024.0) as u64;
    let ctx_v_mb = (ctx_size as f64 * quant_v / 1024.0 / 1024.0) as u64;
    let activation_mb = (ctx_size as f64 * 1.0 / 1024.0 / 1024.0) as u64;
    
    let layers_mb = if ngl > 0 { (ngl as u64 * 100) } else { 1500 };
    
    let required_mb = ctx_k_mb + ctx_v_mb + activation_mb + layers_mb + 500;
    
    let enough = vram.free_mb >= required_mb;
    (enough, required_mb, vram.free_mb)
}