//! CLI: load CFD files; summarize; contour PNG; nondim scales; pathlines.
//! Agent contract: every subcommand accepts `--json` and emits a stable JSON object.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use flowplot::contour::{render_contour, write_png, ContourOptions};
use flowplot::field::{extract_scalar_grid, find_var, PlaneSpec};
use flowplot::nondim::{
    apply_nondim, format_suggestions, suggest_scales, NondimScales, NondimSuggestion,
};
use flowplot::pathlines::{
    default_seeds, guess_velocity_vars, overlay_pathlines, render_pathlines_png, trace_pathlines,
    PathlineOptions,
};
use flowplot::{load_path, Dataset};

#[derive(Parser, Debug)]
#[command(
    name = "flowplot",
    about = "CFD post-processor: load Tecplot/VTK ASCII, contour, nondim, pathlines",
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
    let json = cli.json;
    let result = match cli.command {
        None => {
            let Some(path) = cli.path else {
                eprintln!(
                    "usage: flowplot [--json] <path> | flowplot [--json] <load|info|contour|nondim|pathlines> ..."
                );
                return ExitCode::from(2);
            };
            run_info(&path, json)
        }
        Some(Commands::Info { path }) | Some(Commands::Load { path }) => run_info(&path, json),
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
            &nondim,
            pathlines,
            pathlines_out.as_ref(),
            &pl_args,
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
            width,
            height,
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
            &nondim,
            &pl_args,
            contour_var.as_deref(),
            vmin,
            vmax,
            json,
        ),
    };

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

fn dataset_load_json(path: &PathBuf, ds: &Dataset, suggestions: &[NondimSuggestion]) -> String {
    let mut zones = Vec::new();
    for (i, zone) in ds.zones.iter().enumerate() {
        zones.push(format!(
            "{{\"index\":{i},\"name\":{},\"nodes\":{},\"cells\":{}}}",
            json_string(&zone.name),
            zone.node_count(),
            zone.cell_count()
        ));
    }
    format!(
        "{{\"status\":\"ok\",\"command\":\"load\",\"path\":{},\"title\":{},\"variables\":{},\"zones\":[{}],\"nondim_suggestions\":{}}}",
        json_string(&path.display().to_string()),
        ds.title
            .as_ref()
            .map(|t| json_string(t))
            .unwrap_or_else(|| "null".into()),
        json_string_list(&ds.variables),
        zones.join(","),
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
    nondim: &NondimArgs,
    do_pathlines: bool,
    pathlines_out: Option<&PathBuf>,
    pl_args: &PathlineArgs,
    json: bool,
) -> flowplot::Result<()> {
    // Avoid human-only suggestion prints when agents request JSON.
    let mut nondim_args = nondim.clone();
    if json {
        nondim_args.suggest = false;
    }
    let ds = maybe_nondim(load_path(path)?, &nondim_args, nondim_args.suggest)?;
    let plane = parse_plane(plane)?;
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
        },
    )?;

    let mut n_traces = 0usize;
    let mut pathlines_out_written: Option<String> = None;
    if do_pathlines || pathlines_out.is_some() {
        let pl_opts = build_pathline_opts(&ds, zone, plane, pl_args)?;
        let lines = trace_pathlines(&ds, &pl_opts)?;
        n_traces = lines.len();
        if !json {
            println!("pathlines: {} traces", lines.len());
        }
        if do_pathlines {
            overlay_pathlines(&mut img, &lines);
        }
        if let Some(pout) = pathlines_out {
            let separate = render_pathlines_png(&ds, &pl_opts, width, height)?;
            write_png(pout, &separate)?;
            pathlines_out_written = Some(pout.display().to_string());
            if !json {
                println!("wrote pathlines {}", pout.display());
            }
        }
    }

    write_png(out, &img)?;
    if json {
        println!(
            "{{\"status\":\"ok\",\"command\":\"contour\",\"path\":{},\"var\":{},\"zone\":{},\"out\":{},\"vmin\":{},\"vmax\":{},\"pathline_traces\":{},\"pathlines_out\":{}}}",
            json_string(&path.display().to_string()),
            json_string(variable),
            zone,
            json_string(&out.display().to_string()),
            json_f64(img.vmin),
            json_f64(img.vmax),
            n_traces,
            pathlines_out_written
                .as_ref()
                .map(|p| json_string(p))
                .unwrap_or_else(|| "null".into())
        );
    } else {
        println!(
            "wrote contour {} (var={}, range=[{}, {}])",
            out.display(),
            variable,
            img.vmin,
            img.vmax
        );
    }
    Ok(())
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
        if json {
            println!(
                "{{\"status\":\"ok\",\"command\":\"nondim\",\"path\":{},\"applied\":false,\"message\":{},\"nondim_suggestions\":{}}}",
                json_string(&path.display().to_string()),
                json_string("no scales passed; suggestions only (never invent physics)"),
                suggestions_json(&suggestions)
            );
        } else {
            println!("no scales passed; suggestions only (never invent physics)");
        }
        return Ok(());
    }
    let out = apply_nondim(&ds, &scales)?;
    let mut field_summaries = Vec::new();
    if summary {
        if !json {
            println!("after nondim:");
        }
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
                if json {
                    field_summaries.push(format!(
                        "{{\"variable\":{},\"min\":{},\"max\":{}}}",
                        json_string(name),
                        json_f64(vmin),
                        json_f64(vmax)
                    ));
                } else {
                    println!("  {name}: min={vmin} max={vmax}");
                }
            }
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
    width: u32,
    height: u32,
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
    let pl_opts = build_pathline_opts(&ds, zone, plane, pl_args)?;
    let lines = trace_pathlines(&ds, &pl_opts)?;
    if !json {
        println!("pathlines: {} traces", lines.len());
    }

    let mut img = if let Some(var) = contour_var {
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
            },
        )?;
        overlay_pathlines(&mut img, &lines);
        img
    } else {
        render_pathlines_png(&ds, &pl_opts, width, height)?
    };
    let _ = &mut img;
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
        println!("wrote pathlines {}", out.display());
    }
    Ok(())
}
