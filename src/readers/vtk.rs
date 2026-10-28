//! VTK readers: legacy ASCII (working subset) and XML (stub / minimal).
//!
//! Public Kitware VTK file formats only — no proprietary wrappers.

use std::path::Path;

use crate::dataset::{
    Connectivity, Dataset, ElementType, Location, OrderedDims, Topology, VariableData, Zone,
};
use crate::error::{Error, Result};
use crate::readers::Reader;

pub struct VtkLegacyAsciiReader;

impl Reader for VtkLegacyAsciiReader {
    fn format_name(&self) -> &'static str {
        "VTK legacy ASCII"
    }

    fn read_path(&self, path: &Path) -> Result<Dataset> {
        let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
        parse_vtk_legacy_ascii(&text)
    }
}

pub struct VtkXmlReader;

impl Reader for VtkXmlReader {
    fn format_name(&self) -> &'static str {
        "VTK XML"
    }

    fn read_path(&self, path: &Path) -> Result<Dataset> {
        let _ = path;
        Err(Error::unimplemented(
            "VTK XML",
            "XML VTK (.vtu/.vtp/.vti/…) parsing is planned; use legacy ASCII .vtk for now",
        ))
    }
}

pub fn parse_vtk_legacy_ascii(text: &str) -> Result<Dataset> {
    let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
    if lines.is_empty() {
        return Err(Error::parse("VTK legacy ASCII", "empty file"));
    }
    if !lines[0].starts_with("# vtk DataFile") {
        return Err(Error::parse(
            "VTK legacy ASCII",
            "missing '# vtk DataFile Version' header",
        ));
    }

    let title = lines.get(1).unwrap_or(&"").trim().to_string();
    let format = lines
        .get(2)
        .map(|s| s.trim().to_ascii_uppercase())
        .unwrap_or_default();
    if format != "ASCII" {
        return Err(Error::unimplemented(
            "VTK legacy",
            format!("only ASCII is supported in this slice (got {format})"),
        ));
    }

    let dataset_line = lines
        .get(3)
        .ok_or_else(|| Error::parse("VTK legacy ASCII", "missing DATASET line"))?;
    let ds_upper = dataset_line.to_ascii_uppercase();
    if !ds_upper.starts_with("DATASET") {
        return Err(Error::parse(
            "VTK legacy ASCII",
            format!("expected DATASET, got '{dataset_line}'"),
        ));
    }

    let kind = ds_upper
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .to_string();

    match kind.as_str() {
        "STRUCTURED_GRID" => parse_structured_grid(&lines, title),
        "UNSTRUCTURED_GRID" => parse_unstructured_grid(&lines, title),
        "POLYDATA" => parse_polydata(&lines, title),
        "RECTILINEAR_GRID" | "STRUCTURED_POINTS" | "FIELD" => Err(Error::unimplemented(
            "VTK legacy ASCII",
            format!("DATASET {kind} not yet implemented"),
        )),
        other => Err(Error::parse(
            "VTK legacy ASCII",
            format!("unknown DATASET {other}"),
        )),
    }
}

fn parse_structured_grid(lines: &[&str], title: String) -> Result<Dataset> {
    let (dims, points, point_data) = read_dims_points_and_scalars(lines)?;
    let (i, j, k) = dims;
    let topology = Topology::Ordered(if k > 1 {
        OrderedDims::Ijk { i, j, k }
    } else if j > 1 {
        OrderedDims::Ij { i, j }
    } else {
        OrderedDims::I { i }
    });

    build_dataset(title, points, topology, point_data, Location::Nodal)
}

fn parse_unstructured_grid(lines: &[&str], title: String) -> Result<Dataset> {
    let mut idx = 4usize;
    let (num_points, points, next) = read_points_section(lines, idx)?;
    idx = next;
    let (connectivity, next) = read_cells_section(lines, idx)?;
    idx = next;
    let _ = idx;
    let point_data = read_point_data_scalars(lines, num_points)?;

    let topology = Topology::FiniteElement {
        num_nodes: num_points,
        connectivity,
    };
    build_dataset(title, points, topology, point_data, Location::Nodal)
}

fn parse_polydata(lines: &[&str], title: String) -> Result<Dataset> {
    // Treat as FE with triangles/polys when present; otherwise I-ordered points.
    let mut idx = 4usize;
    let (num_points, points, next) = read_points_section(lines, idx)?;
    idx = next;

    while idx < lines.len() {
        let u = lines[idx].trim().to_ascii_uppercase();
        if u.starts_with("POLYGONS") || u.starts_with("TRIANGLE_STRIPS") || u.starts_with("LINES")
        {
            // Connectivity optional for first-slice summary; advance past header.
            idx += 1;
            break;
        }
        if u.starts_with("POINT_DATA") || u.starts_with("CELL_DATA") {
            break;
        }
        idx += 1;
    }
    let _ = idx;

    let point_data = read_point_data_scalars(lines, num_points)?;
    let topology = Topology::Ordered(OrderedDims::I { i: num_points });
    build_dataset(title, points, topology, point_data, Location::Nodal)
}

type PointList = Vec<[f64; 3]>;
type ScalarFields = Vec<(String, Vec<f64>)>;

fn read_dims_points_and_scalars(
    lines: &[&str],
) -> Result<((usize, usize, usize), PointList, ScalarFields)> {
    let mut dims = None;
    let mut idx = 4usize;
    while idx < lines.len() {
        let u = lines[idx].trim().to_ascii_uppercase();
        if u.starts_with("DIMENSIONS") {
            let parts: Vec<_> = lines[idx].split_whitespace().collect();
            if parts.len() < 4 {
                return Err(Error::parse("VTK legacy ASCII", "bad DIMENSIONS"));
            }
            dims = Some((
                parts[1].parse().map_err(|_| Error::parse("VTK", "dims"))?,
                parts[2].parse().map_err(|_| Error::parse("VTK", "dims"))?,
                parts[3].parse().map_err(|_| Error::parse("VTK", "dims"))?,
            ));
            idx += 1;
            break;
        }
        idx += 1;
    }
    let dims = dims.ok_or_else(|| Error::parse("VTK legacy ASCII", "missing DIMENSIONS"))?;
    let (num_points, points, _next) = read_points_section(lines, idx)?;
    let expected = dims.0 * dims.1 * dims.2;
    if num_points != expected {
        return Err(Error::parse(
            "VTK legacy ASCII",
            format!("POINTS count {num_points} != DIMENSIONS product {expected}"),
        ));
    }
    let scalars = read_point_data_scalars(lines, num_points)?;
    Ok((dims, points, scalars))
}

fn read_points_section(
    lines: &[&str],
    start: usize,
) -> Result<(usize, Vec<[f64; 3]>, usize)> {
    let mut idx = start;
    while idx < lines.len() && lines[idx].trim().is_empty() {
        idx += 1;
    }
    let header = lines
        .get(idx)
        .ok_or_else(|| Error::parse("VTK legacy ASCII", "missing POINTS"))?;
    let u = header.trim().to_ascii_uppercase();
    if !u.starts_with("POINTS") {
        return Err(Error::parse(
            "VTK legacy ASCII",
            format!("expected POINTS, got '{header}'"),
        ));
    }
    let n: usize = header
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| Error::parse("VTK legacy ASCII", "POINTS count"))?;
    idx += 1;
    let (nums, consumed) = take_n_floats(&lines[idx..], n * 3)?;
    idx += consumed;
    let mut points = Vec::with_capacity(n);
    for i in 0..n {
        points.push([nums[i * 3], nums[i * 3 + 1], nums[i * 3 + 2]]);
    }
    Ok((n, points, idx))
}

fn read_cells_section(lines: &[&str], start: usize) -> Result<(Connectivity, usize)> {
    let mut idx = start;
    while idx < lines.len() && lines[idx].trim().is_empty() {
        idx += 1;
    }
    let header = lines
        .get(idx)
        .ok_or_else(|| Error::parse("VTK legacy ASCII", "missing CELLS"))?;
    let u = header.trim().to_ascii_uppercase();
    if !u.starts_with("CELLS") {
        return Err(Error::parse(
            "VTK legacy ASCII",
            format!("expected CELLS, got '{header}'"),
        ));
    }
    let parts: Vec<_> = header.split_whitespace().collect();
    let ncells: usize = parts
        .get(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| Error::parse("VTK", "CELLS count"))?;
    let size: usize = parts
        .get(2)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| Error::parse("VTK", "CELLS size"))?;
    idx += 1;
    let (nums, consumed) = take_n_floats(&lines[idx..], size)?;
    idx += consumed;

    // Flatten connectivity ignoring the leading count per cell for summary.
    let mut indices = Vec::new();
    let mut cursor = 0usize;
    let mut npe = None;
    for _ in 0..ncells {
        if cursor >= nums.len() {
            break;
        }
        let count = nums[cursor] as usize;
        cursor += 1;
        npe = match npe {
            None => Some(count),
            Some(prev) if prev == count => Some(prev),
            _ => None,
        };
        for _ in 0..count {
            if cursor < nums.len() {
                indices.push(nums[cursor] as usize);
                cursor += 1;
            }
        }
    }

    // Optional CELL_TYPES line
    while idx < lines.len() && lines[idx].trim().is_empty() {
        idx += 1;
    }
    if idx < lines.len() && lines[idx].trim().to_ascii_uppercase().starts_with("CELL_TYPES") {
        idx += 1;
        let (_types, ccons) = take_n_floats(&lines[idx..], ncells)?;
        idx += ccons;
    }

    let element_type = match npe {
        Some(2) => ElementType::Line,
        Some(3) => ElementType::Triangle,
        Some(4) => ElementType::Tetra, // could also be quad; Mixed would be safer
        Some(8) => ElementType::Hexa,
        _ => ElementType::Mixed,
    };

    Ok((
        Connectivity {
            element_type,
            indices,
            nodes_per_elem: npe,
        },
        idx,
    ))
}

fn read_point_data_scalars(lines: &[&str], npoints: usize) -> Result<Vec<(String, Vec<f64>)>> {
    let mut out = Vec::new();
    let mut idx = 0usize;
    while idx < lines.len() {
        let u = lines[idx].trim().to_ascii_uppercase();
        if u.starts_with("POINT_DATA") {
            idx += 1;
            break;
        }
        idx += 1;
    }
    if idx >= lines.len() {
        return Ok(out);
    }

    while idx < lines.len() {
        while idx < lines.len() && lines[idx].trim().is_empty() {
            idx += 1;
        }
        if idx >= lines.len() {
            break;
        }
        let u = lines[idx].trim().to_ascii_uppercase();
        if u.starts_with("CELL_DATA") {
            break;
        }
        if u.starts_with("SCALARS") {
            let name = lines[idx]
                .split_whitespace()
                .nth(1)
                .unwrap_or("scalars")
                .to_string();
            idx += 1;
            // Optional LOOKUP_TABLE
            if idx < lines.len() && lines[idx].trim().to_ascii_uppercase().starts_with("LOOKUP_TABLE")
            {
                idx += 1;
            }
            let (vals, consumed) = take_n_floats(&lines[idx..], npoints)?;
            idx += consumed;
            out.push((name, vals));
            continue;
        }
        if u.starts_with("VECTORS") {
            let name = lines[idx]
                .split_whitespace()
                .nth(1)
                .unwrap_or("vectors")
                .to_string();
            idx += 1;
            let (vals, consumed) = take_n_floats(&lines[idx..], npoints * 3)?;
            idx += consumed;
            // Store magnitude as a single scalar for first-slice summary.
            let mut mag = Vec::with_capacity(npoints);
            for i in 0..npoints {
                let x = vals[i * 3];
                let y = vals[i * 3 + 1];
                let z = vals[i * 3 + 2];
                mag.push((x * x + y * y + z * z).sqrt());
            }
            out.push((format!("{name}_mag"), mag));
            continue;
        }
        // Skip unknown POINT_DATA keywords for now.
        idx += 1;
    }
    Ok(out)
}

fn build_dataset(
    title: String,
    points: Vec<[f64; 3]>,
    topology: Topology,
    scalars: Vec<(String, Vec<f64>)>,
    location: Location,
) -> Result<Dataset> {
    let mut variables = vec!["X".into(), "Y".into(), "Z".into()];
    for (name, _) in &scalars {
        variables.push(name.clone());
    }

    let n = points.len();
    let mut fields = vec![
        VariableData {
            location,
            values: points.iter().map(|p| p[0]).collect(),
        },
        VariableData {
            location,
            values: points.iter().map(|p| p[1]).collect(),
        },
        VariableData {
            location,
            values: points.iter().map(|p| p[2]).collect(),
        },
    ];
    for (_, vals) in scalars {
        let mut v = vals;
        if v.len() != n && location == Location::Nodal {
            v.resize(n, 0.0);
        }
        fields.push(VariableData {
            location,
            values: v,
        });
    }

    Ok(Dataset {
        title: if title.is_empty() { None } else { Some(title) },
        variables,
        zones: vec![Zone {
            name: "VTK".into(),
            topology,
            fields,
        }],
    })
}

fn take_n_floats(lines: &[&str], need: usize) -> Result<(Vec<f64>, usize)> {
    let mut out = Vec::with_capacity(need);
    let mut consumed = 0usize;
    for line in lines {
        if out.len() >= need {
            break;
        }
        let t = line.trim();
        if t.is_empty() {
            consumed += 1;
            continue;
        }
        let upper = t.to_ascii_uppercase();
        // Next section keyword — stop if we somehow overshoot.
        if upper.starts_with("POINT_DATA")
            || upper.starts_with("CELL_DATA")
            || upper.starts_with("CELLS")
            || upper.starts_with("CELL_TYPES")
            || upper.starts_with("POLYGONS")
            || upper.starts_with("LINES")
            || upper.starts_with("SCALARS")
            || upper.starts_with("VECTORS")
            || upper.starts_with("LOOKUP_TABLE")
        {
            break;
        }
        for tok in t.split_whitespace() {
            let v: f64 = tok.parse().map_err(|_| {
                Error::parse("VTK legacy ASCII", format!("bad float '{tok}'"))
            })?;
            out.push(v);
            if out.len() >= need {
                break;
            }
        }
        consumed += 1;
    }
    if out.len() < need {
        return Err(Error::parse(
            "VTK legacy ASCII",
            format!("expected {need} floats, got {}", out.len()),
        ));
    }
    Ok((out, consumed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_structured_grid_ascii() {
        let text = r#"# vtk DataFile Version 3.0
tiny structured
ASCII
DATASET STRUCTURED_GRID
DIMENSIONS 2 2 1
POINTS 4 float
0 0 0
1 0 0
0 1 0
1 1 0
POINT_DATA 4
SCALARS P float 1
LOOKUP_TABLE default
1 2 3 4
"#;
        let ds = parse_vtk_legacy_ascii(text).unwrap();
        assert_eq!(ds.zones.len(), 1);
        assert_eq!(ds.zones[0].node_count(), 4);
        assert!(ds.variables.iter().any(|v| v == "P"));
    }
}
