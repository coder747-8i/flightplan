//! Hazard advisor list.

use crate::nav::hazards::Hazard;
use crate::nav::profile::Level;
use crate::theme::{self, Tokens};
use egui::{CornerRadius, Frame, Margin, RichText, Stroke};
use egui_phosphor::regular as ph;

pub fn list(ui: &mut egui::Ui, t: &Tokens, hazards: &[Hazard], compact: bool) {
    if hazards.is_empty() {
        super::widgets::para(ui, "No hazards found for this plan. Still get an official briefing before you fly.", t.ok, 12.5);
        return;
    }
    let id = ui.id().with("hz-all");
    let mut all = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    let shown = if compact && !all { &hazards[..hazards.len().min(4)] } else { hazards };
    for h in shown {
        let (c, icon) = match h.level {
            Level::Warning => (t.bad, ph::OCTAGON),
            Level::Caution => (t.warn, ph::WARNING),
            Level::Info => (t.blue, ph::INFO),
        };
        let r = Frame::new().fill(if h.level == Level::Info { t.surface2 } else { c.gamma_multiply(0.07) }).corner_radius(CornerRadius::same(9)).inner_margin(Margin { left: 12, right: 10, top: 8, bottom: 8 }).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.label(RichText::new(icon).color(c).size(15.0));
                ui.vertical(|ui| {
                    let at = h.at.map(|d| format!(" · {d:.0} NM")).unwrap_or_default();
                    ui.add(egui::Label::new(RichText::new(format!("{}{at}", h.title)).font(theme::semibold(12.5))).wrap());
                    if !h.detail.is_empty() && !compact {
                        ui.add(egui::Label::new(RichText::new(&h.detail).size(12.0).color(t.text2)).wrap());
                    }
                    ui.label(RichText::new(h.category).size(10.5).color(t.text3));
                });
            });
        });
        let rect = r.response.rect;
        ui.painter().line_segment([rect.left_top() + egui::vec2(1.5, 4.0), rect.left_bottom() + egui::vec2(1.5, -4.0)], Stroke::new(3.0, c));
        ui.add_space(4.0);
    }
    if compact && hazards.len() > 4 {
        let label = if all { "Show fewer".to_string() } else { format!("Show all {}", hazards.len()) };
        if super::widgets::link(ui, t, label).clicked() {
            all = !all;
            ui.data_mut(|d| d.insert_temp(id, all));
        }
    }
}
