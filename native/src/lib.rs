//! Flightplan desktop: a native flight planner (Windows, macOS, Linux).

pub mod app;
pub mod backend;
pub mod history;
pub mod io;
pub mod localbuild;
pub mod map;
pub mod model;
pub mod nav;
pub mod store;
pub mod theme;
pub mod ui;
pub mod wx;

pub fn icon() -> egui::IconData {
    let img = image::load_from_memory(include_bytes!("../assets/icons/128x128@2x.png")).map(|i| i.to_rgba8()).ok();
    match img {
        Some(i) => egui::IconData { width: i.width(), height: i.height(), rgba: i.into_raw() },
        None => egui::IconData::default(),
    }
}
