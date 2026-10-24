//! A small page-flow PDF writer for the briefing: wrapped text, tables, vector shapes and an
//! image, on US Letter pages, using the app's Inter font.

use printpdf::*;

const REGULAR: &[u8] = include_bytes!("../../assets/fonts/Inter-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../../assets/fonts/Inter-Bold.ttf");

pub const PAGE_W: f32 = 215.9;
pub const PAGE_H: f32 = 279.4;
pub const MARGIN: f32 = 14.0;

#[derive(Clone, Copy)]
pub struct C(pub f32, pub f32, pub f32);

impl C {
    pub const TEXT: C = C(0.11, 0.12, 0.15);
    pub const MUTED: C = C(0.36, 0.39, 0.44);
    pub const FAINT: C = C(0.55, 0.57, 0.61);
    pub const LINE: C = C(0.86, 0.86, 0.88);
    pub const ACCENT: C = C(0.83, 0.08, 0.55);
    pub const BAD: C = C(0.82, 0.23, 0.23);
    pub const WARN: C = C(0.83, 0.54, 0.04);
    pub const OK: C = C(0.12, 0.62, 0.42);
    pub const BLUE: C = C(0.17, 0.39, 0.72);
    pub fn from(c: egui::Color32) -> C {
        C(c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0)
    }
    fn color(self) -> Color {
        Color::Rgb(Rgb { r: self.0, g: self.1, b: self.2, icc_profile: None })
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Right,
}

pub struct Pdf {
    doc: PdfDocument,
    regular: FontId,
    bold: FontId,
    faces: (ttf_parser::Face<'static>, ttf_parser::Face<'static>),
    pages: Vec<PdfPage>,
    ops: Vec<Op>,
    /// cursor, mm from the top of the page
    pub y: f32,
}

fn pt(x: f32, y_top: f32) -> Point {
    Point { x: Mm(x).into(), y: Mm(PAGE_H - y_top).into() }
}

impl Pdf {
    pub fn new(title: &str) -> anyhow::Result<Self> {
        let mut doc = PdfDocument::new(title);
        let mut w = Vec::new();
        let r = ParsedFont::from_bytes(REGULAR, 0, &mut w).ok_or_else(|| anyhow::anyhow!("font"))?;
        let b = ParsedFont::from_bytes(BOLD, 0, &mut w).ok_or_else(|| anyhow::anyhow!("font"))?;
        let regular = doc.add_font(&r);
        let bold = doc.add_font(&b);
        let faces = (ttf_parser::Face::parse(REGULAR, 0)?, ttf_parser::Face::parse(BOLD, 0)?);
        Ok(Pdf { doc, regular, bold, faces, pages: vec![], ops: vec![], y: MARGIN })
    }

    /// Text width in mm at `size` pt.
    pub fn width(&self, s: &str, size: f32, bold: bool) -> f32 {
        let f = if bold { &self.faces.1 } else { &self.faces.0 };
        let upm = f.units_per_em() as f32;
        let units: f32 = s.chars().map(|c| f.glyph_index(c).and_then(|g| f.glyph_hor_advance(g)).unwrap_or((upm * 0.5) as u16) as f32).sum();
        units / upm * size * 0.3528
    }

    pub fn text(&mut self, x: f32, y_top: f32, s: &str, size: f32, bold: bool, c: C) {
        if s.is_empty() {
            return;
        }
        let font = if bold { self.bold.clone() } else { self.regular.clone() };
        // y_top is the top of the line box; PDF positions text by its baseline
        self.ops.extend([
            Op::StartTextSection,
            Op::SetFillColor { col: c.color() },
            Op::SetFont { font: PdfFontHandle::External(font), size: Pt(size) },
            Op::SetTextCursor { pos: pt(x, y_top + size * 0.3528 * 0.8) },
            Op::ShowText { items: vec![TextItem::Text(s.to_string())] },
            Op::EndTextSection,
        ]);
    }

    pub fn text_aligned(&mut self, x: f32, w: f32, y_top: f32, s: &str, size: f32, bold: bool, c: C, align: Align) {
        let tx = match align {
            Align::Left => x,
            Align::Right => x + w - self.width(s, size, bold),
        };
        self.text(tx, y_top, s, size, bold, c);
    }

    /// Break text into lines that fit `w` mm.
    pub fn wrap(&self, s: &str, size: f32, bold: bool, w: f32) -> Vec<String> {
        let mut lines = Vec::new();
        for para in s.split('\n') {
            let mut cur = String::new();
            for word in para.split_whitespace() {
                let cand = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
                if self.width(&cand, size, bold) > w && !cur.is_empty() {
                    lines.push(std::mem::take(&mut cur));
                    cur = word.to_string();
                } else {
                    cur = cand;
                }
            }
            lines.push(cur);
        }
        lines
    }

    pub fn line_h(size: f32) -> f32 {
        size * 0.3528 * 1.35
    }

    /// Start a new page if `h` mm doesn't fit.
    pub fn ensure(&mut self, h: f32) {
        if self.y + h > PAGE_H - MARGIN {
            self.page_break();
        }
    }

    pub fn page_break(&mut self) {
        let ops = std::mem::take(&mut self.ops);
        self.pages.push(PdfPage::new(Mm(PAGE_W), Mm(PAGE_H), ops));
        self.y = MARGIN;
    }

    pub fn para(&mut self, s: &str, size: f32, bold: bool, c: C) {
        self.para_at(MARGIN, PAGE_W - 2.0 * MARGIN, s, size, bold, c);
    }

    pub fn para_at(&mut self, x: f32, w: f32, s: &str, size: f32, bold: bool, c: C) {
        let lh = Self::line_h(size);
        for l in self.wrap(s, size, bold, w) {
            self.ensure(lh);
            let y = self.y;
            self.text(x, y, &l, size, bold, c);
            self.y += lh;
        }
    }

    pub fn heading(&mut self, s: &str) {
        self.ensure(14.0);
        self.y += 3.0;
        let y = self.y;
        self.text(MARGIN, y, s, 11.5, true, C::TEXT);
        self.y += 6.0;
    }

    pub fn rule(&mut self) {
        self.ensure(3.0);
        self.y += 1.5;
        let y = self.y;
        self.line(&[(MARGIN, y), (PAGE_W - MARGIN, y)], C::LINE, 0.4, None);
        self.y += 1.5;
    }

    pub fn line(&mut self, pts: &[(f32, f32)], c: C, width_pt: f32, dash: Option<[f32; 2]>) {
        if pts.len() < 2 {
            return;
        }
        self.ops.push(Op::SaveGraphicsState);
        if let Some(d) = dash {
            self.ops.push(Op::SetLineDashPattern { dash: LineDashPattern::new(0.0, &d) });
        }
        self.ops.push(Op::SetOutlineColor { col: c.color() });
        self.ops.push(Op::SetOutlineThickness { pt: Pt(width_pt) });
        self.ops.push(Op::DrawLine { line: Line { points: pts.iter().map(|(x, y)| LinePoint { p: pt(*x, *y), bezier: false }).collect(), is_closed: false } });
        self.ops.push(Op::RestoreGraphicsState);
    }

    pub fn polygon(&mut self, pts: &[(f32, f32)], fill: Option<C>, stroke: Option<(C, f32)>) {
        if pts.len() < 3 {
            return;
        }
        self.ops.push(Op::SaveGraphicsState);
        let mode = match (fill, stroke) {
            (Some(f), Some((s, w))) => {
                self.ops.push(Op::SetFillColor { col: f.color() });
                self.ops.push(Op::SetOutlineColor { col: s.color() });
                self.ops.push(Op::SetOutlineThickness { pt: Pt(w) });
                PaintMode::FillStroke
            }
            (Some(f), None) => {
                self.ops.push(Op::SetFillColor { col: f.color() });
                PaintMode::Fill
            }
            (None, Some((s, w))) => {
                self.ops.push(Op::SetOutlineColor { col: s.color() });
                self.ops.push(Op::SetOutlineThickness { pt: Pt(w) });
                PaintMode::Stroke
            }
            (None, None) => PaintMode::Stroke,
        };
        self.ops.push(Op::DrawPolygon {
            polygon: Polygon { rings: vec![PolygonRing { points: pts.iter().map(|(x, y)| LinePoint { p: pt(*x, *y), bezier: false }).collect() }], mode, winding_order: WindingOrder::NonZero },
        });
        self.ops.push(Op::RestoreGraphicsState);
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, fill: Option<C>, stroke: Option<(C, f32)>) {
        self.polygon(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)], fill, stroke);
    }

    pub fn circle(&mut self, x: f32, y: f32, r: f32, fill: C) {
        let pts: Vec<(f32, f32)> = (0..16).map(|i| {
            let a = i as f32 / 16.0 * std::f32::consts::TAU;
            (x + a.cos() * r, y + a.sin() * r)
        })
        .collect();
        self.polygon(&pts, Some(fill), None);
    }

    /// RGB image, `w_mm` wide at the cursor.
    pub fn image(&mut self, rgb: Vec<u8>, px_w: usize, px_h: usize, w_mm: f32) {
        let h_mm = w_mm * px_h as f32 / px_w as f32;
        self.ensure(h_mm + 2.0);
        let img = RawImage { pixels: RawImageData::U8(rgb), width: px_w, height: px_h, data_format: RawImageFormat::RGB8, tag: vec![] };
        let id = self.doc.add_image(&img);
        // at dpi d an image is px/d inches wide; pick the dpi that makes it w_mm
        let dpi = px_w as f32 / (w_mm / 25.4);
        let y = self.y;
        self.ops.push(Op::UseXobject {
            id,
            transform: XObjectTransform { translate_x: Some(Mm(MARGIN).into()), translate_y: Some(Mm(PAGE_H - y - h_mm).into()), dpi: Some(dpi), ..Default::default() },
        });
        self.y += h_mm + 2.0;
    }

    /// Table with a header row; `cols` are (title, width mm, align). Rows wrap to fit.
    pub fn table(&mut self, cols: &[(&str, f32, Align)], rows: &[Vec<String>], size: f32, bold_first: bool) {
        let lh = Self::line_h(size);
        let draw_header = |p: &mut Pdf| {
            p.ensure(lh * 2.0);
            let mut x = MARGIN;
            let y = p.y;
            for (title, w, a) in cols {
                p.text_aligned(x + 0.8, w - 1.6, y, title, size - 0.5, true, C::FAINT, *a);
                x += w;
            }
            p.y += lh;
            let y = p.y;
            p.line(&[(MARGIN, y), (MARGIN + cols.iter().map(|c| c.1).sum::<f32>(), y)], C::LINE, 0.5, None);
            p.y += 0.8;
        };
        draw_header(self);
        for row in rows {
            let wrapped: Vec<Vec<String>> = row.iter().zip(cols).map(|(cell, (_, w, _))| self.wrap(cell, size, false, w - 1.6)).collect();
            let n = wrapped.iter().map(|w| w.len()).max().unwrap_or(1);
            if self.y + lh * n as f32 > PAGE_H - MARGIN {
                self.page_break();
                draw_header(self);
            }
            let mut x = MARGIN;
            let y = self.y;
            for (i, ((_, w, a), lines)) in cols.iter().zip(&wrapped).enumerate() {
                for (k, l) in lines.iter().enumerate() {
                    self.text_aligned(x + 0.8, w - 1.6, y + k as f32 * lh, l, size, bold_first && i == 0, C::TEXT, *a);
                }
                x += w;
            }
            self.y += lh * n as f32 + 0.4;
            let y = self.y;
            self.line(&[(MARGIN, y), (MARGIN + cols.iter().map(|c| c.1).sum::<f32>(), y)], C::LINE, 0.25, None);
            self.y += 0.6;
        }
        self.y += 2.0;
    }

    pub fn finish(mut self) -> Vec<u8> {
        if !self.ops.is_empty() || self.pages.is_empty() {
            self.page_break();
        }
        let pages = std::mem::take(&mut self.pages);
        let mut warnings = Vec::new();
        self.doc.with_pages(pages).save(&PdfSaveOptions { subset_fonts: true, ..Default::default() }, &mut warnings)
    }
}
