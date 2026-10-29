//! Lightweight Tecplot-like overlays: vectors, boundary edges, scatter, isolines.

use crate::contour::{draw_line, put_pixel, world_to_pixel, ContourImage};
use crate::field::Grid2D;

const EDGE_RGBA: [u8; 4] = [20, 24, 28, 255];
const VECTOR_RGBA: [u8; 4] = [30, 60, 120, 230];
const SCATTER_RGBA: [u8; 4] = [180, 40, 40, 255];
const ISOLINE_RGBA: [u8; 4] = [15, 15, 15, 255];

fn to_px(img: &ContourImage, x: f64, y: f64) -> (i32, i32) {
    // Match contour flood mapping (full-width when annotate deferred).
    world_to_pixel(img, x, y)
}

/// Draw isolines of the active scalar at `nlevels` mid-band values (2D iso-surfaces).
pub fn overlay_isolines(img: &mut ContourImage, nlevels: u32, rgba: Option<[u8; 4]>) {
    let color = rgba.unwrap_or(ISOLINE_RGBA);
    let n = nlevels.max(2) as usize;
    let vmin = img.vmin;
    let vmax = img.vmax;
    if !(vmax - vmin).is_finite() || (vmax - vmin).abs() < 1e-30 {
        return;
    }
    let levels: Vec<f64> = (0..n)
        .map(|i| vmin + (i as f64 + 0.5) / n as f64 * (vmax - vmin))
        .collect();
    let ni = img.grid.ni;
    let nj = img.grid.nj;
    if ni < 2 || nj < 2 {
        return;
    }
    for j in 0..nj.saturating_sub(1) {
        for i in 0..ni.saturating_sub(1) {
            let c00 = (img.grid.xy(i, j), img.grid.value(i, j));
            let c10 = (img.grid.xy(i + 1, j), img.grid.value(i + 1, j));
            let c11 = (img.grid.xy(i + 1, j + 1), img.grid.value(i + 1, j + 1));
            let c01 = (img.grid.xy(i, j + 1), img.grid.value(i, j + 1));
            let corners = [c00, c10, c11, c01];
            for &lev in &levels {
                draw_cell_isoline(img, &corners, lev, color);
            }
        }
    }
}

fn draw_cell_isoline(
    img: &mut ContourImage,
    corners: &[((f64, f64), f64); 4],
    lev: f64,
    color: [u8; 4],
) {
    let mut hits: Vec<(f64, f64)> = Vec::with_capacity(4);
    for k in 0..4 {
        let (p0, v0) = corners[k];
        let (p1, v1) = corners[(k + 1) % 4];
        if !v0.is_finite() || !v1.is_finite() {
            continue;
        }
        if (v0 - lev) * (v1 - lev) > 0.0 {
            continue;
        }
        if (v1 - v0).abs() < 1e-30 {
            continue;
        }
        let t = ((lev - v0) / (v1 - v0)).clamp(0.0, 1.0);
        hits.push((p0.0 + t * (p1.0 - p0.0), p0.1 + t * (p1.1 - p0.1)));
    }
    if hits.len() >= 2 {
        let (x0, y0) = to_px(img, hits[0].0, hits[0].1);
        let (x1, y1) = to_px(img, hits[1].0, hits[1].1);
        draw_line(img, x0, y0, x1, y1, color);
        if hits.len() >= 4 {
            let (x2, y2) = to_px(img, hits[2].0, hits[2].1);
            let (x3, y3) = to_px(img, hits[3].0, hits[3].1);
            draw_line(img, x2, y2, x3, y3, color);
        }
    }
}

/// Boundary wireframe only (outer I/J edges of the ordered grid).
pub fn overlay_boundary_edges(img: &mut ContourImage, grid: &Grid2D, rgba: Option<[u8; 4]>) {
    let color = rgba.unwrap_or(EDGE_RGBA);
    if grid.ni < 2 || grid.nj < 2 {
        return;
    }
    let last_i = grid.ni - 1;
    let last_j = grid.nj - 1;
    for i in 0..last_i {
        for &j in &[0usize, last_j] {
            let (x0, y0) = grid.xy(i, j);
            let (x1, y1) = grid.xy(i + 1, j);
            let (px0, py0) = to_px(img, x0, y0);
            let (px1, py1) = to_px(img, x1, y1);
            draw_line(img, px0, py0, px1, py1, color);
        }
    }
    for j in 0..last_j {
        for &i in &[0usize, last_i] {
            let (x0, y0) = grid.xy(i, j);
            let (x1, y1) = grid.xy(i, j + 1);
            let (px0, py0) = to_px(img, x0, y0);
            let (px1, py1) = to_px(img, x1, y1);
            draw_line(img, px0, py0, px1, py1, color);
        }
    }
}

/// Subsampled nodal scatter markers (optionally sized by skip).
pub fn overlay_scatter(img: &mut ContourImage, grid: &Grid2D, skip: usize, rgba: Option<[u8; 4]>) {
    let color = rgba.unwrap_or(SCATTER_RGBA);
    let step = skip.max(1);
    let r = 2i32;
    for j in (0..grid.nj).step_by(step) {
        for i in (0..grid.ni).step_by(step) {
            let (x, y) = grid.xy(i, j);
            let (px, py) = to_px(img, x, y);
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx * dx + dy * dy <= r * r {
                        put_pixel(img, px + dx, py + dy, color);
                    }
                }
            }
        }
    }
}

/// Simple velocity arrows from (U,V) grids (same IJ as the plot).
pub fn overlay_vectors(
    img: &mut ContourImage,
    u: &Grid2D,
    v: &Grid2D,
    skip: usize,
    rgba: Option<[u8; 4]>,
) {
    let color = rgba.unwrap_or(VECTOR_RGBA);
    if u.ni != v.ni || u.nj != v.nj || u.ni < 2 || u.nj < 2 {
        return;
    }
    let step = skip.max(1);
    let (xmin, xmax, ymin, ymax) = u.bounds();
    let diag = ((xmax - xmin).hypot(ymax - ymin)).max(1e-30);
    // Arrow length ~ few percent of domain diagonal.
    let mut umax: f64 = 1e-30;
    for j in (0..u.nj).step_by(step) {
        for i in (0..u.ni).step_by(step) {
            let uu = u.value(i, j);
            let vv = v.value(i, j);
            if uu.is_finite() && vv.is_finite() {
                umax = umax.max((uu * uu + vv * vv).sqrt());
            }
        }
    }
    let scale = (0.045 * diag) / umax;
    for j in (0..u.nj).step_by(step) {
        for i in (0..u.ni).step_by(step) {
            let uu = u.value(i, j);
            let vv = v.value(i, j);
            if !uu.is_finite() || !vv.is_finite() {
                continue;
            }
            let speed = (uu * uu + vv * vv).sqrt();
            if speed < 1e-12 * umax {
                continue;
            }
            let (x0, y0) = u.xy(i, j);
            let x1 = x0 + uu * scale;
            let y1 = y0 + vv * scale;
            let (px0, py0) = to_px(img, x0, y0);
            let (px1, py1) = to_px(img, x1, y1);
            draw_line(img, px0, py0, px1, py1, color);
            draw_arrow_head(img, px0, py0, px1, py1, color);
        }
    }
}

fn draw_arrow_head(img: &mut ContourImage, x0: i32, y0: i32, x1: i32, y1: i32, color: [u8; 4]) {
    let dx = (x1 - x0) as f64;
    let dy = (y1 - y0) as f64;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 4.0 {
        return;
    }
    let ux = dx / len;
    let uy = dy / len;
    let ah = (len * 0.28).clamp(4.0, 10.0);
    let px = -uy;
    let py = ux;
    let bx = x1 as f64 - ux * ah;
    let by = y1 as f64 - uy * ah;
    let (lx, ly) = (bx + px * ah * 0.45, by + py * ah * 0.45);
    let (rx, ry) = (bx - px * ah * 0.45, by - py * ah * 0.45);
    draw_line(img, x1, y1, lx.round() as i32, ly.round() as i32, color);
    draw_line(img, x1, y1, rx.round() as i32, ry.round() as i32, color);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::Grid2D;

    fn tiny_grid() -> Grid2D {
        Grid2D {
            ni: 3,
            nj: 3,
            x: vec![0.0, 1.0, 2.0, 0.0, 1.0, 2.0, 0.0, 1.0, 2.0],
            y: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0],
            values: vec![0.0, 1.0, 2.0, 1.0, 2.0, 3.0, 2.0, 3.0, 4.0],
        }
    }

    #[test]
    fn overlays_do_not_panic() {
        let g = tiny_grid();
        let mut img = ContourImage {
            width: 120,
            height: 100,
            rgba: vec![240u8; 120 * 100 * 4],
            vmin: 0.0,
            vmax: 4.0,
            grid: g.clone(),
        };
        overlay_isolines(&mut img, 4, None);
        overlay_boundary_edges(&mut img, &g, None);
        overlay_scatter(&mut img, &g, 1, None);
        overlay_vectors(&mut img, &g, &g, 1, None);
    }
}
