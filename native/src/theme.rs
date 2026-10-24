//! UI colours, fonts and egui styling. Day and night themes follow the chart's.

use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle};
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    pub night: bool,
    pub bg: Color32,
    pub surface: Color32,
    pub surface_solid: Color32,
    pub surface2: Color32,
    pub line: Color32,
    pub line_strong: Color32,
    pub text: Color32,
    pub text2: Color32,
    pub text3: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub blue: Color32,
    pub magenta: Color32,
    pub ok: Color32,
    pub warn: Color32,
    pub bad: Color32,
}

const fn rgb(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub const DAY: Tokens = Tokens {
    night: false,
    bg: rgb(0xf6f5f1),
    surface: Color32::from_rgba_premultiplied(219, 219, 219, 219),
    surface_solid: rgb(0xffffff),
    surface2: rgb(0xf1f0ec),
    line: Color32::from_rgba_premultiplied(2, 2, 3, 23),
    line_strong: Color32::from_rgba_premultiplied(3, 4, 5, 41),
    text: rgb(0x1b1f26),
    text2: rgb(0x5c6370),
    text3: rgb(0x8b919c),
    accent: rgb(0xd4148c),
    accent_soft: Color32::from_rgba_premultiplied(21, 2, 14, 26),
    blue: rgb(0x2c63b8),
    magenta: rgb(0xa33a86),
    ok: rgb(0x1f9d6a),
    warn: rgb(0xd48a0b),
    bad: rgb(0xd23a3a),
};

pub const NIGHT: Tokens = Tokens {
    night: true,
    bg: rgb(0x0f1318),
    surface: Color32::from_rgba_premultiplied(19, 23, 29, 219),
    surface_solid: rgb(0x161b22),
    surface2: rgb(0x1e252e),
    line: Color32::from_rgba_premultiplied(18, 18, 18, 18),
    line_strong: Color32::from_rgba_premultiplied(36, 36, 36, 36),
    text: rgb(0xe7eaee),
    text2: rgb(0xa2a9b4),
    text3: rgb(0x6f7782),
    accent: rgb(0xff3fb4),
    accent_soft: Color32::from_rgba_premultiplied(36, 9, 25, 36),
    blue: rgb(0x6ea3ff),
    magenta: rgb(0xe77fd0),
    ok: rgb(0x2fbf86),
    warn: rgb(0xe8a23a),
    bad: rgb(0xef5b5b),
};

pub fn tokens(night: bool) -> Tokens {
    if night { NIGHT } else { DAY }
}

/// Flight category colours (VFR / MVFR / IFR / LIFR).
pub fn flt_cat(cat: &str) -> Color32 {
    match cat {
        "VFR" => rgb(0x1f9d4a),
        "MVFR" => rgb(0x2c63b8),
        "IFR" => rgb(0xd23a3a),
        "LIFR" => rgb(0xb03ab8),
        _ => rgb(0x8b919c),
    }
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let add = |fonts: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.into(), Arc::new(FontData::from_static(bytes)));
    };
    add(&mut fonts, "Inter", include_bytes!("../assets/fonts/Inter-Regular.ttf"));
    add(&mut fonts, "Inter-SemiBold", include_bytes!("../assets/fonts/Inter-SemiBold.ttf"));
    add(&mut fonts, "Inter-Bold", include_bytes!("../assets/fonts/Inter-Bold.ttf"));
    add(&mut fonts, "Inter-Italic", include_bytes!("../assets/fonts/Inter-Italic.ttf"));
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    let fallbacks: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    // Inter first, then phosphor icons and egui's defaults for anything Inter lacks
    // the bundled Inter has its private-use-area glyphs removed (see assets/fonts/README.md),
    // so phosphor's icons there are not shadowed by Inter's alternates
    let family = |primary: &str| {
        let mut v = vec![primary.to_string()];
        v.extend(fallbacks.iter().cloned());
        v
    };
    fonts.families.insert(FontFamily::Proportional, family("Inter"));
    fonts.families.insert(FontFamily::Name("semibold".into()), family("Inter-SemiBold"));
    fonts.families.insert(FontFamily::Name("bold".into()), family("Inter-Bold"));
    fonts.families.insert(FontFamily::Name("italic".into()), family("Inter-Italic"));
    ctx.set_fonts(fonts);
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}
pub fn regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

pub fn apply(ctx: &egui::Context, t: &Tokens) {
    let mut style = (*ctx.global_style()).clone();
    style.text_styles = [
        (TextStyle::Small, regular(11.0)),
        (TextStyle::Body, regular(13.0)),
        (TextStyle::Button, regular(13.0)),
        (TextStyle::Heading, bold(17.0)),
        (TextStyle::Monospace, mono(12.5)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(11.0, 6.0);
    style.spacing.interact_size = egui::vec2(32.0, 30.0);
    style.spacing.combo_height = 300.0;
    style.spacing.scroll = egui::style::ScrollStyle::thin();
    let mut v = if t.night { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = t.surface_solid;
    v.window_fill = t.surface_solid;
    v.extreme_bg_color = t.surface_solid;
    v.faint_bg_color = t.surface2;
    v.code_bg_color = t.surface2;
    v.window_stroke = Stroke::new(1.0, t.line_strong);
    v.window_corner_radius = egui::CornerRadius::same(14);
    v.menu_corner_radius = egui::CornerRadius::same(10);
    v.window_shadow = egui::epaint::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(if t.night { 110 } else { 34 }) };
    v.popup_shadow = egui::epaint::Shadow { offset: [0, 6], blur: 20, spread: 0, color: Color32::from_black_alpha(if t.night { 100 } else { 30 }) };
    v.selection.bg_fill = t.accent;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = t.blue;
    v.override_text_color = Some(t.text);
    v.warn_fg_color = t.warn;
    v.error_fg_color = t.bad;
    let r = egui::CornerRadius::same(9);
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open, &mut v.widgets.noninteractive] {
        w.corner_radius = r;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, t.line);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, t.text2);
    v.widgets.inactive.bg_fill = t.surface_solid;
    v.widgets.inactive.weak_bg_fill = t.surface_solid;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, t.line_strong);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, t.text);
    v.widgets.hovered.bg_fill = t.surface2;
    v.widgets.hovered.weak_bg_fill = t.surface2;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, t.line_strong);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, t.text);
    v.widgets.active.bg_fill = t.surface2;
    v.widgets.active.weak_bg_fill = t.surface2;
    v.widgets.active.bg_stroke = Stroke::new(1.0, t.accent);
    v.widgets.active.fg_stroke = Stroke::new(1.0, t.text);
    v.widgets.open.weak_bg_fill = t.surface2;
    v.text_edit_bg_color = Some(t.surface_solid);
    style.visuals = v;
    ctx.set_global_style(style);
}
