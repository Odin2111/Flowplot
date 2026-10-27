# fieldplot

Rust CFD post-processing library with a Tecplot-like data model:

**Dataset → Zone → named Variables**, with ordered (I / IJ / IJK) or finite-element connectivity, and nodal or cell-centered values.

The GUI comes later. This first slice is the core crate, format readers, and a small CLI.

## Build

```bash
cargo build
cargo test
cargo run --bin fieldplot -- tests/fixtures/simple.dat
```

Requires a recent stable Rust toolchain (`edition = "2021"`).

## CLI

```bash
fieldplot <path>
```

Detects format by extension, loads into a `Dataset`, and prints zone count, variable names, and per-zone node/cell counts.

## Format support

| Format | Extensions | Status |
|--------|------------|--------|
| Tecplot ASCII | `.dat`, `.tp` | **Working** (minimal: `TITLE` / `VARIABLES` / ordered or FE `ZONE`) |
| VTK legacy ASCII | `.vtk` | **Working** (structured grid + unstructured grid subset) |
| VTK XML | `.vtu`, `.vtp`, … | **Stub** — clear `Unimplemented` error |
| CGNS | `.cgns` | **Stub** — documents FFI via system `libcgns` / future `cgns-sys`; optional feature `cgns` |
| Fluent case/data | `.cas`, `.dat` | **Stub** — documents public section layout; binary returns `Unimplemented` |

**Not in scope (no reverse engineering):** proprietary Tecplot binary (`.plt`) and Fluent binary case/data decoding. Public format specifications / ASCII layouts only. Do not copy proprietary sample files from vendor installs.

## Crate layout

- `src/dataset.rs` — data model
- `src/readers/` — `Reader` trait + Tecplot / VTK / CGNS / Fluent modules
- `src/bin/fieldplot.rs` — CLI
- `tests/fixtures/` — tiny synthetic Tecplot ASCII and VTK ASCII files

## License

MIT OR Apache-2.0
