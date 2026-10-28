//! VisIt-style plot annotations: title / time / cycle, variable name, colorbar.
//!
//! Drawn as a lightweight bitmap overlay (no external font deps). Mimics the
//! Silo/VisIt habit of putting database info top-left and a vertical legend
//! on the right of the active field plot.

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
        // File-level Time AUX if zone has none.
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
const FG_SOFT: [u8; 4] = [50, 58, 66, 255];
const PANEL: [u8; 4] = [250, 251, 252, 220];
const BAR_FRAME: [u8; 4] = [30, 30, 30, 255];

/// Overlay VisIt-like annotations on an existing raster.
pub fn draw_annotations(img: &mut ContourImage, info: &AnnotationInfo) {
    if img.width < 64 || img.height < 64 {
        return;
    }
    let lines = info_lines(info);
    let pad = 8i32;
    let scale = if img.width >= 700 { 2i32 } else { 1i32 };
    let line_h = 8 * scale + 2;
    let panel_w = lines
        .iter()
        .map(|l| text_width(l, scale))
        .max()
        .unwrap_or(40)
        + 2 * pad;
    let panel_h = (lines.len() as i32) * line_h + pad;
    fill_rect(img, pad - 2, pad - 2, panel_w, panel_h, PANEL);
    for (i, line) in lines.iter().enumerate() {
        let y = pad + (i as i32) * line_h;
        let color = if i == 0 { FG } else { FG_SOFT };
        draw_text(img, pad, y, line, color, scale);
    }

    if info.colorbar {
        if let (Some(vmin), Some(vmax)) = (info.vmin, info.vmax) {
            draw_colorbar(img, info.style, vmin, vmax, info.variable.as_deref());
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
) {
    let bar_w = (img.width / 28).clamp(12, 28) as i32;
    let margin = 10i32;
    let top = (img.height as f64 * 0.12) as i32;
    let bot = (img.height as f64 * 0.88) as i32;
    let x0 = img.width as i32 - margin - bar_w;
    if x0 < 40 || bot - top < 40 {
        return;
    }
    // Soft backdrop so turbo/dark fields stay readable.
    fill_rect(
        img,
        x0 - 4,
        top - 18,
        bar_w + 8 + 56,
        bot - top + 36,
        PANEL,
    );
    if let Some(v) = variable {
        let scale = if img.width >= 700 { 2 } else { 1 };
        draw_text(img, x0 - 2, top - 14, v, FG, scale);
    }
    for y in top..=bot {
        let t = 1.0 - (y - top) as f64 / (bot - top).max(1) as f64;
        let (r, g, b) = style.colormap(t);
        for x in x0..x0 + bar_w {
            put_pixel(img, x, y, [r, g, b, 255]);
        }
    }
    // Frame
    draw_line(img, x0, top, x0 + bar_w, top, BAR_FRAME);
    draw_line(img, x0, bot, x0 + bar_w, bot, BAR_FRAME);
    draw_line(img, x0, top, x0, bot, BAR_FRAME);
    draw_line(img, x0 + bar_w, top, x0 + bar_w, bot, BAR_FRAME);

    let scale = 1i32;
    let hi = format_tick(vmax);
    let lo = format_tick(vmin);
    let mid = format_tick(0.5 * (vmin + vmax));
    draw_text(img, x0 + bar_w + 4, top - 2, &hi, FG, scale);
    draw_text(
        img,
        x0 + bar_w + 4,
        (top + bot) / 2 - 3,
        &mid,
        FG_SOFT,
        scale,
    );
    draw_text(img, x0 + bar_w + 4, bot - 6, &lo, FG, scale);
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
            // Alpha blend over existing pixel when a < 255.
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

fn text_width(s: &str, scale: i32) -> i32 {
    (s.chars().count() as i32) * (6 * scale)
}

/// 5×7 monospace glyphs for printable ASCII (space–~).
fn draw_text(img: &mut ContourImage, x: i32, y: i32, text: &str, rgba: [u8; 4], scale: i32) {
    let scale = scale.max(1);
    let mut cx = x;
    for ch in text.chars() {
        if let Some(glyph) = glyph(ch) {
            for row in 0..7i32 {
                let bits = glyph[row as usize];
                for col in 0..5i32 {
                    if bits & (1 << (4 - col)) != 0 {
                        for dy in 0..scale {
                            for dx in 0..scale {
                                put_pixel(
                                    img,
                                    cx + col * scale + dx,
                                    y + row * scale + dy,
                                    rgba,
                                );
                            }
                        }
                    }
                }
            }
        }
        cx += 6 * scale;
    }
}

fn glyph(ch: char) -> Option<[u8; 7]> {
    // Compact 5-bit rows; bit4 = leftmost. Table covers ASCII 32..=126.
    const G: [[u8; 7]; 95] = include!("glyph5x7.in");
    let idx = ch as usize;
    if (32..=126).contains(&idx) {
        Some(G[idx - 32])
    } else {
        Some(G[(b'?' as usize) - 32])
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
        let mut ds = Dataset {
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
        let _ = &mut ds;
        let mut info = AnnotationInfo::from_dataset(&ds, 0, Some("P"), Some("/tmp/run.plt"));
        info.colorbar = true;
        info.vmin = Some(0.0);
        info.vmax = Some(1.0);
        info.style = PlotStyle::Default;
        draw_annotations(&mut img, &info);
        // Top-left panel should have been painted (not all 200).
        assert!(img.rgba.iter().step_by(4).any(|&r| r > 240));
    }
}
