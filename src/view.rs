//! Orthographic view orientation for mesh / contour planes (Tecplot-like).
//!
//! Axis buttons look along ±X / ±Y / ±Z. Free rotate uses azimuth + elevation
//! (degrees) for mouse-drag / rotate-button orbit.

use crate::error::{Error, Result};

/// Which physical axis the camera looks along for an axis-aligned view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookAxis {
    X,
    Y,
    Z,
}

/// World plane a face-on camera looks at (used for planar edge-on clamps).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrincipalPlane {
    /// X right, Y up — look along ±Z.
    Xy,
    /// X right, Z up — look along ±Y (meridian / RZ slices).
    Xz,
    /// Y right, Z up — look along ±X.
    Yz,
}

impl PrincipalPlane {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Xy => "xy",
            Self::Xz => "xz",
            Self::Yz => "yz",
        }
    }

    /// Face-on Axis view for this plane.
    pub fn face_on_view(self) -> ViewSpec {
        match self {
            Self::Xy => ViewSpec::Axis {
                axis: LookAxis::Z,
                positive: true,
            },
            Self::Xz => ViewSpec::Axis {
                axis: LookAxis::Y,
                positive: true,
            },
            Self::Yz => ViewSpec::Axis {
                axis: LookAxis::X,
                positive: true,
            },
        }
    }
}

/// Screen projection of world (X, Y, Z).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewSpec {
    /// Pick the two varying plot axes (legacy / File→Open default).
    Auto,
    /// Tecplot-style axis view. `positive == true` means the **+Axis** button
    /// (camera on the + side looking toward the origin).
    Axis {
        axis: LookAxis,
        positive: bool,
    },
    /// Orbit camera: azimuth about Z, elevation above the XY plane (degrees).
    /// `(0, 90)` matches **+Z** (X right, Y up).
    Free {
        azimuth_deg: f64,
        elevation_deg: f64,
    },
}

impl Default for ViewSpec {
    fn default() -> Self {
        Self::Auto
    }
}

impl ViewSpec {
    /// Parse `auto`, `+x`/`-x`/`+y`/`-y`/`+z`/`-z`, or `az=<deg>,el=<deg>`.
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        if s.is_empty() || s.eq_ignore_ascii_case("auto") {
            return Ok(Self::Auto);
        }
        let lower = s.to_ascii_lowercase();
        match lower.as_str() {
            "+x" | "x+" | "px" => Ok(Self::Axis {
                axis: LookAxis::X,
                positive: true,
            }),
            "-x" | "x-" | "mx" => Ok(Self::Axis {
                axis: LookAxis::X,
                positive: false,
            }),
            "+y" | "y+" | "py" => Ok(Self::Axis {
                axis: LookAxis::Y,
                positive: true,
            }),
            "-y" | "y-" | "my" => Ok(Self::Axis {
                axis: LookAxis::Y,
                positive: false,
            }),
            "+z" | "z+" | "pz" => Ok(Self::Axis {
                axis: LookAxis::Z,
                positive: true,
            }),
            "-z" | "z-" | "mz" => Ok(Self::Axis {
                axis: LookAxis::Z,
                positive: false,
            }),
            _ => parse_free(&lower),
        }
    }

    /// Project world coordinates into screen (u horizontal, v vertical / up).
    pub fn project(self, x: f64, y: f64, z: f64) -> (f64, f64) {
        match self {
            Self::Auto => {
                // Caller should use pick_plot_coord_indices for Auto; this is a
                // safe XY fallback if Auto is projected directly.
                (x, y)
            }
            Self::Axis { axis, positive } => match (axis, positive) {
                // Looking from +Z toward origin: X right, Y up.
                (LookAxis::Z, true) => (x, y),
                // Looking from -Z: -X right, Y up.
                (LookAxis::Z, false) => (-x, y),
                // Looking from +Y: X right, Z up.
                (LookAxis::Y, true) => (x, z),
                (LookAxis::Y, false) => (-x, z),
                // Looking from +X: -Y right, Z up (right-handed).
                (LookAxis::X, true) => (-y, z),
                (LookAxis::X, false) => (y, z),
            },
            Self::Free {
                azimuth_deg,
                elevation_deg,
            } => project_orbit(x, y, z, azimuth_deg, elevation_deg),
        }
    }

    /// Convert an axis-aligned button into matching free angles (for drag handoff).
    ///
    /// Angles are chosen so [`Self::project`] matches the Axis projection
    /// (same screen (u,v)), not the geometric eye-sphere mnemonic alone.
    pub fn to_free_angles(self) -> (f64, f64) {
        match self {
            Self::Auto | Self::Axis {
                axis: LookAxis::Z,
                positive: true,
            } => (0.0, 90.0),
            Self::Axis {
                axis: LookAxis::Z,
                positive: false,
            } => (180.0, -90.0),
            // +Y Axis → (x,z); Free(-90,0) matches that (Free(90,0) mirrors X).
            Self::Axis {
                axis: LookAxis::Y,
                positive: true,
            } => (-90.0, 0.0),
            Self::Axis {
                axis: LookAxis::Y,
                positive: false,
            } => (90.0, 0.0),
            // +X Axis → (-y,z); Free(180,0) matches (Free(0,0) is -X).
            Self::Axis {
                axis: LookAxis::X,
                positive: true,
            } => (180.0, 0.0),
            Self::Axis {
                axis: LookAxis::X,
                positive: false,
            } => (0.0, 0.0),
            Self::Free {
                azimuth_deg,
                elevation_deg,
            } => (azimuth_deg, elevation_deg),
        }
    }

    /// Which world plane this view is (near) face-on to.
    pub fn principal_plane(self) -> PrincipalPlane {
        match self {
            Self::Auto => PrincipalPlane::Xy,
            Self::Axis { axis, .. } => match axis {
                LookAxis::Z => PrincipalPlane::Xy,
                LookAxis::Y => PrincipalPlane::Xz,
                LookAxis::X => PrincipalPlane::Yz,
            },
            Self::Free {
                azimuth_deg,
                elevation_deg,
            } => principal_plane_from_angles(azimuth_deg, elevation_deg),
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Auto => "auto".into(),
            Self::Axis {
                axis: LookAxis::X,
                positive: true,
            } => "+X".into(),
            Self::Axis {
                axis: LookAxis::X,
                positive: false,
            } => "-X".into(),
            Self::Axis {
                axis: LookAxis::Y,
                positive: true,
            } => "+Y".into(),
            Self::Axis {
                axis: LookAxis::Y,
                positive: false,
            } => "-Y".into(),
            Self::Axis {
                axis: LookAxis::Z,
                positive: true,
            } => "+Z".into(),
            Self::Axis {
                axis: LookAxis::Z,
                positive: false,
            } => "-Z".into(),
            Self::Free {
                azimuth_deg,
                elevation_deg,
            } => format!("az={azimuth_deg:.1},el={elevation_deg:.1}"),
        }
    }
}

fn parse_free(s: &str) -> Result<ViewSpec> {
    // az=30,el=45  or  30,45
    let mut az: Option<f64> = None;
    let mut el: Option<f64> = None;
    if s.contains('=') {
        for part in s.split(',') {
            let part = part.trim();
            if let Some((k, v)) = part.split_once('=') {
                let val: f64 = v.trim().parse().map_err(|_| {
                    Error::plot(format!("invalid view angle in '{s}'"))
                })?;
                match k.trim() {
                    "az" | "azimuth" => az = Some(val),
                    "el" | "elev" | "elevation" => el = Some(val),
                    other => {
                        return Err(Error::plot(format!(
                            "unknown view key '{other}' (use az=, el=)"
                        )))
                    }
                }
            }
        }
    } else if let Some((a, e)) = s.split_once(',') {
        az = Some(
            a.trim()
                .parse()
                .map_err(|_| Error::plot(format!("invalid view '{s}'")))?,
        );
        el = Some(
            e.trim()
                .parse()
                .map_err(|_| Error::plot(format!("invalid view '{s}'")))?,
        );
    }
    match (az, el) {
        (Some(azimuth_deg), Some(elevation_deg)) => Ok(ViewSpec::Free {
            azimuth_deg,
            elevation_deg,
        }),
        _ => Err(Error::plot(format!(
            "invalid --view '{s}' (expected auto|+x|-x|+y|-y|+z|-z|az=…,el=…)"
        ))),
    }
}

/// Classify a free camera by which plane it faces (for edge-on clamps).
pub fn principal_plane_from_angles(azimuth_deg: f64, elevation_deg: f64) -> PrincipalPlane {
    let el = elevation_deg.abs();
    if el > 45.0 {
        return PrincipalPlane::Xy;
    }
    // Near the equator: az near ±90 → looking along ±Y → XZ plane.
    let az = ((azimuth_deg % 360.0) + 360.0) % 360.0;
    let dist_y = (az - 90.0).abs().min((az - 270.0).abs());
    let dist_x = az.min((az - 180.0).abs()).min((360.0 - az).abs());
    if dist_y <= dist_x {
        PrincipalPlane::Xz
    } else {
        PrincipalPlane::Yz
    }
}

/// Keep a planar surface readable — never true edge-on.
///
/// XY data: avoid elevation ≈ 0. XZ/YZ (meridian) data: face-on is elevation ≈ 0,
/// so clamp away from the poles instead.
pub fn clamp_planar_elevation(
    elevation_deg: f64,
    plane: PrincipalPlane,
    min_tilt_deg: f64,
) -> f64 {
    let el = elevation_deg.clamp(-90.0, 90.0);
    let min_tilt = min_tilt_deg.clamp(0.0, 45.0);
    match plane {
        PrincipalPlane::Xy => {
            if el.abs() < min_tilt {
                if el >= 0.0 {
                    min_tilt
                } else {
                    -min_tilt
                }
            } else {
                el
            }
        }
        PrincipalPlane::Xz | PrincipalPlane::Yz => {
            let max_el = 90.0 - min_tilt;
            if el > max_el {
                max_el
            } else if el < -max_el {
                -max_el
            } else {
                el
            }
        }
    }
}

/// Orthographic orbit: eye on sphere, look at origin, world +Z as preferred up.
fn project_orbit(x: f64, y: f64, z: f64, azimuth_deg: f64, elevation_deg: f64) -> (f64, f64) {
    let az = azimuth_deg.to_radians();
    let el = elevation_deg.to_radians();
    let cos_el = el.cos();
    // Eye position (unit sphere). el=90° → +Z; el=0°, az=0° → +X.
    let ex = cos_el * az.cos();
    let ey = cos_el * az.sin();
    let ez = el.sin();
    let mut forward = [-ex, -ey, -ez];
    let fl = (forward[0] * forward[0] + forward[1] * forward[1] + forward[2] * forward[2]).sqrt();
    if fl > 1e-30 {
        forward[0] /= fl;
        forward[1] /= fl;
        forward[2] /= fl;
    }
    // Preferred up = +Z; fall back to +Y near the poles.
    let mut world_up = [0.0, 0.0, 1.0];
    if forward[2].abs() > 0.999 {
        world_up = [0.0, 1.0, 0.0];
    }
    // right = normalize(cross(forward, world_up))?  — use cross(world_up, forward) for RH
    // screen right = normalize(cross(world_up, -forward)) = normalize(cross(forward, world_up))
    let mut right = cross(forward, world_up);
    let rl = (right[0] * right[0] + right[1] * right[1] + right[2] * right[2]).sqrt();
    if rl < 1e-30 {
        right = [0.0, 1.0, 0.0];
    } else {
        right[0] /= rl;
        right[1] /= rl;
        right[2] /= rl;
    }
    let up = cross(right, forward);
    let u = x * right[0] + y * right[1] + z * right[2];
    let v = x * up[0] + y * up[1] + z * up[2];
    (u, v)
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_axis_views() {
        assert_eq!(
            ViewSpec::parse("+X").unwrap(),
            ViewSpec::Axis {
                axis: LookAxis::X,
                positive: true
            }
        );
        assert_eq!(
            ViewSpec::parse("-z").unwrap(),
            ViewSpec::Axis {
                axis: LookAxis::Z,
                positive: false
            }
        );
        let v = ViewSpec::parse("az=30,el=45").unwrap();
        assert_eq!(
            v,
            ViewSpec::Free {
                azimuth_deg: 30.0,
                elevation_deg: 45.0
            }
        );
    }

    #[test]
    fn plus_z_is_xy() {
        let (u, v) = ViewSpec::parse("+z").unwrap().project(1.0, 2.0, 9.0);
        assert!((u - 1.0).abs() < 1e-12 && (v - 2.0).abs() < 1e-12);
    }

    #[test]
    fn plus_x_is_neg_y_z() {
        let (u, v) = ViewSpec::parse("+x").unwrap().project(5.0, 2.0, 3.0);
        assert!((u - (-2.0)).abs() < 1e-12 && (v - 3.0).abs() < 1e-12);
    }

    #[test]
    fn free_top_matches_plus_z() {
        let (u, v) = ViewSpec::Free {
            azimuth_deg: 0.0,
            elevation_deg: 90.0,
        }
        .project(1.0, 2.0, 0.0);
        let (u2, v2) = ViewSpec::parse("+z").unwrap().project(1.0, 2.0, 0.0);
        assert!((u2 - 1.0).abs() < 1e-12 && (v2 - 2.0).abs() < 1e-12);
        assert!((u - u2).abs() < 1e-9 && (v - v2).abs() < 1e-9);
    }

    #[test]
    fn free_angles_match_axis_projections() {
        for tag in ["+x", "-x", "+y", "-y", "+z", "-z"] {
            let axis = ViewSpec::parse(tag).unwrap();
            let (az, el) = axis.to_free_angles();
            let free = ViewSpec::Free {
                azimuth_deg: az,
                elevation_deg: el,
            };
            for (x, y, z) in [(1.0, 2.0, 3.0), (-4.0, 0.5, -1.0), (0.0, 0.0, 1.0)] {
                let (u0, v0) = axis.project(x, y, z);
                let (u1, v1) = free.project(x, y, z);
                assert!(
                    (u0 - u1).abs() < 1e-9 && (v0 - v1).abs() < 1e-9,
                    "{tag} at ({x},{y},{z}): axis=({u0},{v0}) free=({u1},{v1})"
                );
            }
        }
    }

    #[test]
    fn planar_clamp_xy_avoids_edge_on() {
        assert!((clamp_planar_elevation(0.0, PrincipalPlane::Xy, 18.0) - 18.0).abs() < 1e-12);
        assert!((clamp_planar_elevation(5.0, PrincipalPlane::Xy, 18.0) - 18.0).abs() < 1e-12);
        assert!((clamp_planar_elevation(40.0, PrincipalPlane::Xy, 18.0) - 40.0).abs() < 1e-12);
    }

    #[test]
    fn planar_clamp_xz_keeps_face_on() {
        // Meridian face-on is el=0 — must not be forced to 18°.
        assert!((clamp_planar_elevation(0.0, PrincipalPlane::Xz, 18.0) - 0.0).abs() < 1e-12);
        assert!((clamp_planar_elevation(90.0, PrincipalPlane::Xz, 18.0) - 72.0).abs() < 1e-12);
    }
}
