//! Probe for system libcgns so CGNS loading works when headers/libs are installed.
//!
//! Enable with Cargo feature `cgns`, or set `FLOWPLOT_FORCE_CGNS=1`.
//! When found, emits `cargo:rustc-cfg=has_libcgns` and links `-lcgns`.

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(has_libcgns)");
    println!("cargo:rerun-if-env-changed=CGNS_DIR");
    println!("cargo:rerun-if-env-changed=FLOWPLOT_FORCE_CGNS");
    println!("cargo:rerun-if-changed=build.rs");

    let feature_cgns = env::var_os("CARGO_FEATURE_CGNS").is_some();
    let force = env::var_os("FLOWPLOT_FORCE_CGNS").is_some();
    if !feature_cgns && !force {
        // Still auto-enable when a system install is clearly present so `flowplot load`
        // works out of the box on developer machines with libcgns-dev.
        if !system_cgns_present() {
            return;
        }
    }

    if let Some(dir) = env::var_os("CGNS_DIR") {
        let root = PathBuf::from(dir);
        println!("cargo:rustc-link-search=native={}/lib", root.display());
        println!("cargo:rustc-link-search=native={}/lib64", root.display());
        println!("cargo:rustc-cfg=has_libcgns");
        println!("cargo:rustc-link-lib=cgns");
        return;
    }

    if system_cgns_present() || feature_cgns || force {
        println!("cargo:rustc-cfg=has_libcgns");
        println!("cargo:rustc-link-lib=cgns");
    }
}

fn system_cgns_present() -> bool {
    [
        "/usr/include/cgnslib.h",
        "/usr/local/include/cgnslib.h",
        "/usr/include/cgns/cgnslib.h",
    ]
    .iter()
    .any(|p| PathBuf::from(p).is_file())
}
