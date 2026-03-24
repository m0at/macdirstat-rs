#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod console_log;
mod fuzzy;
mod file_types;
mod scanner;
mod search;
mod settings;
mod sidebar;
mod treemap;
mod types;

use app::MacDirStatApp;

fn main() -> eframe::Result<()> {
    // Use 4 cores for parallel scanning
    rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build_global()
        .ok();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MacDirStat")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "MacDirStat",
        native_options,
        Box::new(|cc| Ok(Box::new(MacDirStatApp::new(cc)))),
    )
}
