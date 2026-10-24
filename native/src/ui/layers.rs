//! Layers panel: chart layer groups and presets.

use super::widgets::{self, section};
use crate::app::{App, Panel};
use crate::map::style::Group;
use egui::RichText;

pub fn panel(app: &mut App, ctx: &egui::Context) {
    let t = app.t;
    let close = widgets::side_panel(ctx, &t, "panel-layers", "Layers", "What the chart shows", 340.0, app, |_, _| false, |ui, app| {
        let p = app.palette.clone();
        ui.horizontal(|ui| {
            use Group::*;
            let presets: [(&str, &[Group]); 3] = [
                ("VFR", &[Terrain, Basemap, ClassB, ClassC, ClassD, ClassE, Restricted, Moa, WarningAlert, AirspaceLabels, AirportsPublic, AirportsPrivate, Runways, Navaids, VfrWaypoints, Obstacles]),
                ("IFR low", &[Terrain, ClassB, ClassC, ClassD, Restricted, Moa, WarningAlert, AirportsPublic, Runways, Navaids, IfrFixes, AirwaysLow]),
                ("Minimal", &[Terrain, ClassB, ClassC, ClassD, Restricted, AirportsPublic]),
            ];
            for (label, on) in presets {
                if widgets::small_button(ui, label).clicked() {
                    let keys: Vec<Group> = app.saved.layers.0.keys().copied().collect();
                    for g in keys {
                        app.saved.layers.set(g, on.contains(&g));
                    }
                }
            }
        });
        let groups: [(&str, Vec<(Group, &str, &str, Option<egui::Color32>)>); 4] = [
            (
                "Airspace",
                vec![
                    (Group::ClassB, "Class B", "", Some(p.class_b.egui())),
                    (Group::ClassC, "Class C", "", Some(p.class_c.egui())),
                    (Group::ClassD, "Class D", "", Some(p.class_d.egui())),
                    (Group::ClassE, "Class E", "Surface areas and 700 ft AGL floors", Some(p.class_e.egui())),
                    (Group::ClassEhigh, "Class E, 1200 AGL & en route", "Usually hidden on sectionals", None),
                    (Group::ModeC, "Mode C veil", "30 NM rings around Class B", None),
                    (Group::Restricted, "Restricted & prohibited", "R- and P- areas", Some(p.restricted.egui())),
                    (Group::Moa, "MOAs", "Military operations areas", Some(p.moa.egui())),
                    (Group::WarningAlert, "Warning & alert areas", "", Some(p.warning.egui())),
                    (Group::AirspaceLabels, "Altitude labels", "", None),
                ],
            ),
            (
                "Airports",
                vec![
                    (Group::AirportsPublic, "Public & military", "", Some(p.towered.egui())),
                    (Group::AirportsPrivate, "Private", "", Some(p.nontowered.egui())),
                    (Group::Heliports, "Heliports", "", None),
                    (Group::Runways, "Runway diagrams", "Zoomed in", None),
                ],
            ),
            (
                "Navigation",
                vec![
                    (Group::Navaids, "VOR, NDB & compass roses", "", None),
                    (Group::VfrWaypoints, "VFR waypoints", "", None),
                    (Group::IfrFixes, "IFR fixes", "", None),
                    (Group::AirwaysLow, "Low airways (V, T)", "", None),
                    (Group::AirwaysHigh, "High airways (J, Q)", "", None),
                ],
            ),
            (
                "Map",
                vec![(Group::Terrain, "Terrain relief", "", None), (Group::Obstacles, "Obstacles", "200 ft AGL and taller", None), (Group::Basemap, "Roads, cities & borders", "", None)],
            ),
        ];
        for (title, items) in groups {
            section(ui, &t, title);
            for (g, label, hint, swatch) in items {
                let mut on = app.saved.layers.on(g);
                if widgets::toggle_row(ui, &t, &mut on, label, hint, swatch) {
                    app.saved.layers.set(g, on);
                }
            }
        }
        ui.add_space(8.0);
        ui.label(RichText::new("Chart data: FAA NASR and AIS open data. Basemap © OpenStreetMap contributors, OpenFreeMap. Terrain: Mapzen / AWS.").size(10.5).color(t.text3));
    });
    if close {
        app.panel = Panel::None;
    }
}
