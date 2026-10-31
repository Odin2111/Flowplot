//! PLT binary `.plt` reader (public Data Format Guide layout).
//!
//! Supports `#!TDV111` and `#!TDV112` little-endian files. No SZL (`.szplt`)
//! and no reverse-engineering of proprietary apps.

use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::Path;

use crate::dataset::{
    Connectivity, Dataset, ElementType, Location, OrderedDims, Topology, VariableData, Zone,
    ZoneMeta,
};
use crate::error::{Error, Result};
use crate::readers::Reader;

const ZONE_MARKER: f32 = 299.0;
const EOH_MARKER: f32 = 357.0;
/// Geometry / Text / CustomLabel / UserRec / DatasetAux / VarAux (Data Format Guide).
const GEOMETRY_MARKER: f32 = 399.0;
const TEXT_MARKER: f32 = 499.0;
const CUSTOM_LABEL_MARKER: f32 = 599.0;
const USER_REC_MARKER: f32 = 699.0;
const DATASET_AUX_MARKER: f32 = 799.0;
const VAR_AUX_MARKER: f32 = 899.0;

pub struct TecplotBinaryReader;

impl Reader for TecplotBinaryReader {
    fn format_name(&self) -> &'static str {
        "PLT binary"
    }

    fn read_path(&self, path: &Path) -> Result<Dataset> {
        let mut file = File::open(path).map_err(|e| Error::io(path, e))?;
        parse_tecplot_binary(&mut file)
    }
}

/// True when the file begins with the public PLT binary magic `#!TDV`.
pub fn is_tecplot_binary(path: &Path) -> Result<bool> {
    let mut buf = [0u8; 5];
    let mut f = File::open(path).map_err(|e| Error::io(path, e))?;
    let n = f.read(&mut buf).map_err(|e| Error::io(path, e))?;
    Ok(n >= 5 && &buf[..5] == b"#!TDV")
}

pub fn parse_tecplot_binary<R: Read + Seek>(r: &mut R) -> Result<Dataset> {
    let mut magic = [0u8; 8];
    r.read_exact(&mut magic)
        .map_err(|e| Error::parse("PLT binary", format!("short magic: {e}")))?;
    if &magic[..5] != b"#!TDV" {
        return Err(Error::parse(
            "PLT binary",
            format!(
                "expected #!TDV… magic, got {:?}",
                String::from_utf8_lossy(&magic)
            ),
        ));
    }
    let version = parse_version(&magic)?;
    let endian_one = read_i32(r)?;
    if endian_one != 1 {
        return Err(Error::parse(
            "PLT binary",
            "only little-endian files (byte-order marker 1) are supported",
        ));
    }

    // FileType appears from TDV111 onward.
    let _file_type = if version >= 111 { read_i32(r)? } else { 0 };

    let title = read_tec_string(r)?;
    let num_var = read_i32(r)?;
    if !(1..=4096).contains(&num_var) {
        return Err(Error::parse(
            "PLT binary",
            format!("invalid NumVars={num_var}"),
        ));
    }
    let num_var = num_var as usize;
    let mut variables = Vec::with_capacity(num_var);
    for _ in 0..num_var {
        variables.push(read_tec_string(r)?);
    }

    let mut zone_headers = Vec::new();
    loop {
        let marker = read_f32(r)?;
        if (marker - EOH_MARKER).abs() < 1e-3 {
            break;
        }
        if (marker - ZONE_MARKER).abs() < 1e-3 {
            zone_headers.push(read_zone_header(r, version, num_var)?);
            continue;
        }
        // LEHAR `.combined.plt` writes TECTXT* run metadata (499) before zones.
        // Skip non-zone header records so zone data still loads.
        if (marker - TEXT_MARKER).abs() < 1e-3 {
            skip_text_record(r)?;
            continue;
        }
        if (marker - GEOMETRY_MARKER).abs() < 1e-3 {
            skip_geometry_record(r)?;
            continue;
        }
        if (marker - CUSTOM_LABEL_MARKER).abs() < 1e-3 {
            skip_custom_label_record(r)?;
            continue;
        }
        if (marker - USER_REC_MARKER).abs() < 1e-3 {
            let _ = read_tec_string(r)?;
            continue;
        }
        if (marker - DATASET_AUX_MARKER).abs() < 1e-3 {
            let _ = read_tec_string(r)?;
            let _fmt = read_i32(r)?;
            let _ = read_tec_string(r)?;
            continue;
        }
        if (marker - VAR_AUX_MARKER).abs() < 1e-3 {
            // Variable number is written as a Tecplot int32 string (chars + 0).
            let _ = read_tec_string(r)?;
            let _ = read_tec_string(r)?;
            let _fmt = read_i32(r)?;
            let _ = read_tec_string(r)?;
            continue;
        }
        return Err(Error::parse(
            "PLT binary",
            format!("unsupported header marker {marker}"),
        ));
    }

    if zone_headers.is_empty() {
        return Err(Error::parse("PLT binary", "no zones in header"));
    }

    let mut zones = Vec::with_capacity(zone_headers.len());
    for header in &zone_headers {
        let marker = read_f32(r)?;
        if (marker - ZONE_MARKER).abs() >= 1e-3 {
            return Err(Error::parse(
                "PLT binary",
                format!("expected zone data marker 299, got {marker}"),
            ));
        }
        zones.push(read_zone_data(r, header, num_var)?);
    }

    let zone_meta: Vec<_> = zone_headers
        .iter()
        .map(|h| ZoneMeta {
            solution_time: h.solution_time,
            strand_id: h.strand_id,
            aux: h.aux.clone(),
        })
        .collect();
    Ok(Dataset {
        title: if title.is_empty() {
            None
        } else {
            Some(title)
        },
        variables,
        zones,
        aux: Vec::new(),
        zone_meta,
    })
}

fn parse_version(magic: &[u8; 8]) -> Result<u32> {
    let s = std::str::from_utf8(magic)
        .map_err(|_| Error::parse("PLT binary", "magic is not UTF-8"))?;
    let digits: String = s
        .chars()
        .skip(5)
        .take(3)
        .filter(|c| c.is_ascii_digit())
        .collect();
    digits.parse::<u32>().map_err(|_| {
        Error::parse(
            "PLT binary",
            format!("cannot parse version from magic {s:?}"),
        )
    })
}

struct ZoneHeader {
    name: String,
    zone_type: i32,
    var_locations: Vec<Location>,
    dims: Option<[usize; 3]>,
    num_points: usize,
    num_elements: usize,
    solution_time: Option<f64>,
    strand_id: Option<i32>,
    aux: Vec<(String, String)>,
}

fn ordered_cell_count(i: usize, j: usize, k: usize) -> usize {
    let di = i.saturating_sub(1);
    let dj = j.saturating_sub(1);
    let dk = k.saturating_sub(1);
    match (j, k) {
        (1, 1) => di,
        (_, 1) => di * dj,
        _ => di * dj * dk,
    }
}

/// Skip a Text header record (marker 499 already consumed).
fn skip_text_record<R: Read>(r: &mut R) -> Result<()> {
    let _coord_sys = read_i32(r)?;
    let _scope = read_i32(r)?;
    let _x = read_f64(r)?;
    let _y = read_f64(r)?;
    let _z = read_f64(r)?;
    let _font = read_i32(r)?;
    let _height_units = read_i32(r)?;
    let _height = read_f64(r)?;
    let _box_type = read_i32(r)?;
    let _box_margin = read_f64(r)?;
    let _box_linewidth = read_f64(r)?;
    let _box_outline = read_i32(r)?;
    let _box_fill = read_i32(r)?;
    let _angle = read_f64(r)?;
    let _line_spacing = read_f64(r)?;
    let _anchor = read_i32(r)?;
    let _zone = read_i32(r)?;
    let _color = read_i32(r)?;
    let _macro = read_tec_string(r)?;
    let _clipping = read_i32(r)?;
    let _text = read_tec_string(r)?;
    Ok(())
}

/// Skip a Geometry header record (marker 399 already consumed).
fn skip_geometry_record<R: Read>(r: &mut R) -> Result<()> {
    let coord_sys = read_i32(r)?;
    let _scope = read_i32(r)?;
    let _draw_order = read_i32(r)?;
    let _x = read_f64(r)?;
    let _y = read_f64(r)?;
    let _z = read_f64(r)?;
    let _zone = read_i32(r)?;
    let _color = read_i32(r)?;
    let _fill_color = read_i32(r)?;
    let _is_filled = read_i32(r)?;
    let geom_type = read_i32(r)?;
    let _line_pattern = read_i32(r)?;
    let _pattern_length = read_f64(r)?;
    let _line_thickness = read_f64(r)?;
    let _num_ellipse_pts = read_i32(r)?;
    let _arrow_style = read_i32(r)?;
    let _arrow_attach = read_i32(r)?;
    let _arrow_size = read_f64(r)?;
    let _arrow_angle = read_f64(r)?;
    let _macro = read_tec_string(r)?;
    let gtype = read_i32(r)?; // 1=float, 2=double
    let _clipping = read_i32(r)?;
    let grid3d = coord_sys == 4;
    match geom_type {
        0 => {
            // Line: polylines with X/Y/(Z) blocks.
            let n_poly = read_i32(r)?;
            if !(0..=1_000_000).contains(&n_poly) {
                return Err(Error::parse(
                    "PLT binary",
                    format!("geometry polyline count {n_poly} out of range"),
                ));
            }
            for _ in 0..n_poly {
                let n_pts = read_i32(r)?;
                if !(0..=10_000_000).contains(&n_pts) {
                    return Err(Error::parse(
                        "PLT binary",
                        format!("geometry point count {n_pts} out of range"),
                    ));
                }
                let n = n_pts as usize;
                let blocks = if grid3d { 3 } else { 2 };
                for _ in 0..blocks {
                    skip_gtype_block(r, gtype, n)?;
                }
            }
        }
        1 | 4 => {
            // Rectangle / Ellipse: two GTYPE values.
            skip_gtype_block(r, gtype, 2)?;
        }
        2 | 3 => {
            // Square / Circle: one GTYPE value.
            skip_gtype_block(r, gtype, 1)?;
        }
        other => {
            return Err(Error::parse(
                "PLT binary",
                format!("unsupported geometry type {other}"),
            ));
        }
    }
    Ok(())
}

fn skip_gtype_block<R: Read>(r: &mut R, gtype: i32, count: usize) -> Result<()> {
    match gtype {
        1 => {
            for _ in 0..count {
                let _ = read_f32(r)?;
            }
        }
        2 => {
            for _ in 0..count {
                let _ = read_f64(r)?;
            }
        }
        other => {
            return Err(Error::parse(
                "PLT binary",
                format!("unsupported geometry field type {other}"),
            ));
        }
    }
    Ok(())
}

fn skip_custom_label_record<R: Read>(r: &mut R) -> Result<()> {
    let n = read_i32(r)?;
    if !(0..=100_000).contains(&n) {
        return Err(Error::parse(
            "PLT binary",
            format!("custom label count {n} out of range"),
        ));
    }
    for _ in 0..n {
        let _ = read_tec_string(r)?;
    }
    Ok(())
}

fn read_zone_header<R: Read>(r: &mut R, version: u32, num_var: usize) -> Result<ZoneHeader> {
    let name = read_tec_string(r)?;
    if version >= 107 {
        let _parent = read_i32(r)?;
    }
    let mut strand_id = None;
    let mut solution_time = None;
    if version >= 102 {
        let strand = read_i32(r)?;
        let sol = read_f64(r)?;
        let _color = read_i32(r)?;
        // Format uses -1 / 0 for “no strand”; keep any non-negative id.
        if strand >= 0 {
            strand_id = Some(strand);
        }
        if sol.is_finite() {
            solution_time = Some(sol);
        }
    }
    let zone_type = read_i32(r)?;
    // DataPacking present through TDV111; TDV112+ is always block.
    let data_packing = if version <= 111 {
        read_i32(r)?
    } else {
        0
    };
    if data_packing != 0 {
        return Err(Error::parse(
            "PLT binary",
            format!("zone {name:?} uses POINT packing (unsupported; public block packing only)"),
        ));
    }
    let specify_loc = read_i32(r)?;
    let var_locations = if specify_loc != 0 {
        let mut locs = Vec::with_capacity(num_var);
        for _ in 0..num_var {
            locs.push(if read_i32(r)? == 0 {
                Location::Nodal
            } else {
                Location::CellCentered
            });
        }
        locs
    } else {
        vec![Location::Nodal; num_var]
    };

    let raw_fn = read_i32(r)?;
    let misc_fn = read_i32(r)?;
    if raw_fn != 0 || misc_fn != 0 {
        return Err(Error::parse(
            "PLT binary",
            format!("zone {name:?}: face-neighbor arrays are not supported"),
        ));
    }

    let (dims, num_points, num_elements) = if zone_type == 0 {
        let i = read_i32(r)? as usize;
        let j = read_i32(r)? as usize;
        let k = read_i32(r)? as usize;
        if i == 0 || j == 0 || k == 0 {
            return Err(Error::parse(
                "PLT binary",
                format!("zone {name:?}: invalid IJK ({i},{j},{k})"),
            ));
        }
        (
            Some([i, j, k]),
            i * j * k,
            ordered_cell_count(i, j, k),
        )
    } else if (1..=5).contains(&zone_type) {
        let npts = read_i32(r)? as usize;
        let nelems = read_i32(r)? as usize;
        let _ic = read_i32(r)?;
        let _jc = read_i32(r)?;
        let _kc = read_i32(r)?;
        (None, npts, nelems)
    } else {
        return Err(Error::parse(
            "PLT binary",
            format!("zone {name:?}: unsupported ZoneType={zone_type}"),
        ));
    };

    let mut aux = Vec::new();
    loop {
        let flag = read_i32(r)?;
        if flag == 0 {
            break;
        }
        if flag != 1 {
            return Err(Error::parse(
                "PLT binary",
                format!("zone {name:?}: bad aux flag {flag}"),
            ));
        }
        let aux_name = read_tec_string(r)?;
        let _fmt = read_i32(r)?;
        let aux_val = read_tec_string(r)?;
        if !aux_name.is_empty() {
            aux.push((aux_name, aux_val));
        }
    }

    Ok(ZoneHeader {
        name,
        zone_type,
        var_locations,
        dims,
        num_points,
        num_elements,
        solution_time,
        strand_id,
        aux,
    })
}

fn read_zone_data<R: Read>(r: &mut R, header: &ZoneHeader, num_var: usize) -> Result<Zone> {
    let mut data_types = Vec::with_capacity(num_var);
    for _ in 0..num_var {
        data_types.push(read_i32(r)?);
    }
    let has_passive = read_i32(r)?;
    let passive = if has_passive != 0 {
        let mut v = Vec::with_capacity(num_var);
        for _ in 0..num_var {
            v.push(read_i32(r)?);
        }
        v
    } else {
        vec![0; num_var]
    };
    let has_share = read_i32(r)?;
    let shared_from = if has_share != 0 {
        let mut v = Vec::with_capacity(num_var);
        for _ in 0..num_var {
            v.push(read_i32(r)?);
        }
        v
    } else {
        vec![-1; num_var]
    };
    let conn_share = read_i32(r)?;

    for i in 0..num_var {
        if passive[i] != 0 || shared_from[i] >= 0 {
            continue;
        }
        let _min = read_f64(r)?;
        let _max = read_f64(r)?;
    }

    let mut fields = Vec::with_capacity(num_var);
    for i in 0..num_var {
        if passive[i] != 0 || shared_from[i] >= 0 {
            fields.push(VariableData {
                location: header.var_locations[i],
                values: Vec::new(),
            });
            continue;
        }
        let count = match header.var_locations[i] {
            Location::Nodal => header.num_points,
            Location::CellCentered => header.num_elements,
        };
        let values = read_var_block(r, data_types[i], count)?;
        fields.push(VariableData {
            location: header.var_locations[i],
            values,
        });
    }

    let topology = if header.zone_type == 0 {
        let d = header.dims.unwrap_or([1, 1, 1]);
        let (i, j, k) = (d[0], d[1], d[2]);
        let dims = if k > 1 {
            OrderedDims::Ijk { i, j, k }
        } else if j > 1 {
            OrderedDims::Ij { i, j }
        } else {
            OrderedDims::I { i }
        };
        Topology::Ordered(dims)
    } else {
        let et = match header.zone_type {
            1 => ElementType::Line,
            2 => ElementType::Triangle,
            3 => ElementType::Quad,
            4 => ElementType::Tetra,
            5 => ElementType::Hexa,
            _ => ElementType::Mixed,
        };
        let npe = et.nodes_per_element().unwrap_or(0);
        let mut indices = Vec::new();
        if conn_share < 0 && npe > 0 && header.num_elements > 0 {
            let need = header.num_elements * npe;
            let mut raw = vec![0i32; need];
            for x in &mut raw {
                *x = read_i32(r)?;
            }
            let min = raw.iter().copied().min().unwrap_or(0);
            let max = raw.iter().copied().max().unwrap_or(0);
            let zero_based = min == 0 && (max as usize) < header.num_points;
            let one_based = min >= 1 && (max as usize) <= header.num_points;
            if !zero_based && !one_based {
                return Err(Error::parse(
                    "PLT binary",
                    format!(
                        "zone {:?}: connectivity range [{min},{max}] incompatible with {} nodes",
                        header.name, header.num_points
                    ),
                ));
            }
            indices = raw
                .into_iter()
                .map(|v| {
                    if one_based {
                        (v as usize).saturating_sub(1)
                    } else {
                        v as usize
                    }
                })
                .collect();
        }
        Topology::FiniteElement {
            num_nodes: header.num_points,
            connectivity: Connectivity {
                element_type: et,
                indices,
                nodes_per_elem: if npe > 0 { Some(npe) } else { None },
            },
        }
    };

    Ok(Zone {
        name: if header.name.is_empty() {
            "ZONE".into()
        } else {
            header.name.clone()
        },
        topology,
        fields,
    })
}

fn read_var_block<R: Read>(r: &mut R, data_type: i32, count: usize) -> Result<Vec<f64>> {
    let mut out = Vec::with_capacity(count);
    match data_type {
        1 => {
            for _ in 0..count {
                out.push(read_f32(r)? as f64);
            }
        }
        2 => {
            for _ in 0..count {
                out.push(read_f64(r)?);
            }
        }
        3 => {
            for _ in 0..count {
                out.push(read_i32(r)? as f64);
            }
        }
        4 => {
            for _ in 0..count {
                out.push(read_i16(r)? as f64);
            }
        }
        5 => {
            for _ in 0..count {
                let mut b = [0u8; 1];
                r.read_exact(&mut b)
                    .map_err(|e| Error::parse("PLT binary", e.to_string()))?;
                out.push(b[0] as f64);
            }
        }
        6 => {
            return Err(Error::parse(
                "PLT binary",
                "bit-packed variables are not supported",
            ));
        }
        other => {
            return Err(Error::parse(
                "PLT binary",
                format!("unsupported variable data type {other}"),
            ));
        }
    }
    Ok(out)
}

fn read_tec_string<R: Read>(r: &mut R) -> Result<String> {
    let mut s = String::new();
    loop {
        let c = read_i32(r)?;
        if c == 0 {
            return Ok(s);
        }
        if !(0..=0x10FFFF).contains(&c) {
            return Err(Error::parse(
                "PLT binary",
                format!("invalid string code point {c}"),
            ));
        }
        s.push(char::from_u32(c as u32).unwrap_or('\u{FFFD}'));
    }
}

fn read_i16<R: Read>(r: &mut R) -> Result<i16> {
    let mut b = [0u8; 2];
    r.read_exact(&mut b)
        .map_err(|e| Error::parse("PLT binary", e.to_string()))?;
    Ok(i16::from_le_bytes(b))
}

fn read_i32<R: Read>(r: &mut R) -> Result<i32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)
        .map_err(|e| Error::parse("PLT binary", e.to_string()))?;
    Ok(i32::from_le_bytes(b))
}

fn read_f32<R: Read>(r: &mut R) -> Result<f32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)
        .map_err(|e| Error::parse("PLT binary", e.to_string()))?;
    Ok(f32::from_le_bytes(b))
}

fn read_f64<R: Read>(r: &mut R) -> Result<f64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)
        .map_err(|e| Error::parse("PLT binary", e.to_string()))?;
    Ok(f64::from_le_bytes(b))
}

/// Write a minimal little-endian `#!TDV112` ordered IJ zone (fixtures/tests).
#[allow(dead_code)]
pub fn write_minimal_tdv112_ordered(
    path: &Path,
    title: &str,
    zone_name: &str,
    i: usize,
    j: usize,
    variables: &[&str],
    blocks: &[Vec<f32>],
) -> Result<()> {
    if variables.len() != blocks.len() {
        return Err(Error::parse(
            "PLT binary",
            "variables/blocks length mismatch",
        ));
    }
    let nnodes = i * j;
    for b in blocks {
        if b.len() != nnodes {
            return Err(Error::parse(
                "PLT binary",
                format!("block len {} != I*J={nnodes}", b.len()),
            ));
        }
    }
    let mut f = File::create(path).map_err(|e| Error::io(path, e))?;
    f.write_all(b"#!TDV112")
        .map_err(|e| Error::io(path, e))?;
    write_i32(&mut f, path, 1)?;
    write_i32(&mut f, path, 0)?;
    write_tec_string(&mut f, path, title)?;
    write_i32(&mut f, path, variables.len() as i32)?;
    for v in variables {
        write_tec_string(&mut f, path, v)?;
    }
    // Optional LEHAR-style TECTXT metadata (marker 499) before the zone.
    write_f32(&mut f, path, TEXT_MARKER)?;
    write_i32(&mut f, path, 1)?; // Frame coords
    write_i32(&mut f, path, 1)?; // Local scope
    write_f64(&mut f, path, 0.02)?;
    write_f64(&mut f, path, 0.97)?;
    write_f64(&mut f, path, 0.0)?;
    write_i32(&mut f, path, 1)?; // font
    write_i32(&mut f, path, 2)?; // point height units
    write_f64(&mut f, path, 10.0)?;
    write_i32(&mut f, path, 1)?; // hollow box
    write_f64(&mut f, path, 0.35)?;
    write_f64(&mut f, path, 0.05)?;
    write_i32(&mut f, path, 0)?;
    write_i32(&mut f, path, 7)?;
    write_f64(&mut f, path, 0.0)?;
    write_f64(&mut f, path, 1.1)?;
    write_i32(&mut f, path, 6)?; // anchor
    write_i32(&mut f, path, 0)?; // zone
    write_i32(&mut f, path, 0)?; // color
    write_tec_string(&mut f, path, "")?; // macro
    write_i32(&mut f, path, 0)?; // clipping
    write_tec_string(&mut f, path, "LEHAR\nFlux: hllc")?;

    write_f32(&mut f, path, ZONE_MARKER)?;
    write_tec_string(&mut f, path, zone_name)?;
    write_i32(&mut f, path, -1)?;
    write_i32(&mut f, path, -1)?;
    write_f64(&mut f, path, 0.0)?;
    write_i32(&mut f, path, -1)?;
    write_i32(&mut f, path, 0)?;
    write_i32(&mut f, path, 0)?;
    write_i32(&mut f, path, 0)?;
    write_i32(&mut f, path, 0)?;
    write_i32(&mut f, path, i as i32)?;
    write_i32(&mut f, path, j as i32)?;
    write_i32(&mut f, path, 1)?;
    write_i32(&mut f, path, 0)?;
    write_f32(&mut f, path, EOH_MARKER)?;

    write_f32(&mut f, path, ZONE_MARKER)?;
    for _ in variables {
        write_i32(&mut f, path, 1)?;
    }
    write_i32(&mut f, path, 0)?;
    write_i32(&mut f, path, 0)?;
    write_i32(&mut f, path, -1)?;
    for b in blocks {
        let (mn, mx) = b.iter().copied().fold(
            (f32::INFINITY, f32::NEG_INFINITY),
            |(mn, mx), v| (mn.min(v), mx.max(v)),
        );
        write_f64(&mut f, path, mn as f64)?;
        write_f64(&mut f, path, mx as f64)?;
    }
    for b in blocks {
        for &v in b {
            write_f32(&mut f, path, v)?;
        }
    }
    Ok(())
}

fn write_i32(f: &mut File, path: &Path, v: i32) -> Result<()> {
    f.write_all(&v.to_le_bytes()).map_err(|e| Error::io(path, e))
}
fn write_f32(f: &mut File, path: &Path, v: f32) -> Result<()> {
    f.write_all(&v.to_le_bytes()).map_err(|e| Error::io(path, e))
}
fn write_f64(f: &mut File, path: &Path, v: f64) -> Result<()> {
    f.write_all(&v.to_le_bytes()).map_err(|e| Error::io(path, e))
}
fn write_tec_string(f: &mut File, path: &Path, s: &str) -> Result<()> {
    for c in s.chars() {
        write_i32(f, path, c as i32)?;
    }
    write_i32(f, path, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn roundtrip_minimal_tdv112() {
        let path = std::env::temp_dir().join("flowplot_minimal_tdv112.plt");
        write_minimal_tdv112_ordered(
            &path,
            "demo",
            "z1",
            2,
            2,
            &["X", "Y", "P"],
            &[
                vec![0.0, 1.0, 0.0, 1.0],
                vec![0.0, 0.0, 1.0, 1.0],
                vec![1.0, 2.0, 3.0, 4.0],
            ],
        )
        .unwrap();
        let ds = TecplotBinaryReader.read_path(&path).unwrap();
        assert_eq!(ds.title.as_deref(), Some("demo"));
        assert_eq!(ds.variables, vec!["X", "Y", "P"]);
        assert_eq!(ds.zones[0].node_count(), 4);
        assert_eq!(ds.zones[0].fields[2].values, vec![1.0, 2.0, 3.0, 4.0]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn skips_text_marker_before_zone() {
        // write_minimal_tdv112_ordered embeds a 499 text record like LEHAR combined.plt.
        let path = std::env::temp_dir().join("flowplot_tdv112_with_text.plt");
        write_minimal_tdv112_ordered(
            &path,
            "combined",
            "R0_Tet",
            2,
            2,
            &["X", "Y", "P"],
            &[
                vec![0.0, 1.0, 0.0, 1.0],
                vec![0.0, 0.0, 1.0, 1.0],
                vec![1.0, 2.0, 3.0, 4.0],
            ],
        )
        .unwrap();
        let raw = std::fs::read(&path).unwrap();
        let mut has_text = false;
        for i in 0..raw.len().saturating_sub(4) {
            let v = f32::from_le_bytes(raw[i..i + 4].try_into().unwrap());
            if (v - TEXT_MARKER).abs() < 1e-3 {
                has_text = true;
                break;
            }
        }
        assert!(has_text, "fixture must include text marker 499");
        let ds = TecplotBinaryReader.read_path(&path).unwrap();
        assert_eq!(ds.zone_count(), 1);
        assert_eq!(ds.zones[0].name, "R0_Tet");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rejects_bad_magic() {
        let err = parse_tecplot_binary(&mut Cursor::new(b"TITLE = hi")).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("#!TDV") || msg.contains("magic"), "{msg}");
    }
}
