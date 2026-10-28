//! Tecplot ASCII (`.dat` / `.tp` / ASCII `.plt`) reader — public ASCII layout.
//!
//! Supports a minimal subset: `TITLE`, `VARIABLES`, and `ZONE` with
//! `I`/`J`/`K` ordered data or `N`/`E`/`ET` finite-element connectivity.
//! Binary `.plt` (`#!TDV…`) is handled by [`super::tecplot_binary`].

use std::path::Path;

use crate::dataset::{
    Connectivity, Dataset, ElementType, Location, OrderedDims, Topology, VariableData, Zone,
};
use crate::error::{Error, Result};
use crate::readers::Reader;

pub struct TecplotAsciiReader;

impl Reader for TecplotAsciiReader {
    fn format_name(&self) -> &'static str {
        "Tecplot ASCII"
    }

    fn read_path(&self, path: &Path) -> Result<Dataset> {
        let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
        parse_tecplot_ascii(&text)
    }
}

pub fn parse_tecplot_ascii(text: &str) -> Result<Dataset> {
    let mut dataset = Dataset::new();
    let lines = preprocess_lines(text);
    let mut i = 0usize;

    while i < lines.len() {
        let line = lines[i].as_str();
        let upper = line.to_ascii_uppercase();

        if upper.starts_with("TITLE") {
            dataset.title = Some(parse_quoted_or_rest(line));
            i += 1;
        } else if upper.starts_with("VARIABLES") {
            dataset.variables = parse_variables_line(line);
            i += 1;
            // Continuation lines of quoted names until ZONE / DATAPACKING / etc.
            while i < lines.len() {
                let next = lines[i].as_str();
                let nu = next.to_ascii_uppercase();
                if nu.starts_with("ZONE")
                    || nu.starts_with("TITLE")
                    || nu.starts_with("VARIABLES")
                    || nu.starts_with("DATASETAUXDATA")
                {
                    break;
                }
                if next.contains('"') {
                    dataset.variables.extend(parse_quoted_names(next));
                    i += 1;
                } else {
                    break;
                }
            }
        } else if upper.starts_with("ZONE") {
            let (zone, consumed) = parse_zone(&lines[i..], &dataset.variables)?;
            dataset.zones.push(zone);
            i += consumed;
        } else {
            i += 1;
        }
    }

    if dataset.zones.is_empty() {
        return Err(Error::parse(
            "Tecplot ASCII",
            "no ZONE records found",
        ));
    }

    // Ensure every zone has a field slot per variable name.
    let nvar = dataset.variables.len();
    for zone in &mut dataset.zones {
        while zone.fields.len() < nvar {
            zone.fields.push(VariableData {
                location: Location::Nodal,
                values: Vec::new(),
            });
        }
    }

    Ok(dataset)
}

fn preprocess_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| {
            // Strip Tecplot `#` comments.
            let cut = l.find('#').map(|i| &l[..i]).unwrap_or(l);
            cut.trim().to_string()
        })
        .filter(|l| !l.is_empty())
        .collect()
}

fn parse_quoted_or_rest(line: &str) -> String {
    if let Some(names) = first_quoted(line) {
        names
    } else if let Some(eq) = line.find('=') {
        line[eq + 1..].trim().trim_matches('"').to_string()
    } else {
        String::new()
    }
}

fn first_quoted(s: &str) -> Option<String> {
    let start = s.find('"')?;
    let rest = &s[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn parse_quoted_names(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' {
            let mut name = String::new();
            for c2 in chars.by_ref() {
                if c2 == '"' {
                    break;
                }
                name.push(c2);
            }
            out.push(name);
        }
    }
    out
}

fn parse_variables_line(line: &str) -> Vec<String> {
    let after = line
        .split_once('=')
        .map(|(_, r)| r.trim())
        .unwrap_or(line);
    let quoted = parse_quoted_names(after);
    if !quoted.is_empty() {
        return quoted;
    }
    after
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_matches('"').to_string())
        .collect()
}

fn looks_like_number_start(s: &str) -> bool {
    let t = s.trim_start();
    t.starts_with(|c: char| c.is_ascii_digit() || c == '+' || c == '-' || c == '.')
}

struct ZoneHeader {
    name: String,
    i: Option<usize>,
    j: Option<usize>,
    k: Option<usize>,
    n: Option<usize>,
    e: Option<usize>,
    et: Option<String>,
    datapacking: String,
    varlocation: Location,
}

impl Default for ZoneHeader {
    fn default() -> Self {
        Self {
            name: String::new(),
            i: None,
            j: None,
            k: None,
            n: None,
            e: None,
            et: None,
            datapacking: "POINT".into(),
            varlocation: Location::Nodal,
        }
    }
}

fn parse_zone(lines: &[String], var_names: &[String]) -> Result<(Zone, usize)> {
    let header_line = &lines[0];
    let mut header = parse_zone_header(header_line)?;

    // ZONE parameters may continue on following lines until numeric data.
    let mut idx = 1usize;
    while idx < lines.len() {
        let l = lines[idx].as_str();
        let u = l.to_ascii_uppercase();
        if looks_like_number_start(l) && !u.contains('=') {
            break;
        }
        if u.starts_with("ZONE") || u.starts_with("TITLE") || u.starts_with("VARIABLES") {
            break;
        }
        // Merge additional key=value tokens into header.
        merge_zone_kv(&mut header, l)?;
        idx += 1;
    }

    let nvar = var_names.len().max(1);
    let (topology, values_flat, consumed_data) =
        read_zone_payload(&lines[idx..], &header, nvar)?;

    let mut fields = Vec::with_capacity(nvar);
    let nnodes = topology.node_count();
    let ncells = topology.cell_count();
    let per_var = match header.varlocation {
        Location::Nodal => nnodes,
        Location::CellCentered => ncells,
    };

    if header.datapacking.eq_ignore_ascii_case("POINT") || header.datapacking.is_empty() {
        // POINT: for each node/cell, nvar values interleaved.
        for v in 0..nvar {
            let mut vals = Vec::with_capacity(per_var);
            for n in 0..per_var {
                let flat = n * nvar + v;
                vals.push(values_flat.get(flat).copied().unwrap_or(0.0));
            }
            fields.push(VariableData {
                location: header.varlocation,
                values: vals,
            });
        }
    } else {
        // BLOCK: each variable is a contiguous block of per_var values.
        for v in 0..nvar {
            let start = v * per_var;
            let end = start + per_var;
            let vals = values_flat
                .get(start..end)
                .map(|s| s.to_vec())
                .unwrap_or_default();
            fields.push(VariableData {
                location: header.varlocation,
                values: vals,
            });
        }
    }

    let zone = Zone {
        name: if header.name.is_empty() {
            "ZONE".into()
        } else {
            header.name
        },
        topology,
        fields,
    };

    Ok((zone, idx + consumed_data))
}

fn parse_zone_header(line: &str) -> Result<ZoneHeader> {
    let mut header = ZoneHeader {
        datapacking: "POINT".into(),
        varlocation: Location::Nodal,
        ..Default::default()
    };
    // Optional T="name" right after ZONE
    let rest = line
        .trim_start()
        .strip_prefix(|c: char| c.eq_ignore_ascii_case(&'z'))
        .and_then(|s| s.strip_prefix(|c: char| c.eq_ignore_ascii_case(&'o')))
        .and_then(|s| s.strip_prefix(|c: char| c.eq_ignore_ascii_case(&'n')))
        .and_then(|s| s.strip_prefix(|c: char| c.eq_ignore_ascii_case(&'e')))
        .unwrap_or(line)
        .trim_start();

    merge_zone_kv(&mut header, rest)?;
    Ok(header)
}

fn merge_zone_kv(header: &mut ZoneHeader, s: &str) -> Result<()> {
    // Tokenize roughly on spaces/commas while respecting quotes.
    let tokens = split_zone_tokens(s);
    for token in tokens {
        let Some((k, v)) = token.split_once('=') else {
            continue;
        };
        let key = k.trim().to_ascii_uppercase();
        let val = v.trim().trim_matches('"').to_string();
        match key.as_str() {
            "T" => header.name = val,
            "I" => header.i = Some(parse_usize(&val, "I")?),
            "J" => header.j = Some(parse_usize(&val, "J")?),
            "K" => header.k = Some(parse_usize(&val, "K")?),
            "N" => header.n = Some(parse_usize(&val, "N")?),
            "E" => header.e = Some(parse_usize(&val, "E")?),
            "ET" => header.et = Some(val.to_ascii_uppercase()),
            "DATAPACKING" | "F" => {
                // F=POINT / F=BLOCK legacy synonym.
                header.datapacking = val.to_ascii_uppercase();
                if header.datapacking == "FEPOINT" {
                    header.datapacking = "POINT".into();
                } else if header.datapacking == "FEBLOCK" {
                    header.datapacking = "BLOCK".into();
                }
            }
            "VARLOCATION" => {
                let u = val.to_ascii_uppercase();
                if u.contains("CELLCENTERED") || u.contains("CELL") {
                    header.varlocation = Location::CellCentered;
                } else {
                    header.varlocation = Location::Nodal;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn split_zone_tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    for c in s.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                cur.push(c);
            }
            ',' | ' ' | '\t' if !in_quotes => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn parse_usize(s: &str, label: &str) -> Result<usize> {
    s.parse::<usize>()
        .map_err(|_| Error::parse("Tecplot ASCII", format!("invalid {label}={s}")))
}

fn read_zone_payload(
    lines: &[String],
    header: &ZoneHeader,
    nvar: usize,
) -> Result<(Topology, Vec<f64>, usize)> {
    if let (Some(n), Some(e)) = (header.n, header.e) {
        // Finite element
        let et = match header.et.as_deref() {
            Some("TRIANGLE") | Some("FETRIANGLE") => ElementType::Triangle,
            Some("QUADRILATERAL") | Some("FEQUADRILATERAL") => ElementType::Quad,
            Some("TETRAHEDRON") | Some("FETETRAHEDRON") => ElementType::Tetra,
            Some("BRICK") | Some("FEBRICK") => ElementType::Hexa,
            Some("LINESEG") | Some("FELINESEG") => ElementType::Line,
            _ => ElementType::Mixed,
        };
        let npe = et.nodes_per_element().unwrap_or(0);
        let per = match header.varlocation {
            Location::Nodal => n,
            Location::CellCentered => e,
        };
        let need_vals = per * nvar;
        let (nums, mut consumed) = take_floats(lines, need_vals)?;
        let mut conn = Vec::new();
        if npe > 0 {
            let need_conn = e * npe;
            let (cnums, ccons) = take_floats(&lines[consumed..], need_conn)?;
            consumed += ccons;
            for x in cnums {
                // Tecplot connectivity is 1-based.
                let idx = x as usize;
                conn.push(idx.saturating_sub(1));
            }
        }
        let topology = Topology::FiniteElement {
            num_nodes: n,
            connectivity: Connectivity {
                element_type: et,
                indices: conn,
                nodes_per_elem: if npe > 0 { Some(npe) } else { None },
            },
        };
        Ok((topology, nums, consumed))
    } else {
        let i = header.i.unwrap_or(1);
        let j = header.j.unwrap_or(1);
        let k = header.k.unwrap_or(1);
        let dims = if header.k.is_some() {
            OrderedDims::Ijk { i, j, k }
        } else if header.j.is_some() {
            OrderedDims::Ij { i, j }
        } else {
            OrderedDims::I { i }
        };
        let per = match header.varlocation {
            Location::Nodal => dims.node_count(),
            Location::CellCentered => dims.cell_count(),
        };
        let need = per * nvar;
        let (nums, consumed) = take_floats(lines, need)?;
        Ok((Topology::Ordered(dims), nums, consumed))
    }
}

fn take_floats(lines: &[String], need: usize) -> Result<(Vec<f64>, usize)> {
    let mut out = Vec::with_capacity(need);
    let mut consumed = 0usize;
    for line in lines {
        let u = line.to_ascii_uppercase();
        if u.starts_with("ZONE") || u.starts_with("TITLE") || u.starts_with("VARIABLES") {
            break;
        }
        // Stop if this looks like a key=value header remnant.
        if line.contains('=') && !looks_like_number_start(line) {
            break;
        }
        for tok in line.split(|c: char| c.is_whitespace() || c == ',') {
            let t = tok.trim();
            if t.is_empty() {
                continue;
            }
            match t.parse::<f64>() {
                Ok(v) => {
                    out.push(v);
                    if out.len() >= need {
                        consumed += 1;
                        return Ok((out, consumed));
                    }
                }
                Err(_) => {
                    // Non-numeric token: end of numeric block for this zone.
                    if out.len() >= need {
                        return Ok((out, consumed));
                    }
                    return Err(Error::parse(
                        "Tecplot ASCII",
                        format!("expected float, got '{t}' (have {} of {need})", out.len()),
                    ));
                }
            }
        }
        consumed += 1;
    }
    if out.len() < need {
        return Err(Error::parse(
            "Tecplot ASCII",
            format!("expected {need} values, got {}", out.len()),
        ));
    }
    Ok((out, consumed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ordered_point() {
        let text = r#"
TITLE = "demo"
VARIABLES = "X", "Y", "P"
ZONE T="z1", I=2, J=2, DATAPACKING=POINT
0 0 1
1 0 2
0 1 3
1 1 4
"#;
        let ds = parse_tecplot_ascii(text).unwrap();
        assert_eq!(ds.variables, vec!["X", "Y", "P"]);
        assert_eq!(ds.zones.len(), 1);
        assert_eq!(ds.zones[0].node_count(), 4);
        assert_eq!(ds.zones[0].fields[2].values, vec![1.0, 2.0, 3.0, 4.0]);
    }
}
