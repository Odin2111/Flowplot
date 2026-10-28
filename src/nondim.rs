//! Reference-scale nondimensionalization (CLI / config; GUI later).
//!
//! Users supply scales. Flowplot only suggests defaults from variable *names*
//! when the match is obvious — it never invents physics or fills missing numbers.

use crate::dataset::{Dataset, VariableData};

/// User-chosen reference scales. All fields optional; unset means “leave alone”.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NondimScales {
    /// Velocity scale for U, V, W and magnitude.
    pub u_ref: Option<f64>,
    /// Length scale for the X-like axis quantity.
    pub lx: Option<f64>,
    /// Length scale for the Y-like axis quantity.
    pub ly: Option<f64>,
    /// Length scale for the Z-like axis quantity.
    pub lz: Option<f64>,
    /// Temperature reference for T-like variables.
    pub t_ref: Option<f64>,
    /// Name of the X-like coordinate variable (default guess: X or R).
    pub x_var: Option<String>,
    /// Name of the Y-like coordinate variable (default guess: Y or THETA/θ).
    pub y_var: Option<String>,
    /// Name of the Z-like coordinate variable (default guess: Z).
    pub z_var: Option<String>,
}

/// One printable hint derived from a variable name (no numeric invention).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NondimSuggestion {
    pub variable: String,
    pub kind: ScaleKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleKind {
    Velocity,
    LengthX,
    LengthY,
    LengthZ,
    Temperature,
    ThermalOther,
}

/// Scan variable names and print-oriented suggestions. Never invents scale values.
pub fn suggest_scales(variables: &[String]) -> Vec<NondimSuggestion> {
    let mut out = Vec::new();
    for raw in variables {
        let name = raw.as_str();
        let key = normalize_name(name);
        if matches!(
            key.as_str(),
            "u" | "v" | "w" | "vx" | "vy" | "vz" | "velocity" | "vel" | "mag" | "speed" | "|v|"
                | "velocitymagnitude" | "velocity_magnitude"
        ) || key.starts_with("u(")
            || key.starts_with("v(")
            || key.starts_with("w(")
        {
            out.push(NondimSuggestion {
                variable: name.to_string(),
                kind: ScaleKind::Velocity,
                message: format!(
                    "{name}: looks like a velocity component/magnitude — set --u-ref to normalize"
                ),
            });
        } else if matches!(key.as_str(), "x" | "r" | "radial" | "radius") {
            out.push(NondimSuggestion {
                variable: name.to_string(),
                kind: ScaleKind::LengthX,
                message: format!(
                    "{name}: looks like an X/R length coordinate — set --lx (and --x-var if not X)"
                ),
            });
        } else if matches!(key.as_str(), "y" | "theta" | "th" | "azimuth") {
            out.push(NondimSuggestion {
                variable: name.to_string(),
                kind: ScaleKind::LengthY,
                message: format!(
                    "{name}: looks like a Y/theta coordinate — set --ly (and --y-var if not Y)"
                ),
            });
        } else if matches!(key.as_str(), "z") {
            out.push(NondimSuggestion {
                variable: name.to_string(),
                kind: ScaleKind::LengthZ,
                message: format!("{name}: looks like a Z coordinate — set --lz"),
            });
        } else if matches!(key.as_str(), "t" | "temp" | "temperature")
            || key.starts_with("t(")
            || key.starts_with("temp")
        {
            out.push(NondimSuggestion {
                variable: name.to_string(),
                kind: ScaleKind::Temperature,
                message: format!("{name}: looks like temperature — set --t-ref to normalize T/Tref"),
            });
        } else if matches!(key.as_str(), "rt" | "tke" | "theta_t" | "tt")
            || key.contains("therm")
            || key == "enthalpy"
            || key == "h"
        {
            out.push(NondimSuggestion {
                variable: name.to_string(),
                kind: ScaleKind::ThermalOther,
                message: format!(
                    "{name}: thermal-related name seen — document/pass your own scale; Flowplot will not invent RT/thermal nondim"
                ),
            });
        }
    }
    out
}

/// Return a copy of `ds` with selected fields divided by reference scales.
///
/// Only variables matched by name (or explicit axis overrides) are scaled.
/// Missing scales are skipped (field left unchanged). Zero scales error.
pub fn apply_nondim(ds: &Dataset, scales: &NondimScales) -> crate::Result<Dataset> {
    validate_scales(scales)?;
    let mut out = ds.clone();
    let x_name = scales
        .x_var
        .clone()
        .or_else(|| first_match(&out.variables, &["X", "R"]));
    let y_name = scales
        .y_var
        .clone()
        .or_else(|| first_match(&out.variables, &["Y", "THETA", "TH"]));
    let z_name = scales
        .z_var
        .clone()
        .or_else(|| first_match(&out.variables, &["Z"]));

    for (vi, vname) in out.variables.iter().enumerate() {
        let key = normalize_name(vname);
        let factor = scale_for_variable(
            &key,
            vname,
            scales,
            x_name.as_deref(),
            y_name.as_deref(),
            z_name.as_deref(),
        );
        if let Some(f) = factor {
            for zone in &mut out.zones {
                if let Some(field) = zone.fields.get_mut(vi) {
                    scale_field(field, f);
                }
            }
        }
    }

    // Magnitude from U,V,(W) if all present and u_ref set: rewrite or add Mag* if Mag exists.
    if let Some(u_ref) = scales.u_ref {
        maybe_scale_named_magnitude(&mut out, u_ref);
    }

    Ok(out)
}

fn validate_scales(scales: &NondimScales) -> crate::Result<()> {
    for (name, v) in [
        ("u-ref", scales.u_ref),
        ("lx", scales.lx),
        ("ly", scales.ly),
        ("lz", scales.lz),
        ("t-ref", scales.t_ref),
    ] {
        if let Some(x) = v {
            if !x.is_finite() || x == 0.0 {
                return Err(crate::Error::plot(format!(
                    "nondim scale --{name} must be finite and non-zero (got {x})"
                )));
            }
        }
    }
    Ok(())
}

fn scale_for_variable(
    key: &str,
    raw: &str,
    scales: &NondimScales,
    x_name: Option<&str>,
    y_name: Option<&str>,
    z_name: Option<&str>,
) -> Option<f64> {
    if let Some(u_ref) = scales.u_ref {
        if matches!(
            key,
            "u" | "v" | "w" | "vx" | "vy" | "vz" | "mag" | "speed" | "|v|"
                | "velocity" | "vel" | "velocitymagnitude" | "velocity_magnitude"
        ) || key.starts_with("u(")
            || key.starts_with("v(")
            || key.starts_with("w(")
        {
            return Some(u_ref);
        }
    }
    if let Some(lx) = scales.lx {
        if x_name.is_some_and(|n| n.eq_ignore_ascii_case(raw)) || matches!(key, "x" | "r") {
            return Some(lx);
        }
    }
    if let Some(ly) = scales.ly {
        if y_name.is_some_and(|n| n.eq_ignore_ascii_case(raw))
            || matches!(key, "y" | "theta" | "th")
        {
            return Some(ly);
        }
    }
    if let Some(lz) = scales.lz {
        if z_name.is_some_and(|n| n.eq_ignore_ascii_case(raw)) || key == "z" {
            return Some(lz);
        }
    }
    if let Some(t_ref) = scales.t_ref {
        if matches!(key, "t" | "temp" | "temperature")
            || key.starts_with("t(")
            || key.starts_with("temp")
        {
            return Some(t_ref);
        }
    }
    None
}

fn maybe_scale_named_magnitude(ds: &mut Dataset, u_ref: f64) {
    for (vi, vname) in ds.variables.iter().enumerate() {
        let key = normalize_name(vname);
        if matches!(
            key.as_str(),
            "mag" | "speed" | "|v|" | "velocitymagnitude" | "velocity_magnitude"
        ) {
            for zone in &mut ds.zones {
                if let Some(field) = zone.fields.get_mut(vi) {
                    scale_field(field, u_ref);
                }
            }
        }
    }
}

fn scale_field(field: &mut VariableData, factor: f64) {
    for v in &mut field.values {
        *v /= factor;
    }
}

fn first_match(variables: &[String], candidates: &[&str]) -> Option<String> {
    for c in candidates {
        if let Some(v) = variables.iter().find(|n| n.eq_ignore_ascii_case(c)) {
            return Some(v.clone());
        }
    }
    None
}

pub fn normalize_name(name: &str) -> String {
    name.trim()
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| c.to_ascii_lowercase())
        .collect::<String>()
        .replace(['/', '\\'], "")
}

/// Format suggestion lines for CLI stdout.
pub fn format_suggestions(suggestions: &[NondimSuggestion]) -> String {
    if suggestions.is_empty() {
        return "nondim: no obvious scale suggestions from variable names (pass flags explicitly)"
            .to_string();
    }
    let mut lines = vec!["nondim suggestions (from names only; no values invented):".to_string()];
    for s in suggestions {
        lines.push(format!("  - {}", s.message));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::{Location, OrderedDims, Topology, VariableData, Zone};

    fn toy_ds() -> Dataset {
        Dataset {
            title: Some("toy".into()),
            variables: vec!["X".into(), "Y".into(), "U".into(), "V".into(), "T".into()],
            zones: vec![Zone {
                name: "z0".into(),
                topology: Topology::Ordered(OrderedDims::Ij { i: 2, j: 2 }),
                fields: vec![
                    VariableData {
                        location: Location::Nodal,
                        values: vec![0.0, 2.0, 0.0, 2.0],
                    },
                    VariableData {
                        location: Location::Nodal,
                        values: vec![0.0, 0.0, 4.0, 4.0],
                    },
                    VariableData {
                        location: Location::Nodal,
                        values: vec![10.0, 10.0, 10.0, 10.0],
                    },
                    VariableData {
                        location: Location::Nodal,
                        values: vec![0.0, 0.0, 0.0, 0.0],
                    },
                    VariableData {
                        location: Location::Nodal,
                        values: vec![300.0, 300.0, 300.0, 300.0],
                    },
                ],
            }],
        }
    }

    #[test]
    fn suggestions_from_names() {
        let s = suggest_scales(&["X".into(), "U".into(), "T".into(), "RT".into()]);
        assert!(s.iter().any(|x| x.kind == ScaleKind::LengthX));
        assert!(s.iter().any(|x| x.kind == ScaleKind::Velocity));
        assert!(s.iter().any(|x| x.kind == ScaleKind::Temperature));
        assert!(s.iter().any(|x| x.kind == ScaleKind::ThermalOther));
    }

    #[test]
    fn apply_scales_divides_fields() {
        let ds = toy_ds();
        let scales = NondimScales {
            u_ref: Some(10.0),
            lx: Some(2.0),
            ly: Some(4.0),
            lz: None,
            t_ref: Some(300.0),
            x_var: None,
            y_var: None,
            z_var: None,
        };
        let out = apply_nondim(&ds, &scales).unwrap();
        assert_eq!(out.zones[0].fields[0].values, vec![0.0, 1.0, 0.0, 1.0]); // X/Lx
        assert_eq!(out.zones[0].fields[1].values, vec![0.0, 0.0, 1.0, 1.0]); // Y/Ly
        assert_eq!(out.zones[0].fields[2].values, vec![1.0, 1.0, 1.0, 1.0]); // U/Uref
        assert_eq!(out.zones[0].fields[4].values, vec![1.0, 1.0, 1.0, 1.0]); // T/Tref
    }

    #[test]
    fn rejects_zero_scale() {
        let ds = toy_ds();
        let scales = NondimScales {
            u_ref: Some(0.0),
            ..Default::default()
        };
        assert!(apply_nondim(&ds, &scales).is_err());
    }
}
