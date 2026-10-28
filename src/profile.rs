//! Line / profile extracts from ordered zones (lehar `extract-line` analogue).

use serde::Serialize;

use crate::dataset::{Dataset, OrderedDims, Topology};
use crate::error::{Error, Result};
use crate::field::{extract_scalar_grid, find_var, PlaneSpec};

/// One sample along an extracted profile.
#[derive(Debug, Clone, Serialize)]
pub struct ProfileSample {
    pub s: f64,
    pub x: f64,
    pub y: f64,
    pub value: f64,
}

/// Extracted 1-D profile.
#[derive(Debug, Clone, Serialize)]
pub struct Profile {
    pub variable: String,
    pub zone: usize,
    pub axis: String,
    pub index: usize,
    pub samples: Vec<ProfileSample>,
}

/// Parse `i=3` / `j=0` / `mid-i` / `mid-j` (default mid-j → horizontal cut).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileLine {
    FixedI { i: usize },
    FixedJ { j: usize },
}

impl ProfileLine {
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim().to_ascii_lowercase();
        if s.is_empty() || s == "mid-j" || s == "centerline" || s == "auto" {
            return Ok(Self::FixedJ { j: usize::MAX }); // resolved later
        }
        if s == "mid-i" {
            return Ok(Self::FixedI { i: usize::MAX });
        }
        if let Some(rest) = s.strip_prefix("i=") {
            let i: usize = rest.parse().map_err(|_| {
                Error::plot(format!("invalid profile line '{s}' (expected i=<n>)"))
            })?;
            return Ok(Self::FixedI { i });
        }
        if let Some(rest) = s.strip_prefix("j=") {
            let j: usize = rest.parse().map_err(|_| {
                Error::plot(format!("invalid profile line '{s}' (expected j=<n>)"))
            })?;
            return Ok(Self::FixedJ { j });
        }
        Err(Error::plot(format!(
            "invalid --line '{s}' (use mid-j, mid-i, i=N, j=N, or centerline)"
        )))
    }
}

/// Extract a profile of `var` on the first (or given) ordered zone.
pub fn extract_profile(
    ds: &Dataset,
    var: &str,
    zone_idx: usize,
    line: ProfileLine,
    plane: PlaneSpec,
) -> Result<Profile> {
    let zone = ds
        .zones
        .get(zone_idx)
        .ok_or_else(|| Error::plot(format!("zone {zone_idx} out of range")))?;

    let (ni, nj) = match &zone.topology {
        Topology::Ordered(OrderedDims::Ij { i, j }) => (*i, *j),
        Topology::Ordered(OrderedDims::Ijk { i, j, .. }) => (*i, *j),
        Topology::Ordered(OrderedDims::I { .. }) => {
            return Err(Error::plot(
                "profile extract needs an IJ or IJK ordered zone",
            ));
        }
        Topology::FiniteElement { .. } => {
            return Err(Error::plot(
                "profile extract on FE zones is not implemented in this slice",
            ));
        }
    };

    let resolved = match line {
        ProfileLine::FixedI { i } if i == usize::MAX => ProfileLine::FixedI { i: ni / 2 },
        ProfileLine::FixedJ { j } if j == usize::MAX => ProfileLine::FixedJ { j: nj / 2 },
        other => other,
    };

    let var_idx = find_var(ds, var)?;
    let grid = extract_scalar_grid(ds, zone_idx, var_idx, plane)?;

    let mut samples = Vec::new();
    match resolved {
        ProfileLine::FixedI { i } => {
            if i >= grid.ni {
                return Err(Error::plot(format!(
                    "i={i} out of range (ni={})",
                    grid.ni
                )));
            }
            for j in 0..grid.nj {
                let (x, y) = grid.xy(i, j);
                let value = grid.value(i, j);
                let s = if j == 0 {
                    0.0
                } else {
                    let (px, py) = grid.xy(i, j - 1);
                    samples.last().map(|p: &ProfileSample| p.s).unwrap_or(0.0)
                        + ((x - px).hypot(y - py))
                };
                samples.push(ProfileSample { s, x, y, value });
            }
            Ok(Profile {
                variable: var.to_string(),
                zone: zone_idx,
                axis: "i".into(),
                index: i,
                samples,
            })
        }
        ProfileLine::FixedJ { j } => {
            if j >= grid.nj {
                return Err(Error::plot(format!(
                    "j={j} out of range (nj={})",
                    grid.nj
                )));
            }
            for i in 0..grid.ni {
                let (x, y) = grid.xy(i, j);
                let value = grid.value(i, j);
                let s = if i == 0 {
                    0.0
                } else {
                    let (px, py) = grid.xy(i - 1, j);
                    samples.last().map(|p: &ProfileSample| p.s).unwrap_or(0.0)
                        + ((x - px).hypot(y - py))
                };
                samples.push(ProfileSample { s, x, y, value });
            }
            Ok(Profile {
                variable: var.to_string(),
                zone: zone_idx,
                axis: "j".into(),
                index: j,
                samples,
            })
        }
    }
}

/// Optional RMSE vs a two-column reference CSV (`s,value` or `x,value`).
pub fn compare_profile_rmse(profile: &Profile, ref_s: &[f64], ref_v: &[f64]) -> Option<f64> {
    if ref_s.len() != ref_v.len() || ref_s.is_empty() || profile.samples.is_empty() {
        return None;
    }
    let mut err2 = 0.0;
    let mut n = 0usize;
    for sample in &profile.samples {
        // nearest neighbor in arc-length
        let mut best = 0usize;
        let mut best_d = f64::INFINITY;
        for (i, &rs) in ref_s.iter().enumerate() {
            let d = (rs - sample.s).abs();
            if d < best_d {
                best_d = d;
                best = i;
            }
        }
        let dv = sample.value - ref_v[best];
        err2 += dv * dv;
        n += 1;
    }
    if n == 0 {
        None
    } else {
        Some((err2 / n as f64).sqrt())
    }
}

/// Parse a simple two-column CSV (`#` comments allowed).
pub fn load_ref_csv(text: &str) -> Result<(Vec<f64>, Vec<f64>)> {
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // skip header-ish
        let lower = line.to_ascii_lowercase();
        if lower.contains("s,") || lower.starts_with("x,") || lower.contains("value") {
            if line.chars().any(|c| c.is_ascii_alphabetic()) {
                continue;
            }
        }
        let parts: Vec<_> = line.split(&[',', ' ', '\t'][..]).filter(|p| !p.is_empty()).collect();
        if parts.len() < 2 {
            return Err(Error::plot(format!(
                "ref CSV line {}: expected two columns",
                lineno + 1
            )));
        }
        let x: f64 = parts[0].parse().map_err(|_| {
            Error::plot(format!("ref CSV line {}: bad x/s", lineno + 1))
        })?;
        let y: f64 = parts[1].parse().map_err(|_| {
            Error::plot(format!("ref CSV line {}: bad value", lineno + 1))
        })?;
        xs.push(x);
        ys.push(y);
    }
    if xs.is_empty() {
        return Err(Error::plot("ref CSV has no data rows"));
    }
    Ok((xs, ys))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::readers::load_path;
    use std::path::PathBuf;

    #[test]
    fn extract_mid_j_from_flow_fixture() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flow.dat");
        let ds = load_path(&path).expect("load");
        let p = extract_profile(&ds, "P", 0, ProfileLine::parse("mid-j").unwrap(), PlaneSpec::Auto)
            .expect("profile");
        assert_eq!(p.axis, "j");
        assert_eq!(p.samples.len(), 5);
        assert!(p.samples[0].value.is_finite());
    }
}
