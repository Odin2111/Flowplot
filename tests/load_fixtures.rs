use std::path::PathBuf;

use flowplot::load_path;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn load_tecplot_ascii_fixture() {
    let ds = load_path(&fixture("simple.dat")).expect("Tecplot ASCII fixture");
    assert_eq!(ds.zone_count(), 1);
    assert_eq!(ds.variables, vec!["X", "Y", "P"]);
    assert_eq!(ds.zones[0].node_count(), 4);
    assert_eq!(ds.zones[0].cell_count(), 1);
    assert_eq!(ds.zones[0].fields[2].values, vec![1.0, 2.0, 3.0, 4.0]);
}

#[test]
fn load_vtk_ascii_fixture() {
    let ds = load_path(&fixture("simple.vtk")).expect("VTK ASCII fixture");
    assert_eq!(ds.zone_count(), 1);
    assert!(ds.variables.iter().any(|v| v == "X"));
    assert!(ds.variables.iter().any(|v| v == "P"));
    assert_eq!(ds.zones[0].node_count(), 4);
    let p_idx = ds.variables.iter().position(|v| v == "P").unwrap();
    assert_eq!(ds.zones[0].fields[p_idx].values, vec![1.0, 2.0, 3.0, 4.0]);
}
