//! Wireframe mesh (zone grid / "crate") render to PNG.
//!
//! Default Tecplot-like first view: show the mesh with **equal aspect**
//! (no stretch), Contour off. Uses ordered IJ / IJK plane extraction.

use crate::contour::{
    draw_frame, draw_line, put_pixel, write_png, ContourImage, PlotStyle,
};
use crate::dataset::Dataset;
use crate::error::{Error, Result};
use crate::field::{extract_scalar_grid, find_var, guess_coord_vars, Grid2D, PlaneSpec};

/// Options for a mesh (wireframe) PNG.
#[derive(Debug, Clone)]
pub struct MeshOptions {
    pub zone: usize,
    pub plane: PlaneSpec,
    pub width: u32,
    pub height: u32,
    pub style: PlotStyle,
    /// Line RGBA. Default: green wireframe on white.
    pub line_rgba: [u8; 4],
    /// Background RGBA.
    pub bg_rgba: [u8; 4],
    /// Margin in pixels around the fitted domain.
    pub margin: u32,
}

/// Default mesh wireframe color (green).
pub const DEFAULT_MESH_RGBA: [u8; 4] = [34, 160, 60, 255];

impl Default for MeshOptions {
    fn default() -> Self {
        Self {
            zone: 0,
            plane: PlaneSpec::Auto,
            width: 800,
            height: 600,
            style: PlotStyle::Default,
            line_rgba: DEFAULT_MESH_RGBA,
            bg_rgba: [255, 255, 255, 255],
            margin: 24,
        }
    }
}

/// Equal-aspect map from world (x,y) into a raster with letterboxing.
#[derive(Debug, Clone, Copy)]
pub struct AspectMap {
    pub width: u32,
    pub height: u32,
    pub xmin: f64,
    pub xmax: f64,
    pub ymin: f64,
    pub ymax: f64,
    pub origin_x: f64,
    pub origin_y: f64,
    /// Pixels per world unit in X (and Y when `y_scale` is 0).
    pub scale: f64,
    /// Optional independent Y scale for thin-domain fill. `0` ⇒ use `scale`.
    pub y_scale: f64,
}

impl AspectMap {
    /// Fit `[xmin,xmax]×[ymin,ymax]` into `width×height` with equal aspect.
    ///
    /// If equal-aspect would leave a needle thinner than 8px (extreme aspect
    /// ratios / still-collapsed coords), fall back to fill/stretch so the
    /// mesh remains visible in the viewport.
    pub fn fit(
        width: u32,
        height: u32,
        xmin: f64,
        xmax: f64,
        ymin: f64,
        ymax: f64,
        margin: u32,
    ) -> Self {
        let m = margin as f64;
        let avail_w = (width as f64 - 2.0 * m).max(1.0);
        let avail_h = (height as f64 - 2.0 * m).max(1.0);
        let dx = (xmax - xmin).abs().max(1e-30);
        let dy = (ymax - ymin).abs().max(1e-30);
        let scale = (avail_w / dx).min(avail_h / dy);
        let used_w = dx * scale;
        let used_h = dy * scale;
        if used_w < 8.0 || used_h < 8.0 {
            return Self::fit_fill(width, height, xmin, xmax, ymin, ymax, margin);
        }
        let origin_x = m + (avail_w - used_w) * 0.5;
        let origin_y = m + (avail_h - used_h) * 0.5;
        Self {
            width,
            height,
            xmin,
            xmax,
            ymin,
            ymax,
            origin_x,
            origin_y,
            scale,
            y_scale: 0.0,
        }
    }

    /// Stretch independently in X and Y to fill the drawable area (auto-fit).
    pub fn fit_fill(
        width: u32,
        height: u32,
        xmin: f64,
        xmax: f64,
        ymin: f64,
        ymax: f64,
        margin: u32,
    ) -> Self {
        let m = margin as f64;
        let avail_w = (width as f64 - 2.0 * m).max(1.0);
        let avail_h = (height as f64 - 2.0 * m).max(1.0);
        let dx = (xmax - xmin).abs().max(1e-30);
        let dy = (ymax - ymin).abs().max(1e-30);
        Self {
            width,
            height,
            xmin,
            xmax,
            ymin,
            ymax,
            origin_x: m,
            origin_y: m,
            scale: avail_w / dx,
            y_scale: avail_h / dy,
        }
    }

    pub fn world_to_pixel(self, x: f64, y: f64) -> (i32, i32) {
        let sx = self.scale;
        let sy = if self.y_scale > 0.0 {
            self.y_scale
        } else {
            self.scale
        };
        let px = self.origin_x + (x - self.xmin) * sx;
        // Image y grows downward; world y grows upward.
        let py = self.origin_y + (self.ymax - y) * sy;
        (px.round() as i32, py.round() as i32)
    }
}

/// Extract coordinate grid for mesh drawing (values unused; prefer X if present).
pub fn extract_mesh_grid(ds: &Dataset, zone: usize, plane: PlaneSpec) -> Result<Grid2D> {
    let (x_i, _y_i, _) = guess_coord_vars(ds);
    let var_idx = x_i.unwrap_or(0);
    if ds.variables.is_empty() {
        return Err(Error::plot("mesh requires at least one variable (coordinates)"));
    }
    // Prefer a real coordinate field; fall back to first variable.
    let idx = if x_i.is_some() {
        var_idx
    } else {
        0
    };
    let _ = find_var(ds, &ds.variables[idx])?;
    extract_scalar_grid(ds, zone, idx, plane)
}

/// Render a wireframe mesh PNG with undistorted (equal) aspect.
pub fn render_mesh(ds: &Dataset, opts: &MeshOptions) -> Result<ContourImage> {
    if opts.width == 0 || opts.height == 0 {
        return Err(Error::plot("mesh width/height must be > 0"));
    }
    let grid = extract_mesh_grid(ds, opts.zone, opts.plane)?;
    if grid.ni < 1 || grid.nj < 1 {
        return Err(Error::plot("mesh grid is empty"));
    }
    let (xmin, xmax, ymin, ymax) = grid.bounds();
    let map = AspectMap::fit(
        opts.width,
        opts.height,
        xmin,
        xmax,
        ymin,
        ymax,
        opts.margin,
    );

    let mut rgba = vec![0u8; (opts.width as usize) * (opts.height as usize) * 4];
    for px in rgba.chunks_exact_mut(4) {
        px.copy_from_slice(&opts.bg_rgba);
    }

    let mut img = ContourImage {
        width: opts.width,
        height: opts.height,
        rgba,
        vmin: 0.0,
        vmax: 1.0,
        grid: grid.clone(),
    };

    draw_mesh_lines(&mut img, &grid, map, opts.line_rgba);

    if opts.style.draw_frame() {
        draw_frame(&mut img, opts.style.frame_rgba(), opts.style.frame_thickness());
    }
    Ok(img)
}

/// Overlay mesh lines on an existing image using that image's stretched mapping
/// when `equal_aspect` is false, or letterboxed equal aspect when true.
pub fn overlay_mesh(
    img: &mut ContourImage,
    grid: &Grid2D,
    equal_aspect: bool,
    margin: u32,
    line_rgba: [u8; 4],
) {
    let (xmin, xmax, ymin, ymax) = grid.bounds();
    if equal_aspect {
        let map = AspectMap::fit(img.width, img.height, xmin, xmax, ymin, ymax, margin);
        draw_mesh_lines(img, grid, map, line_rgba);
    } else {
        // Stretch to fill (legacy contour mapping).
        for j in 0..grid.nj {
            for i in 0..grid.ni.saturating_sub(1) {
                let (x0, y0) = grid.xy(i, j);
                let (x1, y1) = grid.xy(i + 1, j);
                let (px0, py0) = stretch_to_pixel(img, xmin, xmax, ymin, ymax, x0, y0);
                let (px1, py1) = stretch_to_pixel(img, xmin, xmax, ymin, ymax, x1, y1);
                draw_line(img, px0, py0, px1, py1, line_rgba);
            }
        }
        for i in 0..grid.ni {
            for j in 0..grid.nj.saturating_sub(1) {
                let (x0, y0) = grid.xy(i, j);
                let (x1, y1) = grid.xy(i, j + 1);
                let (px0, py0) = stretch_to_pixel(img, xmin, xmax, ymin, ymax, x0, y0);
                let (px1, py1) = stretch_to_pixel(img, xmin, xmax, ymin, ymax, x1, y1);
                draw_line(img, px0, py0, px1, py1, line_rgba);
            }
        }
    }
}

fn stretch_to_pixel(
    img: &ContourImage,
    xmin: f64,
    xmax: f64,
    ymin: f64,
    ymax: f64,
    x: f64,
    y: f64,
) -> (i32, i32) {
    let u = (x - xmin) / (xmax - xmin).max(1e-30);
    let v = (y - ymin) / (ymax - ymin).max(1e-30);
    let px = (u * img.width as f64) as i32;
    let py = ((1.0 - v) * img.height as f64) as i32;
    (px, py)
}

fn draw_mesh_lines(img: &mut ContourImage, grid: &Grid2D, map: AspectMap, rgba: [u8; 4]) {
    // I-lines (constant j)
    for j in 0..grid.nj {
        for i in 0..grid.ni.saturating_sub(1) {
            let (x0, y0) = grid.xy(i, j);
            let (x1, y1) = grid.xy(i + 1, j);
            let (px0, py0) = map.world_to_pixel(x0, y0);
            let (px1, py1) = map.world_to_pixel(x1, y1);
            draw_line(img, px0, py0, px1, py1, rgba);
        }
    }
    // J-lines (constant i)
    for i in 0..grid.ni {
        for j in 0..grid.nj.saturating_sub(1) {
            let (x0, y0) = grid.xy(i, j);
            let (x1, y1) = grid.xy(i, j + 1);
            let (px0, py0) = map.world_to_pixel(x0, y0);
            let (px1, py1) = map.world_to_pixel(x1, y1);
            draw_line(img, px0, py0, px1, py1, rgba);
        }
    }
    // Mark nodes lightly on tiny grids (helps sanity checks).
    if grid.ni * grid.nj <= 64 {
        for j in 0..grid.nj {
            for i in 0..grid.ni {
                let (x, y) = grid.xy(i, j);
                let (px, py) = map.world_to_pixel(x, y);
                put_pixel(img, px, py, rgba);
            }
        }
    }
}

/// Convenience: render + write PNG.
pub fn write_mesh_png(path: &std::path::Path, ds: &Dataset, opts: &MeshOptions) -> Result<()> {
    let img = render_mesh(ds, opts)?;
    write_png(path, &img)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::{Location, OrderedDims, Topology, VariableData, Zone};

    fn ds() -> Dataset {
        Dataset {
            title: None,
            variables: vec!["X".into(), "Y".into(), "P".into()],
            zones: vec![Zone {
                name: "z".into(),
                topology: Topology::Ordered(OrderedDims::Ij { i: 3, j: 2 }),
                fields: vec![
                    VariableData {
                        location: Location::Nodal,
                        values: vec![0.0, 2.0, 4.0, 0.0, 2.0, 4.0],
                    },
                    VariableData {
                        location: Location::Nodal,
                        values: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
                    },
                    VariableData {
                        location: Location::Nodal,
                        values: vec![0.0; 6],
                    },
                ],
            }],
        }
    }

    #[test]
    fn aspect_map_preserves_ratio() {
        // Domain 4×1 into 200×100 with margin 0 → scale limited by height? 
        // avail 200×100, dx=4 dy=1 → scale_w=50, scale_h=100 → scale=50
        // used 200×50, letterbox vertically.
        let m = AspectMap::fit(200, 100, 0.0, 4.0, 0.0, 1.0, 0);
        assert!((m.scale - 50.0).abs() < 1e-9);
        let (px0, py0) = m.world_to_pixel(0.0, 0.0);
        let (px1, _) = m.world_to_pixel(4.0, 0.0);
        let (_, py_top) = m.world_to_pixel(0.0, 1.0);
        assert!((px1 - px0).abs() == 200 || (px1 - px0).abs() == 199);
        // Height span should be ~100 (scale*dy) but centered → py0-py_top ≈ 50
        assert!((py0 - py_top).abs() >= 49 && (py0 - py_top).abs() <= 51);
    }

    #[test]
    fn render_mesh_png_bytes() {
        let img = render_mesh(
            &ds(),
            &MeshOptions {
                width: 120,
                height: 80,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(img.rgba.len(), 120 * 80 * 4);
        // Background should be mostly white; green mesh strokes exist.
        let greenish = img
            .rgba
            .chunks_exact(4)
            .filter(|p| p[1] > p[0].saturating_add(20) && p[1] > p[2].saturating_add(20))
            .count();
        assert!(greenish > 10, "expected green mesh strokes, got {greenish}");
    }
}
