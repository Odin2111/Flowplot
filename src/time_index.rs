//! Time scrubber helpers — cycle / solution time / strand frames.

use serde::Serialize;

use crate::dataset::Dataset;

/// One selectable frame for the Fieldplot Time scrubber.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TimeFrame {
    pub index: usize,
    pub zone_indices: Vec<usize>,
    pub time: Option<f64>,
    pub cycle: Option<i64>,
    pub strand_id: Option<i32>,
    pub label: String,
}

/// Build frames from zone metadata (one frame per distinct time/cycle/strand key).
///
/// Zones without time metadata each become their own frame so the scrubber still
/// lists every block.
pub fn list_time_frames(ds: &Dataset) -> Vec<TimeFrame> {
    use std::collections::BTreeMap;

    #[derive(Debug, Clone, PartialEq, PartialOrd, Ord, Eq)]
    struct Key {
        time_bits: Option<u64>,
        cycle: Option<i64>,
        strand: Option<i32>,
        /// When no meta, isolate by zone so each zone is scrubbable.
        lone_zone: Option<usize>,
    }

    let mut map: BTreeMap<Key, Vec<usize>> = BTreeMap::new();
    for zi in 0..ds.zones.len() {
        let meta = ds.meta(zi);
        let time = meta.time_value();
        let cycle = meta.cycle().or_else(|| {
            // file-level cycle only when zone has no other meta
            if time.is_none() && meta.strand_id.is_none() {
                ds.cycle()
            } else {
                None
            }
        });
        let strand = meta.strand_id;
        let key = if time.is_none() && cycle.is_none() && strand.is_none() {
            Key {
                time_bits: None,
                cycle: None,
                strand: None,
                lone_zone: Some(zi),
            }
        } else {
            Key {
                time_bits: time.map(|t| t.to_bits()),
                cycle,
                strand,
                lone_zone: None,
            }
        };
        map.entry(key).or_default().push(zi);
    }

    let mut frames = Vec::new();
    for (i, (key, zones)) in map.into_iter().enumerate() {
        let time = key.time_bits.map(f64::from_bits);
        let cycle = key.cycle;
        let strand_id = key.strand;
        let label = frame_label(i, time, cycle, strand_id, &zones, ds);
        frames.push(TimeFrame {
            index: i,
            zone_indices: zones,
            time,
            cycle,
            strand_id,
            label,
        });
    }
    frames
}

fn frame_label(
    index: usize,
    time: Option<f64>,
    cycle: Option<i64>,
    strand: Option<i32>,
    zones: &[usize],
    ds: &Dataset,
) -> String {
    let mut parts = Vec::new();
    if let Some(c) = cycle {
        parts.push(format!("cycle={c}"));
    }
    if let Some(t) = time {
        parts.push(format!("t={t}"));
    }
    if let Some(s) = strand {
        parts.push(format!("strand={s}"));
    }
    if parts.is_empty() {
        let name = zones
            .first()
            .and_then(|&zi| ds.zones.get(zi))
            .map(|z| z.name.as_str())
            .unwrap_or("zone");
        format!("frame {index}: {name}")
    } else {
        format!("frame {index}: {}", parts.join(" "))
    }
}

/// JSON array fragment for `info` payloads.
pub fn time_frames_json(ds: &Dataset) -> String {
    let frames = list_time_frames(ds);
    let items: Vec<String> = frames
        .iter()
        .map(|f| {
            let zones = f
                .zone_indices
                .iter()
                .map(|z| z.to_string())
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{{\"index\":{},\"zones\":[{}],\"time\":{},\"cycle\":{},\"strand_id\":{},\"label\":{}}}",
                f.index,
                zones,
                opt_f64(f.time),
                opt_i64(f.cycle),
                opt_i32(f.strand_id),
                json_string(&f.label)
            )
        })
        .collect();
    format!("[{}]", items.join(","))
}

fn opt_f64(v: Option<f64>) -> String {
    match v {
        Some(x) if x.is_finite() => format!("{x}"),
        _ => "null".into(),
    }
}

fn opt_i64(v: Option<i64>) -> String {
    match v {
        Some(x) => x.to_string(),
        None => "null".into(),
    }
}

fn opt_i32(v: Option<i32>) -> String {
    match v {
        Some(x) => x.to_string(),
        None => "null".into(),
    }
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::{OrderedDims, Topology, VariableData, Zone, ZoneMeta};
    use crate::dataset::Location;

    #[test]
    fn groups_by_solution_time() {
        let mut ds = Dataset::new();
        ds.variables = vec!["P".into()];
        for (name, t) in [("a", 0.0), ("b", 0.0), ("c", 1.0)] {
            ds.zones.push(Zone {
                name: name.into(),
                topology: Topology::Ordered(OrderedDims::Ij { i: 2, j: 2 }),
                fields: vec![VariableData {
                    location: Location::Nodal,
                    values: vec![0.0; 4],
                }],
            });
            ds.zone_meta.push(ZoneMeta {
                solution_time: Some(t),
                strand_id: Some(1),
                aux: vec![],
            });
        }
        let frames = list_time_frames(&ds);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].zone_indices, vec![0, 1]);
        assert_eq!(frames[1].zone_indices, vec![2]);
    }
}
