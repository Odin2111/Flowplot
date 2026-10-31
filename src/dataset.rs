//! CFD data model: Dataset → Zone → named Variables.
//!
//! Ordered zones use implicit I / IJ / IJK neighbors. Finite-element zones
//! store explicit connectivity. Field values are nodal or cell-centered.

/// Where a variable’s samples live relative to the mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    Nodal,
    CellCentered,
}

/// Ordered zone shape. Neighbors are implied by index order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderedDims {
    /// I-ordered line / point cloud length.
    I { i: usize },
    /// IJ surface.
    Ij { i: usize, j: usize },
    /// IJK volume.
    Ijk { i: usize, j: usize, k: usize },
}

impl OrderedDims {
    pub fn node_count(self) -> usize {
        match self {
            Self::I { i } => i,
            Self::Ij { i, j } => i * j,
            Self::Ijk { i, j, k } => i * j * k,
        }
    }

    /// Cell count for a structured block (product of max(d-1, 0) per axis).
    pub fn cell_count(self) -> usize {
        match self {
            Self::I { i } => i.saturating_sub(1),
            Self::Ij { i, j } => i.saturating_sub(1) * j.saturating_sub(1),
            Self::Ijk { i, j, k } => {
                i.saturating_sub(1) * j.saturating_sub(1) * k.saturating_sub(1)
            }
        }
    }
}

/// Finite-element cell shape (public PLT / VTK-style names only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementType {
    Line,
    Triangle,
    Quad,
    Tetra,
    Hexa,
    Pyramid,
    Wedge,
    /// Mixed or unknown FE; connectivity lists vary per cell.
    Mixed,
}

impl ElementType {
    pub fn nodes_per_element(self) -> Option<usize> {
        Some(match self {
            Self::Line => 2,
            Self::Triangle => 3,
            Self::Quad => 4,
            Self::Tetra => 4,
            Self::Hexa => 8,
            Self::Pyramid => 5,
            Self::Wedge => 6,
            Self::Mixed => return None,
        })
    }
}

/// Explicit FE connectivity: flat node indices, `nodes_per_elem` when uniform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connectivity {
    pub element_type: ElementType,
    /// Node indices into the zone’s nodal arrays (0-based).
    pub indices: Vec<usize>,
    /// Present when every element has the same node count.
    pub nodes_per_elem: Option<usize>,
}

impl Connectivity {
    pub fn element_count(&self) -> usize {
        if let Some(npe) = self.nodes_per_elem {
            if npe == 0 {
                return 0;
            }
            return self.indices.len() / npe;
        }
        // Mixed: treat as unknown without offset table in this first slice.
        0
    }
}

/// Mesh topology for one zone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Topology {
    Ordered(OrderedDims),
    FiniteElement {
        num_nodes: usize,
        connectivity: Connectivity,
    },
}

impl Topology {
    pub fn node_count(&self) -> usize {
        match self {
            Self::Ordered(dims) => dims.node_count(),
            Self::FiniteElement { num_nodes, .. } => *num_nodes,
        }
    }

    pub fn cell_count(&self) -> usize {
        match self {
            Self::Ordered(dims) => dims.cell_count(),
            Self::FiniteElement { connectivity, .. } => connectivity.element_count(),
        }
    }
}

/// One named field on a zone (shared name lives on [`Dataset::variables`]).
#[derive(Debug, Clone, PartialEq)]
pub struct VariableData {
    pub location: Location,
    pub values: Vec<f64>,
}

/// One mesh piece with per-variable samples aligned to [`Dataset::variables`].
#[derive(Debug, Clone, PartialEq)]
pub struct Zone {
    pub name: String,
    pub topology: Topology,
    /// Parallel to `Dataset::variables`; missing entries use empty `values`.
    pub fields: Vec<VariableData>,
}

impl Zone {
    pub fn node_count(&self) -> usize {
        self.topology.node_count()
    }

    pub fn cell_count(&self) -> usize {
        self.topology.cell_count()
    }
}

/// SolutionTime / StrandID / AUXDATA (VisIt/Silo-like) for one zone.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ZoneMeta {
    pub solution_time: Option<f64>,
    pub strand_id: Option<i32>,
    pub aux: Vec<(String, String)>,
}

impl ZoneMeta {
    pub fn cycle(&self) -> Option<i64> {
        for (k, v) in &self.aux {
            if k.eq_ignore_ascii_case("cycle") || k.eq_ignore_ascii_case("ncycle") {
                if let Ok(n) = v.trim().parse::<i64>() {
                    return Some(n);
                }
            }
        }
        None
    }

    pub fn time_value(&self) -> Option<f64> {
        if let Some(t) = self.solution_time {
            return Some(t);
        }
        for (k, v) in &self.aux {
            if k.eq_ignore_ascii_case("time") || k.eq_ignore_ascii_case("solutiontime") {
                if let Ok(t) = v.trim().parse::<f64>() {
                    return Some(t);
                }
            }
        }
        None
    }
}

/// Full loaded file: shared variable names and one or more zones.
#[derive(Debug, Clone, PartialEq)]
pub struct Dataset {
    pub title: Option<String>,
    pub variables: Vec<String>,
    pub zones: Vec<Zone>,
    /// File-level AUXDATA / Silo-like metadata (DATASETAUXDATA).
    pub aux: Vec<(String, String)>,
    /// Parallel to [`Dataset::zones`] when present (readers pad as they load).
    pub zone_meta: Vec<ZoneMeta>,
}

impl Dataset {
    pub fn new() -> Self {
        Self {
            title: None,
            variables: Vec::new(),
            zones: Vec::new(),
            aux: Vec::new(),
            zone_meta: Vec::new(),
        }
    }

    pub fn zone_count(&self) -> usize {
        self.zones.len()
    }

    pub fn meta(&self, zone: usize) -> ZoneMeta {
        self.zone_meta.get(zone).cloned().unwrap_or_default()
    }

    pub fn cycle(&self) -> Option<i64> {
        for (k, v) in &self.aux {
            if k.eq_ignore_ascii_case("cycle") || k.eq_ignore_ascii_case("ncycle") {
                if let Ok(n) = v.trim().parse::<i64>() {
                    return Some(n);
                }
            }
        }
        self.zone_meta.first().and_then(|m| m.cycle())
    }
}

impl Default for Dataset {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_counts() {
        let dims = OrderedDims::Ijk {
            i: 3,
            j: 2,
            k: 2,
        };
        assert_eq!(dims.node_count(), 12);
        assert_eq!(dims.cell_count(), 2);
    }
}
