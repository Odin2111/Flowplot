use std::path::PathBuf;

/// Errors from loading CFD datasets or probing formats.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("unsupported or unknown file format for {path}")]
    UnsupportedFormat { path: PathBuf },

    #[error("parse error in {format}: {message}")]
    Parse { format: &'static str, message: String },

    #[error("{format} reader is not implemented: {detail}")]
    Unimplemented {
        format: &'static str,
        detail: String,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub fn parse(format: &'static str, message: impl Into<String>) -> Self {
        Self::Parse {
            format,
            message: message.into(),
        }
    }

    pub fn unimplemented(format: &'static str, detail: impl Into<String>) -> Self {
        Self::Unimplemented {
            format,
            detail: detail.into(),
        }
    }
}
