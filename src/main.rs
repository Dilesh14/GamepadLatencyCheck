#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod export;
mod input;
mod reaction;
mod stats;

use app::LatencyApp;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 760.0])
            .with_min_inner_size([880.0, 600.0])
            .with_title("Gamepad Latency Check"),
        ..Default::default()
    };

    eframe::run_native(
        "Gamepad Latency Check",
        options,
        Box::new(|cc| Ok(Box::new(LatencyApp::new(cc)))),
    )
}
