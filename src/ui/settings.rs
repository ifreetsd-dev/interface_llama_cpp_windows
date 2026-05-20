use eframe::egui;
use egui::ScrollArea;
use crate::lang::Lang;
use crate::logger::Level;
use super::LlamaApp;

pub fn render(ctx: &egui::Context, app: &mut LlamaApp) {
    let lang = app.data.settings.lang;
    egui::CentralPanel::default().show(ctx, |ui| {
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.heading(lang.t("section_paths"));
            ui.separator();

            let edit_file = |ui: &mut egui::Ui, label: &str, path: &mut String| {
                ui.label(label);
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(path);
                    if ui.button(lang.t("btn_folder")).clicked() {
                        if let Some(f) = rfd::FileDialog::new().pick_file() {
                            *path = f.display().to_string();
                        }
                    }
                });
            };

            let edit_dir = |ui: &mut egui::Ui, label: &str, path: &mut String| {
                ui.label(label);
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(path);
                    if ui.button(lang.t("btn_folder")).clicked() {
                        if let Some(d) = rfd::FileDialog::new().pick_folder() {
                            *path = d.display().to_string();
                        }
                    }
                });
            };

            ui.label(lang.t("lbl_cuda"));
            edit_file(ui, "  CLI :", &mut app.data.paths.cli_cuda);
            edit_file(ui, "  SRV :", &mut app.data.paths.server_cuda);

            ui.label(lang.t("lbl_vulkan"));
            edit_file(ui, "  CLI :", &mut app.data.paths.cli_vulkan);
            edit_file(ui, "  SRV :", &mut app.data.paths.server_vulkan);

            edit_dir(ui, lang.t("lbl_dir_models"), &mut app.data.paths.model_dir);
            edit_dir(ui, lang.t("lbl_dir_cuda"), &mut app.data.paths.cuda_dir);
            edit_dir(ui, lang.t("lbl_dir_vulkan"), &mut app.data.paths.vulkan_dir);
            edit_dir(ui, lang.t("lbl_dir_temp"), &mut app.data.paths.temp_dir);
            edit_dir(ui, lang.t("lbl_dir_backups"), &mut app.data.paths.sauvegardes_dir);

            if ui.button(lang.t("btn_save_paths")).clicked() {
                app.save();
                app.add_log(Level::Info, "✅ Chemins sauvegardés");
            }

            ui.separator();
            ui.heading(lang.t("section_lang"));
            ui.separator();

            ui.horizontal(|ui| {
                ui.label(lang.t("lbl_lang"));
                egui::ComboBox::from_id_source("lang_select")
                    .selected_text(match app.data.settings.lang {
                        Lang::Fr => "Français",
                        Lang::En => "English",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut app.data.settings.lang, Lang::Fr, "Français");
                        ui.selectable_value(&mut app.data.settings.lang, Lang::En, "English");
                    });
                if ui.button(lang.t("btn_apply")).clicked() {
                    app.save();
                }
            });
        });
    });
}
