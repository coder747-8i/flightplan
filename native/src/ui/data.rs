//! Install or update the chart & airport data pack: download it, import a .zip, or copy it
//! from a folder. The backend does the work; this shows status and progress.

use super::widgets::{self, faint, Tone};
use crate::app::App;
use crate::theme;
use egui::{vec2, CornerRadius, Frame, RichText};
use egui_phosphor::regular as ph;

fn mb(n: u64) -> String {
    let v = n as f64 / 1e6;
    if v > 100.0 { format!("{v:.0}") } else { format!("{v:.1}") }
}

pub fn show(app: &mut App, ui: &mut egui::Ui, compact: bool) {
    let t = app.t;
    let job = app.data_job.clone();
    let busy = matches!(job.state, "downloading" | "uploading" | "copying" | "extracting" | "checking");
    match &app.data_info {
        Some(info) => {
            let expired = app.data_expired();
            let line = match (&info.cycle, &info.expires) {
                (Some(c), Some(e)) => format!("FAA cycle {c} · {} {e}", if expired { "expired" } else { "good until" }),
                (Some(c), None) => format!("FAA cycle {c}"),
                _ => "Built locally (no cycle date)".into(),
            };
            widgets::status(ui, &t, if expired { Tone::Warn } else { Tone::Ok }, if expired { ph::WARNING } else { ph::CHECK_CIRCLE }, &[if expired { "Chart data is out of date".into() } else { "Chart data installed".into() }, line]);
        }
        None => widgets::status(ui, &t, Tone::Accent, ph::CLOUD_ARROW_DOWN, &["Chart data needed".into(), "Airports, navaids, airspace, obstacles and chart tiles: one download of about 180 MB, updated every 28 days.".into()]),
    }
    if let Some(c) = &app.sync.cfg {
        widgets::para(ui, format!("{}  Updates come from your Flightplan server ({}) when it has a new cycle.", ph::CLOUD_CHECK, c.url), t.text2, 11.5);
        if let Some(w) = &app.sync.server_waiting {
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::Spinner::new().size(12.0));
                ui.label(RichText::new(w).size(11.5).color(t.text2));
            });
        }
    }
    ui.add_space(8.0);
    let others_id = egui::Id::new("data-other-ways");
    if !compact && !busy {
        // the first-run screen: a Flightplan server (the Proxmox LXC) builds the data, so get it from there
        if !app.sync.connected() {
            Frame::new().stroke(egui::Stroke::new(1.0, t.accent)).corner_radius(CornerRadius::same(11)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(format!("{}  From my Flightplan server", ph::HARD_DRIVES)).font(theme::semibold(13.0)));
                faint(ui, &t, "Your server builds the chart data every FAA cycle; this computer downloads it from there, and your saved plans and aircraft sync with the web app. Get a pairing code in the web app: Settings → Sign-in → Connect a desktop app.");
                ui.add_space(4.0);
                super::settings::sync(app, ui, false);
            });
            ui.add_space(10.0);
            faint(ui, &t, "Or install it on this computer:");
            ui.add_space(4.0);
        } else if app.data_info.is_none() && !ui.data(|d| d.get_temp::<bool>(others_id)).unwrap_or(false) {
            if widgets::link(ui, &t, "Install chart data another way").clicked() {
                ui.data_mut(|d| d.insert_temp(others_id, true));
            }
            return;
        }
    }
    if busy {
        Frame::new().stroke(egui::Stroke::new(1.0, t.line_strong)).corner_radius(CornerRadius::same(11)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(14.0));
                let phase = match job.state {
                    "downloading" => "Downloading",
                    "uploading" => "Reading file",
                    "copying" => "Copying",
                    "extracting" => "Unpacking",
                    _ => "Checking",
                };
                ui.label(RichText::new(format!("{phase}…")).font(theme::semibold(13.0)));
                if job.total > 0 {
                    faint(ui, &t, format!("{} / {} MB", mb(job.done), mb(job.total)));
                } else if job.done > 0 {
                    faint(ui, &t, format!("{} MB", mb(job.done)));
                }
                if job.state != "checking" {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::ghost(ui, &t, format!("{} Cancel", ph::X)).clicked() {
                            flightplan_server::setup::cancel_job(&app.be.server);
                        }
                    });
                }
            });
            let frac = if job.total > 0 { job.done as f32 / job.total as f32 } else { 0.0 };
            ui.add(egui::ProgressBar::new(frac).desired_height(6.0).fill(t.accent).animate(job.total == 0));
        });
        return;
    }
    if local_build_panel(app, ui) {
        return;
    }
    if job.state == "done" && app.data_changed {
        widgets::para(ui, format!("{} Installed. The chart now uses the new data.", ph::CHECK_CIRCLE), t.ok, 12.0);
    }
    if job.state == "error" {
        widgets::para(ui, &job.message, t.bad, 12.0);
    }
    let cols = if compact { 1 } else { 2 };
    let w = (ui.available_width() - 8.0 * (cols as f32 - 1.0)) / cols as f32;
    let mut act = None;
    let default_url = app.be.cfg.default_data_url.clone();
    let url_id = egui::Id::new("data-url");
    let mut url: String = ui.data(|d| d.get_temp(url_id)).unwrap_or_else(|| app.prefs.data_url.clone().unwrap_or(default_url.clone()));
    let mut edit_url: bool = ui.data(|d| d.get_temp(url_id.with("edit"))).unwrap_or(false);
    let grid = |ui: &mut egui::Ui, add: &mut dyn FnMut(&mut egui::Ui, usize)| {
        if compact {
            for i in 0..4 {
                add(ui, i);
                ui.add_space(6.0);
            }
        } else {
            for row in [[0, 1], [2, 3]] {
                ui.horizontal_top(|ui| {
                    for i in row {
                        add(ui, i);
                    }
                });
                ui.add_space(8.0);
            }
        }
    };
    grid(ui, &mut |ui: &mut egui::Ui, i: usize| {
        Frame::new().stroke(egui::Stroke::new(1.0, t.line_strong)).corner_radius(CornerRadius::same(11)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            ui.set_width(w - 26.0);
            ui.set_min_height(if compact { 0.0 } else { 120.0 });
            ui.vertical(|ui| match i {
                0 => {
                    ui.label(RichText::new(format!("{}  {}", ph::CLOUD_ARROW_DOWN, if app.data_info.is_some() { "Download the latest" } else { "Download" })).font(theme::semibold(13.0)));
                    faint(ui, &t, "From the Flightplan data release.");
                    if edit_url {
                        ui.add(egui::TextEdit::singleline(&mut url).font(theme::mono(10.5)).desired_width(f32::INFINITY));
                    } else if widgets::link(ui, &t, "Change address").clicked() {
                        edit_url = true;
                    }
                    ui.add_space(4.0);
                    if widgets::primary(ui, &t, format!("{}  Download", ph::DOWNLOAD_SIMPLE)).clicked() {
                        act = Some(0);
                    }
                }
                1 => {
                    ui.label(RichText::new(format!("{}  Import a file", ph::FILE_ZIP)).font(theme::semibold(13.0)));
                    faint(ui, &t, "A flightplan-data.zip you downloaded or were given.");
                    ui.add_space(4.0);
                    if widgets::button(ui, format!("{}  Choose .zip", ph::FILE_ZIP)).clicked() {
                        act = Some(1);
                    }
                }
                3 => {
                    ui.label(RichText::new(format!("{}  Build on this computer", ph::HAMMER)).font(theme::semibold(13.0)));
                    faint(ui, &t, "From the FAA's current 28-day cycle. About 15 minutes and 2 GB of free space; no download address needed.");
                    ui.add_space(4.0);
                    if widgets::button(ui, format!("{}  Start build", ph::HAMMER)).clicked() {
                        act = Some(3);
                    }
                }
                _ => {
                    ui.label(RichText::new(format!("{}  Use a folder", ph::FOLDER_OPEN)).font(theme::semibold(13.0)));
                    faint(ui, &t, "An unzipped pack, or a Flightplan checkout with data/build and tiles.");
                    ui.add_space(4.0);
                    if widgets::button(ui, format!("{}  Choose folder", ph::FOLDER_OPEN)).clicked() {
                        act = Some(2);
                    }
                }
            });
        });
    });
    ui.data_mut(|d| {
        d.insert_temp(url_id, url.clone());
        d.insert_temp(url_id.with("edit"), edit_url);
    });
    let result = match act {
        Some(0) => {
            let u = url.trim().to_string();
            app.prefs.data_url = if u == default_url { None } else { Some(u.clone()) };
            app.store.save_prefs(&app.prefs);
            let st = app.be.server.clone();
            let _g = app.be.rt().enter();
            Some(flightplan_server::setup::start_download(st, u))
        }
        Some(1) => rfd::FileDialog::new().set_title("Choose a Flightplan data pack").add_filter("Data pack", &["zip"]).pick_file().map(|p| {
            let st = app.be.server.clone();
            let _g = app.be.rt().enter();
            flightplan_server::setup::start_zip(st, p)
        }),
        Some(2) => rfd::FileDialog::new().set_title("Choose a folder with Flightplan chart data").pick_folder().map(|p| {
            let st = app.be.server.clone();
            let _g = app.be.rt().enter();
            flightplan_server::setup::start_folder(st, p)
        }),
        Some(3) => {
            let missing = crate::localbuild::missing_tools();
            if missing.is_empty() {
                Some(app.local_build.start(&app.be.data_dir.join("builder")))
            } else {
                let names: Vec<&str> = missing.iter().map(|m| m.0).collect();
                Some(Err(anyhow::anyhow!("Building needs {}. Install: {}", names.join(", "), missing[0].1)))
            }
        }
        _ => None,
    };
    if let Some(Err(e)) = result {
        app.toast(format!("{e:#}"));
    }
    if !compact {
        faint(ui, &t, format!("Stored in {}", app.be.data_dir.display()));
    }
    let _ = vec2(0.0, 0.0);
}

/// Progress of a local build (or how the last one ended). Returns true while it runs.
fn local_build_panel(app: &mut App, ui: &mut egui::Ui) -> bool {
    use crate::localbuild::State;
    let t = app.t;
    let (state, step, log, started) = {
        let g = app.local_build.0.lock();
        (g.state.clone(), g.step.clone(), g.log.clone(), g.started)
    };
    match state {
        State::Idle | State::Built(_) => false,
        State::Cancelled => {
            widgets::para(ui, "The local build was cancelled.", t.text2, 12.0);
            false
        }
        State::Failed(msg) => {
            widgets::status(ui, &t, Tone::Bad, ph::OCTAGON, &[msg]);
            if ui.add(egui::Button::new(RichText::new("Show the build log").size(11.5)).frame(false)).clicked() {
                let id = egui::Id::new("show-build-log");
                let on = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
                ui.data_mut(|d| d.insert_temp(id, !on));
            }
            if ui.data(|d| d.get_temp::<bool>(egui::Id::new("show-build-log"))).unwrap_or(false) {
                log_box(ui, &t, &log, 30);
            }
            ui.add_space(8.0);
            false
        }
        State::Running => {
            Frame::new().stroke(egui::Stroke::new(1.0, t.line_strong)).corner_radius(CornerRadius::same(11)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(14.0));
                    ui.label(RichText::new(format!("Building chart data · {step}")).font(theme::semibold(13.0)));
                    if let Some(s) = started {
                        let m = s.elapsed().as_secs() / 60;
                        faint(ui, &t, format!("{m}:{:02}", s.elapsed().as_secs() % 60));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::ghost(ui, &t, format!("{} Cancel", ph::X)).clicked() {
                            app.local_build.cancel();
                        }
                    });
                });
                faint(ui, &t, "Downloading the FAA cycle and building the chart. It installs itself when it's done; you can keep using the app.");
                log_box(ui, &t, &log, 8);
            });
            true
        }
    }
}

fn log_box(ui: &mut egui::Ui, t: &crate::theme::Tokens, log: &[String], lines: usize) {
    let tail = &log[log.len().saturating_sub(lines)..];
    Frame::new().fill(t.surface2).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        for l in tail {
            ui.add(egui::Label::new(RichText::new(l).font(theme::mono(10.5)).color(t.text2)).truncate());
        }
    });
}
