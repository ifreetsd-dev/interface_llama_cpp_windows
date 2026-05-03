#![allow(dead_code)]

use crate::config::{AppData, LlamaConfig, RunMode};
use crate::logger::{self, Level};
use crate::process::ProcessManager;
use crate::vram::{get_vram_usage, check_vram_for_config, VramInfo};
use crossbeam_channel::Receiver;
use eframe::egui;
use egui::{ScrollArea, Grid};
use std::collections::VecDeque;
use std::sync::Arc;
use std::path::PathBuf;
use std::time::Instant;

const MAX_LOGS: usize = 3000;

#[derive(Clone)]
struct LogEntry {
    level: Level,
    message: String,
}

#[derive(Clone, Default)]
struct PendingConfig {
    mode: Option<RunMode>,
    use_vulkan: Option<bool>,
    model_path: Option<String>,
    ngl: Option<i32>,
    threads: Option<u32>,
    ctx_size: Option<i32>,
    temperature: Option<f32>,
    max_tokens: Option<i32>,
    cache_k: Option<String>,
    cache_v: Option<String>,
    chat_template: Option<String>,
    additional_args: Option<String>,
    server_host: Option<String>,
    server_port: Option<u16>,
    server_parallel: Option<i32>,
}

pub struct LlamaApp {
    data: AppData,
    data_path: PathBuf,
    process: Arc<ProcessManager>,
    log_rx: Receiver<String>,
    logs: VecDeque<LogEntry>,
    active_log_tab: String,
    prompt_input: String,
    model_list: Vec<String>,
    selected_model: String,
    new_config_name: String,
    status_msg: String,
    vram: VramInfo,
    vram_last_update: Instant,
    selected_config: Option<String>,
    pending_config: PendingConfig,
    scroll_to_bottom: bool,
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

        let mut app = Self {
            data, data_path: path,
            process: Arc::new(ProcessManager::new(tx)),
            log_rx: rx,
            logs: VecDeque::with_capacity(MAX_LOGS),
            active_log_tab: "Tous".into(),
            prompt_input: String::new(),
            model_list: Vec::new(),
            selected_model: String::new(),
            new_config_name: String::new(),
            status_msg: "Prêt".to_string(),
            vram: VramInfo::default(),
            vram_last_update: Instant::now(),
            selected_config: None,
            pending_config: PendingConfig::default(),
            scroll_to_bottom: true,
        };
        app.scan_models();
        app.selected_config = app.data.active_config.clone();
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

    fn save(&mut self) {
        if let Err(e) = self.data.save(&self.data_path) {
            self.status_msg = format!("❌ Erreur: {}", e);
        } else { 
            self.status_msg = "✅ Sauvegardé".to_string(); 
        }
    }

    fn load_config(&mut self, name: String) {
        if self.data.configs.contains_key(&name) {
            self.data.active_config = Some(name.clone());
            self.add_log(Level::Info, format!("📂 Config '{}' chargée", name));
        }
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

        egui::SidePanel::left("left").default_width(280.0).min_width(220.0).max_width(350.0).show(ctx, |ui| {
            ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.heading("📁 Modèles");
                ui.separator();
                
                ui.horizontal(|ui| {
                    ui.label("Répertoire:");
                    ui.text_edit_singleline(&mut self.data.paths.model_dir);
                });
                ui.horizontal(|ui| {
                    if ui.button("📂").clicked() {
                        if let Some(f) = rfd::FileDialog::new().pick_folder() {
                            self.data.paths.model_dir = f.display().to_string();
                            self.save();
                            self.scan_models();
                        }
                    }
                    if ui.button("🔍").clicked() { 
                        if !self.scan_models() {
                            self.status_msg = "⚠️ Aucun modèle trouvé".to_string();
                        } else {
                            self.status_msg = "✅ Modèles scannés".to_string();
                        }
                    }
                });
                
                if !self.model_list.is_empty() {
                    ui.label(format!("💾 {} modèles:", self.model_list.len()));
                    
                    let models: Vec<String> = self.model_list.clone();
                    let mut clicked: Option<String> = None;
                    
                    for m in &models {
                        let display = m.split('/').next_back().unwrap_or(m);
                        let is_selected = self.selected_model == *m;
                        if ui.selectable_label(is_selected, display).clicked() {
                            clicked = Some(m.clone());
                        }
                    }
                    
                    if let Some(m) = clicked {
                        self.selected_model = m.clone();
                        self.add_log(Level::Info, format!("📁 Modèle: {}", m.split('/').next_back().unwrap_or(&m)));
                    }
                    
                    if !self.selected_model.is_empty() {
                        ui.label(format!("✅ {}", self.selected_model.split('/').next_back().unwrap_or(&self.selected_model)));
                    }
                } else {
                    ui.label("Aucun modèle");
                }
                
                ui.separator();
                ui.heading("🔧 Chemins");
                ui.separator();
                
                let edit_path = |ui: &mut egui::Ui, label: &str, path: &mut String| {
                    ui.horizontal(|ui| {
                        ui.label(label);
                        ui.text_edit_singleline(path);
                        if ui.button("📂").clicked() {
                            if let Some(f) = rfd::FileDialog::new().pick_file() {
                                *path = f.display().to_string();
                            }
                        }
                    });
                };
                
                ui.label("CUDA:");
                edit_path(ui, "  CLI:", &mut self.data.paths.cli_cuda);
                edit_path(ui, "  SRV:", &mut self.data.paths.server_cuda);
                
                ui.label("Vulkan:");
                edit_path(ui, "  CLI:", &mut self.data.paths.cli_vulkan);
                edit_path(ui, "  SRV:", &mut self.data.paths.server_vulkan);
                
                if ui.button("💾 Sauvegarder").clicked() {
                    self.save();
                    self.add_log(Level::Info, "✅ Chemins sauvegardés");
                }
                
                ui.separator();
                ui.heading("📝 Logs");
                ui.separator();
                
                ui.checkbox(&mut self.data.settings.log_trace_enabled, "Trace");
                ui.checkbox(&mut self.data.settings.log_debug_enabled, "Debug");
                ui.checkbox(&mut self.data.settings.log_info_enabled, "Info");
                ui.checkbox(&mut self.data.settings.log_warn_enabled, "Warn");
                ui.checkbox(&mut self.data.settings.log_error_enabled, "Error");
                
                if ui.button("💾 Appliquer").clicked() {
                    self.save();
                }
            });
        });

        egui::TopBottomPanel::top("top_controls").default_height(50.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                let process = Arc::clone(&self.process);
                let active_cfg = self.data.active_config.as_ref()
                    .and_then(|n| self.data.configs.get(n).cloned());
                let paths = self.data.paths.clone();
                
                let cli_running = process.is_cli_running();
                ui.label("💬 CLI:");
                if ui.button(if cli_running { "⏹️ Arrêter" } else { "▶️ Lancer" }).clicked() {
                    if let Some(cfg) = &active_cfg {
                        if cli_running {
                            let proc = process.clone();
                            let pths = paths.clone();
                            tokio::spawn(async move {
                                proc.stop_all(&pths).await;
                            });
                            self.add_log(Level::Info, "⏹️ CLI arrêté");
                            self.status_msg = "CLI arrêté".to_string();
                        } else {
                            let (enough, needed, free) = check_vram_for_config(
                                cfg.ctx_size, cfg.ngl, &cfg.cache_k, &cfg.cache_v
                            );
                            if !enough {
                                let msg = format!("❌ VRAM insuffisante: besoin {} MB, libre {} MB", needed, free);
                                self.status_msg = msg.clone();
                                self.add_log(Level::Error, msg);
                            } else {
                                let proc = process.clone();
                                let cfg = active_cfg.clone().unwrap();
                                let mut cfg_cli = cfg.clone();
                                cfg_cli.mode = RunMode::Cli;
                                let pths = paths.clone();
                                tokio::spawn(async move {
                                    proc.stop_all(&pths).await;
                                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                                    let result = proc.start(&cfg_cli, &pths).await;
                                    match result {
                                        Ok(_) => {
                                            let _ = proc.log_tx.send("[CLI] Processus démarré".to_string());
                                        }
                                        Err(e) => {
                                            let _ = proc.log_tx.send(format!("[CLI] Erreur: {}", e));
                                        }
                                    }
                                });
                                self.status_msg = "CLI en cours de démarrage...".to_string();
                                self.add_log(Level::Info, "▶️ Démarrage CLI...");
                            }
                        }
                    }
                }
                
                ui.separator();
                
                let srv_running = process.is_server_running();
                ui.label("🌐 SRV:");
                if ui.button(if srv_running { "⏹️ Arrêter" } else { "▶️ Lancer" }).clicked() {
                    if let Some(cfg) = &active_cfg {
                        if srv_running {
                            let proc = process.clone();
                            let pths = paths.clone();
                            tokio::spawn(async move {
                                proc.stop_all(&pths).await;
                            });
                            self.add_log(Level::Info, "⏹️ Serveur arrêté");
                            self.status_msg = "Serveur arrêté".to_string();
                        } else {
                            let (enough, needed, free) = check_vram_for_config(
                                cfg.ctx_size, cfg.ngl, &cfg.cache_k, &cfg.cache_v
                            );
                            if !enough {
                                let msg = format!("❌ VRAM insuffisante: besoin {} MB, libre {} MB", needed, free);
                                self.status_msg = msg.clone();
                                self.add_log(Level::Error, msg);
                            } else {
                                let proc = process.clone();
                                let cfg = active_cfg.clone().unwrap();
                                let mut cfg_srv = cfg.clone();
                                cfg_srv.mode = RunMode::Server;
                                let pths = paths.clone();
                                tokio::spawn(async move {
                                    proc.stop_all(&pths).await;
                                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                                    let result = proc.start(&cfg_srv, &pths).await;
                                    match result {
                                        Ok(_) => {
                                            let _ = proc.log_tx.send("[SRV] Serveur démarré".to_string());
                                        }
                                        Err(e) => {
                                            let _ = proc.log_tx.send(format!("[SRV] Erreur: {}", e));
                                        }
                                    }
                                });
                                self.status_msg = "Serveur en cours de démarrage...".to_string();
                                self.add_log(Level::Info, "▶️ Démarrage Serveur...");
                            }
                        }
                    }
                }
                
                ui.separator();
                
                if let Some(cfg) = self.data.active_config.as_ref()
                    .and_then(|n| self.data.configs.get(n)) 
                {
                    if cfg.mode == RunMode::Cli {
                        let cli_running = process.is_cli_running();
                        if cli_running {
                            ui.label("Prompt:");
                            ui.add(egui::TextEdit::singleline(&mut self.prompt_input)
                                .desired_width(300.0)
                                .hint_text("Tapez votre message..."));
                            let enter_pressed = ctx.input(|i| i.keys_down.contains(&egui::Key::Enter));
                            if enter_pressed && !self.prompt_input.is_empty() {
                                process.send_cli_prompt(&self.prompt_input);
                                self.add_log(Level::Info, format!("[Vous] {}", self.prompt_input));
                                self.prompt_input.clear();
                            }
                            if ui.button("➤").clicked() && !self.prompt_input.is_empty() {
                                process.send_cli_prompt(&self.prompt_input);
                                self.add_log(Level::Info, format!("[Vous] {}", self.prompt_input));
                                self.prompt_input.clear();
                            }
                        }
                    }
                }
                
                ui.separator();
                ui.label(&self.status_msg);
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("VRAM: {} / {} MB", self.vram.used_mb, self.vram.total_mb));
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("📜 Logs");
            ui.separator();
            
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.active_log_tab, "Tous".to_string(), "📜 Tous");
                ui.selectable_value(&mut self.active_log_tab, "CLI".to_string(), "💬 CLI");
                ui.selectable_value(&mut self.active_log_tab, "SRV".to_string(), "🌐 Serveur");
                if ui.button("🗑️ Effacer").clicked() { self.logs.clear(); }
            });
            
            ui.separator();
            
            let scroll_bot = self.scroll_to_bottom;
            if scroll_bot { self.scroll_to_bottom = false; }
            
            let mut scroll = ScrollArea::vertical().auto_shrink([false, false]);
            if scroll_bot {
                scroll = scroll.stick_to_bottom(true);
            }
            scroll.show(ui, |ui| {
                let settings = &self.data.settings;
                let prefix = match self.active_log_tab.as_str() {
                    "CLI" => "[CLI]",
                    "SRV" => "[SRV]",
                    _ => ""
                };
                for entry in &self.logs {
                    let level_enabled = match entry.level {
                        Level::Trace => settings.log_trace_enabled,
                        Level::Debug => settings.log_debug_enabled,
                        Level::Info => settings.log_info_enabled,
                        Level::Warn => settings.log_warn_enabled,
                        Level::Error => settings.log_error_enabled,
                    };
                    if !level_enabled { continue; }
                    
                    let msg = &entry.message;
                    if !prefix.is_empty() && !msg.contains(prefix) { continue; }
                    
                    let color = match entry.level {
                        Level::Error => egui::Color32::RED,
                        Level::Warn => egui::Color32::YELLOW,
                        Level::Debug => egui::Color32::LIGHT_BLUE,
                        Level::Trace => egui::Color32::GRAY,
                        _ => egui::Color32::WHITE,
                    };
                    ui.colored_label(color, format!("[{}] {}", entry.level, msg));
                }
            });
        });

        egui::SidePanel::right("right").default_width(260.0).min_width(200.0).max_width(350.0).show(ctx, |ui| {
            ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.heading("📋 Configs");
                ui.separator();
                
                let active = self.data.active_config.clone();
                let mut to_select: Option<String> = None;
                
                for name in self.data.configs.keys() {
                    let is_selected = active.as_deref() == Some(name);
                    ui.horizontal(|ui| {
                        if ui.selectable_label(is_selected, name).clicked() {
                            to_select = Some(name.clone());
                        }
                    });
                }
                
                if let Some(name) = to_select {
                    self.selected_config = Some(name.clone());
                    self.pending_config = PendingConfig::default();
                    self.load_config(name);
                }
                
                ui.separator();
                ui.label("Nouvelle:");
                ui.text_edit_singleline(&mut self.new_config_name);
                if ui.button("➕ Créer").clicked() {
                    let n = self.new_config_name.clone().trim().to_string();
                    if !n.is_empty() {
                        let mut cfg = LlamaConfig::default(); 
                        cfg.name = n.clone();
                        self.data.configs.insert(n.clone(), cfg);
                        self.data.active_config = Some(n.clone());
                        self.new_config_name.clear();
                        self.save();
                        self.add_log(Level::Info, format!("✅ Config '{}' créée", n));
                    }
                }
                
                ui.separator();
                ui.heading("⚙️ Configuration");
                ui.separator();
                
                if let Some(name) = &self.selected_config.clone().or_else(|| self.data.active_config.clone()) {
                    if let Some(cfg) = self.data.configs.get(name).cloned() {
                        let pending = &mut self.pending_config;
                        
                        let mode = pending.mode.unwrap_or(cfg.mode);
                        let use_vulkan = pending.use_vulkan.unwrap_or(cfg.use_vulkan);
                        let mut model_path = pending.model_path.clone().unwrap_or_else(|| cfg.model_path.clone());
                        let ngl = pending.ngl.unwrap_or(cfg.ngl);
                        let threads = pending.threads.unwrap_or(cfg.threads);
                        let ctx_size = pending.ctx_size.unwrap_or(cfg.ctx_size);
                        let temperature = pending.temperature.unwrap_or(cfg.temperature);
                        let max_tokens = pending.max_tokens.unwrap_or(cfg.max_tokens);
                        let mut cache_k = pending.cache_k.clone().unwrap_or_else(|| cfg.cache_k.clone());
                        let mut cache_v = pending.cache_v.clone().unwrap_or_else(|| cfg.cache_v.clone());
                        let mut chat_template = pending.chat_template.clone().unwrap_or_else(|| cfg.chat_template.clone());
                        let mut additional_args = pending.additional_args.clone().unwrap_or_else(|| cfg.additional_args.clone());
                        let server_host = pending.server_host.clone().unwrap_or_else(|| cfg.server_host.clone());
                        let server_port = pending.server_port.unwrap_or(cfg.server_port);
                        let server_parallel = pending.server_parallel.unwrap_or(cfg.server_parallel);
                        
                        let cfg_model_path = cfg.model_path.clone();
                        let cfg_cache_k = cfg.cache_k.clone();
                        let cfg_cache_v = cfg.cache_v.clone();
                        let cfg_chat_template = cfg.chat_template.clone();
                        let cfg_server_host = cfg.server_host.clone();
                        let cfg_additional_args = cfg.additional_args.clone();

                        Grid::new("config_grid").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                            ui.label("Modèle:");
                            ui.horizontal_wrapped(|ui| {
                                egui::ComboBox::from_id_source("model_select")
                                    .selected_text(model_path.is_empty().then_some("Sélectionner...").unwrap_or_else(|| {
                                        std::path::Path::new(&model_path)
                                            .file_name()
                                            .and_then(|n| n.to_str())
                                            .unwrap_or("...")
                                    }))
                                    .show_ui(ui, |ui: &mut egui::Ui| {
                                        ui.selectable_value(&mut model_path, String::new(), "Aucun");
                                        for m in &self.model_list {
                                            let name = std::path::Path::new(m)
                                                .file_name()
                                                .and_then(|n| n.to_str())
                                                .unwrap_or(m.as_str());
                                            ui.selectable_value(&mut model_path, m.clone(), name);
                                        }
                                    });
                                if model_path != cfg_model_path {
                                    pending.model_path = Some(model_path.clone());
                                }
                            });
                            ui.end_row();
                            
                            ui.label("Mode:");
                            ui.horizontal(|ui| {
                                if ui.selectable_label(mode == RunMode::Server, "Serveur").clicked() {
                                    pending.mode = Some(RunMode::Server);
                                }
                                if ui.selectable_label(mode == RunMode::Cli, "CLI").clicked() {
                                    pending.mode = Some(RunMode::Cli);
                                }
                            });
                            ui.end_row();

                            ui.label("Backend:");
                            let mut vulkan = use_vulkan;
                            ui.checkbox(&mut vulkan, "Vulkan");
                            if vulkan != use_vulkan {
                                pending.use_vulkan = Some(vulkan);
                            }
                            ui.end_row();

                            ui.label("GPU Layers:");
                            let mut ngl_val = ngl;
                            ui.add(egui::Slider::new(&mut ngl_val, -1..=100).text("-1=tous"));
                            if ngl_val != ngl {
                                pending.ngl = Some(ngl_val);
                            }
                            ui.end_row();

                            ui.label("Threads:");
                            let mut threads_val = threads;
                            ui.add(egui::Slider::new(&mut threads_val, 1..=32).text(""));
                            if threads_val != threads {
                                pending.threads = Some(threads_val);
                            }
                            ui.end_row();

                            ui.label("Contexte:");
                            let mut ctx_val = ctx_size;
                            ui.add(egui::Slider::new(&mut ctx_val, 512..=131072).logarithmic(true).text(""));
                            if ctx_val != ctx_size {
                                pending.ctx_size = Some(ctx_val);
                            }
                            ui.end_row();

                            ui.label("Température:");
                            let mut temp_val = temperature;
                            ui.add(egui::Slider::new(&mut temp_val, 0.0..=2.0).text(""));
                            if (temp_val - temperature).abs() > 0.01 {
                                pending.temperature = Some(temp_val);
                            }
                            ui.end_row();

                            ui.label("Max Tokens:");
                            let mut tokens_val = max_tokens;
                            ui.add(egui::Slider::new(&mut tokens_val, 1..=16384).text(""));
                            if tokens_val != max_tokens {
                                pending.max_tokens = Some(tokens_val);
                            }
                            ui.end_row();

                            ui.label("Cache K:");
                            ui.text_edit_singleline(&mut cache_k);
                            if cache_k != cfg_cache_k {
                                pending.cache_k = Some(cache_k.clone());
                            }
                            ui.end_row();

                            ui.label("Cache V:");
                            ui.text_edit_singleline(&mut cache_v);
                            if cache_v != cfg_cache_v {
                                pending.cache_v = Some(cache_v.clone());
                            }
                            ui.end_row();

                            ui.label("Template:");
                            ui.text_edit_singleline(&mut chat_template);
                            if chat_template != cfg_chat_template {
                                pending.chat_template = Some(chat_template.clone());
                            }
                            ui.end_row();
                        });

                        ui.collapsing("🌐 Serveur", |ui| {
                            let mut host_val = server_host;
                            let mut port_val = server_port;
                            let mut parallel_val = server_parallel;
                            
                            Grid::new("server_grid").num_columns(2).spacing([10.0, 5.0]).show(ui, |ui| {
                                ui.label("Hôte:");
                                ui.text_edit_singleline(&mut host_val);
                                if host_val != cfg_server_host {
                                    pending.server_host = Some(host_val.clone());
                                }
                                ui.end_row();

                                ui.label("Port:");
                                ui.add(egui::DragValue::new(&mut port_val).clamp_range(1..=65535));
                                if port_val != cfg.server_port {
                                    pending.server_port = Some(port_val);
                                }
                                ui.end_row();

                                ui.label("Parallel:");
                                ui.add(egui::Slider::new(&mut parallel_val, 1..=16).text(""));
                                if parallel_val != cfg.server_parallel {
                                    pending.server_parallel = Some(parallel_val);
                                }
                                ui.end_row();
                            });
                        });

                        ui.collapsing("🔧 Avancé", |ui| {
                            ui.text_edit_multiline(&mut additional_args);
                            if additional_args != cfg_additional_args {
                                pending.additional_args = Some(additional_args.clone());
                            }
                        });

                        if ui.button("💾 Sauvegarder").clicked() {
                            if let Some(name) = self.selected_config.clone().or_else(|| self.data.active_config.clone()) {
                                let mut updated_cfg = cfg;
                                
                                if let Some(m) = pending.mode { updated_cfg.mode = m; }
                                if let Some(v) = pending.use_vulkan { updated_cfg.use_vulkan = v; }
                                if let Some(ref m) = pending.model_path { updated_cfg.model_path = m.clone(); }
                                if let Some(n) = pending.ngl { updated_cfg.ngl = n; }
                                if let Some(t) = pending.threads { updated_cfg.threads = t; }
                                if let Some(c) = pending.ctx_size { updated_cfg.ctx_size = c; }
                                if let Some(t) = pending.temperature { updated_cfg.temperature = t; }
                                if let Some(m) = pending.max_tokens { updated_cfg.max_tokens = m; }
                                if let Some(ref c) = pending.cache_k { updated_cfg.cache_k = c.clone(); }
                                if let Some(ref c) = pending.cache_v { updated_cfg.cache_v = c.clone(); }
                                if let Some(ref c) = pending.chat_template { updated_cfg.chat_template = c.clone(); }
                                if let Some(ref a) = pending.additional_args { updated_cfg.additional_args = a.clone(); }
                                if let Some(ref h) = pending.server_host { updated_cfg.server_host = h.clone(); }
                                if let Some(p) = pending.server_port { updated_cfg.server_port = p; }
                                if let Some(p) = pending.server_parallel { updated_cfg.server_parallel = p; }
                                
                                self.data.configs.insert(name, updated_cfg);
                                self.save();
                                self.pending_config = PendingConfig::default();
                                self.add_log(Level::Info, "✅ Config sauvegardée");
                            }
                        }
                        
                        ui.separator();
                        ui.heading("💻 Système");
                        ui.separator();
                        
                        ui.label(format!("GPU: {}", self.vram.gpu_name));
                        ui.label(format!("VRAM: {} / {} MB", self.vram.used_mb, self.vram.total_mb));
                        let pct = if self.vram.total_mb > 0 {
                            self.vram.used_mb as f32 / self.vram.total_mb as f32 * 100.0
                        } else { 0.0 };
                        ui.add(egui::ProgressBar::new(pct / 100.0));
                        ui.label(format!("Libre: {} MB", self.vram.free_mb));
                        
                        ui.separator();
                        ui.label(&self.status_msg);
                        
                        return;
                    }
                }
                
                ui.label("Sélectionnez une config");
            });
        });
    }
}