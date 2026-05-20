#![allow(dead_code)]

pub mod principal;
pub mod logs;
pub mod hf;
pub mod settings;
pub mod aide;

use crate::config::{AppData, RunMode};
use crate::logger::{self, Level};
use crate::process::ProcessManager;
use crate::vram::{get_vram_usage, VramInfo};
use crate::disk::RamInfo;
use crate::huggingface::HfModelInfo;
use crossbeam_channel::Receiver;
use eframe::egui;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use std::time::Instant;

const MAX_LOGS: usize = 3000;

#[derive(Clone)]
pub struct ConversationEntry {
    pub label: String,
    pub text: String,
}

#[derive(Clone)]
pub struct LogEntry {
    pub level: Level,
    pub message: String,
}

#[derive(Clone, Default)]
pub struct PendingConfig {
    pub mode: Option<RunMode>,
    pub use_vulkan: Option<bool>,
    pub model_path: Option<String>,
    pub additional_args: Option<String>,
    pub server_host: Option<String>,
    pub server_port: Option<u16>,
    pub server_parallel: Option<i32>,
}

#[derive(Clone)]
pub enum UpdateStatus {
    Idle,
    Running { step: String, progress: f32 },
    Done(String),
    Error(String),
}

#[derive(Clone)]
pub enum RestoreStatus {
    Idle,
    Scanning,
    Ready { backups: Vec<String>, selected: String },
    Running { step: String, progress: f32 },
    Done(String),
    Error(String),
}

pub struct LlamaApp {
    pub data: AppData,
    pub data_path: PathBuf,
    pub process: Arc<ProcessManager>,
    pub log_tx: crossbeam_channel::Sender<String>,
    pub log_rx: Receiver<String>,
    pub logs: VecDeque<LogEntry>,
    pub active_log_tab: String,
    pub active_tab: String,
    pub log_show_file: bool,
    pub log_file_tab: String,
    pub prompt_input: String,
    pub model_list: Vec<String>,
    pub selected_model: String,
    pub new_config_name: String,
    pub status_msg: String,
    pub vram: VramInfo,
    pub vram_last_update: Instant,
    pub ram: RamInfo,
    pub ram_last_update: Instant,
    pub selected_config: Option<String>,
    pub pending_config: PendingConfig,
    pub scroll_to_bottom: bool,
    pub update_status: Arc<Mutex<UpdateStatus>>,
    pub restore_status: Arc<Mutex<RestoreStatus>>,
    pub conversation: Vec<ConversationEntry>,
    pub hf_query: String,
    pub hf_results: Arc<Mutex<Vec<HfModelInfo>>>,
    pub hf_searching: Arc<Mutex<bool>>,
    pub hf_search_error: Arc<Mutex<String>>,
    pub hf_search_total: Arc<Mutex<usize>>,
    pub hf_downloading: Option<String>,
    pub hf_dl_progress: Arc<Mutex<Option<(String, u64, u64)>>>,
    pub hf_dl_done: Arc<Mutex<Option<String>>>,
    pub hf_expanded: std::collections::HashSet<String>,
    pub hf_details: Arc<Mutex<std::collections::HashMap<String, HfModelInfo>>>,
    pub hf_cancel: Arc<AtomicBool>,
    pub hf_filter_q2: bool,
    pub hf_filter_q3: bool,
    pub hf_filter_q4: bool,
    pub hf_filter_q5: bool,
    pub hf_filter_q6: bool,
    pub hf_filter_q8: bool,
    pub hf_filter_finetune: bool,
    pub hf_filter_adapter: bool,
    pub hf_filter_merge: bool,
    pub disk_warning: Option<String>,
    pub github_version: Arc<Mutex<String>>,
    pub github_version_loading: Arc<AtomicBool>,
    pub cli_help: Arc<Mutex<Option<String>>>,
    pub server_help: Arc<Mutex<Option<String>>>,
    pub help_loading: Arc<AtomicBool>,
}

impl LlamaApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        logger::init();
        
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));
        let path = exe_dir.join("app_data.json");
        
        let data = match AppData::load(&path) {
            Ok(d) => d,
            Err(e) => {
                logger::log(Level::Error, "config", &format!("Échec chargement config: {}", e));
                logger::log(Level::Info, "config", "Utilisation des valeurs par défaut");
                AppData::default()
            }
        };
        
        let (tx, rx) = crossbeam_channel::unbounded();
        let ui_tx = tx.clone();

        let mut app = Self {
            data, data_path: path,
            process: Arc::new(ProcessManager::new(tx)),
            log_tx: ui_tx,
            log_rx: rx,
            logs: VecDeque::with_capacity(MAX_LOGS),
            active_log_tab: "Tous".into(),
            active_tab: "principal".into(),
            log_show_file: false,
            log_file_tab: "Tous".into(),
            prompt_input: String::new(),
            model_list: Vec::new(),
            selected_model: String::new(),
            new_config_name: String::new(),
            status_msg: "Prêt".to_string(),
            vram: VramInfo::default(),
            vram_last_update: Instant::now(),
            ram: RamInfo::default(),
            ram_last_update: Instant::now(),
            selected_config: None,
            pending_config: PendingConfig::default(),
            scroll_to_bottom: true,
            update_status: Arc::new(Mutex::new(UpdateStatus::Idle)),
            restore_status: Arc::new(Mutex::new(RestoreStatus::Idle)),
            conversation: Vec::new(),
            hf_query: String::new(),
            hf_results: Arc::new(Mutex::new(Vec::new())),
            hf_searching: Arc::new(Mutex::new(false)),
            hf_search_error: Arc::new(Mutex::new(String::new())),
            hf_search_total: Arc::new(Mutex::new(0)),
            hf_downloading: None,
            hf_dl_progress: Arc::new(Mutex::new(None)),
            hf_dl_done: Arc::new(Mutex::new(None)),
            hf_expanded: std::collections::HashSet::new(),
            hf_details: Arc::new(Mutex::new(std::collections::HashMap::new())),
            hf_cancel: Arc::new(AtomicBool::new(false)),
            hf_filter_q2: false,
            hf_filter_q3: false,
            hf_filter_q4: false,
            hf_filter_q5: false,
            hf_filter_q6: false,
            hf_filter_q8: false,
            hf_filter_finetune: false,
            hf_filter_adapter: false,
            hf_filter_merge: false,
            disk_warning: None,
            github_version: Arc::new(Mutex::new(String::new())),
            github_version_loading: Arc::new(AtomicBool::new(false)),
            cli_help: Arc::new(Mutex::new(None)),
            server_help: Arc::new(Mutex::new(None)),
            help_loading: Arc::new(AtomicBool::new(false)),
        };
        app.init_paths(&exe_dir);
        app.scan_models();
        app.selected_config = app.data.active_config.clone();
        // Auto-fetch GitHub version
        {
            let gv = Arc::clone(&app.github_version);
            let gl = Arc::clone(&app.github_version_loading);
            tokio::spawn(async move {
                if let Ok(info) = crate::downloader::check_latest().await {
                    *lock(&*gv) = info.tag;
                }
                gl.store(false, Ordering::Relaxed);
            });
        }
        app.status_msg = app.t("msg_ready").to_string();
        logger::log(Level::Info, "ui", "Application initialisée");
        app
    }

    fn add_log(&mut self, level: Level, msg: impl Into<String>) {
        self.logs.push_back(LogEntry { level, message: msg.into() });
        if self.logs.len() > MAX_LOGS { self.logs.pop_front(); }
        self.scroll_to_bottom = true;
    }

    fn scan_models(&mut self) -> bool {
        self.model_list.clear();
        
        if self.data.paths.model_dir.is_empty() { return false; }
        
        let model_dir = std::path::Path::new(&self.data.paths.model_dir);
        if !model_dir.exists() { return false; }
        
        if let Ok(entries) = std::fs::read_dir(model_dir) {
            for e in entries.filter_map(|e| e.ok()) {
                if let Some(p) = e.path().to_str() {
                    if p.ends_with(".gguf") { self.model_list.push(p.to_string()); }
                }
            }
        }
        self.model_list.sort();
        logger::log(Level::Info, "ui", &format!("{} modèles trouvés", self.model_list.len()));
        !self.model_list.is_empty()
    }

    fn init_paths(&mut self, exe_dir: &std::path::Path) {
        let mut changed = false;

        // Default directory paths relative to exe dir
        if self.data.paths.model_dir.is_empty() {
            self.data.paths.model_dir = exe_dir.join("models").display().to_string();
            changed = true;
        }
        if self.data.paths.cuda_dir.is_empty() {
            self.data.paths.cuda_dir = exe_dir.join("cuda").display().to_string();
            changed = true;
        }
        if self.data.paths.vulkan_dir.is_empty() {
            self.data.paths.vulkan_dir = exe_dir.join("vulkan").display().to_string();
            changed = true;
        }
        if self.data.paths.temp_dir.is_empty() {
            self.data.paths.temp_dir = exe_dir.join("temp").display().to_string();
            changed = true;
        }
        if self.data.paths.sauvegardes_dir.is_empty() {
            self.data.paths.sauvegardes_dir = exe_dir.join("sauvegardes").display().to_string();
            changed = true;
        }

        // Create directories if they don't exist
        for d in [&self.data.paths.model_dir, &self.data.paths.cuda_dir,
                  &self.data.paths.vulkan_dir, &self.data.paths.temp_dir,
                  &self.data.paths.sauvegardes_dir] {
            if !d.is_empty() {
                let _ = std::fs::create_dir_all(std::path::Path::new(d));
            }
        }

        // Pre-fill expected executable paths (even if files don't exist yet)
        let cuda_dir = std::path::Path::new(&self.data.paths.cuda_dir);
        if self.data.paths.cli_cuda.is_empty() {
            self.data.paths.cli_cuda = cuda_dir.join("llama-cli.exe").display().to_string();
            changed = true;
        }
        if self.data.paths.server_cuda.is_empty() {
            self.data.paths.server_cuda = cuda_dir.join("llama-server.exe").display().to_string();
            changed = true;
        }
        let vulkan_dir = std::path::Path::new(&self.data.paths.vulkan_dir);
        if self.data.paths.cli_vulkan.is_empty() {
            self.data.paths.cli_vulkan = vulkan_dir.join("llama-cli.exe").display().to_string();
            changed = true;
        }
        if self.data.paths.server_vulkan.is_empty() {
            self.data.paths.server_vulkan = vulkan_dir.join("llama-server.exe").display().to_string();
            changed = true;
        }

        // Fill default config additional_args if empty
        let default_configs: [&str; 3] = ["💬 Chat Rapide", "💻 Low VRAM", "📜 Contexte Max"];
        for cfg_name in default_configs {
            if let Some(cfg) = self.data.configs.get_mut(cfg_name) {
                if cfg.additional_args.is_empty() {
                    cfg.additional_args = match cfg_name {
                        "💬 Chat Rapide" => "-ngl 35 -t 8 --mlock -c 4096 --temp 0.6 -n 1024".into(),
                        "💻 Low VRAM" => "-ngl 20 -t 4 --mlock -c 2048 -n 256".into(),
                        "📜 Contexte Max" => "-ngl 20 -t 6 -c 32768".into(),
                        _ => String::new(),
                    };
                    changed = true;
                }
            }
        }

        if changed {
            self.save();
        }

        logger::log(Level::Info, "ui", "Chemins initialisés");
    }

    fn save(&mut self) {
        if let Err(e) = self.data.save(&self.data_path) {
            self.status_msg = format!("❌ Erreur: {}", e);
        } else { 
            self.status_msg = format!("{}", self.t("msg_saved")); 
        }
    }

    fn load_config(&mut self, name: String) {
        if self.data.configs.contains_key(&name) {
            self.data.active_config = Some(name.clone());
            self.add_log(Level::Info, format!("📂 Config '{}' chargée", name));
        }
    }

    fn t<'a>(&self, key: &'a str) -> &'a str {
        self.data.settings.lang.t(key)
    }

    fn delete_config(&mut self, name: String) {
        self.data.configs.remove(&name);
        if self.data.active_config.as_deref() == Some(&name) { 
            self.data.active_config = None; 
        }
        self.save();
        self.add_log(Level::Info, format!("🗑️ '{}' supprimée", name));
    }
}

impl eframe::App for LlamaApp {
    fn on_exit(&mut self, _ctx: Option<&eframe::glow::Context>) {
        self.add_log(Level::Info, "Fermeture en cours...");
        
        let process = Arc::clone(&self.process);
        let paths = self.data.paths.clone();
        
        let rt = tokio::runtime::Runtime::new().expect("Runtime pour shutdown");
        rt.block_on(async {
            process.stop_all(&paths).await;
        });
        
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(line) = self.log_rx.try_recv() {
            let level = if line.starts_with("[ERROR]") {
                        Level::Error
                    } else if line.starts_with("[WARN]") {
                        Level::Warn
                    } else if line.starts_with("[DEBUG]") {
                        Level::Debug
                    } else if line.starts_with("[TRACE]") {
                        Level::Trace
                    } else {
                        Level::Info
                    };
            // Build conversation from CLI/SRV/User lines
            if line.starts_with("[Vous]") {
                // Remove any pending loading indicator
                if self.conversation.last().map_or(false, |e| e.label == "Waiting") {
                    self.conversation.pop();
                }
                self.conversation.push(ConversationEntry {
                    label: "User".into(),
                    text: line.trim_start_matches("[Vous] ").to_string(),
                });
                self.conversation.push(ConversationEntry {
                    label: "Waiting".into(),
                    text: "⏳".to_string(),
                });
            } else if line.starts_with("[CLI]") {
                let text = line.trim_start_matches("[CLI] ").to_string();
                if !text.is_empty() {
                    if self.conversation.last().map_or(false, |e| e.label == "Waiting") {
                        self.conversation.pop();
                    }
                    if let Some(last) = self.conversation.last_mut() {
                        if last.label == "Model" {
                            last.text.push('\n');
                            last.text.push_str(&text);
                        } else {
                            self.conversation.push(ConversationEntry { label: "Model".into(), text });
                        }
                    } else {
                        self.conversation.push(ConversationEntry { label: "Model".into(), text });
                    }
                }
            } else if line.starts_with("[SRV]") {
                let text = line.trim_start_matches("[SRV] ").to_string();
                if !text.is_empty() {
                    if self.conversation.last().map_or(false, |e| e.label == "Waiting") {
                        self.conversation.pop();
                    }
                    if let Some(last) = self.conversation.last_mut() {
                        if last.label == "Server" {
                            last.text.push('\n');
                            last.text.push_str(&text);
                        } else {
                            self.conversation.push(ConversationEntry { label: "Server".into(), text });
                        }
                    } else {
                        self.conversation.push(ConversationEntry { label: "Server".into(), text });
                    }
                }
            }
            self.add_log(level, line);
            ctx.request_repaint();
        }

        if self.vram_last_update.elapsed().as_secs() >= 1 {
            self.vram = match get_vram_usage() {
                Ok(v) => v,
                Err(e) => {
                    logger::log(Level::Error, "vram", &format!("{}", e));
                    VramInfo::default()
                }
            };
            self.vram_last_update = Instant::now();
        }
        
        if self.ram_last_update.elapsed().as_secs() >= 1 {
            self.ram = crate::disk::get_ram_info();
            self.ram_last_update = Instant::now();
        }
        
        let done_id = lock(&*self.hf_dl_done).take();
        if let Some(done_id) = done_id {
            self.hf_downloading = None;
            if done_id.contains("Espace insuffisant") || done_id.contains("❌") {
                self.add_log(Level::Error, done_id.clone());
                self.disk_warning = Some(done_id);
            } else {
                self.add_log(Level::Info, format!("✅ Modèle téléchargé: {}", done_id));
            }
        }
        
        if self.disk_warning.is_none() {
            if let UpdateStatus::Error(msg) = &*lock(&*self.update_status) {
                if msg.contains("Espace insuffisant") {
                    self.disk_warning = Some(msg.clone());
                }
            }
        }
        
        if let Some(ref msg) = self.disk_warning.clone() {
            let lang = self.data.settings.lang;
            egui::Window::new(lang.t("warning_title"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.colored_label(egui::Color32::YELLOW, lang.t("warning_title"));
                    ui.label(msg);
                    ui.separator();
                    if ui.button(lang.t("btn_ok")).clicked() {
                        self.disk_warning = None;
                    }
                });
        }

        let lang = self.data.settings.lang;
        egui::TopBottomPanel::top("tab_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.active_tab, "principal".to_string(), lang.t("tab_principal"));
                ui.selectable_value(&mut self.active_tab, "logs".to_string(), lang.t("tab_logs"));
                ui.selectable_value(&mut self.active_tab, "huggingface".to_string(), lang.t("tab_huggingface"));
                ui.selectable_value(&mut self.active_tab, "settings".to_string(), lang.t("tab_settings"));
                ui.selectable_value(&mut self.active_tab, "aide".to_string(), lang.t("tab_aide"));
            });
        });

        if self.active_tab == "principal" {
            principal::render(ctx, self);
        } else if self.active_tab == "logs" {
            logs::render(ctx, self);
        } else if self.active_tab == "huggingface" {
            hf::render(ctx, self);
        } else if self.active_tab == "settings" {
            settings::render(ctx, self);
        } else if self.active_tab == "aide" {
            aide::render(ctx, self);
        }
    }
}

pub fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn copy_dir_all(src: impl AsRef<std::path::Path>, dst: impl AsRef<std::path::Path>) -> std::io::Result<()> {
    std::fs::create_dir_all(&dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest = dst.as_ref().join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}
