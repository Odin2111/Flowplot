//! Field sanity / positivity checks on a loaded [`Dataset`].
//!
//! Port of the useful lehar validation ideas that operate on a single field
//! file (NaN/Inf, extrema, density/pressure/temperature positivity) without
//! requiring project telemetry CSVs.

use serde::Serialize;

use crate::dataset::Dataset;

/// Per-variable statistics and flags.
#[derive(Debug, Clone, Serialize)]
pub struct FieldReport {
    pub name: String,
    pub zone: usize,
    pub count: usize,
    pub finite: usize,
    pub nan: usize,
    pub inf: usize,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub mean: Option<f64>,
    /// Named thermodynamic / density fields that must be ≥ 0 (or > 0).
    pub positivity_required: bool,
    pub negative_count: usize,
    pub zero_count: usize,
    pub ok: bool,
    pub issues: Vec<String>,
}

/// Aggregate validation result.
#[derive(Debug, Clone, Serialize)]
pub struct ValidationReport {
    pub path_hint: Option<String>,
    pub variables: usize,
    pub zones: usize,
    pub fields: Vec<FieldReport>,
    pub ok: bool,
    pub issue_count: usize,
    pub summary: String,
}

fn looks_positive(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    matches!(
        n.as_str(),
        "rho"
            | "density"
            | "dens"
            | "r"
            | "p"
            | "pressure"
            | "press"
            | "t"
            | "temp"
            | "temperature"
            | "tt"
            | "static_pressure"
            | "static_temperature"
    ) || n.starts_with("rho")
        || n.starts_with("dens")
        || n.contains("pressure")
        || (n.contains("temp") && !n.contains("template"))
}

/// Scan every zone/field for NaN/Inf and (optionally) positivity.
pub fn validate_dataset(ds: &Dataset, positivity: bool) -> ValidationReport {
    let mut fields = Vec::new();
    let mut issue_count = 0usize;

    for (zi, zone) in ds.zones.iter().enumerate() {
        for (vi, name) in ds.variables.iter().enumerate() {
            let Some(vd) = zone.fields.get(vi) else {
                continue;
            };
            if vd.values.is_empty() {
                continue;
            }
            let mut nan = 0usize;
            let mut inf = 0usize;
            let mut finite = 0usize;
            let mut neg = 0usize;
            let mut zero = 0usize;
            let mut sum = 0.0f64;
            let mut min_v = f64::INFINITY;
            let mut max_v = f64::NEG_INFINITY;

            for &v in &vd.values {
                if v.is_nan() {
                    nan += 1;
                    continue;
                }
                if v.is_infinite() {
                    inf += 1;
                    continue;
                }
                finite += 1;
                sum += v;
                min_v = min_v.min(v);
                max_v = max_v.max(v);
                if v < 0.0 {
                    neg += 1;
                }
                if v == 0.0 {
                    zero += 1;
                }
            }

            let pos_req = positivity && looks_positive(name);
            let mut issues = Vec::new();
            if nan > 0 {
                issues.push(format!("{nan} NaN sample(s)"));
            }
            if inf > 0 {
                issues.push(format!("{inf} Inf sample(s)"));
            }
            if pos_req && neg > 0 {
                issues.push(format!(
                    "{neg} negative sample(s) on positivity field '{name}'"
                ));
            }

            let ok = issues.is_empty();
            if !ok {
                issue_count += issues.len();
            }

            fields.push(FieldReport {
                name: name.clone(),
                zone: zi,
                count: vd.values.len(),
                finite,
                nan,
                inf,
                min: if finite > 0 { Some(min_v) } else { None },
                max: if finite > 0 { Some(max_v) } else { None },
                mean: if finite > 0 {
                    Some(sum / finite as f64)
                } else {
                    None
                },
                positivity_required: pos_req,
                negative_count: neg,
                zero_count: zero,
                ok,
                issues,
            });
        }
    }

    let ok = issue_count == 0;
    let summary = if ok {
        format!(
            "ok: {} field(s) across {} zone(s); no NaN/Inf{}",
            fields.len(),
            ds.zones.len(),
            if positivity {
                "; positivity clean"
            } else {
                ""
            }
        )
    } else {
        format!(
            "issues: {issue_count} across {} field report(s)",
            fields.iter().filter(|f| !f.ok).count()
        )
    };

    ValidationReport {
        path_hint: None,
        variables: ds.variables.len(),
        zones: ds.zones.len(),
        fields,
        ok,
        issue_count,
        summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::{Location, OrderedDims, Topology, VariableData, Zone};

    fn tiny_ds(p_vals: Vec<f64>) -> Dataset {
        Dataset {
            title: Some("t".into()),
            variables: vec!["X".into(), "P".into()],
            zones: vec![Zone {
                name: "z".into(),
                topology: Topology::Ordered(OrderedDims::I { i: p_vals.len() }),
                fields: vec![
                    VariableData {
                        location: Location::Nodal,
                        values: (0..p_vals.len()).map(|i| i as f64).collect(),
                    },
                    VariableData {
                        location: Location::Nodal,
                        values: p_vals,
                    },
                ],
            }],
        }
    }

    #[test]
    fn detects_nan_and_negative_pressure() {
        let ds = tiny_ds(vec![1.0, f64::NAN, -2.0]);
        let r = validate_dataset(&ds, true);
        assert!(!r.ok);
        let p = r.fields.iter().find(|f| f.name == "P").unwrap();
        assert_eq!(p.nan, 1);
        assert_eq!(p.negative_count, 1);
        assert!(!p.ok);
    }

    #[test]
    fn clean_field_ok() {
        let ds = tiny_ds(vec![1.0, 2.0, 3.0]);
        let r = validate_dataset(&ds, true);
        assert!(r.ok);
    }
}
