//! Ansys Fluent case/data (`.cas` / `.dat`) stub.
//!
//! Documents the **public** on-disk layout outline only. Binary sections are
//! not decoded here (no reverse engineering of proprietary Fluent binaries).
//!
//! # Public layout sketch
//!
//! Fluent case (`.cas`) and data (`.dat`) files are sectioned records. Text
//! variants use Scheme-like lists; binary variants embed typed blocks.
//!
//! Common textual markers (illustrative, from public docs / community notes):
//! - `(0 "comment")` — comment
//! - `(1 ...)` — header / version info
//! - `(2 ...)` — dimensions
//! - `(10 ...)` — nodes
//! - `(12 ...)` — cells
//! - `(13 ...)` — faces
//! - `(45 ...)` — zone names
//! - Data file sections carry field values keyed by zone / variable index
//!
//! A future reader should:
//! 1. Detect text vs binary (and compression) from the header.
//! 2. Parse text sections into nodes/faces/cells → [`crate::Topology::FiniteElement`].
//! 3. Map data-file scalars onto [`crate::VariableData`] (cell-centered by default).
//! 4. Refuse opaque binary blobs with [`Error::Unimplemented`] until a
//!    documented, licensed path exists — **do not** reverse-engineer binaries.

use std::path::Path;

use crate::dataset::Dataset;
use crate::error::{Error, Result};
use crate::readers::Reader;

pub struct FluentReader;

impl Reader for FluentReader {
    fn format_name(&self) -> &'static str {
        "Fluent case/data"
    }

    fn read_path(&self, path: &Path) -> Result<Dataset> {
        let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
        probe_and_reject(&bytes, path)
    }
}

fn probe_and_reject(bytes: &[u8], path: &Path) -> Result<Dataset> {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]);
    let looks_binary = bytes.iter().take(256).any(|&b| b == 0)
        || head.contains("binary")
        || (!head.trim_start().starts_with('(') && !head.contains("Fluent"));

    let detail = if looks_binary {
        format!(
            "Fluent binary case/data is not implemented (file {}). \
             Public layout: sectioned records such as (0 comment), (10 nodes), \
             (12 cells), (13 faces), (45 zone names); data file holds field blocks. \
             Text ASCII .cas parsing may be added later; binary decoding is out of scope \
             without a public spec — no reverse engineering",
            path.display()
        )
    } else {
        format!(
            "Fluent text case/data parsing is stubbed (file {}). \
             Recognized public section outline: (0)/(1) header, (2) dimensions, \
             (10) nodes, (12) cells, (13) faces, (45) zone names, plus data-file field \
             sections. Returning Err until a minimal ASCII section parser lands",
            path.display()
        )
    };

    Err(Error::unimplemented("Fluent case/data", detail))
}
