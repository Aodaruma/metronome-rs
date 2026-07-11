mod app;
mod audio;
mod config;
mod fonts;
mod menu;
mod platform;
mod shortcuts;
mod theme;

use app::MetronomeApp;

fn main() -> eframe::Result {
    let start_hidden = std::env::args().any(|argument| argument == "--background");
    let icon = eframe::icon_data::from_png_bytes(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/metronome-rs.png"
    )))
    .expect("embedded application icon should be a valid PNG");
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([520.0, 620.0])
            .with_min_inner_size([420.0, 560.0])
            .with_visible(!start_hidden)
            .with_icon(icon)
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "metronome-rs",
        native_options,
        Box::new(move |cc| Ok(Box::new(MetronomeApp::new(cc, start_hidden)))),
    )
}
