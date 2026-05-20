use eframe::egui;
use egui::ScrollArea;
use crate::huggingface::{search_models, get_model_details, download_specific_gguf, download_model_gguf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use super::{LlamaApp, lock};

fn format_size(s: i64) -> String {
    if s > 1_000_000_000 {
        format!("{:.1} Go", s as f64 / 1_073_741_824.0)
    } else if s > 1_000_000 {
        format!("{:.1} Mo", s as f64 / 1_048_576.0)
    } else if s > 0 {
        format!("{:.0} Ko", s as f64 / 1024.0)
    } else {
        String::new()
    }
}

pub fn render(ctx: &egui::Context, app: &mut LlamaApp) {
    let lang = app.data.settings.lang;
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(lang.t("lbl_search"));
            ui.add(egui::TextEdit::singleline(&mut app.hf_query)
                .desired_width(300.0)
                .hint_text(lang.t("lbl_hint_hf")));
            if ui.button(lang.t("btn_search")).clicked() || ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                if !app.hf_query.is_empty() {
                    let mut q = app.hf_query.clone();
                    if app.hf_filter_q2 { q.push_str(" Q2_"); }
                    if app.hf_filter_q3 { q.push_str(" Q3_"); }
                    if app.hf_filter_q4 { q.push_str(" Q4_"); }
                    if app.hf_filter_q5 { q.push_str(" Q5_"); }
                    if app.hf_filter_q6 { q.push_str(" Q6_"); }
                    if app.hf_filter_q8 { q.push_str(" Q8_"); }
                    if app.hf_filter_finetune { q.push_str(" finetune"); }
                    if app.hf_filter_adapter { q.push_str(" adapter lora"); }
                    if app.hf_filter_merge { q.push_str(" merge"); }
                    let results = Arc::clone(&app.hf_results);
                    let searching = Arc::clone(&app.hf_searching);
                    let error = Arc::clone(&app.hf_search_error);
                    let details = Arc::clone(&app.hf_details);
                    let search_total = Arc::clone(&app.hf_search_total);
                    app.hf_expanded.clear();
                    lock(&*details).clear();
                    *lock(&*results) = Vec::new();
                    *lock(&*searching) = true;
                    *lock(&*error) = String::new();
                    *lock(&*search_total) = 0;
                    tokio::spawn(async move {
                        match search_models(&q).await {
                            Ok(models) => {
                                let total = models.len();
                                *lock(&*search_total) = total;
                                let ids: Vec<String> = models.iter().map(|m| m.id.clone()).collect();
                                // Push results one by one for a streaming effect
                                for model in models {
                                    lock(&*results).push(model);
                                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                }
                                *lock(&*searching) = false;
                                *lock(&*search_total) = 0;
                                // Prefetch details for all results
                                for mid in &ids {
                                    if lock(&*details).contains_key(mid) { continue; }
                                    if let Ok(info) = get_model_details(mid).await {
                                        lock(&*details).insert(mid.clone(), info);
                                    }
                                }
                            }
                            Err(e) => {
                                *lock(&*error) = e;
                                *lock(&*searching) = false;
                            }
                        }
                    });
                }
            }
            ui.separator();
            ui.checkbox(&mut app.hf_filter_q2, "Q2");
            ui.checkbox(&mut app.hf_filter_q3, "Q3");
            ui.checkbox(&mut app.hf_filter_q4, "Q4");
            ui.checkbox(&mut app.hf_filter_q5, "Q5");
            ui.checkbox(&mut app.hf_filter_q6, "Q6");
            ui.checkbox(&mut app.hf_filter_q8, "Q8");
            if ui.button(lang.t("hf_all")).clicked() {
                let all = !(app.hf_filter_q2 && app.hf_filter_q3 && app.hf_filter_q4 && app.hf_filter_q5 && app.hf_filter_q6 && app.hf_filter_q8);
                app.hf_filter_q2 = all;
                app.hf_filter_q3 = all;
                app.hf_filter_q4 = all;
                app.hf_filter_q5 = all;
                app.hf_filter_q6 = all;
                app.hf_filter_q8 = all;
            }
            ui.separator();
            ui.checkbox(&mut app.hf_filter_finetune, lang.t("hf_finetunes"));
            ui.checkbox(&mut app.hf_filter_adapter, lang.t("hf_adapters"));
            ui.checkbox(&mut app.hf_filter_merge, lang.t("hf_merges"));
        });
        ui.separator();

        let searching = *lock(&*app.hf_searching);
        let search_error = lock(&*app.hf_search_error).clone();
        let results = lock(&*app.hf_results).clone();

        if searching {
            let total = *lock(&*app.hf_search_total);
            let current = lock(&*app.hf_results).len();
            ui.label(format!("{} {}/{}", lang.t("msg_search_progress"), current, total));
            if total > 0 {
                ui.add(egui::ProgressBar::new(current as f32 / total as f32).text(format!("{}/{}", current, total)));
            }
        } else if !search_error.is_empty() {
            ui.colored_label(egui::Color32::RED, &search_error);
        } else if results.is_empty() {
            ui.label(lang.t("msg_hf_empty"));
        } else {
            let dl_progress = Arc::clone(&app.hf_dl_progress);
            let free_vram_mb = app.vram.free_mb;

            ui.label(format!("{} {}  |  🖥️ {}: {:.1} Go ({:.0}%)",
                results.len(), lang.t("lbl_hf_results"),
                lang.t("lbl_free_vram"),
                free_vram_mb as f64 / 1024.0,
                if app.vram.total_mb > 0 { 100.0 - app.vram.used_mb as f64 / app.vram.total_mb as f64 * 100.0 } else { 0.0 },
            ));
            ui.separator();
            ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                // Request repaint if any expanded model still needs details
                let needs_repaint = app.hf_expanded.iter().any(|id| !lock(&*app.hf_details).contains_key(id));
                if needs_repaint { ctx.request_repaint(); }
                for model in &results {
                    let is_expanded = app.hf_expanded.contains(&model.id);
                    let model_dl_key = model.id.clone();
                    let is_downloading = app.hf_downloading.as_deref() == Some(&model_dl_key);
                    let progress = lock(&*dl_progress).clone();

                    let (best_size, best_size_bytes) = {
                        let details_guard = lock(&*app.hf_details);
                        let cached = details_guard.get(&model.id);
                        let siblings = cached.map(|m| &m.siblings).unwrap_or(&model.siblings);
                        let best = siblings.iter()
                            .filter(|s| s.rfilename.ends_with(".gguf") && s.size.unwrap_or(0) > 0)
                            .max_by_key(|s| s.size.unwrap_or(0));
                        match best {
                            Some(s) => {
                                let bytes = s.size.unwrap_or(0) as u64;
                                (format_size(s.size.unwrap_or(0)), bytes)
                            }
                            None => (String::new(), 0),
                        }
                    };
                    let fits = best_size_bytes == 0 || best_size_bytes <= free_vram_mb * 1_048_576;

                    ui.horizontal(|ui| {
                        if ui.selectable_label(is_expanded, if is_expanded { "▼" } else { "▶" }).clicked() {
                            if !is_expanded {
                                app.hf_expanded.insert(model.id.clone());
                                let details = Arc::clone(&app.hf_details);
                                let mid = model.id.clone();
                                if lock(&*details).get(&mid).is_none() {
                                    tokio::spawn(async move {
                                        if let Ok(info) = get_model_details(&mid).await {
                                            lock(&*details).insert(mid, info);
                                        }
                                    });
                                }
                            } else {
                                app.hf_expanded.remove(&model.id);
                            }
                        }
                        ui.strong(&model.id);
                        if let Some(d) = model.downloads {
                            ui.label(format!("📥 {}", d));
                        }
                        if !fits {
                            ui.colored_label(egui::Color32::RED, lang.t("msg_vram_insufficient"));
                        } else if !best_size.is_empty() {
                            ui.label(format!("🟢 {}", best_size));
                        }
                        if app.hf_downloading.is_none() {
                            if ui.button(lang.t("btn_download")).clicked() {
                                let mid = model.id.clone();
                                let dl_progress2 = Arc::clone(&app.hf_dl_progress);
                                let dl_done = Arc::clone(&app.hf_dl_done);
                                let dest = std::path::PathBuf::from(&app.data.paths.model_dir);
                                let temp = std::path::PathBuf::from(&app.data.paths.temp_dir).join("hf_dl");
                                let cancel = Arc::clone(&app.hf_cancel);
                                app.hf_cancel.store(false, Ordering::Relaxed);
                                app.hf_downloading = Some(mid.clone());
                                tokio::spawn(async move {
                                    *lock(&*dl_progress2) = None;
                                    let result = download_model_gguf(&mid, &dest, &temp, Some(cancel), Some(Arc::clone(&dl_progress2))).await;
                                    *lock(&*dl_progress2) = None;
                                    match result {
                                        Ok(fname) => {
                                            *lock(&*dl_done) = Some(format!("{} ({})", mid, fname));
                                        }
                                        Err(e) => {
                                            *lock(&*dl_done) = Some(format!("{} ❌ {}", mid, e));
                                        }
                                    }
                                });
                            }
                        } else if is_downloading {
                            ui.label("🔄");
                            if ui.button(lang.t("btn_cancel")).clicked() {
                                app.hf_cancel.store(true, Ordering::Relaxed);
                            }
                        }
                    });

                    if let Some((ref fname, downloaded, total)) = progress {
                        let dl_match = app.hf_downloading.as_ref().map_or(false, |k| k == &model_dl_key || k.starts_with(&format!("{}:", model.id)));
                        if dl_match && total > 0 {
                            let pct = downloaded as f32 / total as f32;
                            ui.add(egui::ProgressBar::new(pct).text(format!("{} - {:.1}%", fname, pct * 100.0)));
                        } else if dl_match {
                            ui.label(format!("📥 {} - {} Mo", fname, downloaded / (1024*1024)));
                        }
                    }

                    if is_expanded {
                        let display_siblings: Vec<crate::huggingface::HfSibling> = {
                            let guard = lock(&*app.hf_details);
                            guard.get(&model.id)
                                .map(|m| m.siblings.clone())
                                .unwrap_or_else(|| model.siblings.clone())
                        };
                        ui.indent("files", |ui| {
                            for sib in &display_siblings {
                                if !sib.rfilename.ends_with(".gguf") { continue; }
                                let dl_key = format!("{}:{}", model.id, sib.rfilename);
                                let file_is_downloading = app.hf_downloading.as_deref() == Some(&dl_key);
                                let size_str = sib.size.map(format_size).unwrap_or_default();

                                ui.horizontal(|ui| {
                                    ui.label(format!("📄 {}  {}", sib.rfilename, size_str));
                                    if app.hf_downloading.is_none() {
                                        if ui.button(lang.t("btn_download")).clicked() {
                                            let mid = model.id.clone();
                                            let fname = sib.rfilename.clone();
                                            let dl_progress2 = Arc::clone(&app.hf_dl_progress);
                                            let dl_done = Arc::clone(&app.hf_dl_done);
                                            let dest = std::path::PathBuf::from(&app.data.paths.model_dir);
                                            let temp = std::path::PathBuf::from(&app.data.paths.temp_dir).join("hf_dl");
                                            let cancel = Arc::clone(&app.hf_cancel);
                                            app.hf_cancel.store(false, Ordering::Relaxed);
                                            app.hf_downloading = Some(dl_key);
                                            tokio::spawn(async move {
                                                *lock(&*dl_progress2) = None;
                                                let result = download_specific_gguf(&mid, &fname, &dest, &temp, Some(cancel), Some(Arc::clone(&dl_progress2))).await;
                                                *lock(&*dl_progress2) = None;
                                                match result {
                                                    Ok(fn_ok) => {
                                                        *lock(&*dl_done) = Some(format!("{} ({})", mid, fn_ok));
                                                    }
                                                    Err(e) => {
                                                        *lock(&*dl_done) = Some(format!("{} ❌ {}", mid, e));
                                                    }
                                                }
                                            });
                                        }
                                    } else if file_is_downloading {
                                        ui.label("🔄");
                                        if ui.button(lang.t("btn_cancel")).clicked() {
                                            app.hf_cancel.store(true, Ordering::Relaxed);
                                        }
                                    }
                                });
                            }
                        });
                    }
                    ui.separator();
                }
            });
        }
    });
}
