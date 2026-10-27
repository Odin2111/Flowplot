//! CLI: detect CFD file format by extension, load into a Dataset, print summary.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use fieldplot::load_path;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: fieldplot <path>");
        eprintln!("  Load a CFD mesh/field file and print zone / variable / size summary.");
        eprintln!("  Formats: .dat/.tp (Tecplot ASCII), .vtk (VTK ASCII), .cgns, .cas/.dat (Fluent stub)");
        return ExitCode::from(2);
    };

    let path = PathBuf::from(path);
    match load_path(&path) {
        Ok(ds) => {
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
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
