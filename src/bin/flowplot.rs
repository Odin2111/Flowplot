//! CLI: load CFD files; summarize; contour PNG; nondim scales; pathlines; postprocess menu.
//! Agent contract: every subcommand accepts `--json` and emits a stable JSON object.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use flowplot::annotate::{draw_annotations, draw_probe_label, AnnotationInfo, LegendAlign};
use flowplot::contour::{
    pixel_to_world, render_contour, write_png, ColorMapName, ContourColoring, ContourOptions,
    PlotStyle,
};
use flowplot::optical::{write_optical_png, OpticalKind, OpticalOptions};
use flowplot::field::{extract_scalar_grid_view, find_var, PlaneSpec};
use flowplot::menu::{
    format_item_text, format_menu_text, format_stub_run, MenuItem, PostprocessMenu,
};
use flowplot::mesh::{extract_mesh_grid_view, overlay_mesh, render_mesh, MeshOptions};
use flowplot::view::ViewSpec;
use flowplot::nondim::{
    apply_nondim, format_suggestions, suggest_scales, NondimScales, NondimSuggestion,
};
use flowplot::pathlines::{
    default_seeds, guess_velocity_vars, overlay_pathlines_styled, render_pathlines_png_styled,
    trace_pathlines, PathlineOptions,
};
use flowplot::derived::{apply_derived, field_minmax, write_plt_ascii};
use flowplot::operators::{parse_operator, OperatorStack};
use flowplot::profile::{
    compare_profile_rmse, extract_line_sample, extract_profile, load_ref_csv, profile_to_csv,
    ProfileLine,
};
use flowplot::validate::validate_dataset;
use flowplot::{load_path, Dataset};

#[derive(Parser, Debug)]
#[command(
    name = "flowplot",
    about = "CFD post-processor: load PLT/VTK, mesh, contour, optical, nondim, pathlines, validate, profile, derived, line-sample, operators, menu",
    arg_required_else_help = true
)]
struct Cli {
    /// Emit a machine-readable JSON object (for lehar / MCP agents).
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to a CFD file (summary / load mode when no subcommand).
    path: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Print zone / variable / size summary (alias of `load`).
    Info {
        path: PathBuf,
    },
    /// Load a CFD file and summarize zones / variables (preferred agent entry).
    Load {
        path: PathBuf,
    },
    /// Wireframe mesh / zone grid PNG (default first view; equal aspect, no contour).
    Mesh {
        path: PathBuf,
        /// Output PNG path.
        #[arg(long, short = 'o')]
        out: PathBuf,
        /// Zone index (default 0).
        #[arg(long, default_value_t = 0)]
        zone: usize,
        /// Plane/slice, e.g. k=0, i=1, j=2 (default: auto).
        #[arg(long)]
        plane: Option<String>,
        /// View: auto|+x|-x|+y|-y|+z|-z|az=deg,el=deg (Fieldplot).
        #[arg(long)]
        view: Option<String>,
        #[arg(long, default_value_t = 800)]
        width: u32,
        #[arg(long, default_value_t = 600)]
        height: u32,
        /// Plot look: `default` or `jfm` (black frame on white).
        #[arg(long, default_value = "default", value_parser = parse_plot_style)]
        style: PlotStyle,
        /// Mesh line color as `R,G,B` or `#RRGGBB` (default green).
        #[arg(long)]
        mesh_color: Option<String>,
        /// Stretch mesh to fill the raster (no letterbox). Default: equal aspect.
        #[arg(long, default_value_t = false)]
        fill: bool,
        /// Pixel margin around the fitted domain.
        #[arg(long, default_value_t = 8)]
        margin: u32,
        /// Screen-space domain pan X (pixels; positive → domain right).
        #[arg(long = "pan-x", default_value_t = 0.0)]
        pan_x: f64,
        /// Screen-space domain pan Y (pixels; positive → domain down).
        #[arg(long = "pan-y", default_value_t = 0.0)]
        pan_y: f64,
        #[command(flatten)]
        nondim: NondimArgs,
    },
    /// Contour (pseudocolor) any scalar variable to PNG.
    Contour {
        path: PathBuf,
        /// Variable name present in the dataset.
        #[arg(long = "var", short = 'v')]
        variable: String,
        /// Output PNG path.
        #[arg(long, short = 'o')]
        out: PathBuf,
        /// Zone index (default 0).
        #[arg(long, default_value_t = 0)]
        zone: usize,
        /// Plane/slice, e.g. k=0, i=1, j=2 (default: auto).
        #[arg(long)]
        plane: Option<String>,
        /// View: auto|+x|-x|+y|-y|+z|-z|az=deg,el=deg (Fieldplot).
        #[arg(long)]
        view: Option<String>,
        /// Color-range minimum (default: data min).
        #[arg(long)]
        vmin: Option<f64>,
        /// Color-range maximum (default: data max).
        #[arg(long)]
        vmax: Option<f64>,
        #[arg(long, default_value_t = 512)]
        width: u32,
        #[arg(long, default_value_t = 512)]
        height: u32,
        /// Plot look: `default` (turbo) or `jfm` (restrained coolwarm, black frame).
        #[arg(long, default_value = "default", value_parser = parse_plot_style)]
        style: PlotStyle,
        #[command(flatten)]
        nondim: NondimArgs,
        /// Overlay the zone mesh (wireframe) on the contour.
        #[arg(long, default_value_t = false)]
        mesh: bool,
        /// Mesh overlay color as `R,G,B` or `#RRGGBB` (default green / JFM black).
        #[arg(long)]
        mesh_color: Option<String>,
        /// Also trace pathlines and overlay them on the contour PNG.
        #[arg(long)]
        pathlines: bool,
        /// Write pathlines to a second PNG instead of (or as well as) overlay.
        #[arg(long)]
        pathlines_out: Option<PathBuf>,
        #[command(flatten)]
        pathline: PathlineArgs,
        /// Contour coloring: `continuous` (flood) or `banded` (discrete levels).
        #[arg(long, default_value = "continuous", value_parser = parse_coloring)]
        coloring: ContourColoring,
        /// Number of contour bands / levels (banded mode; toolbar ± adjusts this).
        #[arg(long, default_value_t = 16)]
        levels: u32,
        /// Colormap: `default` (follow --style), `turbo`, `jfm`, `coolwarm`, `grayscale`.
        #[arg(long, default_value = "default", value_parser = parse_colormap)]
        colormap: ColorMapName,
        /// Draw the colorbar legend (default on).
        #[arg(long, default_value_t = true)]
        legend: bool,
        /// Suppress the colorbar legend.
        #[arg(long, default_value_t = false)]
        no_legend: bool,
        /// Stamp iso-level value labels on the flood (Contour Abc Label).
        #[arg(long, default_value_t = false)]
        labels: bool,
        /// Colorbar font size in pixels (0 = auto from raster width).
        #[arg(long, default_value_t = 0.0)]
        legend_font: f32,
        /// Colorbar side: `right` (default) or `left`.
        #[arg(long, default_value = "right")]
        legend_align: String,
        /// Pixel margin around the filled plot area.
        #[arg(long, default_value_t = 4)]
        margin: u32,
        /// Screen-space domain pan X (pixels; positive → domain right).
        #[arg(long = "pan-x", default_value_t = 0.0)]
        pan_x: f64,
        /// Screen-space domain pan Y (pixels; positive → domain down).
        #[arg(long = "pan-y", default_value_t = 0.0)]
        pan_y: f64,
        /// Overlay velocity vectors (needs --u-var/--v-var or auto-guess).
        #[arg(long, default_value_t = false)]
        vectors: bool,
        /// Overlay boundary edges only (ordered grid perimeter).
        #[arg(long, default_value_t = false)]
        edges: bool,
        /// Overlay subsampled scatter markers at nodes.
        #[arg(long, default_value_t = false)]
        scatter: bool,
        /// Overlay isolines of the active scalar (2D iso-surfaces).
        #[arg(long, default_value_t = false)]
        isolines: bool,
        /// Subsample stride for vectors / scatter (default 4).
        #[arg(long, default_value_t = 4)]
        overlay_skip: usize,
    },
    /// Probe all (or one) variables at world or pixel coordinates (Fieldplot).
    Probe {
        path: PathBuf,
        /// Optional single variable; omit to sample every variable at the point.
        #[arg(long = "var", short = 'v')]
        variable: Option<String>,
        #[arg(long, default_value_t = 0)]
        zone: usize,
        #[arg(long)]
        plane: Option<String>,
        #[arg(long)]
        view: Option<String>,
        /// World X (mutually exclusive with --px).
        #[arg(long)]
        x: Option<f64>,
        /// World Y.
        #[arg(long)]
        y: Option<f64>,
        /// Pixel X in a W×H raster (with --py / --width / --height).
        #[arg(long)]
        px: Option<i32>,
        #[arg(long)]
        py: Option<i32>,
        #[arg(long, default_value_t = 800)]
        width: u32,
        #[arg(long, default_value_t = 600)]
        height: u32,
        /// Optional PNG to stamp the probe label onto (writes --out).
        #[arg(long, short = 'o')]
        out: Option<PathBuf>,
        #[arg(long, default_value = "default", value_parser = parse_plot_style)]
        style: PlotStyle,
        #[command(flatten)]
        nondim: NondimArgs,
    },
    /// Print nondim suggestions and optionally apply scales (no PNG).
    Nondim {
        path: PathBuf,
        #[command(flatten)]
        nondim: NondimArgs,
        /// After applying scales, print a short min/max summary of scaled fields.
        #[arg(long, default_value_t = true)]
        summary: bool,
    },
    /// Trace pathlines (streamlines of U,V) and write PNG.
    Pathlines {
        path: PathBuf,
        #[arg(long, short = 'o')]
        out: PathBuf,
        #[arg(long, default_value_t = 0)]
        zone: usize,
        #[arg(long)]
        plane: Option<String>,
        /// View: auto|+x|-x|+y|-y|+z|-z|az=deg,el=deg.
        #[arg(long)]
        view: Option<String>,
        #[arg(long, default_value_t = 512)]
        width: u32,
        #[arg(long, default_value_t = 512)]
        height: u32,
        /// Plot look: `default` (turbo) or `jfm` (white paper, black frame).
        #[arg(long, default_value = "default", value_parser = parse_plot_style)]
        style: PlotStyle,
        #[command(flatten)]
        nondim: NondimArgs,
        #[command(flatten)]
        pathline: PathlineArgs,
        /// Optional scalar to draw under the pathlines.
        #[arg(long = "var")]
        contour_var: Option<String>,
        #[arg(long)]
        vmin: Option<f64>,
        #[arg(long)]
        vmax: Option<f64>,
    },
    /// Numerical Schlieren / Shadowgraph from a density field (any CFD dataset).
    Optical {
        path: PathBuf,
        /// `schlieren` | `schlieren-alt` | `shadowgraph` (aliases: schlieren-chamarthi, chamarthi)
        #[arg(long, default_value = "schlieren")]
        kind: String,
        /// Density variable (default: guess Density/rho by name).
        #[arg(long = "var")]
        variable: Option<String>,
        #[arg(long, short = 'o')]
        out: PathBuf,
        #[arg(long, default_value_t = 0)]
        zone: usize,
        #[arg(long)]
        plane: Option<String>,
        #[arg(long)]
        view: Option<String>,
        #[arg(long, default_value_t = 700)]
        width: u32,
        #[arg(long, default_value_t = 525)]
        height: u32,
        #[arg(long, default_value = "jfm", value_parser = parse_plot_style)]
        style: PlotStyle,
        #[arg(long)]
        vmin: Option<f64>,
        #[arg(long)]
        vmax: Option<f64>,
        /// Show colorbar (off by default for optical figures).
        #[arg(long, default_value_t = false)]
        legend: bool,
    },
    /// Field sanity / positivity checks (NaN, Inf, extrema; optional ρ/p/T ≥ 0).
    Validate {
        path: PathBuf,
        /// Also flag negative samples on density / pressure / temperature names.
        #[arg(long, default_value_t = false)]
        positivity: bool,
        /// Zone index filter (default: all zones).
        #[arg(long)]
        zone: Option<usize>,
    },
    /// Extract a 1-D profile / centerline (lehar extract-line analogue).
    Profile {
        path: PathBuf,
        /// Variable name to sample.
        #[arg(long = "var", short = 'v')]
        variable: String,
        /// Output JSON path (default: stdout via --json / text).
        #[arg(long, short = 'o')]
        out: Option<PathBuf>,
        /// Line: `mid-j` (default), `mid-i`, `centerline`, `i=N`, `j=N`.
        #[arg(long, default_value = "mid-j")]
        line: String,
        #[arg(long, default_value_t = 0)]
        zone: usize,
        #[arg(long)]
        plane: Option<String>,
        /// Optional two-column reference CSV (`s,value`) for RMSE compare.
        #[arg(long = "ref")]
        reference: Option<PathBuf>,
    },
    /// Derived fields — evaluate an expression and optionally write PLT ASCII.
    Derived {
        path: PathBuf,
        /// New field name.
        #[arg(long)]
        name: String,
        /// Expression using dataset variables (e.g. `hypot(U,V)` or `P/1.4`).
        #[arg(long = "expr")]
        expression: String,
        /// Optional PLT ASCII output with the new field appended.
        #[arg(long, short = 'o')]
        out: Option<PathBuf>,
    },
    /// Line sample — world-space lineout with optional CSV export.
    LineSample {
        path: PathBuf,
        #[arg(long = "var", short = 'v')]
        variable: String,
        /// Start point `x,y`.
        #[arg(long = "from", value_name = "X,Y")]
        from: String,
        /// End point `x,y`.
        #[arg(long = "to", value_name = "X,Y")]
        to: String,
        /// Number of samples along the segment.
        #[arg(long, default_value_t = 64)]
        n: usize,
        #[arg(long, default_value_t = 0)]
        zone: usize,
        #[arg(long)]
        plane: Option<String>,
        /// Write CSV (`s,x,y,value`).
        #[arg(long, short = 'o')]
        out: Option<PathBuf>,
    },
    /// Operator stack stubs (Cut / Threshold / Level set).
    Operators {
        /// Operator kind: `cut`, `threshold`, `level-set` (repeatable via JSON list later).
        #[arg(long)]
        add: Option<String>,
        #[arg(long)]
        plane: Option<String>,
        #[arg(long = "var")]
        variable: Option<String>,
        #[arg(long)]
        min: Option<f64>,
        #[arg(long)]
        max: Option<f64>,
        #[arg(long)]
        value: Option<f64>,
    },
    /// List / show / run CFD post-processing menu items (lehar ↔ Flowplot).
    Menu {
        /// Override path to `agents/postprocess-menu.yaml`.
        #[arg(long = "menu-file")]
        menu_file: Option<PathBuf>,
        #[command(subcommand)]
        action: Option<MenuAction>,
    },
}

#[derive(Subcommand, Debug)]
enum MenuAction {
    /// Print menu groups and items (default for bare `flowplot menu`).
    List {
        /// Only show one group id (e.g. `shocks`).
        #[arg(long)]
        group: Option<String>,
    },
    /// Show one menu item (lehar agent, template, flags).
    Show {
        id: String,
    },
    /// Dispatch a ready item, or document a stub.
    Run {
        id: String,
        /// Arguments forwarded to the underlying `flowplot <command> …`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(clap::Args, Debug, Clone, Default)]
struct NondimArgs {
    /// Velocity reference scale (normalize U,V,W and magnitude).
    #[arg(long = "u-ref")]
    u_ref: Option<f64>,
    /// Length scale for X / R axis quantity.
    #[arg(long)]
    lx: Option<f64>,
    /// Length scale for Y / theta axis quantity.
    #[arg(long)]
    ly: Option<f64>,
    /// Length scale for Z axis quantity.
    #[arg(long)]
    lz: Option<f64>,
    /// Temperature reference (normalize T).
    #[arg(long = "t-ref")]
    t_ref: Option<f64>,
    /// Name of X-like coordinate variable (default: X or R if present).
    #[arg(long = "x-var")]
    x_var: Option<String>,
    /// Name of Y-like coordinate variable (default: Y or THETA if present).
    #[arg(long = "y-var")]
    y_var: Option<String>,
    /// Name of Z-like coordinate variable (default: Z if present).
    #[arg(long = "z-var")]
    z_var: Option<String>,
    /// Print name-based scale suggestions (default on for `nondim` subcommand).
    #[arg(long, default_value_t = false)]
    suggest: bool,
}

#[derive(clap::Args, Debug, Clone, Default)]
struct PathlineArgs {
    /// U velocity variable name (default: guess U/Vx/…).
    #[arg(long = "u-var")]
    u_var: Option<String>,
    /// V velocity variable name (default: guess V/Vy/…).
    #[arg(long = "v-var")]
    v_var: Option<String>,
    /// Optional W velocity variable name.
    #[arg(long = "w-var")]
    w_var: Option<String>,
    /// Seed point `x,y` (repeatable). If omitted, a mid-height rake is used.
    #[arg(long = "seed", value_name = "X,Y")]
    seeds: Vec<String>,
    /// Integration step in coordinate units (`0` = auto from cell size / |U|).
    /// Auto is required for hypersonic fields where a fixed 0.02 leaves the domain.
    #[arg(long = "dt", default_value_t = 0.0)]
    dt: f64,
    #[arg(long = "max-steps", default_value_t = 1000)]
    max_steps: usize,
    /// Only integrate forward (+dt), not bidirectional.
    #[arg(long, default_value_t = false)]
    forward_only: bool,
    /// Number of automatic rake seeds when --seed is omitted.
    #[arg(long = "seed-count", default_value_t = 5)]
    seed_count: usize,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let json = cli.json;
    let result = dispatch(cli.command, cli.path, json);

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            if json {
                println!(
                    "{{\"status\":\"error\",\"error\":{}}}",
                    json_string(&e.to_string())
                );
            } else {
                eprintln!("error: {e}");
            }
            ExitCode::FAILURE
        }
    }
}

fn dispatch(
    command: Option<Commands>,
    bare_path: Option<PathBuf>,
    json: bool,
) -> flowplot::Result<()> {
    match command {
        None => {
            let Some(path) = bare_path else {
                return Err(flowplot::Error::plot(
                    "usage: flowplot [--json] <path> | flowplot [--json] <load|info|mesh|contour|optical|nondim|pathlines|validate|profile|derived|line-sample|operators|menu> ...",
                ));
            };
            run_info(&path, json)
        }
        Some(Commands::Info { path }) | Some(Commands::Load { path }) => run_info(&path, json),
        Some(Commands::Mesh {
            path,
            out,
            zone,
            plane,
            view,
            width,
            height,
            style,
            mesh_color,
            fill,
            margin,
            pan_x,
            pan_y,
            nondim,
        }) => run_mesh(
            &path,
            &out,
            zone,
            plane.as_deref(),
            view.as_deref(),
            width,
            height,
            style,
            mesh_color.as_deref(),
            fill,
            margin,
            pan_x,
            pan_y,
            &nondim,
            json,
        ),
        Some(Commands::Contour {
            path,
            variable,
            out,
            zone,
            plane,
            view,
            vmin,
            vmax,
            width,
            height,
            style,
            nondim,
            mesh,
            mesh_color,
            pathlines,
            pathlines_out,
            pathline: pl_args,
            coloring,
            levels,
            colormap,
            legend,
            no_legend,
            labels,
            legend_font,
            legend_align,
            margin,
            pan_x,
            pan_y,
            vectors,
            edges,
            scatter,
            isolines,
            overlay_skip,
        }) => run_contour(
            &path,
            &variable,
            &out,
            zone,
            plane.as_deref(),
            view.as_deref(),
            vmin,
            vmax,
            width,
            height,
            style,
            &nondim,
            mesh,
            mesh_color.as_deref(),
            pathlines,
            pathlines_out.as_ref(),
            &pl_args,
            coloring,
            levels,
            colormap,
            legend && !no_legend,
            labels,
            legend_font,
            parse_legend_align(&legend_align),
            margin,
            pan_x,
            pan_y,
            vectors,
            edges,
            scatter,
            isolines,
            overlay_skip,
            json,
        ),
        Some(Commands::Probe {
            path,
            variable,
            zone,
            plane,
            view,
            x,
            y,
            px,
            py,
            width,
            height,
            out,
            style,
            nondim,
        }) => run_probe(
            &path,
            variable.as_deref(),
            zone,
            plane.as_deref(),
            view.as_deref(),
            x,
            y,
            px,
            py,
            width,
            height,
            out.as_ref(),
            style,
            &nondim,
            json,
        ),
        Some(Commands::Nondim {
            path,
            nondim,
            summary,
        }) => run_nondim(&path, &nondim, summary, json),
        Some(Commands::Pathlines {
            path,
            out,
            zone,
            plane,
            view,
            width,
            height,
            style,
            nondim,
            pathline: pl_args,
            contour_var,
            vmin,
            vmax,
        }) => run_pathlines_cmd(
            &path,
            &out,
            zone,
            plane.as_deref(),
            view.as_deref(),
            width,
            height,
            style,
            &nondim,
            &pl_args,
            contour_var.as_deref(),
            vmin,
            vmax,
            json,
        ),
        Some(Commands::Optical {
            path,
            kind,
            variable,
            out,
            zone,
            plane,
            view,
            width,
            height,
            style,
            vmin,
            vmax,
            legend,
        }) => run_optical(
            &path,
            &kind,
            variable.as_deref(),
            &out,
            zone,
            plane.as_deref(),
            view.as_deref(),
            width,
            height,
            style,
            vmin,
            vmax,
            legend,
            json,
        ),
        Some(Commands::Validate {
            path,
            positivity,
            zone,
        }) => run_validate(&path, positivity, zone, json),
        Some(Commands::Profile {
            path,
            variable,
            out,
            line,
            zone,
            plane,
            reference,
        }) => run_profile(
            &path,
            &variable,
            out.as_ref(),
            &line,
            zone,
            plane.as_deref(),
            reference.as_ref(),
            json,
        ),
        Some(Commands::Derived {
            path,
            name,
            expression,
            out,
        }) => run_derived(&path, &name, &expression, out.as_ref(), json),
        Some(Commands::LineSample {
            path,
            variable,
            from,
            to,
            n,
            zone,
            plane,
            out,
        }) => run_line_sample(
            &path,
            &variable,
            &from,
            &to,
            n,
            zone,
            plane.as_deref(),
            out.as_ref(),
            json,
        ),
        Some(Commands::Operators {
            add,
            plane,
            variable,
            min,
            max,
            value,
        }) => run_operators(
            add.as_deref(),
            plane.as_deref(),
            variable.as_deref(),
            min,
            max,
            value,
            json,
        ),
        Some(Commands::Menu { menu_file, action }) => run_menu(menu_file.as_ref(), action, json),
    }
}

fn run_optical(
    path: &PathBuf,
    kind: &str,
    variable: Option<&str>,
    out: &PathBuf,
    zone: usize,
    plane: Option<&str>,
    view: Option<&str>,
    width: u32,
    height: u32,
    style: PlotStyle,
    vmin: Option<f64>,
    vmax: Option<f64>,
    legend: bool,
    json: bool,
) -> flowplot::Result<()> {
    let ds = load_path(path)?;
    let kind = OpticalKind::parse(kind)?;
    let plane = match plane {
        Some(s) => PlaneSpec::parse(s)?,
        None => PlaneSpec::Auto,
    };
    let view = match view {
        Some(s) => ViewSpec::parse(s)?,
        None => ViewSpec::Auto,
    };
    let opts = OpticalOptions {
        kind,
        density_var: variable.map(|s| s.to_string()),
        zone,
        plane,
        view,
        width,
        height,
        style,
        annotate: true,
        path_hint: Some(path.display().to_string()),
        legend,
        vmin,
        vmax,
    };
    let img = write_optical_png(&ds, &opts, out)?;
    if json {
        println!(
            "{{\"status\":\"ok\",\"command\":\"optical\",\"kind\":{},\"path\":{},\"out\":{},\"vmin\":{},\"vmax\":{},\"zone\":{}}}",
            json_string(kind.as_str()),
            json_string(&path.display().to_string()),
            json_string(&out.display().to_string()),
            img.vmin,
            img.vmax,
            zone
        );
    } else {
        println!(
            "wrote optical {} ({}) → {}  range=[{}, {}]",
            kind.as_str(),
            path.display(),
            out.display(),
            img.vmin,
            img.vmax
        );
    }
    Ok(())
}

fn run_validate(
    path: &PathBuf,
    positivity: bool,
    zone_filter: Option<usize>,
    json: bool,
) -> flowplot::Result<()> {
    let ds = load_path(path)?;
    let mut report = validate_dataset(&ds, positivity);
    report.path_hint = Some(path.display().to_string());
    if let Some(zi) = zone_filter {
        report.fields.retain(|f| f.zone == zi);
        report.ok = report.fields.iter().all(|f| f.ok);
        report.issue_count = report.fields.iter().map(|f| f.issues.len()).sum();
        report.summary = if report.ok {
            format!("ok: zone {zi}; {} field(s) clean", report.fields.len())
        } else {
            format!(
                "issues: {} on zone {zi}",
                report.issue_count
            )
        };
    }
    if json {
        let payload = serde_json::json!({
            "status": if report.ok { "ok" } else { "issues" },
            "command": "validate",
            "path": path.display().to_string(),
            "positivity": positivity,
            "report": report,
        });
        println!("{payload}");
    } else {
        println!("validate: {}", path.display());
        println!("{}", report.summary);
        for f in &report.fields {
            let mark = if f.ok { "ok" } else { "!!" };
            println!(
                "  [{mark}] zone {}  {:<16} n={} finite={} nan={} inf={} min={:?} max={:?}",
                f.zone, f.name, f.count, f.finite, f.nan, f.inf, f.min, f.max
            );
            for issue in &f.issues {
                println!("       - {issue}");
            }
        }
    }
    // Always Ok after a completed scan — callers inspect `status` / summary for issues.
    Ok(())
}

fn run_profile(
    path: &PathBuf,
    variable: &str,
    out: Option<&PathBuf>,
    line: &str,
    zone: usize,
    plane: Option<&str>,
    reference: Option<&PathBuf>,
    json: bool,
) -> flowplot::Result<()> {
    let ds = load_path(path)?;
    let plane_spec = match plane {
        Some(p) => PlaneSpec::parse(p)?,
        None => PlaneSpec::Auto,
    };
    let line_spec = ProfileLine::parse(line)?;
    let profile = extract_profile(&ds, variable, zone, line_spec, plane_spec)?;

    let mut rmse: Option<f64> = None;
    if let Some(ref_path) = reference {
        let text = std::fs::read_to_string(ref_path).map_err(|e| {
            flowplot::Error::plot(format!("failed to read --ref {}: {e}", ref_path.display()))
        })?;
        let (xs, ys) = load_ref_csv(&text)?;
        rmse = compare_profile_rmse(&profile, &xs, &ys);
    }

    if !json && out.is_none() {
        println!(
            "profile: {}  var={}  {}={}  n={}",
            path.display(),
            profile.variable,
            profile.axis,
            profile.index,
            profile.samples.len()
        );
        println!("# s  x  y  value");
        for s in &profile.samples {
            println!("{:.8}  {:.8}  {:.8}  {:.8}", s.s, s.x, s.y, s.value);
        }
        if let Some(r) = rmse {
            println!("rmse vs ref: {r}");
        }
        return Ok(());
    }

    let payload = serde_json::json!({
        "status": "ok",
        "command": "profile",
        "path": path.display().to_string(),
        "profile": profile,
        "rmse": rmse,
        "reference": reference.map(|p| p.display().to_string()),
    });

    if let Some(out_path) = out {
        std::fs::write(out_path, serde_json::to_string_pretty(&payload).unwrap_or_default())
            .map_err(|e| {
                flowplot::Error::plot(format!("failed to write {}: {e}", out_path.display()))
            })?;
        if !json {
            println!("wrote {}", out_path.display());
            if let Some(r) = rmse {
                println!("rmse vs ref: {r}");
            }
        }
    }
    if json {
        println!("{payload}");
    }
    Ok(())
}

fn parse_xy_pair(s: &str, flag: &str) -> flowplot::Result<(f64, f64)> {
    let parts: Vec<_> = s
        .split(&[',', ' '][..])
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() != 2 {
        return Err(flowplot::Error::plot(format!(
            "{flag} expects X,Y (got '{s}')"
        )));
    }
    let x: f64 = parts[0]
        .parse()
        .map_err(|_| flowplot::Error::plot(format!("{flag}: bad x in '{s}'")))?;
    let y: f64 = parts[1]
        .parse()
        .map_err(|_| flowplot::Error::plot(format!("{flag}: bad y in '{s}'")))?;
    Ok((x, y))
}

fn run_derived(
    path: &PathBuf,
    name: &str,
    expression: &str,
    out: Option<&PathBuf>,
    json: bool,
) -> flowplot::Result<()> {
    let ds = load_path(path)?;
    let derived = apply_derived(&ds, name, expression)?;
    let (mn, mx) = field_minmax(&derived, name)?;
    if let Some(out_path) = out {
        write_plt_ascii(&derived, out_path)?;
    }
    if json {
        println!(
            "{{\"status\":\"ok\",\"command\":\"derived\",\"path\":{},\"name\":{},\"expression\":{},\"min\":{mn},\"max\":{mx},\"variables\":{},\"out\":{}}}",
            json_string(&path.display().to_string()),
            json_string(name),
            json_string(expression),
            json_string_list(&derived.variables),
            out.map(|p| json_string(&p.display().to_string()))
                .unwrap_or_else(|| "null".into())
        );
    } else {
        println!(
            "derived field '{name}' = {expression}  min={mn} max={mx}  vars={}",
            derived.variables.join(", ")
        );
        if let Some(out_path) = out {
            println!("wrote {}", out_path.display());
        }
    }
    Ok(())
}

fn run_line_sample(
    path: &PathBuf,
    variable: &str,
    from: &str,
    to: &str,
    n: usize,
    zone: usize,
    plane: Option<&str>,
    out: Option<&PathBuf>,
    json: bool,
) -> flowplot::Result<()> {
    let ds = load_path(path)?;
    let (x0, y0) = parse_xy_pair(from, "--from")?;
    let (x1, y1) = parse_xy_pair(to, "--to")?;
    let plane_spec = match plane {
        Some(p) => PlaneSpec::parse(p)?,
        None => PlaneSpec::Auto,
    };
    let profile = extract_line_sample(&ds, variable, zone, x0, y0, x1, y1, n, plane_spec)?;
    let csv = profile_to_csv(&profile);
    if let Some(out_path) = out {
        std::fs::write(out_path, &csv).map_err(|e| {
            flowplot::Error::plot(format!("failed to write {}: {e}", out_path.display()))
        })?;
    }
    if json {
        let payload = serde_json::json!({
            "status": "ok",
            "command": "line-sample",
            "path": path.display().to_string(),
            "from": [x0, y0],
            "to": [x1, y1],
            "n": n,
            "profile": profile,
            "out": out.map(|p| p.display().to_string()),
        });
        println!("{payload}");
    } else {
        println!(
            "line sample: var={} from=({x0},{y0}) to=({x1},{y1}) n={}",
            profile.variable,
            profile.samples.len()
        );
        if out.is_none() {
            print!("{csv}");
        } else if let Some(out_path) = out {
            println!("wrote {}", out_path.display());
        }
    }
    Ok(())
}

fn run_operators(
    add: Option<&str>,
    plane: Option<&str>,
    variable: Option<&str>,
    min: Option<f64>,
    max: Option<f64>,
    value: Option<f64>,
    json: bool,
) -> flowplot::Result<()> {
    let mut stack = OperatorStack::default();
    if let Some(kind) = add {
        stack.push(parse_operator(kind, plane, variable, min, max, value)?);
    }
    if json {
        println!(
            "{{\"status\":\"ok\",\"command\":\"operators\",\"stack\":{}}}",
            stack.status_json()
        );
    } else if stack.ops.is_empty() {
        println!("operators: empty stack (add with --add cut|threshold|level-set)");
        println!("  Cut        — structured plane (wired to mesh/contour --plane)");
        println!("  Threshold  — keep samples in [min,max] (stub)");
        println!("  Level set  — iso value of a scalar (stub)");
    } else {
        for (i, op) in stack.ops.iter().enumerate() {
            println!("  [{i}] {}", op.label());
        }
    }
    Ok(())
}

fn load_menu(menu_file: Option<&PathBuf>) -> flowplot::Result<PostprocessMenu> {
    match menu_file {
        Some(path) => PostprocessMenu::load_path(path),
        None => PostprocessMenu::load_default(),
    }
}

fn run_menu(
    menu_file: Option<&PathBuf>,
    action: Option<MenuAction>,
    json: bool,
) -> flowplot::Result<()> {
    let menu = load_menu(menu_file)?;
    match action {
        None | Some(MenuAction::List { group: None }) => print_menu_list(&menu, None, json),
        Some(MenuAction::List { group: Some(gid) }) => {
            if menu.filter_group(Some(&gid)).is_empty() {
                return Err(flowplot::Error::plot(format!(
                    "unknown menu group '{gid}' (see `flowplot menu`)"
                )));
            }
            print_menu_list(&menu, Some(&gid), json)
        }
        Some(MenuAction::Show { id }) => {
            let Some((group, item)) = menu.find_item(&id) else {
                return Err(flowplot::Error::plot(format!(
                    "unknown menu item '{id}' (see `flowplot menu`)"
                )));
            };
            if json {
                println!(
                    "{{\"status\":\"ok\",\"command\":\"menu\",\"action\":\"show\",\"item\":{}}}",
                    serde_json::to_string(item).unwrap_or_else(|_| "null".into())
                );
            } else {
                print!("{}", format_item_text(group, item));
            }
            Ok(())
        }
        Some(MenuAction::Run { id, args }) => run_menu_item(&menu, &id, args, json),
    }
}

fn print_menu_list(
    menu: &PostprocessMenu,
    group: Option<&str>,
    json: bool,
) -> flowplot::Result<()> {
    if json {
        let groups: Vec<_> = menu
            .filter_group(group)
            .into_iter()
            .cloned()
            .collect();
        let payload = serde_json::json!({
            "status": "ok",
            "command": "menu",
            "action": "list",
            "title": menu.title,
            "version": menu.version,
            "groups": groups,
        });
        println!("{payload}");
    } else {
        print!("{}", format_menu_text(menu, group));
    }
    Ok(())
}

fn run_menu_item(
    menu: &PostprocessMenu,
    id: &str,
    args: Vec<String>,
    json: bool,
) -> flowplot::Result<()> {
    let Some((_group, item)) = menu.find_item(id) else {
        return Err(flowplot::Error::plot(format!(
            "unknown menu item '{id}' (see `flowplot menu`)"
        )));
    };
    if item.status != "ready" {
        return emit_stub(item, json);
    }
    dispatch_ready_item(item, args, json)
}

fn emit_stub(item: &MenuItem, json: bool) -> flowplot::Result<()> {
    if json {
        let payload = serde_json::json!({
            "status": "stub",
            "command": "menu",
            "action": "run",
            "id": item.id,
            "title": item.title,
            "requires_agent": item.lehar.agent,
            "lehar_surface": item.lehar.surface,
            "flowplot_template": item.flowplot.template,
            "planned_flags": item.flowplot.planned_flags,
            "notes": item.flowplot.notes,
            "mcp": item.mcp,
        });
        println!("{payload}");
    } else {
        print!("{}", format_stub_run(item));
    }
    Ok(())
}

fn dispatch_ready_item(item: &MenuItem, args: Vec<String>, json: bool) -> flowplot::Result<()> {
    let mut forwarded = args;
    // Allow `menu run <id> -- <args>`; clap may leave a literal "--".
    if forwarded.first().map(String::as_str) == Some("--") {
        forwarded.remove(0);
    }
    // Shock polar chart lives under Agents (matplotlib θ–β–M); CLI documents how.
    if item.id == "shock-polar-field" || item.flowplot.command == "shock-polar" {
        if json {
            let payload = serde_json::json!({
                "status": "ok",
                "command": "shock-polar",
                "id": item.id,
                "note": "Open Agents → Shocks → Shock polar for the θ–β–M chart (gui/shock_polar.py). Ideal-gas freestream via dialog; works on any CFD dataset.",
                "how": "python3 -c \"from gui.shock_polar import plot_polar_png; plot_polar_png('/tmp/polar.png', mach=2.0, gamma=1.4)\"",
                "template": item.flowplot.template,
            });
            println!("{payload}");
        } else {
            println!(
                "shock polar: use Agents → Shocks → Shock polar (gui/shock_polar.py θ–β–M).\n\
                 Freestream M∞ / γ from dialog — solver-agnostic ideal-gas curve.\n\
                 template: {}",
                item.flowplot.template
            );
        }
        return Ok(());
    }
    // contour-pathlines always overlays traces.
    if item.id == "contour-pathlines"
        && !forwarded.iter().any(|a| a == "--pathlines" || a == "--pathlines-out")
    {
        forwarded.push("--pathlines".into());
    }
    // positivity-check always enables the positivity scan.
    if item.id == "positivity-check" && !forwarded.iter().any(|a| a == "--positivity") {
        forwarded.push("--positivity".into());
    }
    // Optical Agents pin --kind from the menu id when not already provided.
    if item.flowplot.command == "optical"
        && !forwarded.iter().any(|a| a == "--kind")
    {
        let kind = match item.id.as_str() {
            "schlieren-alt" | "schlieren-chamarthi" => "schlieren-alt",
            "shadowgraph" => "shadowgraph",
            _ => "schlieren",
        };
        forwarded.push("--kind".into());
        forwarded.push(kind.into());
    }
    if item.id == "entropy-sensor-field"
        && !forwarded.iter().any(|a| a == "--colormap")
    {
        forwarded.push("--colormap".into());
        forwarded.push("elmar".into());
    }
    if json && !forwarded.iter().any(|a| a == "--json") {
        forwarded.push("--json".into());
    }

    let mut argv = vec![
        "flowplot".to_string(),
        item.flowplot.command.clone(),
    ];
    argv.extend(forwarded);

    let cli = Cli::try_parse_from(&argv).map_err(|e| {
        flowplot::Error::plot(format!(
            "menu run {} — invalid args for `flowplot {}`: {e}\ntemplate: {}",
            item.id, item.flowplot.command, item.flowplot.template
        ))
    })?;
    // Avoid re-entering menu; only dispatch leaf commands.
    match &cli.command {
        Some(Commands::Menu { .. }) => Err(flowplot::Error::plot(
            "menu run cannot dispatch another menu action",
        )),
        _ => dispatch(cli.command, cli.path, cli.json || json),
    }
}

fn json_string(s: &str) -> String {
    let mut out = String::from('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_string_list(items: &[String]) -> String {
    let parts: Vec<_> = items.iter().map(|s| json_string(s)).collect();
    format!("[{}]", parts.join(","))
}

fn json_f64(v: f64) -> String {
    if v.is_finite() {
        // Keep a compact but parseable representation.
        format!("{v}")
    } else if v.is_nan() {
        "null".into()
    } else if v.is_sign_positive() {
        "null".into()
    } else {
        "null".into()
    }
}

fn suggestions_json(suggestions: &[NondimSuggestion]) -> String {
    let parts: Vec<_> = suggestions
        .iter()
        .map(|s| {
            format!(
                "{{\"variable\":{},\"kind\":{},\"message\":{}}}",
                json_string(&s.variable),
                json_string(&format!("{:?}", s.kind)),
                json_string(&s.message)
            )
        })
        .collect();
    format!("[{}]", parts.join(","))
}

fn zone_ordered_json(zone: &flowplot::Zone) -> String {
    use flowplot::{OrderedDims, Topology};
    match &zone.topology {
        Topology::Ordered(OrderedDims::I { i }) => format!("{{\"i\":{i}}}"),
        Topology::Ordered(OrderedDims::Ij { i, j }) => format!("{{\"i\":{i},\"j\":{j}}}"),
        Topology::Ordered(OrderedDims::Ijk { i, j, k }) => {
            format!("{{\"i\":{i},\"j\":{j},\"k\":{k}}}")
        }
        Topology::FiniteElement { .. } => "null".into(),
    }
}

fn dataset_load_json(path: &PathBuf, ds: &Dataset, suggestions: &[NondimSuggestion]) -> String {
    let mut zones = Vec::new();
    for (i, zone) in ds.zones.iter().enumerate() {
        let meta = ds.meta(i);
        let time = meta
            .time_value()
            .map(|t| t.to_string())
            .unwrap_or_else(|| "null".into());
        let cycle = meta
            .cycle()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "null".into());
        let strand = meta
            .strand_id
            .map(|s| s.to_string())
            .unwrap_or_else(|| "null".into());
        zones.push(format!(
            "{{\"index\":{i},\"name\":{},\"nodes\":{},\"cells\":{},\"ordered\":{},\"time\":{time},\"cycle\":{cycle},\"strand_id\":{strand}}}",
            json_string(&zone.name),
            zone.node_count(),
            zone.cell_count(),
            zone_ordered_json(zone)
        ));
    }
    let effectively_2d = flowplot::dataset_is_effectively_2d(ds);
    let default_view = flowplot::suggest_default_view(ds, 0, flowplot::PlaneSpec::Auto)
        .unwrap_or(flowplot::ViewSpec::Axis {
            axis: flowplot::LookAxis::Z,
            positive: true,
        });
    let (az, el) = default_view.to_free_angles();
    let plane = default_view.principal_plane().as_str();
    let default_view_json = format!(
        "{{\"az\":{az:.6},\"el\":{el:.6},\"label\":{},\"plane\":{}}}",
        json_string(&default_view.label()),
        json_string(plane)
    );
    let time_frames = flowplot::time_frames_json(ds);
    format!(
        "{{\"status\":\"ok\",\"command\":\"load\",\"path\":{},\"title\":{},\"variables\":{},\"zones\":[{}],\"effectively_2d\":{},\"default_view\":{},\"time_frames\":{},\"nondim_suggestions\":{}}}",
        json_string(&path.display().to_string()),
        ds.title
            .as_ref()
            .map(|t| json_string(t))
            .unwrap_or_else(|| "null".into()),
        json_string_list(&ds.variables),
        zones.join(","),
        if effectively_2d { "true" } else { "false" },
        default_view_json,
        time_frames,
        suggestions_json(suggestions)
    )
}

fn run_info(path: &PathBuf, json: bool) -> flowplot::Result<()> {
    let ds = load_path(path)?;
    let suggestions = suggest_scales(&ds.variables);
    if json {
        println!("{}", dataset_load_json(path, &ds, &suggestions));
        return Ok(());
    }
    print_summary(path, &ds);
    if !suggestions.is_empty() {
        println!("{}", format_suggestions(&suggestions));
    }
    Ok(())
}

fn print_summary(path: &PathBuf, ds: &Dataset) {
    println!("file: {}", path.display());
    if let Some(title) = &ds.title {
        println!("title: {title}");
    }
    println!("zones: {}", ds.zone_count());
    println!(
        "variables ({}): {}",
        ds.variables.len(),
        if ds.variables.is_empty() {
            "(none)".into()
        } else {
            ds.variables.join(", ")
        }
    );
    for (i, zone) in ds.zones.iter().enumerate() {
        println!(
            "  zone[{i}] \"{}\": nodes={}, cells={}",
            zone.name,
            zone.node_count(),
            zone.cell_count()
        );
    }
}

fn scales_from_args(args: &NondimArgs) -> NondimScales {
    NondimScales {
        u_ref: args.u_ref,
        lx: args.lx,
        ly: args.ly,
        lz: args.lz,
        t_ref: args.t_ref,
        x_var: args.x_var.clone(),
        y_var: args.y_var.clone(),
        z_var: args.z_var.clone(),
    }
}

fn maybe_nondim(ds: Dataset, args: &NondimArgs, force_suggest: bool) -> flowplot::Result<Dataset> {
    if args.suggest || force_suggest {
        println!("{}", format_suggestions(&suggest_scales(&ds.variables)));
    }
    let scales = scales_from_args(args);
    if scales.u_ref.is_none()
        && scales.lx.is_none()
        && scales.ly.is_none()
        && scales.lz.is_none()
        && scales.t_ref.is_none()
    {
        return Ok(ds);
    }
    apply_nondim(&ds, &scales)
}

fn parse_plane(plane: Option<&str>) -> flowplot::Result<PlaneSpec> {
    match plane {
        None => Ok(PlaneSpec::Auto),
        Some(s) => PlaneSpec::parse(s),
    }
}

fn parse_view(view: Option<&str>) -> flowplot::Result<ViewSpec> {
    match view {
        None => Ok(ViewSpec::Auto),
        Some(s) => ViewSpec::parse(s),
    }
}

fn build_pathline_opts(
    ds: &Dataset,
    zone: usize,
    plane: PlaneSpec,
    view: ViewSpec,
    args: &PathlineArgs,
) -> flowplot::Result<PathlineOptions> {
    let (gu, gv) = match (args.u_var.clone(), args.v_var.clone()) {
        (Some(u), Some(v)) => (u, v),
        (u_opt, v_opt) => {
            let (gu, gv) = guess_velocity_vars(ds)?;
            (u_opt.unwrap_or(gu), v_opt.unwrap_or(gv))
        }
    };
    let u_var = gu;
    let v_var = gv;
    let mut seeds = Vec::new();
    for s in &args.seeds {
        seeds.push(parse_seed(s)?);
    }
    if seeds.is_empty() {
        let u_idx = find_var(ds, &u_var)?;
        let grid = extract_scalar_grid_view(ds, zone, u_idx, plane, view)?;
        seeds = default_seeds(&grid, args.seed_count);
    }
    Ok(PathlineOptions {
        u_var,
        v_var,
        w_var: args.w_var.clone(),
        zone,
        plane,
        view,
        seeds,
        max_steps: args.max_steps,
        step_size: args.dt,
        bidirectional: !args.forward_only,
    })
}

fn parse_seed(s: &str) -> flowplot::Result<[f64; 2]> {
    let parts: Vec<_> = s.split(',').map(str::trim).collect();
    if parts.len() != 2 {
        return Err(flowplot::Error::plot(format!(
            "invalid --seed '{s}' (expected x,y)"
        )));
    }
    let x: f64 = parts[0]
        .parse()
        .map_err(|_| flowplot::Error::plot(format!("invalid seed x in '{s}'")))?;
    let y: f64 = parts[1]
        .parse()
        .map_err(|_| flowplot::Error::plot(format!("invalid seed y in '{s}'")))?;
    Ok([x, y])
}

fn parse_plot_style(s: &str) -> Result<PlotStyle, String> {
    s.parse()
}

fn parse_coloring(s: &str) -> Result<ContourColoring, String> {
    s.parse()
}

fn parse_colormap(s: &str) -> Result<ColorMapName, String> {
    s.parse()
}

/// When `--style jfm` is set and the user left the 512×512 CLI defaults, use
/// the single-column JFM raster size.
fn resolve_raster_size(style: PlotStyle, width: u32, height: u32) -> (u32, u32) {
    if style == PlotStyle::Jfm && width == 512 && height == 512 {
        (PlotStyle::JFM_WIDTH, PlotStyle::JFM_HEIGHT)
    } else {
        (width, height)
    }
}

fn parse_mesh_color(s: &str) -> flowplot::Result<[u8; 4]> {
    let t = s.trim();
    if let Some(hex) = t.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| {
                flowplot::Error::plot(format!("invalid --mesh-color '{s}' (expected #RRGGBB)"))
            })?;
            let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| {
                flowplot::Error::plot(format!("invalid --mesh-color '{s}' (expected #RRGGBB)"))
            })?;
            let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| {
                flowplot::Error::plot(format!("invalid --mesh-color '{s}' (expected #RRGGBB)"))
            })?;
            return Ok([r, g, b, 255]);
        }
    }
    let parts: Vec<_> = t.split(',').map(str::trim).collect();
    if parts.len() == 3 {
        let parse = |p: &str| -> flowplot::Result<u8> {
            p.parse::<u8>().map_err(|_| {
                flowplot::Error::plot(format!("invalid --mesh-color '{s}' (expected R,G,B)"))
            })
        };
        return Ok([parse(parts[0])?, parse(parts[1])?, parse(parts[2])?, 255]);
    }
    Err(flowplot::Error::plot(format!(
        "invalid --mesh-color '{s}' (expected R,G,B or #RRGGBB)"
    )))
}

fn default_mesh_line_rgba(style: PlotStyle, mesh_color: Option<&str>) -> flowplot::Result<[u8; 4]> {
    if let Some(s) = mesh_color {
        return parse_mesh_color(s);
    }
    Ok(match style {
        PlotStyle::Jfm => [0, 0, 0, 255],
        PlotStyle::Default => flowplot::DEFAULT_MESH_RGBA,
    })
}

fn run_mesh(
    path: &PathBuf,
    out: &PathBuf,
    zone: usize,
    plane: Option<&str>,
    view: Option<&str>,
    width: u32,
    height: u32,
    style: PlotStyle,
    mesh_color: Option<&str>,
    fill: bool,
    margin: u32,
    pan_x: f64,
    pan_y: f64,
    nondim: &NondimArgs,
    json: bool,
) -> flowplot::Result<()> {
    let mut nondim_args = nondim.clone();
    if json {
        nondim_args.suggest = false;
    }
    let ds = maybe_nondim(load_path(path)?, &nondim_args, nondim_args.suggest)?;
    let plane = parse_plane(plane)?;
    let view_in = parse_view(view)?;
    let view = flowplot::resolve_view(&ds, zone, plane, view_in)?;
    let (width, height) = if style == PlotStyle::Jfm && width == 800 && height == 600 {
        (PlotStyle::JFM_WIDTH, PlotStyle::JFM_HEIGHT)
    } else {
        (width, height)
    };
    // Standalone mesh defaults to green unless --mesh-color / JFM black.
    let line_rgba = match mesh_color {
        Some(s) => parse_mesh_color(s)?,
        None if style == PlotStyle::Jfm => [0, 0, 0, 255],
        None => flowplot::DEFAULT_MESH_RGBA,
    };
    let img = render_mesh(
        &ds,
        &MeshOptions {
            zone,
            plane,
            view,
            width,
            height,
            style,
            line_rgba,
            fill,
            margin,
            pan_x,
            pan_y,
            path_hint: Some(path.display().to_string()),
            ..Default::default()
        },
    )?;
    write_png(out, &img)?;
    let (xmin, xmax, ymin, ymax) = img.grid.bounds();
    if json {
        println!(
            "{{\"status\":\"ok\",\"command\":\"mesh\",\"path\":{},\"zone\":{},\"out\":{},\"ni\":{},\"nj\":{},\"view\":{},\"bounds\":[{},{},{},{}]}}",
            json_string(&path.display().to_string()),
            zone,
            json_string(&out.display().to_string()),
            img.grid.ni,
            img.grid.nj,
            json_string(&view.label()),
            json_f64(xmin),
            json_f64(xmax),
            json_f64(ymin),
            json_f64(ymax)
        );
    } else {
        println!(
            "wrote mesh {} (zone={}, {}×{}, view={}, equal-aspect bounds=[{}, {}]×[{}, {}])",
            out.display(),
            zone,
            img.grid.ni,
            img.grid.nj,
            view.label(),
            xmin,
            xmax,
            ymin,
            ymax
        );
    }
    Ok(())
}

fn parse_legend_align(s: &str) -> LegendAlign {
    if s.eq_ignore_ascii_case("left") {
        LegendAlign::Left
    } else {
        LegendAlign::Right
    }
}

fn run_contour(
    path: &PathBuf,
    variable: &str,
    out: &PathBuf,
    zone: usize,
    plane: Option<&str>,
    view: Option<&str>,
    vmin: Option<f64>,
    vmax: Option<f64>,
    width: u32,
    height: u32,
    style: PlotStyle,
    nondim: &NondimArgs,
    do_mesh: bool,
    mesh_color: Option<&str>,
    do_pathlines: bool,
    pathlines_out: Option<&PathBuf>,
    pl_args: &PathlineArgs,
    coloring: ContourColoring,
    levels: u32,
    colormap: ColorMapName,
    legend: bool,
    show_labels: bool,
    legend_font_px: f32,
    legend_align: LegendAlign,
    margin: u32,
    pan_x: f64,
    pan_y: f64,
    do_vectors: bool,
    do_edges: bool,
    do_scatter: bool,
    do_isolines: bool,
    overlay_skip: usize,
    json: bool,
) -> flowplot::Result<()> {
    // Avoid human-only suggestion prints when agents request JSON.
    let mut nondim_args = nondim.clone();
    if json {
        nondim_args.suggest = false;
    }
    let ds = maybe_nondim(load_path(path)?, &nondim_args, nondim_args.suggest)?;
    let plane = parse_plane(plane)?;
    let view = parse_view(view)?;
    let (width, height) = resolve_raster_size(style, width, height);
    let mut img = render_contour(
        &ds,
        &ContourOptions {
            variable: variable.to_string(),
            zone,
            plane,
            view,
            vmin,
            vmax,
            width,
            height,
            style,
            // Annotate after mesh/pathline overlays so the legend stays on top.
            annotate: false,
            path_hint: Some(path.display().to_string()),
            coloring,
            nlevels: levels,
            colormap,
            legend,
            show_labels,
            legend_font_px,
            legend_align,
            margin,
            pan_x,
            pan_y,
        },
    )?;

    if do_mesh {
        let mesh_grid = extract_mesh_grid_view(&ds, zone, plane, view)?;
        let line = default_mesh_line_rgba(style, mesh_color)?;
        // Match stretched contour mapping so wires sit on the color field.
        overlay_mesh(&mut img, &mesh_grid, false, 0, line);
    }

    if do_edges {
        let mesh_grid = extract_mesh_grid_view(&ds, zone, plane, view)?;
        flowplot::overlay_boundary_edges(&mut img, &mesh_grid, None);
    }
    if do_scatter {
        let mesh_grid = extract_mesh_grid_view(&ds, zone, plane, view)?;
        flowplot::overlay_scatter(&mut img, &mesh_grid, overlay_skip.max(1), None);
    }
    if do_isolines {
        flowplot::overlay_isolines(&mut img, levels.max(2), None);
    }
    if do_vectors {
        let (gu, gv) = guess_velocity_vars(&ds)?;
        let u_name = pl_args.u_var.clone().unwrap_or(gu);
        let v_name = pl_args.v_var.clone().unwrap_or(gv);
        let u_idx = find_var(&ds, &u_name)?;
        let v_idx = find_var(&ds, &v_name)?;
        let u_grid = extract_scalar_grid_view(&ds, zone, u_idx, plane, view)?;
        let v_grid = extract_scalar_grid_view(&ds, zone, v_idx, plane, view)?;
        flowplot::overlay_vectors(&mut img, &u_grid, &v_grid, overlay_skip.max(1), None);
        if !json {
            println!("vectors: U={u_name} V={v_name} skip={}", overlay_skip.max(1));
        }
    }

    let mut n_traces = 0usize;
    let mut pathlines_out_written: Option<String> = None;
    if do_pathlines || pathlines_out.is_some() {
        let pl_opts = build_pathline_opts(&ds, zone, plane, view, pl_args)?;
        let lines = trace_pathlines(&ds, &pl_opts)?;
        n_traces = lines.len();
        if !json {
            println!("pathlines: {} traces", lines.len());
        }
        if do_pathlines {
            overlay_pathlines_styled(&mut img, &lines, style);
        }
        if let Some(pout) = pathlines_out {
            let separate = render_pathlines_png_styled(&ds, &pl_opts, width, height, style)?;
            write_png(pout, &separate)?;
            pathlines_out_written = Some(pout.display().to_string());
            if !json {
                println!("wrote pathlines {}", pout.display());
            }
        }
    }

    // VisIt-style legend on top of mesh/pathline overlays.
    {
        let mut info = AnnotationInfo::from_dataset(
            &ds,
            zone,
            Some(variable),
            Some(&path.display().to_string()),
        );
        info.colorbar = legend;
        info.vmin = Some(img.vmin);
        info.vmax = Some(img.vmax);
        info.style = style;
        info.colormap = colormap;
        info.nlevels = if coloring == ContourColoring::Banded || show_labels {
            Some(levels.max(2))
        } else {
            None
        };
        info.show_labels = show_labels;
        info.legend_font_px = legend_font_px;
        info.legend_align = legend_align;
        info.view = Some(view.label());
        info.view_orient = view;
        info.plane = Some(match plane {
            PlaneSpec::Auto => "auto".into(),
            PlaneSpec::Index { axis, index } => format!("{axis}={index}"),
        });
        draw_annotations(&mut img, &info);
    }

    write_png(out, &img)?;
    if json {
        println!(
            "{{\"status\":\"ok\",\"command\":\"contour\",\"path\":{},\"var\":{},\"zone\":{},\"out\":{},\"vmin\":{},\"vmax\":{},\"mesh\":{},\"coloring\":{},\"levels\":{},\"colormap\":{},\"legend\":{},\"labels\":{},\"pathline_traces\":{},\"pathlines_out\":{}}}",
            json_string(&path.display().to_string()),
            json_string(variable),
            zone,
            json_string(&out.display().to_string()),
            json_f64(img.vmin),
            json_f64(img.vmax),
            if do_mesh { "true" } else { "false" },
            json_string(match coloring {
                ContourColoring::Continuous => "continuous",
                ContourColoring::Banded => "banded",
            }),
            levels,
            json_string(colormap.as_cli()),
            if legend { "true" } else { "false" },
            if show_labels { "true" } else { "false" },
            n_traces,
            pathlines_out_written
                .as_ref()
                .map(|p| json_string(p))
                .unwrap_or_else(|| "null".into())
        );
    } else {
        println!(
            "wrote contour {} (var={}, style={:?}, coloring={:?}, levels={}, mesh={}, range=[{}, {}])",
            out.display(),
            variable,
            style,
            coloring,
            levels,
            do_mesh,
            img.vmin,
            img.vmax
        );
    }
    Ok(())
}

fn run_probe(
    path: &PathBuf,
    variable: Option<&str>,
    zone: usize,
    plane: Option<&str>,
    view: Option<&str>,
    x: Option<f64>,
    y: Option<f64>,
    px: Option<i32>,
    py: Option<i32>,
    width: u32,
    height: u32,
    out: Option<&PathBuf>,
    style: PlotStyle,
    nondim: &NondimArgs,
    json: bool,
) -> flowplot::Result<()> {
    let mut nondim_args = nondim.clone();
    if json {
        nondim_args.suggest = false;
    }
    let ds = maybe_nondim(load_path(path)?, &nondim_args, nondim_args.suggest)?;
    let plane = parse_plane(plane)?;
    let view = parse_view(view)?;
    // Use the first variable (or requested) only to establish the pixel↔world map.
    let map_name = variable
        .map(|s| s.to_string())
        .or_else(|| ds.variables.first().cloned())
        .ok_or_else(|| flowplot::Error::plot("probe: dataset has no variables"))?;
    let map_idx = find_var(&ds, &map_name)?;
    let map_grid = extract_scalar_grid_view(&ds, zone, map_idx, plane, view)?;
    let legend_reserve = ((width as f64) * 0.14).round().clamp(56.0, 110.0) as u32;
    let plot_w = width.saturating_sub(legend_reserve).max(32);
    let margin = 4u32;
    let (wx, wy, pix_x, pix_y) = if let (Some(px), Some(py)) = (px, py) {
        let img = flowplot::ContourImage {
            width,
            height,
            rgba: vec![0; (width as usize) * (height as usize) * 4],
            vmin: 0.0,
            vmax: 1.0,
            grid: map_grid.clone(),
            pan_x: 0.0,
            pan_y: 0.0,
        };
        let (wx, wy) = pixel_to_world(&img, px, py, plot_w, margin);
        (wx, wy, px, py)
    } else if let (Some(wx), Some(wy)) = (x, y) {
        let img = flowplot::ContourImage {
            width,
            height,
            rgba: vec![0; (width as usize) * (height as usize) * 4],
            vmin: 0.0,
            vmax: 1.0,
            grid: map_grid.clone(),
            pan_x: 0.0,
            pan_y: 0.0,
        };
        let (px, py) = flowplot::world_to_pixel_plot(&img, wx, wy, plot_w, margin);
        (wx, wy, px, py)
    } else {
        return Err(flowplot::Error::plot(
            "probe requires --x/--y or --px/--py",
        ));
    };

    // Sample every variable at (wx, wy); optional --var still reported as primary.
    let mut values: Vec<(String, Option<f64>)> = Vec::with_capacity(ds.variables.len());
    for (vi, name) in ds.variables.iter().enumerate() {
        let g = extract_scalar_grid_view(&ds, zone, vi, plane, view)?;
        values.push((name.clone(), g.sample(wx, wy)));
    }
    if values.iter().all(|(_, v)| v.is_none()) {
        return Err(flowplot::Error::plot(format!(
            "probe ({wx}, {wy}) is outside the active plane"
        )));
    }

    let primary_name = variable.unwrap_or(map_name.as_str());
    let primary_val = values
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(primary_name))
        .and_then(|(_, v)| *v);
    let label = match primary_val {
        Some(v) => format!("{primary_name}={v:.6}"),
        None => format!("{primary_name}=n/a"),
    };

    let zone_name = ds
        .zones
        .get(zone)
        .map(|z| z.name.clone())
        .unwrap_or_else(|| format!("zone_{zone}"));
    let (xmin, xmax, ymin, ymax) = map_grid.bounds();

    if let Some(out_path) = out {
        let stamp_var = primary_name.to_string();
        let mut img = render_contour(
            &ds,
            &ContourOptions {
                variable: stamp_var,
                zone,
                plane,
                view,
                width,
                height,
                style,
                annotate: true,
                path_hint: Some(path.display().to_string()),
                ..Default::default()
            },
        )?;
        draw_probe_label(&mut img, pix_x, pix_y, &label);
        write_png(out_path, &img)?;
    }

    if json {
        let mut vals_json = String::from("{");
        for (i, (name, val)) in values.iter().enumerate() {
            if i > 0 {
                vals_json.push(',');
            }
            vals_json.push_str(&json_string(name));
            vals_json.push(':');
            match val {
                Some(v) => vals_json.push_str(&json_f64(*v)),
                None => vals_json.push_str("null"),
            }
        }
        vals_json.push('}');
        println!(
            "{{\"status\":\"ok\",\"command\":\"probe\",\"path\":{},\"var\":{},\"zone\":{},\"zone_name\":{},\"x\":{},\"y\":{},\"px\":{},\"py\":{},\"value\":{},\"label\":{},\"values\":{},\"bounds\":{{\"xmin\":{},\"xmax\":{},\"ymin\":{},\"ymax\":{}}},\"ni\":{},\"nj\":{}}}",
            json_string(&path.display().to_string()),
            json_string(primary_name),
            zone,
            json_string(&zone_name),
            json_f64(wx),
            json_f64(wy),
            pix_x,
            pix_y,
            primary_val
                .map(json_f64)
                .unwrap_or_else(|| "null".into()),
            json_string(&label),
            vals_json,
            json_f64(xmin),
            json_f64(xmax),
            json_f64(ymin),
            json_f64(ymax),
            map_grid.ni,
            map_grid.nj
        );
    } else {
        println!(
            "probe @ ({wx:.6}, {wy:.6}) px=({pix_x},{pix_y}) zone={zone_name}"
        );
        for (name, val) in &values {
            match val {
                Some(v) => println!("  {name} = {v:.8}"),
                None => println!("  {name} = n/a"),
            }
        }
    }
    Ok(())
}

fn field_minmax_summaries(ds: &Dataset, json: bool) -> Vec<String> {
    let mut field_summaries = Vec::new();
    for (i, name) in ds.variables.iter().enumerate() {
        let mut vmin = f64::INFINITY;
        let mut vmax = f64::NEG_INFINITY;
        for zone in &ds.zones {
            if let Some(f) = zone.fields.get(i) {
                for &v in &f.values {
                    if v.is_finite() {
                        vmin = vmin.min(v);
                        vmax = vmax.max(v);
                    }
                }
            }
        }
        if vmin.is_finite() {
            if json {
                field_summaries.push(format!(
                    "{{\"variable\":{},\"min\":{},\"max\":{}}}",
                    json_string(name),
                    json_f64(vmin),
                    json_f64(vmax)
                ));
            } else {
                field_summaries.push(format!("  {name}: min={vmin} max={vmax}"));
            }
        }
    }
    field_summaries
}

fn run_nondim(path: &PathBuf, nondim: &NondimArgs, summary: bool, json: bool) -> flowplot::Result<()> {
    let ds = load_path(path)?;
    let suggestions = suggest_scales(&ds.variables);
    if !json {
        print_summary(path, &ds);
        println!("{}", format_suggestions(&suggestions));
    }
    let scales = scales_from_args(nondim);
    if scales.u_ref.is_none()
        && scales.lx.is_none()
        && scales.ly.is_none()
        && scales.lz.is_none()
        && scales.t_ref.is_none()
    {
        let fields = if summary {
            field_minmax_summaries(&ds, json)
        } else {
            Vec::new()
        };
        if json {
            println!(
                "{{\"status\":\"ok\",\"command\":\"nondim\",\"path\":{},\"applied\":false,\"message\":{},\"nondim_suggestions\":{},\"fields\":[{}]}}",
                json_string(&path.display().to_string()),
                json_string("no scales passed; suggestions only (never invent physics)"),
                suggestions_json(&suggestions),
                fields.join(",")
            );
        } else {
            println!("no scales passed; suggestions only (never invent physics)");
            for line in &fields {
                println!("{line}");
            }
        }
        return Ok(());
    }
    let out = apply_nondim(&ds, &scales)?;
    let field_summaries = if summary {
        if !json {
            println!("after nondim:");
        }
        field_minmax_summaries(&out, json)
    } else {
        Vec::new()
    };
    if !json && summary {
        for line in &field_summaries {
            println!("{line}");
        }
    }
    if json {
        println!(
            "{{\"status\":\"ok\",\"command\":\"nondim\",\"path\":{},\"applied\":true,\"nondim_suggestions\":{},\"fields\":[{}]}}",
            json_string(&path.display().to_string()),
            suggestions_json(&suggestions),
            field_summaries.join(",")
        );
    }
    Ok(())
}

fn run_pathlines_cmd(
    path: &PathBuf,
    out: &PathBuf,
    zone: usize,
    plane: Option<&str>,
    view: Option<&str>,
    width: u32,
    height: u32,
    style: PlotStyle,
    nondim: &NondimArgs,
    pl_args: &PathlineArgs,
    contour_var: Option<&str>,
    vmin: Option<f64>,
    vmax: Option<f64>,
    json: bool,
) -> flowplot::Result<()> {
    let mut nondim_args = nondim.clone();
    if json {
        nondim_args.suggest = false;
    }
    let ds = maybe_nondim(load_path(path)?, &nondim_args, nondim_args.suggest)?;
    let plane = parse_plane(plane)?;
    let view = parse_view(view)?;
    let (width, height) = resolve_raster_size(style, width, height);
    let pl_opts = build_pathline_opts(&ds, zone, plane, view, pl_args)?;
    let lines = trace_pathlines(&ds, &pl_opts)?;
    if !json {
        println!("pathlines: {} traces", lines.len());
    }

    let img = if let Some(var) = contour_var {
        let mut img = render_contour(
            &ds,
            &ContourOptions {
                variable: var.to_string(),
                zone,
                plane,
                view,
                vmin,
                vmax,
                width,
                height,
                style,
                annotate: false,
                path_hint: Some(path.display().to_string()),
                ..Default::default()
            },
        )?;
        overlay_pathlines_styled(&mut img, &lines, style);
        let mut info = AnnotationInfo::from_dataset(
            &ds,
            zone,
            Some(var),
            Some(&path.display().to_string()),
        );
        info.colorbar = true;
        info.vmin = Some(img.vmin);
        info.vmax = Some(img.vmax);
        info.style = style;
        info.view = Some(view.label());
        info.view_orient = view;
        draw_annotations(&mut img, &info);
        img
    } else {
        render_pathlines_png_styled(&ds, &pl_opts, width, height, style)?
    };
    write_png(out, &img)?;
    if json {
        println!(
            "{{\"status\":\"ok\",\"command\":\"pathlines\",\"path\":{},\"out\":{},\"zone\":{},\"traces\":{},\"u_var\":{},\"v_var\":{},\"contour_var\":{}}}",
            json_string(&path.display().to_string()),
            json_string(&out.display().to_string()),
            zone,
            lines.len(),
            json_string(&pl_opts.u_var),
            json_string(&pl_opts.v_var),
            contour_var
                .map(json_string)
                .unwrap_or_else(|| "null".into())
        );
    } else {
        println!("wrote pathlines {} (style={:?})", out.display(), style);
    }
    Ok(())
}
