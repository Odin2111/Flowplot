//! VisIt-style plot annotations: title / time / cycle, variable name, colorbar.
//!
//! Text uses native fontdue coverage AA at the destination pixel size (no
//! bitmap stretch / supersample mismatch). Pair with a 1:1 Viewport raster.

use std::sync::OnceLock;

use fontdue::Font;

use crate::contour::{draw_line, put_pixel, ContourImage, PlotStyle};
use crate::dataset::Dataset;
use crate::view::ViewSpec;

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
    /// Stamp iso-level value labels on the flood (Contour Abc Label).
    pub show_labels: bool,
    /// Colorbar tick / variable font size in pixels (0 = auto from width).
    pub legend_font_px: f32,
    /// `right` (default) or `left` colorbar alignment.
    pub legend_align: LegendAlign,
    /// Camera orientation for the XYZ triad (origin visible).
    pub view_orient: ViewSpec,
    /// Draw Cartesian XYZ axes triad in the plot (default on).
    pub show_triad: bool,
}

/// Colorbar side (Tecplot Contour Details → Legend).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LegendAlign {
    #[default]
    Right,
    Left,
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
            show_labels: false,
            legend_font_px: 0.0,
            legend_align: LegendAlign::Right,
            view_orient: ViewSpec::Auto,
            show_triad: true,
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
    // Larger HUD readable at 1:1 Viewport pixels (no post-scale stretch).
    let title_px = if img.width >= 1000 {
        18.0
    } else if img.width >= 700 {
        16.0
    } else {
        14.0
    };
    let body_px = if img.width >= 1000 {
        14.0
    } else if img.width >= 700 {
        13.0
    } else {
        12.0
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

    // Integer line tops — avoids cumulative float drift / baseline jitter in the HUD.
    let mut y = pad;
    for (i, line) in lines.iter().enumerate() {
        let (px, _w, h) = sizes[i];
        let color = if i == 0 { FG } else { FG_SOFT };
        draw_text_aa(img, pad as f32, y as f32, line, px, color);
        y += h + line_gap;
    }

    if info.show_triad {
        draw_axes_triad(img, info.view_orient);
    }

    if info.colorbar {
        if let (Some(vmin), Some(vmax)) = (info.vmin, info.vmax) {
            let label_px = if info.legend_font_px > 0.0 {
                info.legend_font_px
            } else {
                body_px
            };
            draw_colorbar(
                img,
                info.style,
                info.colormap,
                vmin,
                vmax,
                info.variable.as_deref(),
                label_px,
                info.legend_align,
                info.nlevels,
            );
        }
    }

    if info.show_labels {
        if let (Some(vmin), Some(vmax)) = (info.vmin, info.vmax) {
            let n = info.nlevels.unwrap_or(8).max(2);
            draw_level_labels(img, vmin, vmax, n);
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
        lines.push("Fieldplot".into());
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
    align: LegendAlign,
    nlevels: Option<u32>,
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
    let block_w = bar_w + label_w + 22;
    let x0 = match align {
        LegendAlign::Right => img.width as i32 - margin - bar_w - label_w - 10,
        LegendAlign::Left => margin + 4,
    };
    if x0 < 4 || bot - top < 40 {
        return;
    }
    fill_rect(
        img,
        x0 - 8,
        top - 24,
        block_w,
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
    let ticks = nlevels.unwrap_or(2).clamp(2, 16);
    if ticks <= 3 {
        draw_text_aa(img, tx, top as f32, &hi, tick_px, FG);
        draw_text_aa(img, tx, ((top + bot) / 2) as f32, &mid, tick_px, FG_SOFT);
        draw_text_aa(img, tx, (bot - 14) as f32, &lo, tick_px, FG);
    } else {
        for i in 0..ticks {
            let t = i as f64 / (ticks - 1) as f64;
            let y = (bot as f64 - t * (bot - top) as f64).round() as i32;
            let v = vmin + t * (vmax - vmin);
            draw_text_aa(img, tx, (y - 6) as f32, &format_tick(v), tick_px, FG);
        }
    }
}

/// Place iso-level value stamps along a mid-j scan (Contour Abc Label).
fn draw_level_labels(img: &mut ContourImage, vmin: f64, vmax: f64, nlevels: u32) {
    let n = nlevels.max(2) as usize;
    let levels: Vec<f64> = (0..n)
        .map(|i| vmin + (i as f64 + 0.5) / n as f64 * (vmax - vmin))
        .collect();
    let ni = img.grid.ni;
    let nj = img.grid.nj;
    if ni < 2 || nj < 2 {
        return;
    }
    let j = nj / 2;
    let font_px = if img.width >= 700 { 11.0 } else { 10.0 };
    let legend_reserve = ((img.width as f64) * 0.10).round().clamp(48.0, 88.0) as u32;
    let plot_w = img.width.saturating_sub(legend_reserve).max(32);
    let margin = 2u32;
    // One hit per level (first mid-j crossing), then enforce pixel gaps.
    let mut world_hits: Vec<(f64, f64, String)> = Vec::new();
    let mut found = vec![false; n];
    for i in 0..ni.saturating_sub(1) {
        let v0 = img.grid.value(i, j);
        let v1 = img.grid.value(i + 1, j);
        if !v0.is_finite() || !v1.is_finite() {
            continue;
        }
        for (li, &lev) in levels.iter().enumerate() {
            if found[li] {
                continue;
            }
            let cross = (v0 - lev) * (v1 - lev) <= 0.0 && (v0 - v1).abs() > 1e-30;
            if !cross {
                continue;
            }
            let t = ((lev - v0) / (v1 - v0)).clamp(0.0, 1.0);
            let (x0, y0) = img.grid.xy(i, j);
            let (x1, y1) = img.grid.xy(i + 1, j);
            world_hits.push((
                x0 + t * (x1 - x0),
                y0 + t * (y1 - y0),
                format_tick(lev),
            ));
            found[li] = true;
        }
        if found.iter().all(|&f| f) {
            break;
        }
    }
    let mut stamps: Vec<(i32, i32, String)> = Vec::with_capacity(world_hits.len());
    for (wx, wy, text) in world_hits {
        let (px, py) = crate::contour::world_to_pixel_plot(img, wx, wy, plot_w, margin);
        stamps.push((px, py, text));
    }
    // Sort left→right and keep only stamps with a comfortable gap (Tecplot-like).
    stamps.sort_by_key(|(px, _, _)| *px);
    let min_gap = ((img.width as f32) * 0.07).clamp(48.0, 96.0);
    let mut placed: Vec<(i32, i32, String)> = Vec::new();
    for (px, py, text) in stamps {
        if let Some((lpx, _, _)) = placed.last() {
            if (px - lpx).abs() < min_gap as i32 {
                continue;
            }
        }
        placed.push((px, py, text));
    }
    for (px, py, text) in placed {
        let (tw, th) = measure_text(&text, font_px);
        let tx = px + 4;
        let ty = py - th - 2;
        fill_rect(img, tx - 2, ty - 1, tw + 4, th + 2, PANEL);
        draw_text_aa(img, tx as f32, ty as f32, &text, font_px, FG);
    }
}

/// Cartesian XYZ triad with visible origin (Tecplot-like corner widget).
pub fn draw_axes_triad(img: &mut ContourImage, view: ViewSpec) {
    if img.width < 80 || img.height < 80 {
        return;
    }
    let ox = 36i32;
    let oy = img.height as i32 - 36;
    let axis_len = 28.0_f64;
    // Screen vectors for unit world axes.
    let axes = [
        ((1.0, 0.0, 0.0), [200u8, 40, 40, 255], "X"),
        ((0.0, 1.0, 0.0), [40u8, 160, 60, 255], "Y"),
        ((0.0, 0.0, 1.0), [40u8, 80, 200, 255], "Z"),
    ];
    // Soft plate behind triad so it stays readable on dark floods.
    fill_rect(img, ox - 18, oy - 34, 56, 52, PANEL);
    // Origin marker.
    for dy in -2..=2 {
        for dx in -2..=2 {
            if dx * dx + dy * dy <= 4 {
                put_pixel(img, ox + dx, oy + dy, FG);
            }
        }
    }
    for &((wx, wy, wz), rgba, label) in &axes {
        let (su, sv) = view.project(wx, wy, wz);
        let norm = (su * su + sv * sv).sqrt();
        if norm < 1e-6 {
            // Axis into/out of camera — mark near origin with label only.
            draw_text_aa(img, (ox + 6) as f32, (oy - 14) as f32, label, 11.0, rgba);
            continue;
        }
        let sx = (su / norm * axis_len).round() as i32;
        let sy = (-sv / norm * axis_len).round() as i32; // image y down
        let x1 = ox + sx;
        let y1 = oy + sy;
        draw_line(img, ox, oy, x1, y1, rgba);
        // Arrow tip
        draw_line(img, x1, y1, x1 - sx / 5 + sy / 8, y1 - sy / 5 - sx / 8, rgba);
        draw_line(img, x1, y1, x1 - sx / 5 - sy / 8, y1 - sy / 5 + sx / 8, rgba);
        draw_text_aa(
            img,
            (x1 + sx.signum() * 2) as f32,
            (y1 - 10) as f32,
            label,
            11.0,
            rgba,
        );
    }
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
    let px = px.round().clamp(8.0, 48.0);
    let (ascent, descent) = line_metrics(px);
    let mut w = 0i32;
    for ch in text.chars() {
        // Skip unknowns that would leave empty tofu boxes looking "broken".
        if font.lookup_glyph_index(ch) == 0 && ch != ' ' {
            let (metrics, _) = font.rasterize('?', px);
            w += metrics.advance_width.round() as i32;
            continue;
        }
        let (metrics, _) = font.rasterize(ch, px);
        w += metrics.advance_width.round() as i32;
    }
    let h = (ascent.round() as i32) + (descent.round() as i32) + 2;
    (w.max(1), h.max(1))
}

/// Anti-aliased TrueType text at exact destination size (coverage blend).
///
/// `y` is the top of the line box. Pen and baseline are integers so every glyph
/// shares one alignment line — no float floor/round jitter in the Viewport.
fn draw_text_aa(img: &mut ContourImage, x: f32, y: f32, text: &str, px: f32, rgba: [u8; 4]) {
    let font = annotation_font();
    let px = px.round().clamp(8.0, 48.0);
    let (ascent, _descent) = line_metrics(px);
    let baseline = y.round() as i32 + ascent.round() as i32;
    let mut pen_x = x.round() as i32;
    for ch in text.chars() {
        let draw_ch = if font.lookup_glyph_index(ch) == 0 && ch != ' ' {
            '?'
        } else {
            ch
        };
        let (metrics, bitmap) = font.rasterize(draw_ch, px);
        // Integer glyph origin from shared baseline (fontdue ymin is relative to baseline).
        let glyph_x = pen_x + metrics.xmin;
        let glyph_y = baseline + metrics.ymin;
        for row in 0..metrics.height {
            for col in 0..metrics.width {
                let cover = bitmap[row * metrics.width + col];
                if cover == 0 {
                    continue;
                }
                let a = ((cover as u16 * rgba[3] as u16) / 255) as u8;
                if a == 0 {
                    continue;
                }
                blend_pixel(
                    img,
                    glyph_x + col as i32,
                    glyph_y + row as i32,
                    [rgba[0], rgba[1], rgba[2], a],
                );
            }
        }
        pen_x += metrics.advance_width.round().max(1.0) as i32;
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
