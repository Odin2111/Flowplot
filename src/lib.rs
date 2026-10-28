//! `flowplot` — CFD post-processing core with a Tecplot-like data model.
//!
//! # Data model
//!
//! [`Dataset`] holds shared variable names and one or more [`Zone`]s. Each zone
//! is either **ordered** (I / IJ / IJK) or **finite-element** (explicit
//! connectivity). Values are [`Location::Nodal`] or [`Location::CellCentered`].
//!
//! # Readers
//!
//! | Format | Status |
//! |--------|--------|
//! | Tecplot ASCII (`.dat`, `.tp`) | Minimal working parser |
//! | VTK legacy ASCII (`.vtk`) | Minimal working parser |
//! | VTK XML (`.vtu`, …) | Stub (`Unimplemented`) |
//! | CGNS (`.cgns`) | FFI stub (`feature = "cgns"` reserved) |
//! | Fluent (`.cas` / `.dat`) | Layout-documented stub |
//!
//! Binary Tecplot `.plt` and Fluent binaries are **not** reverse-engineered.
//!
//! # Plotting (this slice)
//!
//! - [`contour`] — scalar color-field / contour PNG for any variable
//! - [`nondim`] — reference-scale nondimensionalization (CLI/config; no invented physics)
//! - [`pathlines`] — particle traces / streamlines of the velocity field

pub mod contour;
pub mod dataset;
pub mod error;
pub mod field;
pub mod nondim;
pub mod pathlines;
pub mod readers;

pub use contour::{render_contour, write_png, ContourImage, ContourOptions};
pub use dataset::{
    Connectivity, Dataset, ElementType, Location, OrderedDims, Topology, VariableData, Zone,
};
pub use error::{Error, Result};
pub use field::PlaneSpec;
pub use nondim::{
    apply_nondim, format_suggestions, suggest_scales, NondimScales, NondimSuggestion, ScaleKind,
};
pub use pathlines::{
    default_seeds, guess_velocity_vars, overlay_pathlines, render_pathlines_png, trace_pathlines,
    Pathline, PathlineOptions,
};
pub use readers::{load_path, Reader};
