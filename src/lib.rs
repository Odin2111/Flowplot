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
//! - [`view`] — Tecplot-like ±XYZ / free-rotate orthographic projection
//! - [`nondim`] — reference-scale nondimensionalization (CLI/config; no invented physics)
//! - [`pathlines`] — particle traces / streamlines of the velocity field
//! - [`validate`] — field sanity / positivity checks (lehar validation ideas)
//! - [`profile`] — line / centerline extracts (lehar extract-line analogue)
//! - [`optical`] — numerical Schlieren / Shadowgraph from density

pub mod annotate;
pub mod contour;
pub mod dataset;
pub mod error;
pub mod field;
pub mod menu;
pub mod mesh;
pub mod nondim;
pub mod optical;
pub mod overlays;
pub mod pathlines;
pub mod profile;
pub mod readers;
pub mod validate;
pub mod view;

pub use annotate::{
    draw_annotations, draw_axes_triad, draw_probe_label, AnnotationInfo, LegendAlign,
};
pub use contour::{
    pixel_to_world, render_contour, render_contour_grid, world_to_pixel, world_to_pixel_plot,
    write_png, ColorMapName, ContourColoring, ContourImage, ContourOptions, PlotStyle,
};
pub use optical::{
    guess_density_var, render_optical, schlieren_from_grid, shadowgraph_from_grid,
    write_optical_png, OpticalKind, OpticalOptions,
};
pub use dataset::{
    Connectivity, Dataset, ElementType, Location, OrderedDims, Topology, VariableData, Zone,
    ZoneMeta,
};
pub use error::{Error, Result};
pub use field::{
    dataset_is_effectively_2d, extract_scalar_grid_view, pick_plot_coord_indices, resolve_view,
    suggest_default_view, PlaneSpec,
};
pub use menu::{format_item_text, format_menu_text, format_stub_run, PostprocessMenu};
pub use mesh::{
    extract_mesh_grid, extract_mesh_grid_view, overlay_mesh, render_mesh, write_mesh_png,
    AspectMap, MeshOptions, DEFAULT_MESH_RGBA,
};
pub use view::{
    clamp_planar_elevation, principal_plane_from_angles, LookAxis, PrincipalPlane, ViewSpec,
};
pub use nondim::{
    apply_nondim, format_suggestions, suggest_scales, NondimScales, NondimSuggestion, ScaleKind,
};
pub use overlays::{
    overlay_boundary_edges, overlay_isolines, overlay_scatter, overlay_vectors,
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
