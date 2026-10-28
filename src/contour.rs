//! Scalar contour / color-field render to PNG.

use std::path::Path;

use crate::dataset::Dataset;
use crate::error::{Error, Result};
use crate::field::{extract_scalar_grid, find_var, Grid2D, PlaneSpec};

/// Options for rendering a scalar contour of any variable present in the dataset.
#[derive(Debug, Clone)]
pub struct ContourOptions {
    pub variable: String,
    pub zone: usize,
    pub plane: PlaneSpec,
    pub vmin: Option<f64>,
    pub vmax: Option<f64>,
    pub width: u32,
    pub height: u32,
}

impl Default for ContourOptions {
    fn default() -> Self {
        Self {
            variable: String::new(),
            zone: 0,
            plane: PlaneSpec::Auto,
            vmin: None,
            vmax: None,
            width: 512,
            height: 512,
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
    let grid = extract_scalar_grid(ds, opts.zone, var_idx, opts.plane)?;
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
    let mut rgba = vec![0u8; (opts.width as usize) * (opts.height as usize) * 4];
    for py in 0..opts.height {
        let v = 1.0 - (py as f64 + 0.5) / opts.height as f64;
        let y = ymin + v * (ymax - ymin);
        for px in 0..opts.width {
            let u = (px as f64 + 0.5) / opts.width as f64;
            let x = xmin + u * (xmax - xmin);
            let val = grid.sample(x, y).unwrap_or(f64::NAN);
            let (r, g, b) = if val.is_finite() {
                let t = ((val - vmin) / (vmax - vmin)).clamp(0.0, 1.0);
                colormap_turbo(t)
            } else {
                (20, 20, 20)
            };
            let i = ((py * opts.width + px) * 4) as usize;
            rgba[i] = r;
            rgba[i + 1] = g;
            rgba[i + 2] = b;
            rgba[i + 3] = 255;
        }
    }

    Ok(ContourImage {
        width: opts.width,
        height: opts.height,
        rgba,
        vmin,
        vmax,
        grid,
    })
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

/// Map physical (x,y) in grid bounds to pixel coordinates.
pub fn world_to_pixel(img: &ContourImage, x: f64, y: f64) -> (i32, i32) {
    let (xmin, xmax, ymin, ymax) = img.grid.bounds();
    let u = (x - xmin) / (xmax - xmin);
    let v = (y - ymin) / (ymax - ymin);
    let px = (u * img.width as f64) as i32;
    let py = ((1.0 - v) * img.height as f64) as i32;
    (px, py)
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
}
