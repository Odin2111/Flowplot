//! HDF5 / Silo-compatible dataset reader stub.
//!
//! Internal notes may say “Silo-compatible”; UI copy must use Fieldplot
//! **Open dataset** format labels only (never VisIt / Tecplot / Silo product brands).
//!
//! This stub recognizes common extensions and returns a clear
//! [`Error::Unimplemented`] until a licensed HDF5 path lands. No reverse
//! engineering of proprietary database layouts.

use std::path::Path;

use crate::dataset::Dataset;
use crate::error::{Error, Result};
use crate::readers::Reader;

pub struct SiloHdf5Reader;

impl Reader for SiloHdf5Reader {
    fn format_name(&self) -> &'static str {
        // Internal identifier only — not shown as a product brand in the GUI.
        "HDF5/Silo-compatible"
    }

    fn read_path(&self, path: &Path) -> Result<Dataset> {
        let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
        probe_and_reject(&bytes, path)
    }
}

fn probe_and_reject(bytes: &[u8], path: &Path) -> Result<Dataset> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let magic_hdf5 = bytes.len() >= 8 && &bytes[..8] == b"\x89HDF\r\n\x1a\n";
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(64)]);
    let detail = format!(
        "HDF5 / mesh-database reader is not implemented yet (file {}, ext=.{ext}, \
         hdf5_magic={magic_hdf5}, head_ascii={:?}). \
         Planned: optional system libhdf5 path for mesh + nodal/cell fields; \
         Silo-compatible layouts stay an internal implementation detail. \
         UI continues to list this under Open dataset format filters only.",
        path.display(),
        head.chars().take(32).collect::<String>()
    );
    Err(Error::unimplemented("HDF5/Silo-compatible", detail))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    #[test]
    fn stub_rejects_fake_hdf5() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp-silo-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("probe.h5");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"\x89HDF\r\n\x1a\nnot-a-real-file").unwrap();
        let err = SiloHdf5Reader.read_path(&path).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("not implemented") || msg.contains("Unimplemented") || msg.contains("HDF5"));
    }
}
