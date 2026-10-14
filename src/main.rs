#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod fsutil;
mod i18n;
mod tab;
mod shell;
mod win;

use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    let start = std::env::args().nth(1).map(PathBuf::from);
    // İlk çalıştırmada (kayıtlı pencere konumu yoksa) ekranı kaplayarak aç
    let first_run = eframe::storage_dir("Swift Explorer").is_none_or(|d| !d.join("app.ron").exists());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_maximized(first_run)
            .with_title("Swift Explorer")
            // Başlık çubuğu yok: sekmeler ve pencere düğmeleri Explorer gibi en üstte
            .with_decorations(false)
            .with_inner_size([1060.0, 640.0])
            .with_min_inner_size([640.0, 400.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native("Swift Explorer", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, start)))))
}
