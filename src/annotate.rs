//! VisIt-style plot annotations: title / time / cycle, variable name, colorbar.
//!
//! Text uses anti-aliased TrueType (DejaVu Sans via `fontdue`) for a modern
//! VisIt-quality look — not a 5×7 bitmap.

use std::sync::OnceLock;

use fontdue::Font;

use crate::contour::{draw_line, put_pixel, ContourImage, PlotStyle};
use crate::dataset::Dataset;

/// What to stamp onto a rendered mesh/contour PNG.
#[derive(Debug, Clone)]
pub struct AnnotationInfo {
    pub title: Option<String>,
    pub database: Option<String>,
    pub zone_name: Option<String>,
    pub variable: Option<String>,
    pub time: Option<f64>,
    pub cycle: Option<i64>,
    pub strand_id: Option<i32>,
    pub plane: Option<String>,
    pub view: Option<String>,
    pub vmin: Option<f64>,
    pub vmax: Option<f64>,
    pub style: PlotStyle,
    /// Draw the vertical colorbar (contour / scalar views).
    pub colorbar: bool,
}

impl Default for AnnotationInfo {
    fn default() -> Self {
        Self {
            title: None,
            database: None,
            zone_name: None,
            variable: None,
            time: None,
            cycle: None,
            strand_id: None,
            plane: None,
            view: None,
            vmin: None,
            vmax: None,
            style: PlotStyle::Default,
            colorbar: false,
        }
    }
}

impl AnnotationInfo {
    /// Build annotation text from a loaded dataset + active zone/variable.
    pub fn from_dataset(
        ds: &Dataset,
        zone: usize,
        variable: Option<&str>,
        path_hint: Option<&str>,
    ) -> Self {
        let z = ds.zones.get(zone);
        let meta = ds.meta(zone);
        let mut info = Self {
            title: ds.title.clone(),
            database: path_hint.map(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(p)
                    .to_string()
            }),
            zone_name: z.map(|zz| zz.name.clone()),
            variable: variable.map(|s| s.to_string()),
            time: meta.time_value(),
            cycle: ds.cycle().or(meta.cycle()),
            strand_id: meta.strand_id,
            ..Default::default()
        };
        if info.time.is_none() {
            for (k, v) in &ds.aux {
                if k.eq_ignore_ascii_case("time") || k.eq_ignore_ascii_case("solutiontime") {
                    if let Ok(t) = v.trim().parse::<f64>() {
                        info.time = Some(t);
                        break;
                    }
                }
            }
        }
        info
    }
}

const FG: [u8; 4] = [20, 24, 28, 255];
const FG_SOFT: [u8; 4] = [55, 64, 72, 255];
const PANEL: [u8; 4] = [250, 251, 252, 230];
const BAR_FRAME: [u8; 4] = [30, 30, 30, 255];

fn annotation_font() -> &'static Font {
    static FONT: OnceLock<Font> = OnceLock::new();
    FONT.get_or_init(|| {
        let bytes = include_bytes!("../assets/DejaVuSans.ttf");
        Font::from_bytes(bytes as &[u8], fontdue::FontSettings::default())
            .expect("bundled assets/DejaVuSans.ttf must parse")
    })
}

/// Overlay VisIt-like annotations on an existing raster.
pub fn draw_annotations(img: &mut ContourImage, info: &AnnotationInfo) {
    if img.width < 64 || img.height < 64 {
        return;
    }
    let lines = info_lines(info);
    let title_px = if img.width >= 700 { 15.0 } else { 13.0 };
    let body_px = if img.width >= 700 { 12.0 } else { 11.0 };
    let pad = 10i32;
    let line_gap = 4i32;

    let mut sizes = Vec::with_capacity(lines.len());
    let mut max_w = 0i32;
    let mut total_h = pad;
    for (i, line) in lines.iter().enumerate() {
        let px = if i == 0 { title_px } else { body_px };
        let (w, h) = measure_text(line, px);
        sizes.push((px, w, h));
        max_w = max_w.max(w);
        total_h += h + line_gap;
    }
    total_h += pad / 2;
    let panel_w = max_w + 2 * pad;
    fill_rect(img, pad - 4, pad - 4, panel_w, total_h, PANEL);

    let mut y = pad;
    for (i, line) in lines.iter().enumerate() {
        let (px, _w, h) = sizes[i];
        let color = if i == 0 { FG } else { FG_SOFT };
        draw_text_aa(img, pad as f32, y as f32, line, px, color);
        y += h + line_gap;
    }

    if info.colorbar {
        if let (Some(vmin), Some(vmax)) = (info.vmin, info.vmax) {
            draw_colorbar(img, info.style, vmin, vmax, info.variable.as_deref(), body_px);
        }
    }
}

fn info_lines(info: &AnnotationInfo) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(t) = &info.title {
        if !t.is_empty() {
            lines.push(t.clone());
        }
    }
    if let Some(db) = &info.database {
        lines.push(format!("Database: {db}"));
    }
    let mut meta = String::new();
    if let Some(c) = info.cycle {
        meta.push_str(&format!("Cycle {c}"));
    }
    if let Some(t) = info.time {
        if !meta.is_empty() {
            meta.push_str("  ");
        }
        meta.push_str(&format!("Time {t}"));
    }
    if let Some(s) = info.strand_id {
        if !meta.is_empty() {
            meta.push_str("  ");
        }
        meta.push_str(&format!("Strand {s}"));
    }
    if !meta.is_empty() {
        lines.push(meta);
    }
    if let Some(z) = &info.zone_name {
        if !z.is_empty() {
            lines.push(format!("Zone: {z}"));
        }
    }
    if let Some(v) = &info.variable {
        lines.push(format!("Variable: {v}"));
    }
    if let Some(p) = &info.plane {
        lines.push(format!("Plane: {p}"));
    }
    if let Some(v) = &info.view {
        if v != "auto" {
            lines.push(format!("View: {v}"));
        }
    }
    if lines.is_empty() {
        lines.push("Flowplot".into());
    }
    lines
}

fn draw_colorbar(
    img: &mut ContourImage,
    style: PlotStyle,
    vmin: f64,
    vmax: f64,
    variable: Option<&str>,
    label_px: f32,
) {
    let bar_w = (img.width / 28).clamp(14, 28) as i32;
    let margin = 12i32;
    let top = (img.height as f64 * 0.12) as i32;
    let bot = (img.height as f64 * 0.88) as i32;
    let x0 = img.width as i32 - margin - bar_w - 48;
    if x0 < 40 || bot - top < 40 {
        return;
    }
    fill_rect(
        img,
        x0 - 6,
        top - 22,
        bar_w + 60,
        bot - top + 44,
        PANEL,
    );
    if let Some(v) = variable {
        draw_text_aa(img, x0 as f32, (top - 18) as f32, v, label_px, FG);
    }
    for y in top..=bot {
        let t = 1.0 - (y - top) as f64 / (bot - top).max(1) as f64;
        let (r, g, b) = style.colormap(t);
        for x in x0..x0 + bar_w {
            put_pixel(img, x, y, [r, g, b, 255]);
        }
    }
    draw_line(img, x0, top, x0 + bar_w, top, BAR_FRAME);
    draw_line(img, x0, bot, x0 + bar_w, bot, BAR_FRAME);
    draw_line(img, x0, top, x0, bot, BAR_FRAME);
    draw_line(img, x0 + bar_w, top, x0 + bar_w, bot, BAR_FRAME);

    let tick_px = (label_px - 1.0).max(9.0);
    let hi = format_tick(vmax);
    let lo = format_tick(vmin);
    let mid = format_tick(0.5 * (vmin + vmax));
    let tx = (x0 + bar_w + 6) as f32;
    draw_text_aa(img, tx, top as f32, &hi, tick_px, FG);
    draw_text_aa(img, tx, ((top + bot) / 2) as f32, &mid, tick_px, FG_SOFT);
    draw_text_aa(img, tx, (bot - 12) as f32, &lo, tick_px, FG);
}

fn format_tick(v: f64) -> String {
    if !v.is_finite() {
        return "nan".into();
    }
    let a = v.abs();
    if a != 0.0 && (a >= 1e4 || a < 1e-2) {
        format!("{v:.2e}")
    } else {
        format!("{v:.4}")
    }
}

fn fill_rect(img: &mut ContourImage, x: i32, y: i32, w: i32, h: i32, rgba: [u8; 4]) {
    for yy in y..y + h {
        for xx in x..x + w {
            if rgba[3] >= 250 {
                put_pixel(img, xx, yy, rgba);
            } else {
                blend_pixel(img, xx, yy, rgba);
            }
        }
    }
}

fn blend_pixel(img: &mut ContourImage, x: i32, y: i32, rgba: [u8; 4]) {
    if x < 0 || y < 0 || x >= img.width as i32 || y >= img.height as i32 {
        return;
    }
    let i = ((y as u32 * img.width + x as u32) * 4) as usize;
    let a = rgba[3] as f64 / 255.0;
    for c in 0..3 {
        let src = img.rgba[i + c] as f64;
        let dst = rgba[c] as f64;
        img.rgba[i + c] = (src * (1.0 - a) + dst * a).round().clamp(0.0, 255.0) as u8;
    }
    img.rgba[i + 3] = 255;
}

fn measure_text(text: &str, px: f32) -> (i32, i32) {
    let font = annotation_font();
    let mut w = 0.0f32;
    let mut h = px.ceil() as i32;
    for ch in text.chars() {
        let (metrics, _) = font.rasterize(ch, px);
        w += metrics.advance_width;
        h = h.max(metrics.height as i32 + metrics.ymin.abs());
    }
    ((w.ceil() as i32).max(1), h.max(1) + 2)
}

/// Anti-aliased TrueType text (VisIt-quality).
fn draw_text_aa(img: &mut ContourImage, x: f32, y: f32, text: &str, px: f32, rgba: [u8; 4]) {
    let font = annotation_font();
    let mut pen_x = x;
    // `y` is the top of the line box; baseline sits ~80% down (VisIt-like HUD).
    let baseline = y + px * 0.80;
    for ch in text.chars() {
        let (metrics, bitmap) = font.rasterize(ch, px);
        let glyph_x = (pen_x + metrics.xmin as f32).round() as i32;
        let glyph_y = (baseline + metrics.ymin as f32).round() as i32;
        for row in 0..metrics.height {
            for col in 0..metrics.width {
                let cover = bitmap[row * metrics.width + col];
                if cover == 0 {
                    continue;
                }
                let a = ((cover as u16 * rgba[3] as u16) / 255) as u8;
                blend_pixel(
                    img,
                    glyph_x + col as i32,
                    glyph_y + row as i32,
                    [rgba[0], rgba[1], rgba[2], a],
                );
            }
        }
        pen_x += metrics.advance_width;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::{Location, OrderedDims, Topology, VariableData, Zone};
    use crate::field::Grid2D;

    #[test]
    fn annotations_draw_without_panic() {
        let mut img = ContourImage {
            width: 200,
            height: 160,
            rgba: vec![200u8; 200 * 160 * 4],
            vmin: 0.0,
            vmax: 1.0,
            grid: Grid2D {
                ni: 2,
                nj: 2,
                x: vec![0.0, 1.0, 0.0, 1.0],
                y: vec![0.0, 0.0, 1.0, 1.0],
                values: vec![0.0, 0.5, 0.5, 1.0],
            },
        };
        let ds = Dataset {
            title: Some("demo".into()),
            variables: vec!["P".into()],
            zones: vec![Zone {
                name: "z1".into(),
                topology: Topology::Ordered(OrderedDims::Ij { i: 2, j: 2 }),
                fields: vec![VariableData {
                    location: Location::Nodal,
                    values: vec![0.0; 4],
                }],
            }],
            aux: vec![],
            zone_meta: vec![crate::dataset::ZoneMeta {
                solution_time: Some(1.25),
                strand_id: Some(0),
                aux: vec![("CYCLE".into(), "42".into())],
            }],
        };
        let mut info = AnnotationInfo::from_dataset(&ds, 0, Some("P"), Some("/tmp/run.plt"));
        info.colorbar = true;
        info.vmin = Some(0.0);
        info.vmax = Some(1.0);
        info.style = PlotStyle::Default;
        draw_annotations(&mut img, &info);
        assert!(img.rgba.iter().step_by(4).any(|&r| r > 240));
    }

    #[test]
    fn font_loads_bundled_dejavu() {
        let f = annotation_font();
        let (m, bmp) = f.rasterize('A', 16.0);
        assert!(m.width > 0 && !bmp.is_empty());
    }
}
