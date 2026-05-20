use eframe::egui;
use egui::ScrollArea;
use crate::logger::{self, Level};
use super::LlamaApp;

pub fn render(ctx: &egui::Context, app: &mut LlamaApp) {
    let lang = app.data.settings.lang;
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut app.active_log_tab, "Tous".to_string(), lang.t("tab_logs_all"));
            ui.selectable_value(&mut app.active_log_tab, "CLI".to_string(), "💬 CLI");
            ui.selectable_value(&mut app.active_log_tab, "SRV".to_string(), lang.t("tab_logs_srv"));
            if ui.button(lang.t("btn_clear")).clicked() { app.logs.clear(); }
            ui.separator();
            if ui.selectable_label(app.log_show_file, lang.t("btn_file")).clicked() {
                app.log_show_file = !app.log_show_file;
            }
            if app.log_show_file {
                if ui.button(lang.t("btn_refresh")).clicked() { }
            }
            ui.separator();
            ui.label(lang.t("lbl_filters"));
            let mut chg = false;
            chg |= ui.checkbox(&mut app.data.settings.log_trace_enabled, lang.t("log_trace")).changed();
            chg |= ui.checkbox(&mut app.data.settings.log_debug_enabled, lang.t("log_debug")).changed();
            chg |= ui.checkbox(&mut app.data.settings.log_info_enabled, lang.t("log_info")).changed();
            chg |= ui.checkbox(&mut app.data.settings.log_warn_enabled, lang.t("log_warn")).changed();
            chg |= ui.checkbox(&mut app.data.settings.log_error_enabled, lang.t("log_error")).changed();
            if chg { app.save(); }
        });
        
        ui.separator();
        
        let source_label = if app.log_show_file { lang.t("tab_logs_file") } else { lang.t("tab_logs_buffer") };
        ui.label(source_label);
        
        let scroll_bot = app.scroll_to_bottom;
        if scroll_bot { app.scroll_to_bottom = false; }
        
        let mut scroll = ScrollArea::vertical().auto_shrink([false, false]);
        if !app.log_show_file && scroll_bot {
            scroll = scroll.stick_to_bottom(true);
        }
        scroll.show(ui, |ui| {
            if app.log_show_file {
                let lines = logger::read_log_content(2000);
                let prefix = match app.active_log_tab.as_str() {
                    "CLI" => "[CLI]",
                    "SRV" => "[SRV]",
                    _ => ""
                };
                for line in &lines {
                    if !prefix.is_empty() && !line.contains(prefix) { continue; }
                    let level = if line.contains("[ERROR]") { Level::Error }
                        else if line.contains("[WARN]") { Level::Warn }
                        else if line.contains("[DEBUG]") { Level::Debug }
                        else if line.contains("[TRACE]") { Level::Trace }
                        else { Level::Info };
                    let color = match level {
                        Level::Error => egui::Color32::RED,
                        Level::Warn => egui::Color32::YELLOW,
                        Level::Debug => egui::Color32::LIGHT_BLUE,
                        Level::Trace => egui::Color32::GRAY,
                        _ => egui::Color32::WHITE,
                    };
                    ui.colored_label(color, line);
                }
            } else {
                let settings = &app.data.settings;
                let prefix = match app.active_log_tab.as_str() {
                    "CLI" => "[CLI]",
                    "SRV" => "[SRV]",
                    _ => ""
                };
                for entry in &app.logs {
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
            }
        });
    });
}
