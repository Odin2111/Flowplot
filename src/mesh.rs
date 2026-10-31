//! Wireframe mesh (zone grid / "crate") render to PNG.
//!
//! Default first view: show the mesh with **equal aspect**
//! (no stretch), Contour off. Ordered IJ / IJK use plane extraction;
//! finite-element zones draw projected element edges (LEHAR tet/hex combined.plt).

use std::collections::HashSet;

use crate::annotate::{draw_annotations, AnnotationInfo};
use crate::contour::{
    draw_frame, draw_line, put_pixel, write_png, ContourImage, PlotStyle,
};
use crate::dataset::{Dataset, ElementType, Topology};
use crate::error::{Error, Result};
use crate::field::{
    extract_scalar_grid_view, find_var, guess_coord_vars, resolve_view, Grid2D, PlaneSpec,
};
use crate::view::ViewSpec;

/// Options for a mesh (wireframe) PNG.
#[derive(Debug, Clone)]
pub struct MeshOptions {
    pub zone: usize,
    pub plane: PlaneSpec,
    pub view: ViewSpec,
    pub width: u32,
    pub height: u32,
    pub style: PlotStyle,
    /// Line RGBA. Default: green wireframe on white.
    pub line_rgba: [u8; 4],
    /// Background RGBA.
    pub bg_rgba: [u8; 4],
    /// Margin in pixels around the fitted domain.
    pub margin: u32,
    /// When true, stretch to fill the raster (no letterbox). GUI viewport uses this.
    pub fill: bool,
    /// Screen-space domain pan (pixels); positive moves domain right/down.
    pub pan_x: f64,
    pub pan_y: f64,
    /// VisIt-like title / time / zone overlay (default on).
    pub annotate: bool,
    pub path_hint: Option<String>,
}

/// Default mesh wireframe color (green).
pub const DEFAULT_MESH_RGBA: [u8; 4] = [34, 160, 60, 255];

impl Default for MeshOptions {
    fn default() -> Self {
        Self {
            zone: 0,
            plane: PlaneSpec::Auto,
            view: ViewSpec::Auto,
            width: 800,
            height: 600,
            style: PlotStyle::Default,
            line_rgba: DEFAULT_MESH_RGBA,
            bg_rgba: [255, 255, 255, 255],
            margin: 8,
            fill: false,
            pan_x: 0.0,
            pan_y: 0.0,
            annotate: true,
            path_hint: None,
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
    /// Screen-space pan (pixels); positive moves domain right/down.
    pub pan_x: f64,
    pub pan_y: f64,
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
            pan_x: 0.0,
            pan_y: 0.0,
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
            pan_x: 0.0,
            pan_y: 0.0,
        }
    }

    pub fn world_to_pixel(self, x: f64, y: f64) -> (i32, i32) {
        let sx = self.scale;
        let sy = if self.y_scale > 0.0 {
            self.y_scale
        } else {
            self.scale
        };
        let px = self.origin_x + (x - self.xmin) * sx + self.pan_x;
        // Image y grows downward; world y grows upward.
        let py = self.origin_y + (self.ymax - y) * sy + self.pan_y;
        (px.round() as i32, py.round() as i32)
    }
}

/// Extract coordinate grid for mesh drawing (values unused; prefer X if present).
pub fn extract_mesh_grid(ds: &Dataset, zone: usize, plane: PlaneSpec) -> Result<Grid2D> {
    extract_mesh_grid_view(ds, zone, plane, ViewSpec::Auto)
}

/// Extract coordinate grid and project with `view`.
pub fn extract_mesh_grid_view(
    ds: &Dataset,
    zone: usize,
    plane: PlaneSpec,
    view: ViewSpec,
) -> Result<Grid2D> {
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
    extract_scalar_grid_view(ds, zone, idx, plane, view)
}

/// Render a wireframe mesh PNG with undistorted (equal) aspect.
pub fn render_mesh(ds: &Dataset, opts: &MeshOptions) -> Result<ContourImage> {
    if opts.width == 0 || opts.height == 0 {
        return Err(Error::plot("mesh width/height must be > 0"));
    }
    let zone = ds
        .zones
        .get(opts.zone)
        .ok_or_else(|| Error::plot(format!("zone index {} out of range", opts.zone)))?;
    if matches!(zone.topology, Topology::FiniteElement { .. }) {
        return render_fe_mesh(ds, opts);
    }

    let grid = extract_mesh_grid_view(ds, opts.zone, opts.plane, opts.view)?;
    if grid.ni < 1 || grid.nj < 1 {
        return Err(Error::plot("mesh grid is empty"));
    }
    let (xmin, xmax, ymin, ymax) = grid.bounds();
    let mut map = if opts.fill {
        AspectMap::fit_fill(
            opts.width,
            opts.height,
            xmin,
            xmax,
            ymin,
            ymax,
            opts.margin,
        )
    } else {
        AspectMap::fit(
            opts.width,
            opts.height,
            xmin,
            xmax,
            ymin,
            ymax,
            opts.margin,
        )
    };
    map.pan_x = opts.pan_x;
    map.pan_y = opts.pan_y;

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
        pan_x: opts.pan_x,
        pan_y: opts.pan_y,
    };

    draw_mesh_lines(&mut img, &grid, map, opts.line_rgba);

    if opts.style.draw_frame() {
        draw_frame(&mut img, opts.style.frame_rgba(), opts.style.frame_thickness());
    }
    if opts.annotate {
        let camera = resolve_view(ds, opts.zone, opts.plane, opts.view).unwrap_or(opts.view);
        let mut info =
            AnnotationInfo::from_dataset(ds, opts.zone, None, opts.path_hint.as_deref());
        info.colorbar = false;
        info.view = Some(camera.label());
        info.view_orient = camera;
        info.plane = Some(match opts.plane {
            PlaneSpec::Auto => "auto".into(),
            PlaneSpec::Index { axis, index } => format!("{axis}={index}"),
        });
        draw_annotations(&mut img, &info);
    }
    Ok(img)
}

fn element_edge_pairs(et: ElementType) -> &'static [(usize, usize)] {
    match et {
        ElementType::Line => &[(0, 1)],
        ElementType::Triangle => &[(0, 1), (1, 2), (2, 0)],
        ElementType::Quad => &[(0, 1), (1, 2), (2, 3), (3, 0)],
        ElementType::Tetra => &[
            (0, 1),
            (0, 2),
            (0, 3),
            (1, 2),
            (1, 3),
            (2, 3),
        ],
        ElementType::Hexa => &[
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0),
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4),
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
        ],
        ElementType::Pyramid => &[
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0),
            (0, 4),
            (1, 4),
            (2, 4),
            (3, 4),
        ],
        ElementType::Wedge => &[
            (0, 1),
            (1, 2),
            (2, 0),
            (3, 4),
            (4, 5),
            (5, 3),
            (0, 3),
            (1, 4),
            (2, 5),
        ],
        ElementType::Mixed => &[],
    }
}

fn fe_node_coord(zone: &crate::dataset::Zone, var_idx: Option<usize>, node: usize) -> f64 {
    let Some(vi) = var_idx else {
        return 0.0;
    };
    zone.fields
        .get(vi)
        .and_then(|f| f.values.get(node).copied())
        .unwrap_or(0.0)
}

/// Project FE element edges into screen space and draw a wireframe.
fn render_fe_mesh(ds: &Dataset, opts: &MeshOptions) -> Result<ContourImage> {
    let zone = &ds.zones[opts.zone];
    let Topology::FiniteElement {
        num_nodes,
        connectivity,
    } = &zone.topology
    else {
        return Err(Error::plot("internal: expected finite-element zone"));
    };
    if *num_nodes == 0 {
        return Err(Error::plot("finite-element zone has no nodes"));
    }
    let Some(npe) = connectivity.nodes_per_elem else {
        return Err(Error::plot(
            "mixed finite-element connectivity is not supported for mesh wireframe",
        ));
    };
    if npe == 0 || connectivity.indices.len() < npe {
        return Err(Error::plot("finite-element connectivity is empty"));
    }

    let (x_i, y_i, z_i) = guess_coord_vars(ds);
    let camera = resolve_view(ds, opts.zone, opts.plane, opts.view)?;
    let mut projected = Vec::with_capacity(*num_nodes);
    let mut xmin = f64::INFINITY;
    let mut xmax = f64::NEG_INFINITY;
    let mut ymin = f64::INFINITY;
    let mut ymax = f64::NEG_INFINITY;
    for node in 0..*num_nodes {
        let wx = fe_node_coord(zone, x_i, node);
        let wy = fe_node_coord(zone, y_i, node);
        let wz = fe_node_coord(zone, z_i, node);
        let (u, v) = camera.project(wx, wy, wz);
        xmin = xmin.min(u);
        xmax = xmax.max(u);
        ymin = ymin.min(v);
        ymax = ymax.max(v);
        projected.push((u, v));
    }
    if !xmin.is_finite() || !xmax.is_finite() || !ymin.is_finite() || !ymax.is_finite() {
        return Err(Error::plot("finite-element coordinates are non-finite"));
    }
    if (xmax - xmin).abs() < 1e-30 {
        xmax = xmin + 1.0;
    }
    if (ymax - ymin).abs() < 1e-30 {
        ymax = ymin + 1.0;
    }

    let mut map = if opts.fill {
        AspectMap::fit_fill(
            opts.width,
            opts.height,
            xmin,
            xmax,
            ymin,
            ymax,
            opts.margin,
        )
    } else {
        AspectMap::fit(
            opts.width,
            opts.height,
            xmin,
            xmax,
            ymin,
            ymax,
            opts.margin,
        )
    };
    map.pan_x = opts.pan_x;
    map.pan_y = opts.pan_y;

    let mut rgba = vec![0u8; (opts.width as usize) * (opts.height as usize) * 4];
    for px in rgba.chunks_exact_mut(4) {
        px.copy_from_slice(&opts.bg_rgba);
    }
    // FE has no structured I×J; expose node span as an I×1 line for status/HUD.
    let mut img = ContourImage {
        width: opts.width,
        height: opts.height,
        rgba,
        vmin: 0.0,
        vmax: 1.0,
        grid: Grid2D {
            ni: (*num_nodes).max(1),
            nj: 1,
            x: projected.iter().map(|(u, _)| *u).collect(),
            y: projected.iter().map(|(_, v)| *v).collect(),
            values: vec![0.0; *num_nodes],
        },
        pan_x: opts.pan_x,
        pan_y: opts.pan_y,
    };

    let pairs = element_edge_pairs(connectivity.element_type);
    if pairs.is_empty() {
        return Err(Error::plot(
            "unsupported finite-element type for mesh wireframe",
        ));
    }
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    let nelem = connectivity.indices.len() / npe;
    for e in 0..nelem {
        let base = e * npe;
        for &(a, b) in pairs {
            if a >= npe || b >= npe {
                continue;
            }
            let na = connectivity.indices[base + a];
            let nb = connectivity.indices[base + b];
            if na >= *num_nodes || nb >= *num_nodes {
                continue;
            }
            let key = if na < nb { (na, nb) } else { (nb, na) };
            if !seen.insert(key) {
                continue;
            }
            let (x0, y0) = projected[na];
            let (x1, y1) = projected[nb];
            let (px0, py0) = map.world_to_pixel(x0, y0);
            let (px1, py1) = map.world_to_pixel(x1, y1);
            draw_line(&mut img, px0, py0, px1, py1, opts.line_rgba);
        }
    }

    if opts.style.draw_frame() {
        draw_frame(&mut img, opts.style.frame_rgba(), opts.style.frame_thickness());
    }
    if opts.annotate {
        let mut info =
            AnnotationInfo::from_dataset(ds, opts.zone, None, opts.path_hint.as_deref());
        info.colorbar = false;
        info.view = Some(camera.label());
        info.view_orient = camera;
        info.plane = Some(match opts.plane {
            PlaneSpec::Auto => "fe-edges".into(),
            PlaneSpec::Index { axis, index } => format!("{axis}={index}"),
        });
        draw_annotations(&mut img, &info);
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
        let mut map = AspectMap::fit(img.width, img.height, xmin, xmax, ymin, ymax, margin);
        map.pan_x = img.pan_x;
        map.pan_y = img.pan_y;
        draw_mesh_lines(img, grid, map, line_rgba);
    } else {
        // Stretch to fill (legacy contour mapping) — honor ContourImage pan.
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
    let px = (u * img.width as f64 + img.pan_x).round() as i32;
    let py = ((1.0 - v) * img.height as f64 + img.pan_y).round() as i32;
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
            ..Default::default()
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
