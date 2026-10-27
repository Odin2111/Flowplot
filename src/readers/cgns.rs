//! CGNS reader stub.
//!
//! Public API: the [CGNS](https://cgns.github.io/) standard is typically accessed
//! through `libcgns` (C). A future `cgns` Cargo feature can bind via `cgns-sys`
//! or `bindgen` when the system library and headers are installed.
//!
//! This stub documents the intended FFI surface and returns a clear error so
//! the crate compiles without a system CGNS install.

use std::path::Path;

use crate::dataset::Dataset;
use crate::error::{Error, Result};
use crate::readers::Reader;

/// Placeholder CGNS loader.
///
/// Intended FFI entry points (when `feature = "cgns"` is wired):
/// - `cg_open` / `cg_close`
/// - `cg_nbases` / `cg_nzones` / `cg_nvars`
/// - `cg_zone_read` / `cg_coord_read` / `cg_field_read`
/// - structured (`Structured`) and unstructured (`Unstructured`) zone types
///
/// Mapping into [`Dataset`]: one CGNS zone → one [`crate::Zone`]; CGNS flow
/// solution fields → named variables (nodal or cell-centered per `GridLocation_t`).
pub struct CgnsReader;

impl Reader for CgnsReader {
    fn format_name(&self) -> &'static str {
        "CGNS"
    }

    fn read_path(&self, path: &Path) -> Result<Dataset> {
        let _ = path;
        #[cfg(feature = "cgns")]
        {
            // Feature flag reserved for a future `cgns-sys` / libcgns binding.
            // Keep returning unimplemented until the FFI crate is linked in CI.
            return Err(Error::unimplemented(
                "CGNS",
                "feature \"cgns\" is enabled but libcgns FFI is not linked in this build; \
                 install libcgns and wire cgns-sys (cg_open, cg_zone_read, cg_field_read)",
            ));
        }
        #[cfg(not(feature = "cgns"))]
        {
            Err(Error::unimplemented(
                "CGNS",
                "enable Cargo feature \"cgns\" and install system libcgns to bind via \
                 cgns-sys / FFI (cg_open, cg_nbases, cg_nzones, cg_coord_read, cg_field_read). \
                 No proprietary format reverse-engineering — public CGNS SIDS/ Mid-Level API only",
            ))
        }
    }
}
