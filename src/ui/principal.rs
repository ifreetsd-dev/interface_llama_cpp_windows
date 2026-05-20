use crate::config::{AppData, LlamaConfig, RunMode};
use crate::downloader;
use crate::logger::{self, Level};
use crate::process::ProcessManager;
use crate::disk::{get_disk_info, format_gb};
use eframe::egui;
use egui::ScrollArea;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::path::PathBuf;
use super::{LlamaApp, UpdateStatus, RestoreStatus, PendingConfig, lock, copy_dir_all};

pub fn render(ctx: &egui::Context, app: &mut LlamaApp) {
    let lang = app.data.settings.lang;
    egui::SidePanel::left("left").show(ctx, |ui| {
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.heading(lang.t("section_models"));
            ui.separator();
            
            ui.horizontal(|ui| {
                if ui.button(lang.t("btn_scan")).clicked() {
                    if !app.scan_models() {
                        app.status_msg = lang.t("msg_no_models_found").to_string();
                    } else {
                        app.status_msg = lang.t("msg_models_scanned").to_string();
                    }
                }
            });
            
            if !app.model_list.is_empty() {
                ui.label(format!("💾 {} {}", app.model_list.len(), lang.t("lbl_models_count")));
                
                let models: Vec<String> = app.model_list.clone();
                let mut clicked: Option<String> = None;
                
                for m in &models {
                    let display = m.split('/').next_back().unwrap_or(m);
                    let is_selected = app.selected_model == *m;
                    if ui.selectable_label(is_selected, display).clicked() {
                        clicked = Some(m.clone());
                    }
                }
                
                if let Some(m) = clicked {
                    app.selected_model = m.clone();
                    app.add_log(Level::Info, format!("📁 Modèle: {}", m.split('/').next_back().unwrap_or(&m)));
                }
                
                if !app.selected_model.is_empty() {
                    ui.label(format!("✅ {}", app.selected_model.split('/').next_back().unwrap_or(&app.selected_model)));
                }
            } else {
                ui.label(lang.t("msg_no_models"));
            }
            
            ui.separator();
            ui.heading(lang.t("section_versions"));
            ui.separator();
            
            let read_ver = |dir: &str| -> String {
                if dir.is_empty() { return String::new(); }
                std::fs::read_to_string(std::path::Path::new(dir).join("version.txt")).unwrap_or_default().trim().to_string()
            };
            let cuda_ver = read_ver(&app.data.paths.cuda_dir);
            let vulkan_ver = read_ver(&app.data.paths.vulkan_dir);
            ui.label(format!("{} {}", lang.t("lbl_cuda_local"), if cuda_ver.is_empty() { "N/A".into() } else { cuda_ver }));
            ui.label(format!("{} {}", lang.t("lbl_vulkan_local"), if vulkan_ver.is_empty() { "N/A".into() } else { vulkan_ver }));
            
            let gv = lock(&*app.github_version).clone();
            ui.label(format!("{} {}", lang.t("lbl_cuda_github"), if gv.is_empty() { "..." } else { &gv }));
            ui.label(format!("{} {}", lang.t("lbl_vulkan_github"), if gv.is_empty() { "..." } else { &gv }));
            let loading = app.github_version_loading.load(Ordering::Relaxed);
            if !loading && gv.is_empty() {
                if ui.button(lang.t("btn_check_versions")).clicked() {
                    app.github_version_loading.store(true, Ordering::Relaxed);
                    let gv_arc = Arc::clone(&app.github_version);
                    let gl_arc = Arc::clone(&app.github_version_loading);
                    tokio::spawn(async move {
                        if let Ok(info) = downloader::check_latest().await {
                            *lock(&*gv_arc) = info.tag;
                        }
                        gl_arc.store(false, Ordering::Relaxed);
                    });
                }
            } else if loading {
                ui.label(lang.t("msg_checking"));
            }
            
            ui.separator();
            ui.heading(lang.t("section_update"));
            ui.separator();
            
            let up_status = lock(&*app.update_status).clone();
            match &up_status {
                UpdateStatus::Idle => {
                    if ui.button(lang.t("btn_launch_update")).clicked() {
                        let status = Arc::clone(&app.update_status);
                        let paths = app.data.paths.clone();
                        let data_path = app.data_path.clone();
                        *lock(&*status) = UpdateStatus::Running { step: "Préparation...".into(), progress: 0.0 };
                        tokio::spawn(async move {
                            let set = |s: &str, p: f32| { logger::log(Level::Info, "update", s); *lock(&*status) = UpdateStatus::Running { step: s.into(), progress: p }; };
                            
                            let result: Result<String, String> = (async {
                                set("Vérification de la version...", 0.01);
                                let info = downloader::check_latest().await?;
                                let tag = info.tag.clone();
                                
                                set("Arrêt des processus...", 0.02);
                                ProcessManager::kill_all_by_paths(&paths);
                                
                                // Lire la version locale pour nommer le dossier de sauvegarde
                                let local_ver = |dir: &str| -> String {
                                    if dir.is_empty() { return String::new(); }
                                    std::fs::read_to_string(std::path::Path::new(dir).join("version.txt")).unwrap_or_default().trim().to_string()
                                };
                                let cuda_local = local_ver(&paths.cuda_dir);
                                let vulkan_local = local_ver(&paths.vulkan_dir);
                                let backup_tag = if !cuda_local.is_empty() { cuda_local.clone() } else if !vulkan_local.is_empty() { vulkan_local.clone() } else { tag.clone() };
                                
                                let dir_non_empty = |p: &PathBuf| -> bool {
                                    p.exists() && std::fs::read_dir(p).map(|mut e| e.next().is_some()).unwrap_or(false)
                                };

                                if !paths.vulkan_dir.is_empty() {
                                    let vulkan = PathBuf::from(&paths.vulkan_dir);
                                    if dir_non_empty(&vulkan) {
                                        set("Sauvegarde Vulkan...", 0.05);
                                        let backup_dir = PathBuf::from(&paths.sauvegardes_dir).join(&backup_tag);
                                        let _ = std::fs::create_dir_all(&backup_dir);
                                        let zip_path = backup_dir.join("vulkan_backup.zip");
                                        downloader::zip_dir(&vulkan, &zip_path).map_err(|e| format!("Erreur sauvegarde Vulkan: {}", e))?;
                                        logger::log(Level::Info, "update", &format!("Sauvegarde Vulkan → {}", zip_path.display()));
                                    } else {
                                        logger::log(Level::Info, "update", "Dossier Vulkan vide ou introuvable, sauvegarde ignorée");
                                    }
                                }
                                
                                if !paths.cuda_dir.is_empty() {
                                    let cuda = PathBuf::from(&paths.cuda_dir);
                                    if dir_non_empty(&cuda) {
                                        set("Sauvegarde CUDA...", 0.10);
                                        let backup_dir = PathBuf::from(&paths.sauvegardes_dir).join(&tag);
                                        let _ = std::fs::create_dir_all(&backup_dir);
                                        let zip_path = backup_dir.join("cuda_backup.zip");
                                        downloader::zip_dir(&cuda, &zip_path).map_err(|e| format!("Erreur sauvegarde CUDA: {}", e))?;
                                        logger::log(Level::Info, "update", &format!("Sauvegarde CUDA → {}", zip_path.display()));
                                    } else {
                                        logger::log(Level::Info, "update", "Dossier CUDA vide ou introuvable, sauvegarde ignorée");
                                    }
                                }
                                
                                if !info.vulkan_url.is_empty() {
                                    set("Téléchargement Vulkan...", 0.15);
                                    logger::log(Level::Info, "update", &format!("URL Vulkan: {}", info.vulkan_url));
                                    let temp_dir = PathBuf::from(&paths.temp_dir).join(&tag).join("vulkan");
                                    let zip_path = temp_dir.with_file_name("vulkan.zip");
                                    let _ = std::fs::create_dir_all(&temp_dir);
                                    logger::log(Level::Info, "update", &format!("Destination Vulkan: {}", zip_path.display()));
                                    downloader::download_file(&info.vulkan_url, &zip_path, None, None).await
                                        .map_err(|e| format!("Erreur téléchargement Vulkan: {}", e))?;
                                    set("Extraction Vulkan...", 0.25);
                                    logger::log(Level::Info, "update", "Extraction Vulkan...");
                                    downloader::extract_zip(&zip_path, &temp_dir).await
                                        .map_err(|e| format!("Erreur extraction Vulkan: {}", e))?;
                                    logger::log(Level::Info, "update", "Nettoyage zip Vulkan");
                                    let _ = tokio::fs::remove_file(&zip_path).await;
                                } else {
                                    logger::log(Level::Warn, "update", "URL Vulkan vide, téléchargement ignoré");
                                }
                                
                                if !info.cuda_url.is_empty() || !info.cuda_dll_url.is_empty() {
                                    let temp_dir = PathBuf::from(&paths.temp_dir).join(&tag).join("cuda");
                                    let _ = std::fs::create_dir_all(&temp_dir);
                                    logger::log(Level::Info, "update", &format!("Dossier temp CUDA: {}", temp_dir.display()));
                                    
                                    if !info.cuda_url.is_empty() {
                                        set("Téléchargement CUDA binaires...", 0.30);
                                        logger::log(Level::Info, "update", &format!("URL CUDA: {}", info.cuda_url));
                                        let zip_path = temp_dir.with_file_name("cuda-bin.zip");
                                        logger::log(Level::Info, "update", &format!("Destination CUDA: {}", zip_path.display()));
                                        downloader::download_file(&info.cuda_url, &zip_path, None, None).await
                                            .map_err(|e| format!("Erreur téléchargement CUDA: {}", e))?;
                                        set("Extraction CUDA binaires...", 0.45);
                                        logger::log(Level::Info, "update", "Extraction CUDA binaires...");
                                        downloader::extract_zip(&zip_path, &temp_dir).await
                                            .map_err(|e| format!("Erreur extraction CUDA: {}", e))?;
                                        let _ = tokio::fs::remove_file(&zip_path).await;
                                    } else {
                                        logger::log(Level::Warn, "update", "URL CUDA vide, binaires ignorés");
                                    }
                                    
                                    if !info.cuda_dll_url.is_empty() {
                                        set("Téléchargement CUDA DLLs...", 0.55);
                                        logger::log(Level::Info, "update", &format!("URL CUDA DLLs: {}", info.cuda_dll_url));
                                        let zip_path = temp_dir.with_file_name("cuda-dlls.zip");
                                        logger::log(Level::Info, "update", &format!("Destination DLLs: {}", zip_path.display()));
                                        downloader::download_file(&info.cuda_dll_url, &zip_path, None, None).await
                                            .map_err(|e| format!("Erreur téléchargement DLLs: {}", e))?;
                                        set("Extraction CUDA DLLs...", 0.65);
                                        logger::log(Level::Info, "update", "Extraction CUDA DLLs...");
                                        downloader::extract_zip(&zip_path, &temp_dir).await
                                            .map_err(|e| format!("Erreur extraction DLLs: {}", e))?;
                                        let _ = tokio::fs::remove_file(&zip_path).await;
                                    } else {
                                        logger::log(Level::Warn, "update", "URL CUDA DLLs vide, DLLs ignorés");
                                    }
                                } else {
                                    logger::log(Level::Warn, "update", "URLs CUDA vides, téléchargement CUDA ignoré");
                                }
                                
                                set("Écriture version.txt...", 0.70);
                                if !paths.vulkan_dir.is_empty() {
                                    let temp_vulkan = PathBuf::from(&paths.temp_dir).join(&tag).join("vulkan");
                                    if temp_vulkan.exists() {
                                        let _ = std::fs::write(temp_vulkan.join("version.txt"), &tag);
                                    }
                                    let _ = std::fs::write(PathBuf::from(&paths.vulkan_dir).join("version.txt"), &tag);
                                }
                                if !paths.cuda_dir.is_empty() {
                                    let temp_cuda = PathBuf::from(&paths.temp_dir).join(&tag).join("cuda");
                                    if temp_cuda.exists() {
                                        let _ = std::fs::write(temp_cuda.join("version.txt"), &tag);
                                    }
                                    let _ = std::fs::write(PathBuf::from(&paths.cuda_dir).join("version.txt"), &tag);
                                }
                                
                                if !paths.vulkan_dir.is_empty() {
                                    set("Remplacement Vulkan...", 0.75);
                                    let temp_vulkan = PathBuf::from(&paths.temp_dir).join(&tag).join("vulkan");
                                    let main_vulkan = PathBuf::from(&paths.vulkan_dir);
                                    if temp_vulkan.exists() {
                                        let _ = std::fs::create_dir_all(&main_vulkan);
                                        for entry in std::fs::read_dir(&temp_vulkan).into_iter().flatten().flatten() {
                                            let dest = main_vulkan.join(entry.file_name());
                                            let _ = if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                                                copy_dir_all(entry.path(), &dest)
                                            } else {
                                                std::fs::copy(entry.path(), &dest).map(|_| ())
                                            };
                                        }
                                    }
                                }
                                
                                if !paths.cuda_dir.is_empty() {
                                    set("Remplacement CUDA...", 0.85);
                                    let temp_cuda = PathBuf::from(&paths.temp_dir).join(&tag).join("cuda");
                                    let main_cuda = PathBuf::from(&paths.cuda_dir);
                                    if temp_cuda.exists() {
                                        let _ = std::fs::create_dir_all(&main_cuda);
                                        for entry in std::fs::read_dir(&temp_cuda).into_iter().flatten().flatten() {
                                            let dest = main_cuda.join(entry.file_name());
                                            let _ = if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                                                copy_dir_all(entry.path(), &dest)
                                            } else {
                                                std::fs::copy(entry.path(), &dest).map(|_| ())
                                            };
                                        }
                                    }
                                }
                                
                                set("Sauvegarde configuration...", 0.95);
                                if let Ok(mut data) = AppData::load(&data_path) {
                                    data.installed_version = tag.clone();
                                    if let Err(e) = data.save(&data_path) {
                                        logger::log(Level::Warn, "update", &format!("Impossible de sauvegarder config: {}", e));
                                    }
                                } else {
                                    logger::log(Level::Warn, "update", "Impossible de charger app_data.json");
                                }
                                
                                Ok(tag)
                            }).await;
                            
                            if let Ok(ref tag) = result {
                                let temp_tag = PathBuf::from(&paths.temp_dir).join(tag);
                                if let Err(e) = std::fs::remove_dir_all(&temp_tag) {
                                    logger::log(Level::Warn, "update", &format!("Nettoyage temp ignoré: {}", e));
                                }
                            }
                            
                            match result {
                                Ok(tag) => { logger::log(Level::Info, "update", &format!("✅ Mise à jour {} terminée", tag)); *lock(&*status) = UpdateStatus::Done(tag); }
                                Err(e) => { logger::log(Level::Error, "update", &e); *lock(&*status) = UpdateStatus::Error(e); }
                            }
                        });
                    }
                }
                UpdateStatus::Running { step, progress } => {
                    ui.label(format!("🔄 {}...", step));
                    ui.add(egui::ProgressBar::new(*progress).text(format!("{:.0}%", *progress * 100.0)));
                }
                UpdateStatus::Done(tag) => {
                    ui.label(format!("✅ Mise à jour {} terminée", tag));
                    ui.add(egui::ProgressBar::new(1.0).text("100%"));
                    if ui.button(lang.t("btn_ok")).clicked() {
                        *lock(&*app.update_status) = UpdateStatus::Idle;
                    }
                }
                UpdateStatus::Error(e) => {
                    ui.colored_label(egui::Color32::RED, &format!("❌ {}", e));
                    if ui.button(lang.t("btn_retry")).clicked() {
                        *lock(&*app.update_status) = UpdateStatus::Idle;
                    }
                }
            }
            
            ui.separator();
            ui.heading(lang.t("section_restore"));
            ui.separator();
            
            let rs_status = lock(&*app.restore_status).clone();
            match &rs_status {
                RestoreStatus::Idle => {
                    if ui.button(lang.t("btn_scan_backups")).clicked() {
                        let status = Arc::clone(&app.restore_status);
                        let sauvegardes = app.data.paths.sauvegardes_dir.clone();
                        *lock(&*status) = RestoreStatus::Scanning;
                        tokio::spawn(async move {
                            let dir = PathBuf::from(&sauvegardes);
                            let mut backups = Vec::new();
                            if dir.exists() {
                                for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                                        backups.push(entry.file_name().to_string_lossy().to_string());
                                    }
                                }
                                backups.sort();
                                backups.reverse();
                            }
                            if backups.is_empty() {
                                *lock(&*status) = RestoreStatus::Error("Aucune sauvegarde trouvée".into());
                            } else {
                                *lock(&*status) = RestoreStatus::Ready { backups, selected: String::new() };
                            }
                        });
                    }
                }
                RestoreStatus::Scanning => {
                    ui.label(lang.t("msg_scanning"));
                }
                RestoreStatus::Ready { backups, selected } => {
                    ui.label(format!("📋 {} {}", backups.len(), lang.t("lbl_backups_found")));
                    let mut to_delete: Option<String> = None;
                    for b in backups {
                        let is_sel = selected == b;
                        ui.horizontal(|ui| {
                        if ui.selectable_label(is_sel, b).clicked() {
                            *lock(&*app.restore_status) = RestoreStatus::Ready { backups: backups.clone(), selected: b.clone() };
                            }
                            if ui.button(lang.t("btn_delete")).clicked() {
                                to_delete = Some(b.clone());
                            }
                        });
                    }
                    if let Some(name) = to_delete {
                        let path = PathBuf::from(&app.data.paths.sauvegardes_dir).join(&name);
                        let _ = std::fs::remove_dir_all(&path);
                        app.add_log(Level::Info, format!("🗑️ Sauvegarde supprimée: {}", name));
                        let status = Arc::clone(&app.restore_status);
                        let sauvegardes = app.data.paths.sauvegardes_dir.clone();
                        *lock(&*status) = RestoreStatus::Scanning;
                        tokio::spawn(async move {
                            let dir = PathBuf::from(&sauvegardes);
                            let mut backups = Vec::new();
                            if dir.exists() {
                                for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                                        backups.push(entry.file_name().to_string_lossy().to_string());
                                    }
                                }
                                backups.sort();
                                backups.reverse();
                            }
                            if backups.is_empty() {
                                *lock(&*status) = RestoreStatus::Error("Aucune sauvegarde trouvée".into());
                            } else {
                                *lock(&*status) = RestoreStatus::Ready { backups, selected: String::new() };
                            }
                        });
                    }
                    if !selected.is_empty() {
                        ui.label(format!("{}: {}", lang.t("lbl_selected"), selected));
                        if ui.button(lang.t("btn_restore")).clicked() {
                            let status = Arc::clone(&app.restore_status);
                            let paths = app.data.paths.clone();
                            let data_path = app.data_path.clone();
                            let sel = selected.clone();
                            *lock(&*status) = RestoreStatus::Running { step: "Préparation...".into(), progress: 0.0 };
                            tokio::spawn(async move {
                                let set = |s: &str, p: f32| { logger::log(Level::Info, "restore", s); *lock(&*status) = RestoreStatus::Running { step: s.into(), progress: p }; };
                                let result: Result<(), String> = (async {
                                    set("Arrêt des processus...", 0.05);
                                    ProcessManager::kill_all_by_paths(&paths);
                                    
                                    set("Restauration Vulkan...", 0.10);
                                    let backup_dir = PathBuf::from(&paths.sauvegardes_dir).join(&sel);
                                    let vulkan_zip = backup_dir.join("vulkan_backup.zip");
                                    if vulkan_zip.exists() && !paths.vulkan_dir.is_empty() {
                                        let main_vulkan = PathBuf::from(&paths.vulkan_dir);
                                        let _ = std::fs::create_dir_all(&main_vulkan);
                                        downloader::extract_zip(&vulkan_zip, &main_vulkan).await
                                            .map_err(|e| format!("Erreur extraction Vulkan: {}", e))?;
                                    }
                                    
                                    set("Restauration CUDA...", 0.50);
                                    let cuda_zip = backup_dir.join("cuda_backup.zip");
                                    if cuda_zip.exists() && !paths.cuda_dir.is_empty() {
                                        let main_cuda = PathBuf::from(&paths.cuda_dir);
                                        let _ = std::fs::create_dir_all(&main_cuda);
                                        downloader::extract_zip(&cuda_zip, &main_cuda).await
                                            .map_err(|e| format!("Erreur extraction CUDA: {}", e))?;
                                    }
                                    
                                    set("Sauvegarde configuration...", 0.90);
                                    if let Ok(mut data) = AppData::load(&data_path) {
                                        data.installed_version.clear();
                                        if let Err(e) = data.save(&data_path) {
                                            logger::log(Level::Warn, "restore", &format!("Impossible de sauvegarder config: {}", e));
                                        }
                                    } else {
                                        logger::log(Level::Warn, "restore", "Impossible de charger app_data.json");
                                    }
                                    
                                    Ok(())
                                }).await;
                                match result {
                                    Ok(()) => { logger::log(Level::Info, "restore", &format!("✅ Restauration de {} terminée", sel)); *lock(&*status) = RestoreStatus::Done(sel.clone()); }
                                    Err(e) => { logger::log(Level::Error, "restore", &e); *lock(&*status) = RestoreStatus::Error(e); }
                                }
                            });
                        }
                    }
                }
                RestoreStatus::Running { step, progress } => {
                    ui.label(format!("🔄 {}...", step));
                    ui.add(egui::ProgressBar::new(*progress).text(format!("{:.0}%", *progress * 100.0)));
                }
                RestoreStatus::Done(tag) => {
                    ui.label(format!("✅ Restauration de {} terminée", tag));
                    ui.add(egui::ProgressBar::new(1.0).text("100%"));
                    if ui.button(lang.t("btn_ok")).clicked() {
                        *app.restore_status.lock().unwrap() = RestoreStatus::Idle;
                    }
                }
                RestoreStatus::Error(e) => {
                    ui.colored_label(egui::Color32::RED, &format!("❌ {}", e));
                    if ui.button(lang.t("btn_retry")).clicked() {
                        *app.restore_status.lock().unwrap() = RestoreStatus::Idle;
                    }
                }
            }
            
            ui.separator();
            ui.heading(lang.t("section_logs"));
            ui.separator();
            
            ui.checkbox(&mut app.data.settings.log_trace_enabled, lang.t("log_trace"));
            ui.checkbox(&mut app.data.settings.log_debug_enabled, lang.t("log_debug"));
            ui.checkbox(&mut app.data.settings.log_info_enabled, lang.t("log_info"));
            ui.checkbox(&mut app.data.settings.log_warn_enabled, lang.t("log_warn"));
            ui.checkbox(&mut app.data.settings.log_error_enabled, lang.t("log_error"));
            
            if ui.button(lang.t("btn_apply")).clicked() {
                app.save();
            }
        });
    });
    
    egui::TopBottomPanel::top("top_controls").default_height(50.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            let process = Arc::clone(&app.process);
            let active_cfg = app.data.active_config.as_ref()
                .and_then(|n| app.data.configs.get(n).cloned());
            let paths = app.data.paths.clone();
            
            let cli_running = process.is_cli_running();
            ui.label(lang.t("lbl_cli"));
            if ui.button(if cli_running { lang.t("btn_stop") } else { lang.t("btn_start") }).clicked() {
                if let Some(cfg) = &active_cfg {
                    if cli_running {
                        let proc = process.clone();
                        let pths = paths.clone();
                        tokio::spawn(async move {
                            proc.stop_all(&pths).await;
                        });
                        app.add_log(Level::Info, "⏹️ CLI arrêté");
                        app.status_msg = "CLI arrêté".to_string();
                    } else {
                        let proc = process.clone();
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
                        app.status_msg = "CLI en cours de démarrage...".to_string();
                        app.add_log(Level::Info, "▶️ Démarrage CLI...");
                    }
                }
            }
            
            ui.separator();
            
            let srv_running = process.is_server_running();
            ui.label(lang.t("lbl_srv"));
            if ui.button(if srv_running { lang.t("btn_stop") } else { lang.t("btn_start") }).clicked() {
                if let Some(cfg) = &active_cfg {
                    if srv_running {
                        let proc = process.clone();
                        let pths = paths.clone();
                        tokio::spawn(async move {
                            proc.stop_all(&pths).await;
                        });
                        app.add_log(Level::Info, "⏹️ Serveur arrêté");
                        app.status_msg = "Serveur arrêté".to_string();
                    } else {
                        let proc = process.clone();
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
                        app.status_msg = "Serveur en cours de démarrage...".to_string();
                        app.add_log(Level::Info, "▶️ Démarrage Serveur...");
                    }
                }
            }
            
            ui.separator();
            
            if let Some(cfg) = app.data.active_config.as_ref()
                .and_then(|n| app.data.configs.get(n)) 
            {
                if cfg.mode == RunMode::Cli {
                    let cli_running = process.is_cli_running();
                    if cli_running {
                        ui.label(lang.t("lbl_prompt"));
                        ui.add(egui::TextEdit::singleline(&mut app.prompt_input)
                            .desired_width(300.0)
                            .hint_text(lang.t("lbl_hint_prompt")));
                        let enter_pressed = ctx.input(|i| i.keys_down.contains(&egui::Key::Enter));
                        if enter_pressed && !app.prompt_input.is_empty() {
                            process.send_cli_prompt(&app.prompt_input);
                            let _ = app.log_tx.send(format!("[Vous] {}", app.prompt_input));
                            app.prompt_input.clear();
                        }
                        if ui.button(lang.t("btn_send")).clicked() && !app.prompt_input.is_empty() {
                            process.send_cli_prompt(&app.prompt_input);
                            let _ = app.log_tx.send(format!("[Vous] {}", app.prompt_input));
                            app.prompt_input.clear();
                        }
                    }
                }
            }
            
            ui.separator();
            ui.label(&app.status_msg);
            
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(format!("{}: {} / {} MB", lang.t("lbl_vram"), app.vram.used_mb, app.vram.total_mb));
            });
        });
    });
    
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading(lang.t("section_conversation"));
        ui.separator();
        ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
            for entry in &app.conversation {
                let (prefix_key, color) = match entry.label.as_str() {
                    "User" => ("conv_user", egui::Color32::LIGHT_BLUE),
                    "Model" => ("conv_model", egui::Color32::LIGHT_GREEN),
                    "Server" => ("conv_server", egui::Color32::YELLOW),
                    "Waiting" => ("conv_waiting", egui::Color32::GRAY),
                    _ => ("", egui::Color32::WHITE),
                };
                let prefix = lang.t(prefix_key);
                ui.horizontal(|ui| {
                    ui.strong(format!("{} ", prefix));
                    let text = &entry.text;
                    if text.contains("Cmd:") {
                        let fmt = text.replace(" --", "\n--").replace(" -", "\n -");
                        ui.colored_label(color, fmt);
                    } else if text.contains("Prompt:") || text.contains("Prompt :") || text.contains("Generation:") || text.contains("Génération :") {
                        ui.colored_label(color, text);
                    } else {
                        ui.colored_label(color, text.replace(".", ".\n"));
                    }
                });
            }
            if app.conversation.is_empty() {
                ui.label(lang.t("msg_conv_empty"));
            }
        });
    });
    
    egui::SidePanel::right("right").show(ctx, |ui| {
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.heading(lang.t("section_configs"));
            ui.separator();
            
            let active = app.data.active_config.clone();
            let mut to_select: Option<String> = None;
            let mut to_delete: Option<String> = None;
            
            for name in app.data.configs.keys() {
                let is_selected = active.as_deref() == Some(name);
                ui.horizontal(|ui| {
                    if ui.selectable_label(is_selected, name).clicked() {
                        to_select = Some(name.clone());
                    }
                    if ui.button(lang.t("btn_delete")).clicked() {
                        to_delete = Some(name.clone());
                    }
                });
            }
            
            if let Some(name) = to_select {
                app.selected_config = Some(name.clone());
                app.pending_config = PendingConfig::default();
                app.load_config(name);
            }
            
            if let Some(name) = to_delete {
                app.data.configs.remove(&name);
                if app.data.active_config.as_deref() == Some(&name) {
                    app.data.active_config = None;
                    app.selected_config = None;
                }
                app.save();
                app.add_log(Level::Info, format!("🗑️ Config supprimée: {}", name));
            }
            
            ui.separator();
            ui.label(lang.t("lbl_new"));
            ui.text_edit_singleline(&mut app.new_config_name);
            if ui.button(lang.t("btn_new_config")).clicked() {
                let n = app.new_config_name.clone().trim().to_string();
                if !n.is_empty() {
                    let mut cfg = LlamaConfig::default(); 
                    cfg.name = n.clone();
                    app.data.configs.insert(n.clone(), cfg);
                    app.data.active_config = Some(n.clone());
                    app.new_config_name.clear();
                    app.save();
                    app.add_log(Level::Info, format!("✅ Config '{}' créée", n));
                }
            }
            
            ui.separator();
            ui.heading(lang.t("section_config"));
            ui.separator();
            
            if let Some(name) = &app.selected_config.clone().or_else(|| app.data.active_config.clone()) {
                if let Some(cfg) = app.data.configs.get(name).cloned() {
                    let pending = &mut app.pending_config;
                    
                    let mode = pending.mode.unwrap_or(cfg.mode);
                    let use_vulkan = pending.use_vulkan.unwrap_or(cfg.use_vulkan);
                    let mut model_path = pending.model_path.clone().unwrap_or_else(|| cfg.model_path.clone());
                    let mut additional_args = pending.additional_args.clone().unwrap_or_else(|| cfg.additional_args.clone());
                    let cfg_model_path = cfg.model_path.clone();
                    let cfg_additional_args = cfg.additional_args.clone();
                    let mut server_host = pending.server_host.clone().unwrap_or_else(|| cfg.server_host.clone());
                    let mut server_port = pending.server_port.unwrap_or(cfg.server_port);
                    let mut server_parallel = pending.server_parallel.unwrap_or(cfg.server_parallel);
                    
                    ui.label(lang.t("lbl_model"));
                    ui.horizontal_wrapped(|ui| {
                        egui::ComboBox::from_id_source("model_select")
                            .selected_text(model_path.is_empty().then_some(lang.t("msg_select_model")).unwrap_or_else(|| {
                                std::path::Path::new(&model_path)
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("...")
                            }))
                            .show_ui(ui, |ui: &mut egui::Ui| {
                                ui.selectable_value(&mut model_path, String::new(), lang.t("lbl_none"));
                                for m in &app.model_list {
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
                    
                    ui.horizontal(|ui| {
                        if ui.selectable_label(mode == RunMode::Server, lang.t("mode_server")).clicked() {
                            pending.mode = Some(RunMode::Server);
                        }
                        if ui.selectable_label(mode == RunMode::Cli, lang.t("mode_cli")).clicked() {
                            pending.mode = Some(RunMode::Cli);
                        }
                    });
                    
                    let mut vulkan = use_vulkan;
                    ui.checkbox(&mut vulkan, "Vulkan");
                    if vulkan != use_vulkan {
                        pending.use_vulkan = Some(vulkan);
                    }
                    
                    if mode == RunMode::Server {
                        ui.horizontal(|ui| {
                            ui.label("Host:");
                            ui.text_edit_singleline(&mut server_host);
                            if server_host != cfg.server_host {
                                pending.server_host = Some(server_host.clone());
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Port:");
                            let mut port_str = server_port.to_string();
                            if ui.text_edit_singleline(&mut port_str).changed() {
                                if let Ok(p) = port_str.parse() {
                                    server_port = p;
                                    pending.server_port = Some(server_port);
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("-np:");
                            let mut np_str = server_parallel.to_string();
                            if ui.text_edit_singleline(&mut np_str).changed() {
                                if let Ok(p) = np_str.parse() {
                                    server_parallel = p;
                                    pending.server_parallel = Some(server_parallel);
                                }
                            }
                        });
                    }
                    
                    ui.label(lang.t("lbl_extra_args"));
                    ui.text_edit_multiline(&mut additional_args);
                    if additional_args != cfg_additional_args {
                        pending.additional_args = Some(additional_args.clone());
                    }
                    
                    if ui.button(lang.t("btn_save")).clicked() {
                        if let Some(name) = app.selected_config.clone().or_else(|| app.data.active_config.clone()) {
                            let mut updated_cfg = cfg;
                            
                            if let Some(m) = pending.mode { updated_cfg.mode = m; }
                            if let Some(v) = pending.use_vulkan { updated_cfg.use_vulkan = v; }
                            if let Some(ref m) = pending.model_path { updated_cfg.model_path = m.clone(); }
                            if let Some(ref a) = pending.additional_args { updated_cfg.additional_args = a.clone(); }
                            if let Some(ref h) = pending.server_host { updated_cfg.server_host = h.clone(); }
                            if let Some(p) = pending.server_port { updated_cfg.server_port = p; }
                            if let Some(p) = pending.server_parallel { updated_cfg.server_parallel = p; }
                            
                            app.data.configs.insert(name, updated_cfg);
                            app.save();
                            app.pending_config = PendingConfig::default();
                            app.add_log(Level::Info, "✅ Config sauvegardée");
                        }
                    }
                    
                    ui.separator();
                    ui.heading(lang.t("section_system"));
                    ui.separator();
                    
                    ui.label(format!("{}: {}", lang.t("lbl_gpu"), app.vram.gpu_name));
                    ui.label(format!("{}: {} / {} MB", lang.t("lbl_vram"), app.vram.used_mb, app.vram.total_mb));
                    let pct = if app.vram.total_mb > 0 {
                        app.vram.used_mb as f32 / app.vram.total_mb as f32 * 100.0
                    } else { 0.0 };
                    ui.add(egui::ProgressBar::new(pct / 100.0));
                    ui.label(format!("{}: {} MB", lang.t("lbl_free"), app.vram.free_mb));
                    
                    ui.separator();
                    ui.heading(lang.t("section_ram"));
                    ui.separator();
                    ui.label(format!("{}: {} / {} Mo", lang.t("lbl_ram"), app.ram.used_mb, app.ram.total_mb));
                    let ram_pct = if app.ram.total_mb > 0 {
                        app.ram.used_mb as f32 / app.ram.total_mb as f32 * 100.0
                    } else { 0.0 };
                    ui.add(egui::ProgressBar::new(ram_pct / 100.0));
                    ui.label(format!("{}: {} Mo", lang.t("lbl_free"), app.ram.free_mb));
                    
                    ui.separator();
                    ui.heading(lang.t("section_disks"));
                    ui.separator();
                    for disk in get_disk_info() {
                        let pct = if disk.total_gb > 0.0 {
                            (disk.total_gb - disk.free_gb) as f32 / disk.total_gb as f32 * 100.0
                        } else { 0.0 };
                        ui.horizontal(|ui| {
                            ui.strong(&disk.drive);
                            ui.label(format!("{} / {}", format_gb(disk.free_gb), format_gb(disk.total_gb)));
                            ui.add(egui::ProgressBar::new(pct / 100.0)
                                .desired_width(60.0));
                        });
                    }
                    
                    return;
                }
            }
            
            ui.label(lang.t("msg_no_config"));
        });
    });
}
