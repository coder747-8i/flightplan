//! Flightplan desktop app entry point.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Flightplan")
            .with_app_id("io.github.coder747_8i.flightplan")
            .with_inner_size([1440.0, 920.0])
            .with_min_inner_size([960.0, 620.0])
            .with_icon(std::sync::Arc::new(flightplan::icon()))
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "Flightplan",
        options,
        Box::new(|cc| match flightplan::app::App::new(cc) {
            Ok(a) => Ok(Box::new(a) as Box<dyn eframe::App>),
            Err(e) => Err(format!("{e:#}").into()),
        }),
    )
}
