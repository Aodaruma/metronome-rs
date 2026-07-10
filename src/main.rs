mod app;
mod audio;
mod config;
mod fonts;
mod menu;

use app::MetronomeApp;

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([520.0, 620.0])
            .with_min_inner_size([420.0, 560.0])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "metronome-rs",
        native_options,
        Box::new(|cc| Ok(Box::new(MetronomeApp::new(cc)))),
    )
}
