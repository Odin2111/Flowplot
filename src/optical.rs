//! Numerical Schlieren and Shadowgraph (density-gradient optics).
//!
//! Solver-agnostic field transforms from Tecplot-style macros:
//! - Primary / Chamarthi Schlieren: `log(1 + |∇ρ|)`
//! - Shadowgraph (Zhang–Zhang–Shu style): `sign(∇²ρ) · log(1 + |∇²ρ|)`
//!
//! Operates on any loaded CFD density field (`.plt` / `.dat`), not a solver API.

use crate::contour::{
    render_contour_grid, write_png, ColorMapName, ContourColoring, ContourImage, ContourOptions,
    PlotStyle,
};
use crate::dataset::Dataset;
use crate::error::{Error, Result};
use crate::field::{extract_scalar_grid_view, find_var, Grid2D, PlaneSpec};
use crate::view::ViewSpec;
use std::path::Path;

/// Which optical transform to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpticalKind {
    /// Primary numerical Schlieren: log(1+|∇ρ|), Schlieren-Crisp colormap.
    Schlieren,
    /// Chamarthi-style: same formula, reversed grayscale, default display [0, 5].
    SchlierenChamarthi,
    /// Numerical shadowgraph from density Laplacian.
    Shadowgraph,
}

impl OpticalKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Schlieren => "schlieren",
            Self::SchlierenChamarthi => "schlieren-chamarthi",
            Self::Shadowgraph => "shadowgraph",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "schlieren" | "schlieren-primary" | "primary" => Ok(Self::Schlieren),
            "chamarthi" | "schlieren-chamarthi" | "chamarthi2" => Ok(Self::SchlierenChamarthi),
            "shadowgraph" | "shadow" => Ok(Self::Shadowgraph),
            other => Err(Error::plot(format!(
                "unknown optical kind '{other}' (expected schlieren|schlieren-chamarthi|shadowgraph)"
            ))),
        }
    }
}

/// Guess a density-like variable name from the dataset.
pub fn guess_density_var(ds: &Dataset) -> Option<String> {
    let names = &ds.variables;
    let lower: Vec<(String, &str)> = names
        .iter()
        .map(|n| (n.to_ascii_lowercase(), n.as_str()))
        .collect();
    for key in ["density", "rho", "dens", "density_kg_m3", "r"] {
        if let Some((_, orig)) = lower.iter().find(|(l, _)| l == key) {
            return Some((*orig).to_string());
        }
    }
    for (l, orig) in &lower {
        if l.contains("density") || l.starts_with("rho") {
            return Some((*orig).to_string());
        }
    }
    None
}

/// Central-difference |∇f| then log(1+|∇f|) on an IJ grid.
pub fn schlieren_from_grid(grid: &Grid2D) -> Vec<f64> {
    let ni = grid.ni;
    let nj = grid.nj;
    let mut out = vec![0.0; ni * nj];
    if ni < 2 || nj < 2 {
        return out;
    }
    for j in 0..nj {
        for i in 0..ni {
            let (dx, dfdx) = deriv_i(grid, i, j);
            let (dy, dfdy) = deriv_j(grid, i, j);
            let gx = if dx.abs() > 1e-30 { dfdx / dx } else { 0.0 };
            let gy = if dy.abs() > 1e-30 { dfdy / dy } else { 0.0 };
            let mag = (gx * gx + gy * gy).sqrt();
            out[grid.idx(i, j)] = (mag + 1.0).ln();
        }
    }
    out
}

/// sign(∇²f)·log(1+|∇²f|) on an IJ grid.
pub fn shadowgraph_from_grid(grid: &Grid2D) -> Vec<f64> {
    let ni = grid.ni;
    let nj = grid.nj;
    let mut lap = vec![0.0; ni * nj];
    if ni < 3 || nj < 3 {
        return lap;
    }
    // First derivatives, then their derivatives (Laplacian).
    let mut dfdx = vec![0.0; ni * nj];
    let mut dfdy = vec![0.0; ni * nj];
    for j in 0..nj {
        for i in 0..ni {
            let (dx, df) = deriv_i(grid, i, j);
            dfdx[grid.idx(i, j)] = if dx.abs() > 1e-30 { df / dx } else { 0.0 };
            let (dy, dg) = deriv_j(grid, i, j);
            dfdy[grid.idx(i, j)] = if dy.abs() > 1e-30 { dg / dy } else { 0.0 };
        }
    }
    for j in 0..nj {
        for i in 0..ni {
            let (dx, d2) = deriv_field_i(grid, &dfdx, i, j);
            let d2x = if dx.abs() > 1e-30 { d2 / dx } else { 0.0 };
            let (dy, d2y_raw) = deriv_field_j(grid, &dfdy, i, j);
            let d2y = if dy.abs() > 1e-30 { d2y_raw / dy } else { 0.0 };
            let l = d2x + d2y;
            lap[grid.idx(i, j)] = l.signum() * (1.0 + l.abs()).ln();
        }
    }
    lap
}

fn deriv_i(grid: &Grid2D, i: usize, j: usize) -> (f64, f64) {
    let ni = grid.ni;
    if i == 0 {
        let (x0, _) = grid.xy(0, j);
        let (x1, _) = grid.xy(1.min(ni - 1), j);
        (
            x1 - x0,
            grid.value(1.min(ni - 1), j) - grid.value(0, j),
        )
    } else if i + 1 >= ni {
        let (x0, _) = grid.xy(ni - 2, j);
        let (x1, _) = grid.xy(ni - 1, j);
        (x1 - x0, grid.value(ni - 1, j) - grid.value(ni - 2, j))
    } else {
        let (x0, _) = grid.xy(i - 1, j);
        let (x1, _) = grid.xy(i + 1, j);
        (x1 - x0, grid.value(i + 1, j) - grid.value(i - 1, j))
    }
}

fn deriv_j(grid: &Grid2D, i: usize, j: usize) -> (f64, f64) {
    let nj = grid.nj;
    if j == 0 {
        let (_, y0) = grid.xy(i, 0);
        let (_, y1) = grid.xy(i, 1.min(nj - 1));
        (
            y1 - y0,
            grid.value(i, 1.min(nj - 1)) - grid.value(i, 0),
        )
    } else if j + 1 >= nj {
        let (_, y0) = grid.xy(i, nj - 2);
        let (_, y1) = grid.xy(i, nj - 1);
        (y1 - y0, grid.value(i, nj - 1) - grid.value(i, nj - 2))
    } else {
        let (_, y0) = grid.xy(i, j - 1);
        let (_, y1) = grid.xy(i, j + 1);
        (y1 - y0, grid.value(i, j + 1) - grid.value(i, j - 1))
    }
}

fn deriv_field_i(grid: &Grid2D, field: &[f64], i: usize, j: usize) -> (f64, f64) {
    let ni = grid.ni;
    let idx = |ii: usize| grid.idx(ii, j);
    if i == 0 {
        let (x0, _) = grid.xy(0, j);
        let (x1, _) = grid.xy(1.min(ni - 1), j);
        (x1 - x0, field[idx(1.min(ni - 1))] - field[idx(0)])
    } else if i + 1 >= ni {
        let (x0, _) = grid.xy(ni - 2, j);
        let (x1, _) = grid.xy(ni - 1, j);
        (x1 - x0, field[idx(ni - 1)] - field[idx(ni - 2)])
    } else {
        let (x0, _) = grid.xy(i - 1, j);
        let (x1, _) = grid.xy(i + 1, j);
        (x1 - x0, field[idx(i + 1)] - field[idx(i - 1)])
    }
}

fn deriv_field_j(grid: &Grid2D, field: &[f64], i: usize, j: usize) -> (f64, f64) {
    let nj = grid.nj;
    let idx = |jj: usize| grid.idx(i, jj);
    if j == 0 {
        let (_, y0) = grid.xy(i, 0);
        let (_, y1) = grid.xy(i, 1.min(nj - 1));
        (y1 - y0, field[idx(1.min(nj - 1))] - field[idx(0)])
    } else if j + 1 >= nj {
        let (_, y0) = grid.xy(i, nj - 2);
        let (_, y1) = grid.xy(i, nj - 1);
        (y1 - y0, field[idx(nj - 1)] - field[idx(nj - 2)])
    } else {
        let (_, y0) = grid.xy(i, j - 1);
        let (_, y1) = grid.xy(i, j + 1);
        (y1 - y0, field[idx(j + 1)] - field[idx(j - 1)])
    }
}

/// Options for an optical PNG render.
#[derive(Debug, Clone)]
pub struct OpticalOptions {
    pub kind: OpticalKind,
    pub density_var: Option<String>,
    pub zone: usize,
    pub plane: PlaneSpec,
    pub view: ViewSpec,
    pub width: u32,
    pub height: u32,
    pub style: PlotStyle,
    pub annotate: bool,
    pub path_hint: Option<String>,
    pub legend: bool,
    /// Override vmin/vmax (Chamarthi defaults to 0..5 when unset).
    pub vmin: Option<f64>,
    pub vmax: Option<f64>,
}

impl Default for OpticalOptions {
    fn default() -> Self {
        Self {
            kind: OpticalKind::Schlieren,
            density_var: None,
            zone: 0,
            plane: PlaneSpec::Auto,
            view: ViewSpec::Auto,
            width: 700,
            height: 525,
            style: PlotStyle::Jfm,
            annotate: true,
            path_hint: None,
            legend: false,
            vmin: None,
            vmax: None,
        }
    }
}

fn default_maps(kind: OpticalKind) -> (ColorMapName, Option<f64>, Option<f64>, ContourColoring) {
    match kind {
        OpticalKind::Schlieren => (
            ColorMapName::SchlierenCrisp,
            None,
            None,
            ContourColoring::Continuous,
        ),
        OpticalKind::SchlierenChamarthi => (
            ColorMapName::GrayscaleReverse,
            Some(0.0),
            Some(5.0),
            ContourColoring::Continuous,
        ),
        OpticalKind::Shadowgraph => (
            ColorMapName::Grayscale,
            None,
            None,
            ContourColoring::Continuous,
        ),
    }
}

/// Build optical field grid + render options, then contour.
pub fn render_optical(ds: &Dataset, opts: &OpticalOptions) -> Result<ContourImage> {
    let var_name = opts
        .density_var
        .clone()
        .or_else(|| guess_density_var(ds))
        .ok_or_else(|| {
            Error::plot(
                "optical requires a Density/rho variable (pass --var) — none found by name",
            )
        })?;
    let var_idx = find_var(ds, &var_name)?;
    let mut grid = extract_scalar_grid_view(ds, opts.zone, var_idx, opts.plane, opts.view)?;
    let values = match opts.kind {
        OpticalKind::Schlieren | OpticalKind::SchlierenChamarthi => schlieren_from_grid(&grid),
        OpticalKind::Shadowgraph => shadowgraph_from_grid(&grid),
    };
    grid.values = values;

    let (cmap, def_vmin, def_vmax, coloring) = default_maps(opts.kind);
    let label = match opts.kind {
        OpticalKind::Schlieren => format!("Schlieren(log|∇{var_name}|)"),
        OpticalKind::SchlierenChamarthi => format!("ChamarthiSchlieren({var_name})"),
        OpticalKind::Shadowgraph => format!("Shadowgraph(∇²{var_name})"),
    };

    let contour_opts = ContourOptions {
        variable: label.clone(),
        zone: opts.zone,
        plane: opts.plane,
        view: opts.view,
        vmin: opts.vmin.or(def_vmin),
        vmax: opts.vmax.or(def_vmax),
        width: opts.width,
        height: opts.height,
        style: opts.style,
        annotate: opts.annotate,
        path_hint: opts.path_hint.clone(),
        coloring,
        nlevels: 24,
        colormap: cmap,
        legend: opts.legend,
        show_labels: false,
        legend_font_px: 0.0,
        legend_align: crate::annotate::LegendAlign::Right,
        margin: 2,
        pan_x: 0.0,
        pan_y: 0.0,
    };
    render_contour_grid(ds, &grid, &contour_opts, &label)
}

/// Write optical PNG to `out`.
pub fn write_optical_png(ds: &Dataset, opts: &OpticalOptions, out: &Path) -> Result<ContourImage> {
    let img = render_optical(ds, opts)?;
    write_png(out, &img)?;
    Ok(img)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::{Location, OrderedDims, Topology, VariableData, Zone};
    use crate::Dataset;

    fn tiny_density() -> Dataset {
        // 3×3 with a density jump so |∇ρ| and Laplacian are nonzero.
        let mut dens = vec![1.0; 9];
        dens[4] = 3.0; // center spike
        dens[5] = 2.0;
        Dataset {
            title: None,
            variables: vec!["X".into(), "Y".into(), "Density".into()],
            zones: vec![Zone {
                name: "z".into(),
                topology: Topology::Ordered(OrderedDims::Ij { i: 3, j: 3 }),
                fields: vec![
                    VariableData {
                        location: Location::Nodal,
                        values: (0..9).map(|n| (n % 3) as f64).collect(),
                    },
                    VariableData {
                        location: Location::Nodal,
                        values: (0..9).map(|n| (n / 3) as f64).collect(),
                    },
                    VariableData {
                        location: Location::Nodal,
                        values: dens,
                    },
                ],
            }],
            ..Default::default()
        }
    }

    #[test]
    fn schlieren_nonzero_on_spike() {
        let ds = tiny_density();
        let var = find_var(&ds, "Density").unwrap();
        let grid = extract_scalar_grid_view(
            &ds,
            0,
            var,
            PlaneSpec::Auto,
            ViewSpec::Auto,
        )
        .unwrap();
        let s = schlieren_from_grid(&grid);
        assert!(s.iter().any(|v| *v > 0.01), "expected nonzero schlieren");
    }

    #[test]
    fn shadowgraph_runs() {
        let ds = tiny_density();
        let img = render_optical(
            &ds,
            &OpticalOptions {
                kind: OpticalKind::Shadowgraph,
                width: 64,
                height: 48,
                annotate: false,
                ..OpticalOptions::default()
            },
        )
        .unwrap();
        assert_eq!(img.width, 64);
    }
}
