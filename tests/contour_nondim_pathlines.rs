use std::path::PathBuf;

use flowplot::contour::{render_contour, write_png, ContourOptions};
use flowplot::field::PlaneSpec;
use flowplot::mesh::{render_mesh, MeshOptions};
use flowplot::nondim::{apply_nondim, suggest_scales, NondimScales, ScaleKind};
use flowplot::pathlines::{overlay_pathlines, trace_pathlines, PathlineOptions};
use flowplot::load_path;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn contour_png_from_flow_fixture() {
    let ds = load_path(&fixture("flow.dat")).expect("flow.dat");
    let img = render_contour(
        &ds,
        &ContourOptions {
            variable: "P".into(),
            plane: PlaneSpec::Auto,
            vmin: Some(0.0),
            vmax: Some(2.0),
            width: 64,
            height: 64,
            ..Default::default()
        },
    )
    .expect("contour");
    assert_eq!(img.width, 64);
    assert_eq!(img.rgba.len(), 64 * 64 * 4);

    let out = std::env::temp_dir().join("flowplot-test-contour.png");
    write_png(&out, &img).expect("png");
    assert!(out.metadata().unwrap().len() > 64);
    let _ = std::fs::remove_file(&out);
}

#[test]
fn nondim_suggestions_and_apply_on_flow_fixture() {
    let ds = load_path(&fixture("flow.dat")).expect("flow.dat");
    let suggestions = suggest_scales(&ds.variables);
    assert!(suggestions.iter().any(|s| s.kind == ScaleKind::Velocity));
    assert!(suggestions.iter().any(|s| s.kind == ScaleKind::LengthX));
    assert!(suggestions.iter().any(|s| s.kind == ScaleKind::Temperature));

    let out = apply_nondim(
        &ds,
        &NondimScales {
            u_ref: Some(1.0),
            lx: Some(1.0),
            ly: Some(1.0),
            t_ref: Some(300.0),
            ..Default::default()
        },
    )
    .expect("nondim");
    let t_idx = out.variables.iter().position(|v| v == "T").unwrap();
    let t0 = out.zones[0].fields[t_idx].values[0];
    assert!((t0 - 1.0).abs() < 1e-12);
}

#[test]
fn tiny_pathline_on_flow_fixture_and_overlay() {
    let ds = load_path(&fixture("flow.dat")).expect("flow.dat");
    let lines = trace_pathlines(
        &ds,
        &PathlineOptions {
            seeds: vec![[0.1, 0.5]],
            max_steps: 30,
            step_size: 0.05,
            bidirectional: false,
            ..Default::default()
        },
    )
    .expect("pathlines");
    assert_eq!(lines.len(), 1);
    assert!(lines[0].points.len() > 3);
    assert!(lines[0].points.last().unwrap()[0] > lines[0].points[0][0]);

    let mut img = render_contour(
        &ds,
        &ContourOptions {
            variable: "P".into(),
            width: 48,
            height: 48,
            ..Default::default()
        },
    )
    .unwrap();
    overlay_pathlines(&mut img, &lines);
    let out = std::env::temp_dir().join("flowplot-test-pathlines.png");
    write_png(&out, &img).unwrap();
    assert!(out.metadata().unwrap().len() > 48);
    let _ = std::fs::remove_file(&out);
}

#[test]
fn mesh_png_from_flow_fixture_equal_aspect() {
    let ds = load_path(&fixture("flow.dat")).expect("flow.dat");
    let img = render_mesh(
        &ds,
        &MeshOptions {
            width: 160,
            height: 120,
            ..Default::default()
        },
    )
    .expect("mesh");
    assert_eq!(img.width, 160);
    assert_eq!(img.height, 120);
    let dark = img
        .rgba
        .chunks_exact(4)
        .filter(|p| p[0] < 200)
        .count();
    assert!(dark > 20, "expected wireframe strokes, got {dark}");
    let out = std::env::temp_dir().join("flowplot-test-mesh.png");
    write_png(&out, &img).unwrap();
    assert!(out.metadata().unwrap().len() > 64);
    let _ = std::fs::remove_file(&out);
}

#[test]
fn existing_ascii_loaders_still_work() {
    let tec = load_path(&fixture("simple.dat")).unwrap();
    assert_eq!(tec.variables, vec!["X", "Y", "P"]);
    let vtk = load_path(&fixture("simple.vtk")).unwrap();
    assert!(vtk.variables.iter().any(|v| v == "P"));
}
