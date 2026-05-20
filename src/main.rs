#![windows_subsystem = "windows"]

mod config; 
mod disk;
mod downloader;
mod error;
mod huggingface;
mod logger; 
mod process;
mod vram;
mod lang;
mod ui;

use std::path::PathBuf;
use ui::LlamaApp;

fn main() -> eframe::Result<()> {
    logger::init();

    // Runtime Tokio multi-threadé (threads de travail en arrière-plan)
    let rt = tokio::runtime::Runtime::new()
        .expect("❌ Échec création runtime Tokio");
    let _guard = rt.enter();

    let config_path = {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));
        exe_dir.join("app_data.json")
    };

    logger::log(logger::Level::Info, "main", &format!("Chemin config: {:?}", config_path));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("🦙 Interface - Llama Control Panel")
            .with_resizable(true)
            .with_maximized(true),
        ..Default::default()
    };

    eframe::run_native(
        "Interface",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                LlamaApp::new(cc)
            })) {
                Ok(app) => Box::new(app) as Box<dyn eframe::App>,
                Err(e) => {
                    eprintln!("❌ Panic dans LlamaApp::new(): {:?}", e);
                    Box::new(ErrorApp { message: format!("Erreur au démarrage: {:?}", e) }) as Box<dyn eframe::App>
                }
            }
        }),
    )
}

// UI de secours en cas d'erreur au démarrage
struct ErrorApp {
    message: String,
}

impl eframe::App for ErrorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.colored_label(egui::Color32::RED, "❌ Erreur au démarrage");
            ui.monospace(&self.message);
            ui.separator();
            if ui.button("Quitter").clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }
}