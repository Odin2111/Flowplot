//! CLI: load CFD files; summarize; contour PNG; nondim scales; pathlines.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use flowplot::contour::{render_contour, write_png, ContourOptions, PlotStyle};
use flowplot::field::{extract_scalar_grid, find_var, PlaneSpec};
use flowplot::nondim::{
    apply_nondim, format_suggestions, suggest_scales, NondimScales,
};
use flowplot::pathlines::{
    default_seeds, guess_velocity_vars, overlay_pathlines_styled, render_pathlines_png_styled,
    trace_pathlines, PathlineOptions,
};
use flowplot::{load_path, Dataset};

#[derive(Parser, Debug)]
#[command(
    name = "flowplot",
    about = "CFD post-processor: load Tecplot/VTK ASCII, contour, nondim, pathlines",
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to a CFD file (summary mode when no subcommand).
    path: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Print zone / variable / size summary (same as bare `flowplot <path>`).
    Info {
        path: PathBuf,
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
        /// Also trace pathlines and overlay them on the contour PNG.
        #[arg(long)]
        pathlines: bool,
        /// Write pathlines to a second PNG instead of (or as well as) overlay.
        #[arg(long)]
        pathlines_out: Option<PathBuf>,
        #[command(flatten)]
        pathline: PathlineArgs,
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
    /// Integration step size in coordinate units.
    #[arg(long = "dt", default_value_t = 0.02)]
    dt: f64,
    #[arg(long = "max-steps", default_value_t = 200)]
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
    let result = match cli.command {
        None => {
            let Some(path) = cli.path else {
                eprintln!("usage: flowplot <path> | flowplot <info|contour|nondim|pathlines> ...");
                return ExitCode::from(2);
            };
            run_info(&path)
        }
        Some(Commands::Info { path }) => run_info(&path),
        Some(Commands::Contour {
            path,
            variable,
            out,
            zone,
            plane,
            vmin,
            vmax,
            width,
            height,
            style,
            nondim,
            pathlines,
            pathlines_out,
            pathline: pl_args,
        }) => run_contour(
            &path,
            &variable,
            &out,
            zone,
            plane.as_deref(),
            vmin,
            vmax,
            width,
            height,
            style,
            &nondim,
            pathlines,
            pathlines_out.as_ref(),
            &pl_args,
        ),
        Some(Commands::Nondim {
            path,
            nondim,
            summary,
        }) => run_nondim(&path, &nondim, summary),
        Some(Commands::Pathlines {
            path,
            out,
            zone,
            plane,
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
            width,
            height,
            style,
            &nondim,
            &pl_args,
            contour_var.as_deref(),
            vmin,
            vmax,
        ),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_info(path: &PathBuf) -> flowplot::Result<()> {
    let ds = load_path(path)?;
    print_summary(path, &ds);
    let suggestions = suggest_scales(&ds.variables);
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

fn build_pathline_opts(
    ds: &Dataset,
    zone: usize,
    plane: PlaneSpec,
    args: &PathlineArgs,
) -> flowplot::Result<PathlineOptions> {
    let (gu, gv) = guess_velocity_vars(ds).unwrap_or_else(|_| ("U".into(), "V".into()));
    let u_var = args.u_var.clone().unwrap_or(gu);
    let v_var = args.v_var.clone().unwrap_or(gv);
    let mut seeds = Vec::new();
    for s in &args.seeds {
        seeds.push(parse_seed(s)?);
    }
    if seeds.is_empty() {
        let u_idx = find_var(ds, &u_var)?;
        let grid = extract_scalar_grid(ds, zone, u_idx, plane)?;
        seeds = default_seeds(&grid, args.seed_count);
    }
    Ok(PathlineOptions {
        u_var,
        v_var,
        w_var: args.w_var.clone(),
        zone,
        plane,
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

/// When `--style jfm` is set and the user left the 512×512 CLI defaults, use
/// the single-column JFM raster size.
fn resolve_raster_size(style: PlotStyle, width: u32, height: u32) -> (u32, u32) {
    if style == PlotStyle::Jfm && width == 512 && height == 512 {
        (PlotStyle::JFM_WIDTH, PlotStyle::JFM_HEIGHT)
    } else {
        (width, height)
    }
}

fn run_contour(
    path: &PathBuf,
    variable: &str,
    out: &PathBuf,
    zone: usize,
    plane: Option<&str>,
    vmin: Option<f64>,
    vmax: Option<f64>,
    width: u32,
    height: u32,
    style: PlotStyle,
    nondim: &NondimArgs,
    do_pathlines: bool,
    pathlines_out: Option<&PathBuf>,
    pl_args: &PathlineArgs,
) -> flowplot::Result<()> {
    let ds = maybe_nondim(load_path(path)?, nondim, nondim.suggest)?;
    let plane = parse_plane(plane)?;
    let (width, height) = resolve_raster_size(style, width, height);
    let mut img = render_contour(
        &ds,
        &ContourOptions {
            variable: variable.to_string(),
            zone,
            plane,
            vmin,
            vmax,
            width,
            height,
            style,
        },
    )?;

    if do_pathlines || pathlines_out.is_some() {
        let pl_opts = build_pathline_opts(&ds, zone, plane, pl_args)?;
        let lines = trace_pathlines(&ds, &pl_opts)?;
        println!("pathlines: {} traces", lines.len());
        if do_pathlines {
            overlay_pathlines_styled(&mut img, &lines, style);
        }
        if let Some(pout) = pathlines_out {
            let separate = render_pathlines_png_styled(&ds, &pl_opts, width, height, style)?;
            write_png(pout, &separate)?;
            println!("wrote pathlines {}", pout.display());
        }
    }

    write_png(out, &img)?;
    println!(
        "wrote contour {} (var={}, style={:?}, range=[{}, {}])",
        out.display(),
        variable,
        style,
        img.vmin,
        img.vmax
    );
    Ok(())
}

fn run_nondim(path: &PathBuf, nondim: &NondimArgs, summary: bool) -> flowplot::Result<()> {
    let ds = load_path(path)?;
    print_summary(path, &ds);
    println!("{}", format_suggestions(&suggest_scales(&ds.variables)));
    let scales = scales_from_args(nondim);
    if scales.u_ref.is_none()
        && scales.lx.is_none()
        && scales.ly.is_none()
        && scales.lz.is_none()
        && scales.t_ref.is_none()
    {
        println!("no scales passed; suggestions only (never invent physics)");
        return Ok(());
    }
    let out = apply_nondim(&ds, &scales)?;
    if summary {
        println!("after nondim:");
        for (i, name) in out.variables.iter().enumerate() {
            let mut vmin = f64::INFINITY;
            let mut vmax = f64::NEG_INFINITY;
            for zone in &out.zones {
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
                println!("  {name}: min={vmin} max={vmax}");
            }
        }
    }
    Ok(())
}

fn run_pathlines_cmd(
    path: &PathBuf,
    out: &PathBuf,
    zone: usize,
    plane: Option<&str>,
    width: u32,
    height: u32,
    style: PlotStyle,
    nondim: &NondimArgs,
    pl_args: &PathlineArgs,
    contour_var: Option<&str>,
    vmin: Option<f64>,
    vmax: Option<f64>,
) -> flowplot::Result<()> {
    let ds = maybe_nondim(load_path(path)?, nondim, nondim.suggest)?;
    let plane = parse_plane(plane)?;
    let (width, height) = resolve_raster_size(style, width, height);
    let pl_opts = build_pathline_opts(&ds, zone, plane, pl_args)?;
    let lines = trace_pathlines(&ds, &pl_opts)?;
    println!("pathlines: {} traces", lines.len());

    let img = if let Some(var) = contour_var {
        let mut img = render_contour(
            &ds,
            &ContourOptions {
                variable: var.to_string(),
                zone,
                plane,
                vmin,
                vmax,
                width,
                height,
                style,
            },
        )?;
        overlay_pathlines_styled(&mut img, &lines, style);
        img
    } else {
        render_pathlines_png_styled(&ds, &pl_opts, width, height, style)?
    };
    write_png(out, &img)?;
    println!("wrote pathlines {} (style={:?})", out.display(), style);
    Ok(())
}
