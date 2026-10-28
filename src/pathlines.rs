//! Particle traces / streamlines along the velocity field (“pathlines” in Flowplot CLI).
//!
//! On a steady loaded dataset these are streamlines of (U,V[,W]); the user-facing
//! name is pathlines.

use crate::contour::{draw_line, world_to_pixel, ContourImage};
use crate::dataset::Dataset;
use crate::error::{Error, Result};
use crate::field::{extract_scalar_grid, find_var, Grid2D, PlaneSpec};

/// Seed and integration controls.
#[derive(Debug, Clone)]
pub struct PathlineOptions {
    pub u_var: String,
    pub v_var: String,
    pub w_var: Option<String>,
    pub zone: usize,
    pub plane: PlaneSpec,
    pub seeds: Vec<[f64; 2]>,
    pub max_steps: usize,
    pub step_size: f64,
    /// Integrate both forward (+dt) and backward (−dt) from each seed.
    pub bidirectional: bool,
}

impl Default for PathlineOptions {
    fn default() -> Self {
        Self {
            u_var: "U".into(),
            v_var: "V".into(),
            w_var: None,
            zone: 0,
            plane: PlaneSpec::Auto,
            seeds: Vec::new(),
            max_steps: 200,
            step_size: 0.02,
            bidirectional: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Pathline {
    pub points: Vec<[f64; 2]>,
}

/// Guess U/V variable names from the dataset when the user did not override.
pub fn guess_velocity_vars(ds: &Dataset) -> Result<(String, String)> {
    let u = first_var(
        ds,
        &["U", "Vx", "VX", "VelocityX", "x-velocity", "X-VELOCITY"],
    )
    .ok_or_else(|| Error::plot("could not guess U velocity variable; pass --u-var"))?;
    let v = first_var(
        ds,
        &["V", "Vy", "VY", "VelocityY", "y-velocity", "Y-VELOCITY"],
    )
    .ok_or_else(|| Error::plot("could not guess V velocity variable; pass --v-var"))?;
    Ok((u, v))
}

fn first_var(ds: &Dataset, cands: &[&str]) -> Option<String> {
    for c in cands {
        if let Some(v) = ds.variables.iter().find(|n| n.eq_ignore_ascii_case(c)) {
            return Some(v.clone());
        }
    }
    None
}

/// Trace pathlines on an ordered zone velocity field.
pub fn trace_pathlines(ds: &Dataset, opts: &PathlineOptions) -> Result<Vec<Pathline>> {
    if opts.seeds.is_empty() {
        return Err(Error::plot(
            "pathlines require at least one --seed x,y",
        ));
    }
    if opts.max_steps == 0 {
        return Err(Error::plot("pathlines --max-steps must be > 0"));
    }
    if !(opts.step_size.is_finite() && opts.step_size > 0.0) {
        return Err(Error::plot("pathlines --dt must be finite and > 0"));
    }

    let u_idx = find_var(ds, &opts.u_var)?;
    let v_idx = find_var(ds, &opts.v_var)?;
    let _w_idx = if let Some(w) = &opts.w_var {
        Some(find_var(ds, w)?)
    } else {
        // Optional W — used only if present and user asked; 2D plane integrates U,V.
        first_var(ds, &["W", "Vz", "VZ", "VelocityZ"]).and_then(|name| find_var(ds, &name).ok())
    };

    let base = extract_scalar_grid(ds, opts.zone, u_idx, opts.plane)?;
    let u_field = base.values.clone();
    let v_grid = extract_scalar_grid(ds, opts.zone, v_idx, opts.plane)?;
    if v_grid.ni != base.ni || v_grid.nj != base.nj {
        return Err(Error::plot("U/V grids disagree after plane extract"));
    }
    let v_field = v_grid.values;
    let grid = Grid2D {
        ni: base.ni,
        nj: base.nj,
        x: base.x,
        y: base.y,
        values: u_field.clone(),
    };

    let mut lines = Vec::new();
    for seed in &opts.seeds {
        let mut pts = Vec::new();
        if opts.bidirectional {
            let mut back = integrate(&grid, &u_field, &v_field, *seed, -opts.step_size, opts.max_steps);
            back.reverse();
            if let Some(last) = back.last() {
                if (last[0] - seed[0]).abs() < 1e-15 && (last[1] - seed[1]).abs() < 1e-15 {
                    back.pop();
                }
            }
            pts.extend(back);
        }
        let fwd = integrate(
            &grid,
            &u_field,
            &v_field,
            *seed,
            opts.step_size,
            opts.max_steps,
        );
        pts.extend(fwd);
        if pts.len() >= 2 {
            lines.push(Pathline { points: pts });
        }
    }
    Ok(lines)
}

fn integrate(
    grid: &Grid2D,
    u: &[f64],
    v: &[f64],
    seed: [f64; 2],
    dt: f64,
    max_steps: usize,
) -> Vec<[f64; 2]> {
    let mut pts = vec![seed];
    let mut x = seed[0];
    let mut y = seed[1];
    for _ in 0..max_steps {
        let Some((vx, vy)) = velocity_at(grid, u, v, x, y) else {
            break;
        };
        let speed = (vx * vx + vy * vy).sqrt();
        if !speed.is_finite() || speed < 1e-14 {
            break;
        }
        // RK2 (Heun)
        let x1 = x + dt * vx;
        let y1 = y + dt * vy;
        let Some((vx2, vy2)) = velocity_at(grid, u, v, x1, y1) else {
            break;
        };
        let nx = x + 0.5 * dt * (vx + vx2);
        let ny = y + 0.5 * dt * (vy + vy2);
        if !nx.is_finite() || !ny.is_finite() {
            break;
        }
        if velocity_at(grid, u, v, nx, ny).is_none() {
            break;
        }
        x = nx;
        y = ny;
        pts.push([x, y]);
    }
    pts
}

fn velocity_at(grid: &Grid2D, u: &[f64], v: &[f64], x: f64, y: f64) -> Option<(f64, f64)> {
    let uu = grid.sample_vec(u, x, y)?;
    let vv = grid.sample_vec(v, x, y)?;
    if uu.is_finite() && vv.is_finite() {
        Some((uu, vv))
    } else {
        None
    }
}

/// Overlay pathlines onto an existing contour image (black strokes).
pub fn overlay_pathlines(img: &mut ContourImage, lines: &[Pathline]) {
    const COLOR: [u8; 4] = [10, 10, 10, 255];
    for line in lines {
        for w in line.points.windows(2) {
            let (x0, y0) = world_to_pixel(img, w[0][0], w[0][1]);
            let (x1, y1) = world_to_pixel(img, w[1][0], w[1][1]);
            draw_line(img, x0, y0, x1, y1, COLOR);
        }
    }
}

/// Render pathlines alone on a light background using the scalar grid bounds.
pub fn render_pathlines_png(
    ds: &Dataset,
    opts: &PathlineOptions,
    width: u32,
    height: u32,
) -> Result<ContourImage> {
    let u_idx = find_var(ds, &opts.u_var)?;
    let grid = extract_scalar_grid(ds, opts.zone, u_idx, opts.plane)?;
    let mut rgba = vec![245u8; (width as usize) * (height as usize) * 4];
    for px in rgba.chunks_mut(4) {
        px[3] = 255;
    }
    let mut img = ContourImage {
        width,
        height,
        rgba,
        vmin: 0.0,
        vmax: 1.0,
        grid,
    };
    let lines = trace_pathlines(ds, opts)?;
    overlay_pathlines(&mut img, &lines);
    Ok(img)
}

/// Default seeds: a small rake across the mid-height of the grid.
pub fn default_seeds(grid: &Grid2D, count: usize) -> Vec<[f64; 2]> {
    let (xmin, xmax, ymin, ymax) = grid.bounds();
    let y = 0.5 * (ymin + ymax);
    let n = count.max(1);
    (0..n)
        .map(|i| {
            let t = (i as f64 + 0.5) / n as f64;
            let x = xmin + t * (xmax - xmin);
            [x, y]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::{Location, OrderedDims, Topology, VariableData, Zone};

    fn uniform_flow() -> Dataset {
        // 3x3 grid, U=1, V=0 → pathlines are horizontal.
        let mut x = Vec::new();
        let mut y = Vec::new();
        let mut u = Vec::new();
        let mut v = Vec::new();
        for j in 0..3 {
            for i in 0..3 {
                x.push(i as f64);
                y.push(j as f64);
                u.push(1.0);
                v.push(0.0);
            }
        }
        Dataset {
            title: None,
            variables: vec!["X".into(), "Y".into(), "U".into(), "V".into()],
            zones: vec![Zone {
                name: "z".into(),
                topology: Topology::Ordered(OrderedDims::Ij { i: 3, j: 3 }),
                fields: vec![
                    VariableData {
                        location: Location::Nodal,
                        values: x,
                    },
                    VariableData {
                        location: Location::Nodal,
                        values: y,
                    },
                    VariableData {
                        location: Location::Nodal,
                        values: u,
                    },
                    VariableData {
                        location: Location::Nodal,
                        values: v,
                    },
                ],
            }],
        }
    }

    #[test]
    fn tiny_pathline_moves_with_flow() {
        let ds = uniform_flow();
        let lines = trace_pathlines(
            &ds,
            &PathlineOptions {
                seeds: vec![[0.2, 1.0]],
                max_steps: 40,
                step_size: 0.05,
                bidirectional: false,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(lines.len(), 1);
        let pts = &lines[0].points;
        assert!(pts.len() > 5);
        assert!(pts.last().unwrap()[0] > pts[0][0] + 0.2);
        assert!((pts.last().unwrap()[1] - pts[0][1]).abs() < 1e-6);
    }
}
