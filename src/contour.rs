//! Scalar contour / color-field render to PNG.

use std::path::Path;
use std::str::FromStr;

use crate::dataset::Dataset;
use crate::error::{Error, Result};
use crate::annotate::{draw_annotations, AnnotationInfo};
use crate::field::{extract_scalar_grid_view, find_var, Grid2D, PlaneSpec};
use crate::view::ViewSpec;

/// Publication / screen look for contour and pathline PNGs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlotStyle {
    /// Saturated Turbo colormap, dark NaN fill (default screen look).
    #[default]
    Default,
    /// Journal of Fluid Mechanics–like: restrained coolwarm, white exterior,
    /// black frame, thin black pathlines. Single-column aspect when size is left
    /// at the CLI default (512×512 → 700×525).
    Jfm,
}

impl FromStr for PlotStyle {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "default" | "turbo" => Ok(Self::Default),
            "jfm" => Ok(Self::Jfm),
            other => Err(format!("unknown plot style '{other}' (expected default|jfm)")),
        }
    }
}

/// Contour fill mode (Tecplot Contour Details → Levels and Color).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContourColoring {
    /// Smooth continuous colormap (default).
    #[default]
    Continuous,
    /// Discrete bands at `nlevels` steps.
    Banded,
}

impl FromStr for ContourColoring {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "continuous" | "flood" | "smooth" => Ok(Self::Continuous),
            "banded" | "bands" | "steps" => Ok(Self::Banded),
            other => Err(format!(
                "unknown coloring '{other}' (expected continuous|banded)"
            )),
        }
    }
}

/// Named colormap override (Tecplot Multi-Coloring).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorMapName {
    #[default]
    StyleDefault,
    Turbo,
    Jfm,
    Coolwarm,
    Grayscale,
}

impl FromStr for ColorMapName {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "default" | "auto" | "" => Ok(Self::StyleDefault),
            "turbo" => Ok(Self::Turbo),
            "jfm" => Ok(Self::Jfm),
            "coolwarm" | "cool-warm" => Ok(Self::Coolwarm),
            "gray" | "grey" | "grayscale" | "greyscale" => Ok(Self::Grayscale),
            other => Err(format!(
                "unknown colormap '{other}' (expected default|turbo|jfm|coolwarm|grayscale)"
            )),
        }
    }
}

impl PlotStyle {
    /// Suggested raster size for a single-column JFM figure (~88 mm at ~200 dpi).
    pub const JFM_WIDTH: u32 = 700;
    pub const JFM_HEIGHT: u32 = 525;

    pub fn colormap(self, t: f64) -> (u8, u8, u8) {
        match self {
            Self::Default => colormap_turbo(t),
            Self::Jfm => colormap_jfm(t),
        }
    }

    pub fn colormap_named(self, name: ColorMapName, t: f64) -> (u8, u8, u8) {
        match name {
            ColorMapName::StyleDefault => self.colormap(t),
            ColorMapName::Turbo => colormap_turbo(t),
            ColorMapName::Jfm => colormap_jfm(t),
            ColorMapName::Coolwarm => colormap_coolwarm(t),
            ColorMapName::Grayscale => colormap_gray(t),
        }
    }

    pub fn nan_rgba(self) -> [u8; 4] {
        match self {
            Self::Default => [20, 20, 20, 255],
            Self::Jfm => [255, 255, 255, 255],
        }
    }

    pub fn pathline_rgba(self) -> [u8; 4] {
        match self {
            Self::Default => [10, 10, 10, 255],
            Self::Jfm => [0, 0, 0, 255],
        }
    }

    pub fn pathline_width(self) -> u32 {
        match self {
            Self::Default => 1,
            Self::Jfm => 1,
        }
    }

    pub fn draw_frame(self) -> bool {
        matches!(self, Self::Jfm)
    }

    pub fn frame_rgba(self) -> [u8; 4] {
        [0, 0, 0, 255]
    }

    pub fn frame_thickness(self) -> u32 {
        2
    }

    /// CSS / matplotlib-oriented defaults for a future GUI or companion plots.
    pub fn css_defaults(self) -> &'static str {
        match self {
            Self::Default => {
                "--fp-font-family: system-ui, sans-serif;\n\
                 --fp-axis-color: #141414;\n\
                 --fp-axis-lw: 1.0px;\n\
                 --fp-pathline-lw: 1.0px;\n\
                 --fp-colormap: turbo;\n\
                 --fp-bg: #141414;"
            }
            Self::Jfm => {
                "--fp-font-family: \"Times New Roman\", Times, \"Liberation Serif\", serif;\n\
                 --fp-math-font: \"STIX Two Math\", \"Latin Modern Math\", serif;\n\
                 --fp-label-size: 9pt;\n\
                 --fp-tick-size: 8pt;\n\
                 --fp-axis-color: #000000;\n\
                 --fp-axis-lw: 0.75pt;\n\
                 --fp-pathline-lw: 0.5pt;\n\
                 --fp-colormap: coolwarm-restrained;\n\
                 --fp-bg: #ffffff;\n\
                 --fp-fig-width: 88mm;\n\
                 --fp-dpi: 300;"
            }
        }
    }
}

/// Options for rendering a scalar contour of any variable present in the dataset.
#[derive(Debug, Clone)]
pub struct ContourOptions {
    pub variable: String,
    pub zone: usize,
    pub plane: PlaneSpec,
    pub view: ViewSpec,
    pub vmin: Option<f64>,
    pub vmax: Option<f64>,
    pub width: u32,
    pub height: u32,
    pub style: PlotStyle,
    /// VisIt-like title / time / variable / colorbar overlay (default on).
    pub annotate: bool,
    pub path_hint: Option<String>,
    /// Continuous flood vs banded levels (Tecplot Contour Details).
    pub coloring: ContourColoring,
    /// Number of contour bands / levels (used when banded; also HUD).
    pub nlevels: u32,
    /// Colormap override; `StyleDefault` follows `style`.
    pub colormap: ColorMapName,
    /// Draw the colorbar legend when annotating.
    pub legend: bool,
    /// Pixel margin around the filled plot (tight → no letterbox waste).
    pub margin: u32,
}

impl Default for ContourOptions {
    fn default() -> Self {
        Self {
            variable: String::new(),
            zone: 0,
            plane: PlaneSpec::Auto,
            view: ViewSpec::Auto,
            vmin: None,
            vmax: None,
            width: 512,
            height: 512,
            style: PlotStyle::Default,
            annotate: true,
            path_hint: None,
            coloring: ContourColoring::Continuous,
            nlevels: 16,
            colormap: ColorMapName::StyleDefault,
            legend: true,
            margin: 2,
        }
    }
}

/// RGBA raster of a scalar field (filled contour / pseudocolor).
#[derive(Debug, Clone)]
pub struct ContourImage {
    pub width: u32,
    pub height: u32,
    /// Length = width * height * 4 (RGBA).
    pub rgba: Vec<u8>,
    pub vmin: f64,
    pub vmax: f64,
    pub grid: Grid2D,
}

/// Thin API: build a contour image for `opts.variable`.
pub fn render_contour(ds: &Dataset, opts: &ContourOptions) -> Result<ContourImage> {
    if opts.variable.is_empty() {
        return Err(Error::plot("contour requires a variable name (--var)"));
    }
    if opts.width == 0 || opts.height == 0 {
        return Err(Error::plot("contour width/height must be > 0"));
    }
    let var_idx = find_var(ds, &opts.variable)?;
    let grid = extract_scalar_grid_view(ds, opts.zone, var_idx, opts.plane, opts.view)?;
    let (mut vmin, mut vmax) = data_range(&grid.values);
    if let Some(v) = opts.vmin {
        vmin = v;
    }
    if let Some(v) = opts.vmax {
        vmax = v;
    }
    if !vmin.is_finite() || !vmax.is_finite() {
        return Err(Error::plot("contour color range is non-finite"));
    }
    if (vmax - vmin).abs() < 1e-30 {
        vmax = vmin + 1.0;
    }

    let (xmin, xmax, ymin, ymax) = grid.bounds();
    let nan = opts.style.nan_rgba();
    // Reserve a right strip for the legend so the flood fills the plot area
    // without large unused letterbox margins (Tecplot Contour Details layout).
    // Slim legend strip — flood fills almost the whole Viewport.
    let legend_reserve = if opts.annotate && opts.legend {
        ((opts.width as f64) * 0.10).round().clamp(48.0, 88.0) as u32
    } else {
        0
    };
    let plot_w = opts.width.saturating_sub(legend_reserve).max(32);
    // Cell-driven raster: O(cells × pixels/cell) ≈ O(image).
    let mut rgba = vec![0u8; (opts.width as usize) * (opts.height as usize) * 4];
    for px in rgba.chunks_exact_mut(4) {
        px.copy_from_slice(&nan);
    }
    rasterize_cells(
        &grid,
        &mut rgba,
        opts.width,
        opts.height,
        plot_w,
        opts.margin,
        xmin,
        xmax,
        ymin,
        ymax,
        vmin,
        vmax,
        opts.style,
        opts.colormap,
        opts.coloring,
        opts.nlevels,
        nan,
    );

    let mut img = ContourImage {
        width: opts.width,
        height: opts.height,
        rgba,
        vmin,
        vmax,
        grid,
    };
    if opts.style.draw_frame() {
        draw_frame(&mut img, opts.style.frame_rgba(), opts.style.frame_thickness());
    }
    if opts.annotate {
        let mut info = AnnotationInfo::from_dataset(
            ds,
            opts.zone,
            Some(&opts.variable),
            opts.path_hint.as_deref(),
        );
        info.colorbar = opts.legend;
        info.vmin = Some(vmin);
        info.vmax = Some(vmax);
        info.style = opts.style;
        info.colormap = opts.colormap;
        info.nlevels = if opts.coloring == ContourColoring::Banded {
            Some(opts.nlevels.max(2))
        } else {
            None
        };
        info.view = Some(opts.view.label());
        info.plane = Some(match opts.plane {
            PlaneSpec::Auto => "auto".into(),
            PlaneSpec::Index { axis, index } => format!("{axis}={index}"),
        });
        draw_annotations(&mut img, &info);
    }
    Ok(img)
}

/// Write contour (or overlay) raster as PNG.
pub fn write_png(path: &Path, img: &ContourImage) -> Result<()> {
    write_rgba_png(path, img.width, img.height, &img.rgba)
}

pub fn write_rgba_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<()> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| Error::plot("PNG dimensions overflow"))?;
    if rgba.len() != expected {
        return Err(Error::plot(format!(
            "RGBA buffer length {} != {}",
            rgba.len(),
            expected
        )));
    }
    let file = std::fs::File::create(path).map_err(|e| Error::io(path, e))?;
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| Error::plot(format!("PNG header: {e}")))?;
    writer
        .write_image_data(rgba)
        .map_err(|e| Error::plot(format!("PNG data: {e}")))?;
    Ok(())
}

fn data_range(values: &[f64]) -> (f64, f64) {
    let mut vmin = f64::INFINITY;
    let mut vmax = f64::NEG_INFINITY;
    for &v in values {
        if v.is_finite() {
            vmin = vmin.min(v);
            vmax = vmax.max(v);
        }
    }
    if !vmin.is_finite() {
        (0.0, 1.0)
    } else {
        (vmin, vmax)
    }
}

/// Paint each structured cell as two triangles into the image (fast interactive path).
///
/// Flood fills the left `plot_w` pixels (minus margin), leaving a right strip for
/// the legend so the Viewport shows data edge-to-edge without letterbox waste.
fn rasterize_cells(
    grid: &Grid2D,
    rgba: &mut [u8],
    width: u32,
    height: u32,
    plot_w: u32,
    margin: u32,
    xmin: f64,
    xmax: f64,
    ymin: f64,
    ymax: f64,
    vmin: f64,
    vmax: f64,
    style: PlotStyle,
    colormap: ColorMapName,
    coloring: ContourColoring,
    nlevels: u32,
    nan: [u8; 4],
) {
    if grid.ni < 2 || grid.nj < 2 {
        return;
    }
    let dx = (xmax - xmin).abs().max(1e-30);
    let dy = (ymax - ymin).abs().max(1e-30);
    let m = margin as f64;
    let avail_w = (plot_w as f64 - 2.0 * m).max(1.0);
    let avail_h = (height as f64 - 2.0 * m).max(1.0);
    // Independent X/Y scale = fill the plot rect (no unused letterbox).
    let sx = avail_w / dx;
    let sy = avail_h / dy;
    let to_px = |x: f64, y: f64| -> (f64, f64) {
        let px = m + (x - xmin) * sx;
        let py = m + (ymax - y) * sy;
        (px, py)
    };
    let span = (vmax - vmin).abs().max(1e-30);
    let levels = nlevels.max(2);

    for j in 0..grid.nj - 1 {
        for i in 0..grid.ni - 1 {
            let (x00, y00) = grid.xy(i, j);
            let (x10, y10) = grid.xy(i + 1, j);
            let (x01, y01) = grid.xy(i, j + 1);
            let (x11, y11) = grid.xy(i + 1, j + 1);
            let v00 = grid.value(i, j);
            let v10 = grid.value(i + 1, j);
            let v01 = grid.value(i, j + 1);
            let v11 = grid.value(i + 1, j + 1);
            let p00 = to_px(x00, y00);
            let p10 = to_px(x10, y10);
            let p01 = to_px(x01, y01);
            let p11 = to_px(x11, y11);
            fill_tri(
                rgba, width, height, plot_w, p00, p10, p11, v00, v10, v11, vmin, span, style,
                colormap, coloring, levels, nan,
            );
            fill_tri(
                rgba, width, height, plot_w, p00, p11, p01, v00, v11, v01, vmin, span, style,
                colormap, coloring, levels, nan,
            );
        }
    }
}

fn quantize_band(t: f64, levels: u32) -> f64 {
    let n = levels.max(2) as f64;
    let idx = (t.clamp(0.0, 1.0) * n).floor().min(n - 1.0);
    (idx + 0.5) / n
}

fn fill_tri(
    rgba: &mut [u8],
    width: u32,
    height: u32,
    plot_w: u32,
    a: (f64, f64),
    b: (f64, f64),
    c: (f64, f64),
    va: f64,
    vb: f64,
    vc: f64,
    vmin: f64,
    span: f64,
    style: PlotStyle,
    colormap: ColorMapName,
    coloring: ContourColoring,
    levels: u32,
    nan: [u8; 4],
) {
    let min_x = a.0.min(b.0).min(c.0).floor() as i32;
    let max_x = a.0.max(b.0).max(c.0).ceil() as i32;
    let min_y = a.1.min(b.1).min(c.1).floor() as i32;
    let max_y = a.1.max(b.1).max(c.1).ceil() as i32;
    let min_x = min_x.max(0);
    let min_y = min_y.max(0);
    let max_x = max_x.min(plot_w as i32 - 1).min(width as i32 - 1);
    let max_y = max_y.min(height as i32 - 1);
    if min_x > max_x || min_y > max_y {
        return;
    }
    let area = edge(a, b, c);
    if area.abs() < 1e-18 {
        return;
    }
    let inv = 1.0 / area;
    for py in min_y..=max_y {
        for px in min_x..=max_x {
            let p = (px as f64 + 0.5, py as f64 + 0.5);
            let w0 = edge(b, c, p) * inv;
            let w1 = edge(c, a, p) * inv;
            let w2 = edge(a, b, p) * inv;
            if w0 < -1e-9 || w1 < -1e-9 || w2 < -1e-9 {
                continue;
            }
            let val = w0 * va + w1 * vb + w2 * vc;
            let i = ((py as u32 * width + px as u32) * 4) as usize;
            if val.is_finite() {
                let mut t = ((val - vmin) / span).clamp(0.0, 1.0);
                if coloring == ContourColoring::Banded {
                    t = quantize_band(t, levels);
                }
                let (r, g, b) = style.colormap_named(colormap, t);
                rgba[i] = r;
                rgba[i + 1] = g;
                rgba[i + 2] = b;
                rgba[i + 3] = 255;
            } else {
                rgba[i..i + 4].copy_from_slice(&nan);
            }
        }
    }
}

#[inline]
fn edge(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    (c.0 - a.0) * (b.1 - a.1) - (c.1 - a.1) * (b.0 - a.0)
}

/// Turbo-like colormap (approximate), t in [0,1].
fn colormap_turbo(t: f64) -> (u8, u8, u8) {
    // Piecewise polynomial approximation of Google Turbo.
    let t = t.clamp(0.0, 1.0);
    let r = (34.61
        + t * (1172.33
            + t * (-10793.56
                + t * (33300.12 + t * (-38394.49 + t * 14825.05)))))
        .clamp(0.0, 255.0);
    let g = (23.31
        + t * (557.33 + t * (1225.33 + t * (-3574.96 + t * (2072.92 + t * 213.66)))))
        .clamp(0.0, 255.0);
    let b = (27.2
        + t * (3211.1
            + t * (-15327.97
                + t * (27814.0 + t * (-22569.18 + t * 6838.66)))))
        .clamp(0.0, 255.0);
    (r as u8, g as u8, b as u8)
}

fn colormap_gray(t: f64) -> (u8, u8, u8) {
    let g = (t.clamp(0.0, 1.0) * 255.0).round() as u8;
    (g, g, g)
}

fn colormap_coolwarm(t: f64) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    // Classic cool→warm (blue → white → red).
    if t < 0.5 {
        let u = t * 2.0;
        let r = (59.0 + u * (255.0 - 59.0)) as u8;
        let g = (76.0 + u * (255.0 - 76.0)) as u8;
        let b = (192.0 + u * (255.0 - 192.0)) as u8;
        (r, g, b)
    } else {
        let u = (t - 0.5) * 2.0;
        let r = (255.0 + u * (180.0 - 255.0)) as u8;
        let g = (255.0 + u * (4.0 - 255.0)) as u8;
        let b = (255.0 + u * (38.0 - 255.0)) as u8;
        (r, g, b)
    }
}

/// Restrained coolwarm (blue → soft gray → muted red) for JFM-style figures.
fn colormap_jfm(t: f64) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    // Anchors chosen to avoid neon saturation; mid is near paper white-gray.
    const STOPS: [(f64, (f64, f64, f64)); 5] = [
        (0.0, (33.0, 68.0, 120.0)),    // navy-blue
        (0.25, (95.0, 140.0, 185.0)),  // soft blue
        (0.5, (235.0, 235.0, 235.0)),  // light gray
        (0.75, (200.0, 120.0, 100.0)), // muted coral
        (1.0, (140.0, 45.0, 40.0)),    // brick red
    ];
    for w in STOPS.windows(2) {
        let (t0, c0) = w[0];
        let (t1, c1) = w[1];
        if t <= t1 || (t1 - t0).abs() < 1e-12 {
            let u = if (t1 - t0).abs() < 1e-12 {
                0.0
            } else {
                ((t - t0) / (t1 - t0)).clamp(0.0, 1.0)
            };
            let r = c0.0 + u * (c1.0 - c0.0);
            let g = c0.1 + u * (c1.1 - c0.1);
            let b = c0.2 + u * (c1.2 - c0.2);
            return (r as u8, g as u8, b as u8);
        }
    }
    (140, 45, 40)
}

/// Draw a rectangular frame (black axes look) around the image.
pub fn draw_frame(img: &mut ContourImage, rgba: [u8; 4], thickness: u32) {
    let t = thickness.max(1);
    let w = img.width;
    let h = img.height;
    for y in 0..h {
        for x in 0..w {
            let on_border = x < t || y < t || x >= w.saturating_sub(t) || y >= h.saturating_sub(t);
            if on_border {
                put_pixel(img, x as i32, y as i32, rgba);
            }
        }
    }
}

/// Map physical (x,y) in grid bounds to pixel coordinates (full-width stretch).
pub fn world_to_pixel(img: &ContourImage, x: f64, y: f64) -> (i32, i32) {
    world_to_pixel_plot(img, x, y, img.width, 0)
}

/// Map physical (x,y) into the filled plot rect (`plot_w` × height, margin 0).
pub fn world_to_pixel_plot(
    img: &ContourImage,
    x: f64,
    y: f64,
    plot_w: u32,
    margin: u32,
) -> (i32, i32) {
    let (xmin, xmax, ymin, ymax) = img.grid.bounds();
    let dx = (xmax - xmin).abs().max(1e-30);
    let dy = (ymax - ymin).abs().max(1e-30);
    let m = margin as f64;
    let avail_w = (plot_w as f64 - 2.0 * m).max(1.0);
    let avail_h = (img.height as f64 - 2.0 * m).max(1.0);
    let px = m + (x - xmin) / dx * avail_w;
    let py = m + (ymax - y) / dy * avail_h;
    (px.round() as i32, py.round() as i32)
}

/// Inverse of [`world_to_pixel_plot`] — pixel → world (for probe).
pub fn pixel_to_world(
    img: &ContourImage,
    px: i32,
    py: i32,
    plot_w: u32,
    margin: u32,
) -> (f64, f64) {
    let (xmin, xmax, ymin, ymax) = img.grid.bounds();
    let dx = (xmax - xmin).abs().max(1e-30);
    let dy = (ymax - ymin).abs().max(1e-30);
    let m = margin as f64;
    let avail_w = (plot_w as f64 - 2.0 * m).max(1.0);
    let avail_h = (img.height as f64 - 2.0 * m).max(1.0);
    let x = xmin + (px as f64 - m) / avail_w * dx;
    let y = ymax - (py as f64 - m) / avail_h * dy;
    (x, y)
}

/// Set a pixel if inside the image.
pub fn put_pixel(img: &mut ContourImage, x: i32, y: i32, rgba: [u8; 4]) {
    if x < 0 || y < 0 || x >= img.width as i32 || y >= img.height as i32 {
        return;
    }
    let i = ((y as u32 * img.width + x as u32) * 4) as usize;
    img.rgba[i..i + 4].copy_from_slice(&rgba);
}

/// Bresenham line in image space.
pub fn draw_line(img: &mut ContourImage, x0: i32, y0: i32, x1: i32, y1: i32, rgba: [u8; 4]) {
    let mut x0 = x0;
    let mut y0 = y0;
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        put_pixel(img, x0, y0, rgba);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::{Location, OrderedDims, Topology, VariableData, Zone};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn ds() -> Dataset {
        Dataset {
            title: None,
            variables: vec!["X".into(), "Y".into(), "P".into()],
            zones: vec![Zone {
                name: "z".into(),
                topology: Topology::Ordered(OrderedDims::Ij { i: 2, j: 2 }),
                fields: vec![
                    VariableData {
                        location: Location::Nodal,
                        values: vec![0.0, 1.0, 0.0, 1.0],
                    },
                    VariableData {
                        location: Location::Nodal,
                        values: vec![0.0, 0.0, 1.0, 1.0],
                    },
                    VariableData {
                        location: Location::Nodal,
                        values: vec![0.0, 1.0, 2.0, 3.0],
                    },
                ],
            }],
            ..Default::default()
        }
    }

    #[test]
    fn render_and_write_png() {
        let img = render_contour(
            &ds(),
            &ContourOptions {
                variable: "P".into(),
                width: 32,
                height: 32,
                vmin: Some(0.0),
                vmax: Some(3.0),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(img.rgba.len(), 32 * 32 * 4);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("flowplot-contour-{nanos}.png"));
        write_png(&path, &img).unwrap();
        assert!(path.metadata().unwrap().len() > 32);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn jfm_style_draws_black_frame() {
        let img = render_contour(
            &ds(),
            &ContourOptions {
                variable: "P".into(),
                width: 40,
                height: 30,
                style: PlotStyle::Jfm,
                vmin: Some(0.0),
                vmax: Some(3.0),
                ..Default::default()
            },
        )
        .unwrap();
        // Corner pixel should be black frame.
        assert_eq!(&img.rgba[0..4], &[0, 0, 0, 255]);
        assert!(PlotStyle::Jfm.css_defaults().contains("Times"));
        assert_eq!("jfm".parse::<PlotStyle>().unwrap(), PlotStyle::Jfm);
    }
}
