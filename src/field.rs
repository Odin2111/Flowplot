//! Shared helpers: locate variables, extract ordered 2D slices, bilinear sample.

use crate::dataset::{Dataset, Location, OrderedDims, Topology, Zone};
use crate::error::{Error, Result};
use crate::view::{LookAxis, PrincipalPlane, ViewSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    I,
    J,
    K,
}

impl std::fmt::Display for Axis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::I => write!(f, "i"),
            Self::J => write!(f, "j"),
            Self::K => write!(f, "k"),
        }
    }
}

/// Slice specification for extracting a 2D plane from an ordered zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaneSpec {
    /// Whole zone when already 2D (IJ), or K=0 default for IJK.
    Auto,
    /// Fixed index on one structured axis.
    Index { axis: Axis, index: usize },
}

impl PlaneSpec {
    /// Parse `k=0`, `i=1`, `j=2` (case-insensitive).
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        if s.is_empty() || s.eq_ignore_ascii_case("auto") {
            return Ok(Self::Auto);
        }
        let (axis_s, idx_s) = s
            .split_once('=')
            .ok_or_else(|| Error::plot(format!("invalid --plane '{s}' (expected e.g. k=0)")))?;
        let axis = match axis_s.trim().to_ascii_lowercase().as_str() {
            "i" => Axis::I,
            "j" => Axis::J,
            "k" => Axis::K,
            other => {
                return Err(Error::plot(format!(
                    "invalid plane axis '{other}' (use i, j, or k)"
                )))
            }
        };
        let index: usize = idx_s.trim().parse().map_err(|_| {
            Error::plot(format!("invalid plane index in '{s}'"))
        })?;
        Ok(Self::Index { axis, index })
    }
}

/// 2D nodal field on a structured ni×nj grid, with optional physical coords.
#[derive(Debug, Clone)]
pub struct Grid2D {
    pub ni: usize,
    pub nj: usize,
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    pub values: Vec<f64>,
}

impl Grid2D {
    pub fn idx(&self, i: usize, j: usize) -> usize {
        j * self.ni + i
    }

    pub fn value(&self, i: usize, j: usize) -> f64 {
        self.values[self.idx(i, j)]
    }

    pub fn xy(&self, i: usize, j: usize) -> (f64, f64) {
        let k = self.idx(i, j);
        (self.x[k], self.y[k])
    }

    pub fn bounds(&self) -> (f64, f64, f64, f64) {
        let mut xmin = f64::INFINITY;
        let mut xmax = f64::NEG_INFINITY;
        let mut ymin = f64::INFINITY;
        let mut ymax = f64::NEG_INFINITY;
        for (&x, &y) in self.x.iter().zip(self.y.iter()) {
            xmin = xmin.min(x);
            xmax = xmax.max(x);
            ymin = ymin.min(y);
            ymax = ymax.max(y);
        }
        if !xmin.is_finite() {
            xmin = 0.0;
            xmax = 1.0;
            ymin = 0.0;
            ymax = 1.0;
        }
        if (xmax - xmin).abs() < 1e-30 {
            xmax = xmin + 1.0;
        }
        if (ymax - ymin).abs() < 1e-30 {
            ymax = ymin + 1.0;
        }
        (xmin, xmax, ymin, ymax)
    }

    /// Bilinear sample of `values` at physical (x,y). None if outside.
    pub fn sample(&self, x: f64, y: f64) -> Option<f64> {
        sample_bilinear(self, &self.values, x, y)
    }

    pub fn sample_vec(&self, field: &[f64], x: f64, y: f64) -> Option<f64> {
        sample_bilinear(self, field, x, y)
    }
}

fn sample_bilinear(grid: &Grid2D, field: &[f64], x: f64, y: f64) -> Option<f64> {
    if grid.ni < 2 || grid.nj < 2 || field.len() != grid.ni * grid.nj {
        return None;
    }
    // Assume roughly rectilinear: locate cell by scanning bounding boxes.
    // For tiny synthetic grids this is fine; production can add a tree later.
    for j in 0..grid.nj - 1 {
        for i in 0..grid.ni - 1 {
            let (x00, y00) = grid.xy(i, j);
            let (x10, y10) = grid.xy(i + 1, j);
            let (x01, y01) = grid.xy(i, j + 1);
            let (x11, y11) = grid.xy(i + 1, j + 1);
            let xmin = x00.min(x10).min(x01).min(x11);
            let xmax = x00.max(x10).max(x01).max(x11);
            let ymin = y00.min(y10).min(y01).min(y11);
            let ymax = y00.max(y10).max(y01).max(y11);
            if x < xmin || x > xmax || y < ymin || y > ymax {
                continue;
            }
            // Parametric bilinear in index space using axis-aligned assumption.
            let dx = (x10 - x00).abs().max(1e-30);
            let dy = (y01 - y00).abs().max(1e-30);
            let u = ((x - x00) / dx).clamp(0.0, 1.0);
            let v = ((y - y00) / dy).clamp(0.0, 1.0);
            let f00 = field[grid.idx(i, j)];
            let f10 = field[grid.idx(i + 1, j)];
            let f01 = field[grid.idx(i, j + 1)];
            let f11 = field[grid.idx(i + 1, j + 1)];
            let a = f00 * (1.0 - u) + f10 * u;
            let b = f01 * (1.0 - u) + f11 * u;
            return Some(a * (1.0 - v) + b * v);
        }
    }
    None
}

pub fn find_var(ds: &Dataset, name: &str) -> Result<usize> {
    ds.variables
        .iter()
        .position(|v| v.eq_ignore_ascii_case(name))
        .ok_or_else(|| {
            Error::plot(format!(
                "variable '{name}' not found; have: {}",
                ds.variables.join(", ")
            ))
        })
}

pub fn guess_coord_vars(ds: &Dataset) -> (Option<usize>, Option<usize>, Option<usize>) {
    let find = |cands: &[&str]| {
        cands
            .iter()
            .find_map(|c| ds.variables.iter().position(|v| v.eq_ignore_ascii_case(c)))
    };
    (
        find(&["X", "R"]),
        find(&["Y", "THETA", "TH"]),
        find(&["Z"]),
    )
}

/// Span of a nodal field over `nodes` (0 if missing / empty).
fn field_span_on_nodes(zone: &Zone, var_idx: Option<usize>, nodes: &[usize]) -> f64 {
    let Some(vi) = var_idx else {
        return 0.0;
    };
    let Some(field) = zone.fields.get(vi) else {
        return 0.0;
    };
    let mut mn = f64::INFINITY;
    let mut mx = f64::NEG_INFINITY;
    for &n in nodes {
        if let Some(&v) = field.values.get(n) {
            if v.is_finite() {
                mn = mn.min(v);
                mx = mx.max(v);
            }
        }
    }
    if mn.is_finite() && mx.is_finite() {
        (mx - mn).abs()
    } else {
        0.0
    }
}

/// Choose two plot axes that actually vary on the extracted plane.
///
/// Meridian / axisymmetric slices often have Y≡0 with geometry in X/R and Z;
/// defaulting to (X,Y) collapses the mesh to a 1-pixel line in the viewport.
pub fn pick_plot_coord_indices(
    zone: &Zone,
    nodes: &[usize],
    x_i: Option<usize>,
    y_i: Option<usize>,
    z_i: Option<usize>,
) -> (Option<usize>, Option<usize>) {
    let sx = field_span_on_nodes(zone, x_i, nodes);
    let sy = field_span_on_nodes(zone, y_i, nodes);
    let sz = field_span_on_nodes(zone, z_i, nodes);
    let max_span = sx.max(sy).max(sz).max(1e-30);
    let eps = (1e-8 * max_span).max(1e-30);

    // Prefer XY when both vary; otherwise fall back to XZ / YZ.
    if sx > eps && sy > eps {
        return (x_i, y_i);
    }
    if sx > eps && sz > eps {
        return (x_i, z_i);
    }
    if sy > eps && sz > eps {
        return (y_i, z_i);
    }
    // Single varying axis + best available second (may be synthetic later).
    if sx >= sy && sx >= sz {
        (x_i, if sy >= sz { y_i } else { z_i }.or(y_i).or(z_i))
    } else if sy >= sz {
        (y_i, if sx >= sz { x_i } else { z_i }.or(x_i).or(z_i))
    } else {
        (z_i, if sx >= sy { x_i } else { y_i }.or(x_i).or(y_i))
    }
}

/// True when the dataset is effectively a 2D Cartesian plot (no thick 3D volume).
///
/// Used by the GUI to fall back from "3D Cartesian" with a clear message.
pub fn dataset_is_effectively_2d(ds: &Dataset) -> bool {
    let Some(zone) = ds.zones.first() else {
        return true;
    };
    match &zone.topology {
        Topology::Ordered(OrderedDims::I { .. }) | Topology::Ordered(OrderedDims::Ij { .. }) => {
            true
        }
        Topology::Ordered(OrderedDims::Ijk { i, j, k }) => {
            // Unit thickness on any structured axis ⇒ a single plane, not a volume.
            *i <= 1 || *j <= 1 || *k <= 1
        }
        Topology::FiniteElement { .. } => {
            let (x_i, y_i, z_i) = guess_coord_vars(ds);
            let nodes: Vec<usize> = (0..zone.node_count()).collect();
            let sx = field_span_on_nodes(zone, x_i, &nodes);
            let sy = field_span_on_nodes(zone, y_i, &nodes);
            let sz = field_span_on_nodes(zone, z_i, &nodes);
            let max_span = sx.max(sy).max(sz).max(1e-30);
            sz <= 1e-8 * max_span
        }
    }
}

/// Extract a nodal 2D grid of `var_idx` from an ordered zone (Auto view).
pub fn extract_scalar_grid(
    ds: &Dataset,
    zone_idx: usize,
    var_idx: usize,
    plane: PlaneSpec,
) -> Result<Grid2D> {
    extract_scalar_grid_view(ds, zone_idx, var_idx, plane, ViewSpec::Auto)
}

/// Suggest a face-on camera for the zone/plane (matches Auto plot-axis pick).
///
/// Meridian / RZ slices (X+Z vary, Y thin) → look along ±Y so orbit starts on the
/// real domain instead of jumping to a collapsed XY edge-on view.
pub fn suggest_default_view(ds: &Dataset, zone_idx: usize, plane: PlaneSpec) -> Result<ViewSpec> {
    let zone = ds
        .zones
        .get(zone_idx)
        .ok_or_else(|| Error::plot(format!("zone index {zone_idx} out of range")))?;
    let Topology::Ordered(dims) = &zone.topology else {
        return Ok(ViewSpec::Axis {
            axis: LookAxis::Z,
            positive: true,
        });
    };
    let (x_i, y_i, z_i) = guess_coord_vars(ds);
    let (_ni, _nj, node_indices) = plane_nodes(*dims, plane)?;
    let (plot_u, plot_v) = pick_plot_coord_indices(zone, &node_indices, x_i, y_i, z_i);
    Ok(view_for_plot_axes(plot_u, plot_v, x_i, y_i, z_i))
}

/// Resolve `Auto` into a concrete Axis/Free camera so mesh geometry and orbit share one path.
pub fn resolve_view(
    ds: &Dataset,
    zone_idx: usize,
    plane: PlaneSpec,
    view: ViewSpec,
) -> Result<ViewSpec> {
    if matches!(view, ViewSpec::Auto) {
        suggest_default_view(ds, zone_idx, plane)
    } else {
        Ok(view)
    }
}

fn view_for_plot_axes(
    plot_u: Option<usize>,
    plot_v: Option<usize>,
    x_i: Option<usize>,
    y_i: Option<usize>,
    z_i: Option<usize>,
) -> ViewSpec {
    let is_x = |i: Option<usize>| i.is_some() && i == x_i;
    let is_y = |i: Option<usize>| i.is_some() && i == y_i;
    let is_z = |i: Option<usize>| i.is_some() && i == z_i;
    let plane = if is_x(plot_u) && is_y(plot_v) {
        PrincipalPlane::Xy
    } else if is_x(plot_u) && is_z(plot_v) {
        PrincipalPlane::Xz
    } else if is_y(plot_u) && is_z(plot_v) {
        PrincipalPlane::Yz
    } else if is_y(plot_u) && is_x(plot_v) {
        PrincipalPlane::Xy
    } else if is_z(plot_u) && is_x(plot_v) {
        PrincipalPlane::Xz
    } else if is_z(plot_u) && is_y(plot_v) {
        PrincipalPlane::Yz
    } else {
        PrincipalPlane::Xy
    };
    plane.face_on_view()
}

/// Extract a nodal 2D grid and project world XYZ with `view`.
pub fn extract_scalar_grid_view(
    ds: &Dataset,
    zone_idx: usize,
    var_idx: usize,
    plane: PlaneSpec,
    view: ViewSpec,
) -> Result<Grid2D> {
    let zone = ds
        .zones
        .get(zone_idx)
        .ok_or_else(|| Error::plot(format!("zone index {zone_idx} out of range")))?;
    let Topology::Ordered(dims) = &zone.topology else {
        return Err(Error::plot(
            "contour/pathlines currently support ordered (I/IJ/IJK) zones only",
        ));
    };

    let field = zone
        .fields
        .get(var_idx)
        .ok_or_else(|| Error::plot("variable field missing on zone"))?;
    if field.location != Location::Nodal {
        return Err(Error::plot(
            "cell-centered contour not implemented in this slice (need nodal values)",
        ));
    }

    let (x_i, y_i, z_i) = guess_coord_vars(ds);
    let (ni, nj, node_indices) = plane_nodes(*dims, plane)?;
    let n = ni * nj;
    let mut values = Vec::with_capacity(n);
    let mut x = Vec::with_capacity(n);
    let mut y = Vec::with_capacity(n);

    // Always project through a concrete camera so Auto and Free share geometry.
    let camera = resolve_view(ds, zone_idx, plane, view)?;
    for (local, &node) in node_indices.iter().enumerate() {
        let i = local % ni;
        let j = local / ni;
        values.push(*field.values.get(node).unwrap_or(&f64::NAN));
        let wx = coord_at(zone, x_i, node, i);
        let wy = coord_at(zone, y_i, node, j);
        let wz = coord_at(zone, z_i, node, 0);
        let (u, v) = camera.project(wx, wy, wz);
        x.push(u);
        y.push(v);
    }

    Ok(Grid2D {
        ni,
        nj,
        x,
        y,
        values,
    })
}

/// Auto plane for IJK: slice the unit-thickness axis that leaves the largest 2D plane.
fn auto_ijk_plane(i: usize, j: usize, k: usize) -> (Axis, usize) {
    let candidates = [
        (k <= 1, Axis::K, i.saturating_mul(j)),
        (j <= 1, Axis::J, i.saturating_mul(k)),
        (i <= 1, Axis::I, j.saturating_mul(k)),
    ];
    let mut best: Option<(Axis, usize)> = None;
    let mut best_area = 0usize;
    for &(unit, axis, area) in &candidates {
        if unit && area > best_area {
            best_area = area;
            best = Some((axis, 0));
        }
    }
    best.unwrap_or((Axis::K, 0))
}

fn coord_at(zone: &Zone, var_idx: Option<usize>, node: usize, fallback_index: usize) -> f64 {
    if let Some(vi) = var_idx {
        if let Some(field) = zone.fields.get(vi) {
            if let Some(v) = field.values.get(node) {
                return *v;
            }
        }
    }
    fallback_index as f64
}

fn plane_nodes(dims: OrderedDims, plane: PlaneSpec) -> Result<(usize, usize, Vec<usize>)> {
    match dims {
        OrderedDims::I { i } => {
            // Degenerate: 1×i as a line — treat as i×1
            Ok((i, 1, (0..i).collect()))
        }
        OrderedDims::Ij { i, j } => {
            if let PlaneSpec::Index { axis, index } = plane {
                match axis {
                    Axis::K if index == 0 => {}
                    Axis::I => {
                        if index >= i {
                            return Err(Error::plot(format!("plane i={index} out of range (I={i})")));
                        }
                        let nodes: Vec<_> = (0..j).map(|jj| jj * i + index).collect();
                        return Ok((1, j, nodes));
                    }
                    Axis::J => {
                        if index >= j {
                            return Err(Error::plot(format!("plane j={index} out of range (J={j})")));
                        }
                        let nodes: Vec<_> = (0..i).map(|ii| index * i + ii).collect();
                        return Ok((i, 1, nodes));
                    }
                    Axis::K => {
                        return Err(Error::plot("IJ zone has no K axis"));
                    }
                }
            }
            let mut nodes = Vec::with_capacity(i * j);
            for jj in 0..j {
                for ii in 0..i {
                    nodes.push(jj * i + ii);
                }
            }
            Ok((i, j, nodes))
        }
        OrderedDims::Ijk { i, j, k } => {
            let (axis, index) = match plane {
                // Slice a unit-thickness axis first so Auto does not collapse
                // I×1×K meridian zones to a 523×1 line (K=0 of J=1).
                PlaneSpec::Auto => auto_ijk_plane(i, j, k),
                PlaneSpec::Index { axis, index } => (axis, index),
            };
            match axis {
                Axis::K => {
                    if index >= k {
                        return Err(Error::plot(format!("plane k={index} out of range (K={k})")));
                    }
                    let mut nodes = Vec::with_capacity(i * j);
                    for jj in 0..j {
                        for ii in 0..i {
                            nodes.push(index * (i * j) + jj * i + ii);
                        }
                    }
                    Ok((i, j, nodes))
                }
                Axis::J => {
                    if index >= j {
                        return Err(Error::plot(format!("plane j={index} out of range (J={j})")));
                    }
                    let mut nodes = Vec::with_capacity(i * k);
                    for kk in 0..k {
                        for ii in 0..i {
                            nodes.push(kk * (i * j) + index * i + ii);
                        }
                    }
                    Ok((i, k, nodes))
                }
                Axis::I => {
                    if index >= i {
                        return Err(Error::plot(format!("plane i={index} out of range (I={i})")));
                    }
                    let mut nodes = Vec::with_capacity(j * k);
                    for kk in 0..k {
                        for jj in 0..j {
                            nodes.push(kk * (i * j) + jj * i + index);
                        }
                    }
                    Ok((j, k, nodes))
                }
            }
        }
    }
}

/// Extract a velocity component field on the same plane as `grid` (same node order).
pub fn extract_component_on_plane(
    ds: &Dataset,
    zone_idx: usize,
    var_name: &str,
    plane: PlaneSpec,
) -> Result<Vec<f64>> {
    let var_idx = find_var(ds, var_name)?;
    let g = extract_scalar_grid(ds, zone_idx, var_idx, plane)?;
    Ok(g.values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::{VariableData, Zone};

    #[test]
    fn parse_plane() {
        assert_eq!(
            PlaneSpec::parse("k=0").unwrap(),
            PlaneSpec::Index {
                axis: Axis::K,
                index: 0
            }
        );
    }

    #[test]
    fn extract_ij() {
        let ds = Dataset {
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
                        values: vec![1.0, 2.0, 3.0, 4.0],
                    },
                ],
            }],
            ..Default::default()
        };
        let g = extract_scalar_grid(&ds, 0, 2, PlaneSpec::Auto).unwrap();
        assert_eq!(g.ni, 2);
        assert_eq!(g.nj, 2);
        assert_eq!(g.values, vec![1.0, 2.0, 3.0, 4.0]);
        let s = g.sample(0.5, 0.5).unwrap();
        assert!((s - 2.5).abs() < 1e-9);
    }

    #[test]
    fn auto_plane_meridian_ijk_j1() {
        // jet_cyl-like meridian: I×1×K — Auto must slice J, not K=0 of J=1.
        let (axis, idx) = auto_ijk_plane(523, 1, 58);
        assert_eq!(axis, Axis::J);
        assert_eq!(idx, 0);
        let (ni, nj, nodes) =
            plane_nodes(OrderedDims::Ijk { i: 4, j: 1, k: 3 }, PlaneSpec::Auto).unwrap();
        assert_eq!((ni, nj), (4, 3));
        assert_eq!(nodes.len(), 12);
    }

    #[test]
    fn pick_coords_prefers_xz_when_y_flat() {
        let zone = Zone {
            name: "m".into(),
            topology: Topology::Ordered(OrderedDims::Ijk {
                i: 2,
                j: 1,
                k: 2,
            }),
            fields: vec![
                VariableData {
                    location: Location::Nodal,
                    values: vec![0.0, 1.0, 0.0, 1.0], // X
                },
                VariableData {
                    location: Location::Nodal,
                    values: vec![0.0; 4], // Y flat
                },
                VariableData {
                    location: Location::Nodal,
                    values: vec![0.0, 0.0, 2.0, 2.0], // Z
                },
            ],
        };
        let nodes: Vec<usize> = (0..4).collect();
        let (px, py) = pick_plot_coord_indices(&zone, &nodes, Some(0), Some(1), Some(2));
        assert_eq!(px, Some(0));
        assert_eq!(py, Some(2));
    }

    #[test]
    fn extract_meridian_uses_xz() {
        let ds = Dataset {
            title: None,
            variables: vec!["X".into(), "Y".into(), "Z".into(), "Ux".into()],
            zones: vec![Zone {
                name: "meridian".into(),
                topology: Topology::Ordered(OrderedDims::Ijk {
                    i: 2,
                    j: 1,
                    k: 2,
                }),
                fields: vec![
                    VariableData {
                        location: Location::Nodal,
                        values: vec![0.1, 0.2, 0.1, 0.2],
                    },
                    VariableData {
                        location: Location::Nodal,
                        values: vec![0.0; 4],
                    },
                    VariableData {
                        location: Location::Nodal,
                        values: vec![0.0, 0.0, 1.0, 1.0],
                    },
                    VariableData {
                        location: Location::Nodal,
                        values: vec![1.0, 2.0, 3.0, 4.0],
                    },
                ],
            }],
            ..Default::default()
        };
        assert!(dataset_is_effectively_2d(&ds));
        let g = extract_scalar_grid(&ds, 0, 3, PlaneSpec::Auto).unwrap();
        assert_eq!((g.ni, g.nj), (2, 2));
        let (xmin, xmax, ymin, ymax) = g.bounds();
        assert!((xmin - 0.1).abs() < 1e-12 && (xmax - 0.2).abs() < 1e-12);
        assert!((ymin - 0.0).abs() < 1e-12 && (ymax - 1.0).abs() < 1e-12);
    }

    #[test]
    fn plus_x_view_projects_neg_y_z() {
        use crate::view::ViewSpec;
        let ds = Dataset {
            title: None,
            variables: vec!["X".into(), "Y".into(), "Z".into(), "P".into()],
            zones: vec![Zone {
                name: "box".into(),
                topology: Topology::Ordered(OrderedDims::Ij { i: 2, j: 2 }),
                fields: vec![
                    VariableData {
                        location: Location::Nodal,
                        values: vec![0.0, 1.0, 0.0, 1.0],
                    },
                    VariableData {
                        location: Location::Nodal,
                        values: vec![0.0, 0.0, 2.0, 2.0],
                    },
                    VariableData {
                        location: Location::Nodal,
                        values: vec![0.0, 0.0, 0.0, 3.0],
                    },
                    VariableData {
                        location: Location::Nodal,
                        values: vec![1.0; 4],
                    },
                ],
            }],
            ..Default::default()
        };
        let g = extract_scalar_grid_view(
            &ds,
            0,
            3,
            PlaneSpec::Auto,
            ViewSpec::parse("+x").unwrap(),
        )
        .unwrap();
        // +X view: u=-Y, v=Z. Y in {0,2} → u in {0,-2}; Z in {0,3}.
        let (xmin, xmax, ymin, ymax) = g.bounds();
        assert!(xmin <= -2.0 + 1e-12 && xmax >= 0.0 - 1e-12);
        assert!(ymin <= 0.0 + 1e-12 && ymax >= 3.0 - 1e-12);
    }
}
