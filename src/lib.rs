//! `fieldplot` — CFD post-processing core with a Tecplot-like data model.
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

pub mod dataset;
pub mod error;
pub mod readers;

pub use dataset::{
    Connectivity, Dataset, ElementType, Location, OrderedDims, Topology, VariableData, Zone,
};
pub use error::{Error, Result};
pub use readers::{load_path, Reader};
