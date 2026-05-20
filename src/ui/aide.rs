use eframe::egui;
use egui::ScrollArea;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use super::LlamaApp;

pub fn render(ctx: &egui::Context, app: &mut LlamaApp) {
    let lang = app.data.settings.lang;
    egui::CentralPanel::default().show(ctx, |ui| {
        let loading = app.help_loading.load(Ordering::Relaxed);

        ui.horizontal(|ui| {
            ui.heading(lang.t("tab_aide"));
            if loading {
                ui.label(lang.t("msg_help_loading"));
            }
        });
        ui.separator();

        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // CLI help section
            ui.heading("llama-cli.exe --help");
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(lang.t("btn_load_cli_help")).clicked() && !loading {
                    let cli_path = if !app.data.paths.cli_cuda.is_empty() {
                        app.data.paths.cli_cuda.clone()
                    } else if !app.data.paths.cli_vulkan.is_empty() {
                        app.data.paths.cli_vulkan.clone()
                    } else {
                        String::new()
                    };
                    if !cli_path.is_empty() {
                        app.help_loading.store(true, Ordering::Relaxed);
                        let result = Arc::clone(&app.cli_help);
                        let loading2 = Arc::clone(&app.help_loading);
                        std::thread::spawn(move || {
                            let output = std::process::Command::new(&cli_path)
                                .arg("--help")
                                .output();
                            match output {
                                Ok(out) => {
                                    let text = String::from_utf8_lossy(&out.stdout).to_string();
                                    *result.lock().unwrap_or_else(|e| e.into_inner()) = Some(text);
                                }
                                Err(e) => {
                                    *result.lock().unwrap_or_else(|e| e.into_inner()) = Some(format!("Erreur: {}", e));
                                }
                            }
                            loading2.store(false, Ordering::Relaxed);
                        });
                    } else {
                        *app.cli_help.lock().unwrap_or_else(|e| e.into_inner()) = Some(lang.t("msg_no_cli_exe").to_string());
                    }
                }
            });
            let cli_help = app.cli_help.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if let Some(ref text) = cli_help {
                ui.add(egui::TextEdit::multiline(&mut text.clone())
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::MAX)
                    .desired_rows(20)
                    .interactive(false));
            } else {
                ui.label(lang.t("msg_help_prompt"));
            }

            ui.separator();
            ui.add_space(20.0);

            // Server help section
            ui.heading("llama-server.exe --help");
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(lang.t("btn_load_srv_help")).clicked() && !loading {
                    let srv_path = if !app.data.paths.server_cuda.is_empty() {
                        app.data.paths.server_cuda.clone()
                    } else if !app.data.paths.server_vulkan.is_empty() {
                        app.data.paths.server_vulkan.clone()
                    } else {
                        String::new()
                    };
                    if !srv_path.is_empty() {
                        app.help_loading.store(true, Ordering::Relaxed);
                        let result = Arc::clone(&app.server_help);
                        let loading2 = Arc::clone(&app.help_loading);
                        std::thread::spawn(move || {
                            let output = std::process::Command::new(&srv_path)
                                .arg("--help")
                                .output();
                            match output {
                                Ok(out) => {
                                    let text = String::from_utf8_lossy(&out.stdout).to_string();
                                    *result.lock().unwrap_or_else(|e| e.into_inner()) = Some(text);
                                }
                                Err(e) => {
                                    *result.lock().unwrap_or_else(|e| e.into_inner()) = Some(format!("Erreur: {}", e));
                                }
                            }
                            loading2.store(false, Ordering::Relaxed);
                        });
                    } else {
                        *app.server_help.lock().unwrap_or_else(|e| e.into_inner()) = Some(lang.t("msg_no_srv_exe").to_string());
                    }
                }
            });
            let srv_help = app.server_help.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if let Some(ref text) = srv_help {
                ui.add(egui::TextEdit::multiline(&mut text.clone())
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::MAX)
                    .desired_rows(20)
                    .interactive(false));
            } else {
                ui.label(lang.t("msg_help_prompt"));
            }
        });
    });
}
