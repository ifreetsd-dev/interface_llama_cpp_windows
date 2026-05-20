use crate::error::Result;
use crate::lang::Lang;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]  
pub enum RunMode { Cli, Server }

#[derive(Serialize, Deserialize, Debug)]
pub struct AppSettings {
    #[serde(default = "default_log_level")]
    pub log_level: u8,
    #[serde(default = "default_true")]
    pub log_trace_enabled: bool,
    #[serde(default = "default_true")]
    pub log_debug_enabled: bool,
    #[serde(default = "default_true")]
    pub log_info_enabled: bool,
    #[serde(default = "default_true")]
    pub log_warn_enabled: bool,
    #[serde(default = "default_true")]
    pub log_error_enabled: bool,
    #[serde(default)]
    pub lang: Lang,
}

fn default_log_level() -> u8 { 2 }
fn default_true() -> bool { true }

impl Default for AppSettings {
    fn default() -> Self {
        Self { 
            log_level: 2,
            log_trace_enabled: true,
            log_debug_enabled: true,
            log_info_enabled: true,
            log_warn_enabled: true,
            log_error_enabled: true,
            lang: Lang::Fr,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct GlobalPaths {
    pub cli_cuda: String,
    pub cli_vulkan: String,
    pub server_cuda: String,
    pub server_vulkan: String,
    pub model_dir: String,
    #[serde(default)]
    pub temp_dir: String,
    #[serde(default)]
    pub sauvegardes_dir: String,
    #[serde(default)]
    pub cuda_dir: String,
    #[serde(default)]
    pub vulkan_dir: String,
}

impl Default for GlobalPaths {
    fn default() -> Self {
        Self {
            cli_cuda: String::new(), cli_vulkan: String::new(),
            server_cuda: String::new(), server_vulkan: String::new(),
            model_dir: String::new(),
            temp_dir: String::new(),
            sauvegardes_dir: String::new(),
            cuda_dir: String::new(),
            vulkan_dir: String::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct LlamaConfig {
    pub name: String,
    pub mode: RunMode,
    pub use_vulkan: bool,
    pub model_path: String,
    pub additional_args: String,
    pub server_host: String,
    pub server_port: u16,
    pub server_parallel: i32,
}

impl Default for LlamaConfig {
    fn default() -> Self {
        Self {
            name: "Nouvelle config".into(), mode: RunMode::Cli, use_vulkan: true,
            model_path: String::new(), additional_args: String::new(),
            server_host: "127.0.0.1".into(), server_port: 8080, server_parallel: 4,
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct AppData {
    pub paths: GlobalPaths,
    pub configs: HashMap<String, LlamaConfig>,
    pub active_config: Option<String>,
    pub settings: AppSettings,
    #[serde(default)]
    pub installed_version: String,
}

impl AppData {
    pub fn load(path: &PathBuf) -> Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            Ok(serde_json::from_str::<AppData>(&content)?)
        } else {
            let mut data = Self::default();
            let mut p1 = LlamaConfig::default(); 
            p1.name = "💬 Chat Rapide".into();
            p1.additional_args = "-ngl 35 -t 8 --mlock -c 4096 --temp 0.6 -n 1024".into();
            let mut p2 = LlamaConfig::default(); 
            p2.name = "💻 Low VRAM".into();
            p2.additional_args = "-ngl 20 -t 4 --mlock -c 2048 -n 256".into();
            let mut p3 = LlamaConfig::default(); 
            p3.name = "📜 Contexte Max".into();
            p3.additional_args = "-ngl 20 -t 6 -c 32768".into();
            for p in [p1, p2, p3] { data.configs.insert(p.name.clone(), p); }
            Ok(data)
        }
    }

    pub fn save(&self, path: &PathBuf) -> Result<()> {
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}