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
//! | Tecplot ASCII (`.dat`, `.tp`, ASCII `.plt`) | Minimal working parser |
//! | Tecplot binary (`.plt`, `#!TDV111`/`#!TDV112`) | Public Data Format Guide reader |
//! | VTK legacy ASCII (`.vtk`) | Minimal working parser |
//! | VTK XML (`.vtu`, …) | Stub (`Unimplemented`) |
//! | CGNS (`.cgns` / `.cgz`) | System `libcgns` when present; else clear stub |
//! | Fluent (`.cas` / `.dat`) | Layout-documented stub |
//!
//! SZL (`.szplt`) and Fluent binaries are **not** reverse-engineered.
//!
//! # Plotting (this slice)
//!
//! - [`mesh`] — wireframe zone grid / "crate" PNG (default first view; equal aspect)
//! - [`contour`] — scalar color-field / contour PNG for any variable
//! - [`nondim`] — reference-scale nondimensionalization (CLI/config; no invented physics)
//! - [`pathlines`] — particle traces / streamlines of the velocity field
//! - [`validate`] — field sanity / positivity checks (lehar validation ideas)
//! - [`profile`] — line / centerline extracts (lehar extract-line analogue)

pub mod contour;
pub mod dataset;
pub mod error;
pub mod field;
pub mod menu;
pub mod mesh;
pub mod nondim;
pub mod pathlines;
pub mod profile;
pub mod readers;
pub mod validate;

pub use contour::{render_contour, write_png, ContourImage, ContourOptions, PlotStyle};
pub use dataset::{
    Connectivity, Dataset, ElementType, Location, OrderedDims, Topology, VariableData, Zone,
};
pub use error::{Error, Result};
pub use field::{dataset_is_effectively_2d, pick_plot_coord_indices, PlaneSpec};
pub use menu::{format_item_text, format_menu_text, format_stub_run, PostprocessMenu};
pub use mesh::{
    extract_mesh_grid, overlay_mesh, render_mesh, write_mesh_png, AspectMap, MeshOptions,
    DEFAULT_MESH_RGBA,
};
pub use nondim::{
    apply_nondim, format_suggestions, suggest_scales, NondimScales, NondimSuggestion, ScaleKind,
};
pub use pathlines::{
    default_seeds, guess_velocity_vars, overlay_pathlines, overlay_pathlines_styled,
    render_pathlines_png, render_pathlines_png_styled, trace_pathlines, Pathline, PathlineOptions,
};
pub use profile::{
    compare_profile_rmse, extract_profile, load_ref_csv, Profile, ProfileLine, ProfileSample,
};
pub use readers::{load_path, Reader};
pub use validate::{validate_dataset, FieldReport, ValidationReport};
