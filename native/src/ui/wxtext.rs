//! Plain-English rendering of a coded report, with a one-click switch to the raw text.

use crate::app::Decode;
use crate::theme::{self, Tokens};
use crate::wx::decode::{decode_metar, decode_pirep, decode_taf, describe_gairmet, describe_sigmet, expand, str_of};
use egui::RichText;

pub enum Kind<'a> {
    Metar(&'a str),
    Taf(&'a str),
    Pirep(&'a str),
    Sigmet(&'a serde_json::Value),
    Gairmet(&'a serde_json::Value),
    Remark(&'a str),
}

impl<'a> From<&'a Decode> for Kind<'a> {
    fn from(d: &'a Decode) -> Self {
        match d {
            Decode::Metar(s) => Kind::Metar(s),
            Decode::Pirep(s) => Kind::Pirep(s),
            Decode::Sigmet(v) => Kind::Sigmet(v),
            Decode::Gairmet(v) => Kind::Gairmet(v),
        }
    }
}

/// Returns true when the user toggled raw / plain-English.
pub fn show(ui: &mut egui::Ui, t: &Tokens, kind: Kind, raw_mode: bool, compact: bool) -> bool {
    let (blocks, raw): (Vec<(Option<String>, Vec<String>)>, String) = match &kind {
        Kind::Metar(r) => (vec![(None, decode_metar(r))], r.to_string()),
        Kind::Taf(r) => (decode_taf(r).into_iter().map(|p| (Some(p.title), p.lines)).collect(), r.to_string()),
        Kind::Pirep(r) => (vec![(None, decode_pirep(r))], r.to_string()),
        Kind::Sigmet(v) => {
            let (mut s, text) = describe_sigmet(v);
            s.push(text);
            (vec![(None, s)], str_of(&v["rawAirSigmet"]))
        }
        Kind::Gairmet(v) => (vec![(None, vec![describe_gairmet(v)])], String::new()),
        Kind::Remark(r) => (vec![(None, vec![expand(r)])], String::new()),
    };
    let show_raw = (raw_mode && !raw.is_empty()) || blocks.iter().all(|b| b.1.is_empty());
    if show_raw {
        let text = if matches!(kind, Kind::Taf(_)) {
            let re = regex::Regex::new(r" (FM|BECMG|TEMPO|PROB)").unwrap();
            re.replace_all(&raw, "\n  $1").into_owned()
        } else {
            raw.clone()
        };
        ui.add(egui::Label::new(RichText::new(text).font(theme::mono(11.5)).color(t.text2)).wrap());
    } else {
        for (title, lines) in &blocks {
            if let Some(tt) = title {
                ui.label(RichText::new(tt).font(theme::semibold(12.0)));
            }
            if compact {
                ui.add(egui::Label::new(RichText::new(lines.join(". ").replace("..", ".")).size(12.0).color(t.text2)).wrap());
            } else {
                for l in lines {
                    ui.horizontal_top(|ui| {
                        ui.label(RichText::new("•").color(t.text3));
                        ui.add(egui::Label::new(RichText::new(l).size(12.0)).wrap());
                    });
                }
            }
        }
    }
    let mut toggled = false;
    if !raw.is_empty() && !matches!(kind, Kind::Remark(_)) {
        if ui.add(egui::Button::new(RichText::new(if raw_mode { "Plain English" } else { "Raw" }).size(10.5).color(t.accent)).frame(false)).clicked() {
            toggled = true;
        }
    }
    toggled
}
