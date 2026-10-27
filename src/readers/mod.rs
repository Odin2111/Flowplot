//! Format readers. Each module implements loading into [`crate::Dataset`].
//!
//! Detection is by file extension (case-insensitive). Binary proprietary
//! layouts are not reverse-engineered; stubs return clear errors.

mod cgns;
mod fluent;
mod tecplot;
mod vtk;

pub use cgns::CgnsReader;
pub use fluent::FluentReader;
pub use tecplot::TecplotAsciiReader;
pub use vtk::{VtkLegacyAsciiReader, VtkXmlReader};

use std::path::Path;

use crate::dataset::Dataset;
use crate::error::{Error, Result};

/// Trait for CFD format loaders.
pub trait Reader {
    fn format_name(&self) -> &'static str;
    fn read_path(&self, path: &Path) -> Result<Dataset>;
}

/// Detect format from extension and load into a [`Dataset`].
pub fn load_path(path: &Path) -> Result<Dataset> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "dat" | "tp" => {
            // `.dat` is shared by Tecplot ASCII and Fluent data. Prefer Tecplot
            // when the file starts with TITLE/VARIABLES/ZONE; otherwise try Fluent stub.
            if looks_like_tecplot_ascii(path)? {
                TecplotAsciiReader.read_path(path)
            } else if looks_like_fluent_cas_dat(path)? {
                FluentReader.read_path(path)
            } else {
                // Default: Tecplot ASCII attempt (clear parse error if wrong).
                TecplotAsciiReader.read_path(path)
            }
        }
        "cas" => FluentReader.read_path(path),
        "vtk" => VtkLegacyAsciiReader.read_path(path),
        "vtu" | "vtp" | "vti" | "vts" | "vtr" | "pvtu" | "pvtp" => VtkXmlReader.read_path(path),
        "cgns" => CgnsReader.read_path(path),
        _ => Err(Error::UnsupportedFormat {
            path: path.to_path_buf(),
        }),
    }
}

fn looks_like_tecplot_ascii(path: &Path) -> Result<bool> {
    let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
    let head: String = text.chars().take(4096).collect();
    let upper = head.to_ascii_uppercase();
    Ok(upper.contains("VARIABLES") || upper.contains("ZONE") || upper.contains("TITLE"))
}

fn looks_like_fluent_cas_dat(path: &Path) -> Result<bool> {
    let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
    // Fluent case/data often start with "(0 " comments or binary markers.
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(256)]);
    Ok(head.starts_with("(0 ") || head.starts_with("(1 ") || head.contains("Fluent"))
}
