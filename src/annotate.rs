//! VisIt-style plot annotations: title / time / cycle, variable name, colorbar.
//!
//! Text is drawn with 2× supersampled TrueType coverage (DejaVu Sans via
//! `fontdue`) so HUD labels stay sharp and undistorted at viewport scale.

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
    /// Colormap used by the colorbar (follows plot style when StyleDefault).
    pub colormap: crate::contour::ColorMapName,
    /// Draw the vertical colorbar (contour / scalar views).
    pub colorbar: bool,
    /// Contour level count shown in the HUD (when banded / labeled).
    pub nlevels: Option<u32>,
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
            colormap: crate::contour::ColorMapName::StyleDefault,
            colorbar: false,
            nlevels: None,
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

/// Supersample factor for glyph coverage (reduces stretch/jaggies when PNGs
/// are shown near 1:1 in the GUI viewport).
const AA_SCALE: f32 = 2.0;

fn annotation_font() -> &'static Font {
    static FONT: OnceLock<Font> = OnceLock::new();
    FONT.get_or_init(|| {
        let bytes = include_bytes!("../assets/DejaVuSans.ttf");
        Font::from_bytes(bytes as &[u8], fontdue::FontSettings::default())
            .expect("bundled assets/DejaVuSans.ttf must parse")
    })
}

fn line_metrics(px: f32) -> (f32, f32) {
    let font = annotation_font();
    if let Some(m) = font.horizontal_line_metrics(px) {
        (m.ascent, m.descent.abs())
    } else {
        (px * 0.8, px * 0.2)
    }
}

/// Overlay VisIt-like annotations on an existing raster.
pub fn draw_annotations(img: &mut ContourImage, info: &AnnotationInfo) {
    if img.width < 64 || img.height < 64 {
        return;
    }
    let lines = info_lines(info);
    // Slightly larger HUD so supersampled glyphs stay readable after LANCZOS fit.
    let title_px = if img.width >= 900 {
        16.0
    } else if img.width >= 700 {
        15.0
    } else {
        13.0
    };
    let body_px = if img.width >= 900 {
        13.0
    } else if img.width >= 700 {
        12.0
    } else {
        11.0
    };
    let pad = 10i32;
    let line_gap = 3i32;

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

    let mut y = pad as f32;
    for (i, line) in lines.iter().enumerate() {
        let (px, _w, h) = sizes[i];
        let color = if i == 0 { FG } else { FG_SOFT };
        draw_text_aa(img, pad as f32, y, line, px, color);
        y += h as f32 + line_gap as f32;
    }

    if info.colorbar {
        if let (Some(vmin), Some(vmax)) = (info.vmin, info.vmax) {
            draw_colorbar(
                img,
                info.style,
                info.colormap,
                vmin,
                vmax,
                info.variable.as_deref(),
                body_px,
            );
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
    if let Some(n) = info.nlevels {
        lines.push(format!("Levels: {n}"));
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
    colormap: crate::contour::ColorMapName,
    vmin: f64,
    vmax: f64,
    variable: Option<&str>,
    label_px: f32,
) {
    let bar_w = (img.width / 28).clamp(14, 28) as i32;
    let margin = 10i32;
    let top = (img.height as f64 * 0.10) as i32;
    let bot = (img.height as f64 * 0.90) as i32;
    let tick_px = (label_px - 1.0).max(10.0);
    let hi = format_tick(vmax);
    let lo = format_tick(vmin);
    let mid = format_tick(0.5 * (vmin + vmax));
    let var_label = variable.unwrap_or("");
    let (tw_hi, _) = measure_text(&hi, tick_px);
    let (tw_lo, _) = measure_text(&lo, tick_px);
    let (tw_mid, _) = measure_text(&mid, tick_px);
    let (tw_var, _) = measure_text(var_label, label_px);
    let label_w = tw_hi.max(tw_lo).max(tw_mid).max(tw_var).max(36);
    let x0 = img.width as i32 - margin - bar_w - label_w - 10;
    if x0 < 40 || bot - top < 40 {
        return;
    }
    fill_rect(
        img,
        x0 - 8,
        top - 24,
        bar_w + label_w + 22,
        bot - top + 48,
        PANEL,
    );
    if !var_label.is_empty() {
        draw_text_aa(img, x0 as f32, (top - 20) as f32, var_label, label_px, FG);
    }
    for y in top..=bot {
        let t = 1.0 - (y - top) as f64 / (bot - top).max(1) as f64;
        let (r, g, b) = style.colormap_named(colormap, t);
        for x in x0..x0 + bar_w {
            put_pixel(img, x, y, [r, g, b, 255]);
        }
    }
    draw_line(img, x0, top, x0 + bar_w, top, BAR_FRAME);
    draw_line(img, x0, bot, x0 + bar_w, bot, BAR_FRAME);
    draw_line(img, x0, top, x0, bot, BAR_FRAME);
    draw_line(img, x0 + bar_w, top, x0 + bar_w, bot, BAR_FRAME);

    let tx = (x0 + bar_w + 6) as f32;
    draw_text_aa(img, tx, top as f32, &hi, tick_px, FG);
    draw_text_aa(img, tx, ((top + bot) / 2) as f32, &mid, tick_px, FG_SOFT);
    draw_text_aa(img, tx, (bot - 14) as f32, &lo, tick_px, FG);
}

fn format_tick(v: f64) -> String {
    if !v.is_finite() {
        return "nan".into();
    }
    let a = v.abs();
    if a != 0.0 && (a >= 1e4 || a < 1e-2) {
        format!("{v:.3e}")
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
    let (ascent, descent) = line_metrics(px);
    let mut w = 0.0f32;
    for ch in text.chars() {
        let (metrics, _) = font.rasterize(ch, px);
        w += metrics.advance_width;
    }
    let h = (ascent + descent).ceil() as i32 + 2;
    ((w.ceil() as i32).max(1), h.max(1))
}

/// Anti-aliased TrueType text via 2× coverage supersample.
///
/// Glyphs are rasterized at `2×px` and downsampled with matching 2× metrics
/// (xmin/ymin/advance all ÷2) so strokes never look horizontally stretched.
fn draw_text_aa(img: &mut ContourImage, x: f32, y: f32, text: &str, px: f32, rgba: [u8; 4]) {
    let font = annotation_font();
    let (ascent, _descent) = line_metrics(px);
    let baseline = y + ascent;
    let mut pen_x = x;
    let hi_px = px * AA_SCALE;
    let scale = AA_SCALE as i32;
    for ch in text.chars() {
        let (metrics, bitmap) = font.rasterize(ch, hi_px);
        // All placement from the high-res metrics, scaled back to destination.
        let glyph_x0 = pen_x + (metrics.xmin as f32) / AA_SCALE;
        let glyph_y0 = baseline + (metrics.ymin as f32) / AA_SCALE;
        let dst_w = ((metrics.width as f32) / AA_SCALE).ceil() as i32;
        let dst_h = ((metrics.height as f32) / AA_SCALE).ceil() as i32;
        for row in 0..dst_h {
            for col in 0..dst_w {
                let mut acc = 0u32;
                let mut n = 0u32;
                for sy in 0..scale {
                    for sx in 0..scale {
                        let src_x = col * scale + sx;
                        let src_y = row * scale + sy;
                        if src_x >= 0
                            && src_y >= 0
                            && src_x < metrics.width as i32
                            && src_y < metrics.height as i32
                        {
                            acc += bitmap[src_y as usize * metrics.width + src_x as usize] as u32;
                            n += 1;
                        }
                    }
                }
                if n == 0 || acc == 0 {
                    continue;
                }
                let cover = ((acc + n / 2) / n) as u8;
                let a = ((cover as u16 * rgba[3] as u16) / 255) as u8;
                if a == 0 {
                    continue;
                }
                blend_pixel(
                    img,
                    (glyph_x0 + col as f32).round() as i32,
                    (glyph_y0 + row as f32).round() as i32,
                    [rgba[0], rgba[1], rgba[2], a],
                );
            }
        }
        pen_x += metrics.advance_width / AA_SCALE;
    }
}

/// Stamp a probe readout near a pixel (Tecplot-like click label).
pub fn draw_probe_label(img: &mut ContourImage, px: i32, py: i32, text: &str) {
    if img.width < 32 || img.height < 32 || text.is_empty() {
        return;
    }
    let font_px = if img.width >= 700 { 12.0 } else { 11.0 };
    let (tw, th) = measure_text(text, font_px);
    let pad = 4i32;
    let mut x = px + 10;
    let mut y = py - th - 8;
    if x + tw + pad * 2 > img.width as i32 {
        x = px - tw - pad * 2 - 10;
    }
    if y < 4 {
        y = py + 10;
    }
    x = x.max(4);
    y = y.max(4);
    fill_rect(img, x - pad, y - pad, tw + pad * 2, th + pad * 2, PANEL);
    // Crosshair
    draw_line(img, px - 5, py, px + 5, py, FG);
    draw_line(img, px, py - 5, px, py + 5, FG);
    draw_text_aa(img, x as f32, y as f32, text, font_px, FG);
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

    #[test]
    fn probe_label_draws() {
        let mut img = ContourImage {
            width: 120,
            height: 80,
            rgba: vec![180u8; 120 * 80 * 4],
            vmin: 0.0,
            vmax: 1.0,
            grid: Grid2D {
                ni: 2,
                nj: 2,
                x: vec![0.0, 1.0, 0.0, 1.0],
                y: vec![0.0, 0.0, 1.0, 1.0],
                values: vec![0.0, 1.0, 1.0, 2.0],
            },
        };
        draw_probe_label(&mut img, 40, 40, "P=1.23");
        assert!(img.rgba.iter().any(|&v| v < 50));
    }
}
