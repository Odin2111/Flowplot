# flowplot

Rust CFD post-processing library with a Tecplot-like data model:

**Dataset → Zone → named Variables**, with ordered (I / IJ / IJK) or finite-element connectivity, and nodal or cell-centered values.

This slice adds **scalar contour PNGs**, **reference-scale nondimensionalization** (CLI/config), and **pathlines** (streamlines of the velocity field). A first **GUI shell** (layout only; CLI-backed) lives under `gui/`.

## Build

```bash
cargo build
cargo test
cargo run --bin flowplot -- tests/fixtures/simple.dat
cargo run --bin flowplot -- contour tests/fixtures/flow.dat --var P -o /tmp/p.png
```

Requires a recent stable Rust toolchain (`edition = "2021"`).

## GUI (layout shell)

```bash
cargo build
python3 gui/flowplot_gui.py
```

Menus: File, Plot, Style, Agents, Help. Tool strip: Load, Contour, Nondim, Pathlines, Export PNG. Right column: nondim scales (`U_ref`, `Lx`, `Ly`, `Lz`, `T_ref`). See [docs/gui-layout.md](docs/gui-layout.md).

## CLI

```bash
# Summary + name-based nondim suggestions
flowplot <path>
flowplot info <path>

# Contour any scalar to PNG
flowplot contour <path> --var P -o out.png [--plane k=0] [--vmin 0] [--vmax 1] \
  [--width 512] [--height 512] [--zone 0] [--style default|jfm] \
  [--u-ref U] [--lx Lx] [--ly Ly] [--lz Lz] [--t-ref T] \
  [--pathlines] [--pathlines-out lines.png] [--seed x,y] [--u-var U] [--v-var V] [--dt 0.02]

# Nondim suggestions / apply scales (no invented physics)
flowplot nondim <path> [--u-ref U] [--lx Lx] [--ly Ly] [--lz Lz] [--t-ref T] \
  [--x-var X] [--y-var Y] [--z-var Z]

# Pathlines (particle traces / streamlines)
flowplot pathlines <path> -o lines.png [--var P] [--seed x,y] [--u-var U] [--v-var V] \
  [--style default|jfm]
```

`--pathlines` overlays traces on the contour PNG; `--pathlines-out` writes a second PNG.
`--style jfm` uses a restrained coolwarm map, white exterior, black frame, and (if size left at 512×512) a single-column 700×525 raster.

## Format support

| Format | Extensions | Status |
|--------|------------|--------|
| Tecplot ASCII | `.dat`, `.tp` | **Working** (minimal: `TITLE` / `VARIABLES` / ordered or FE `ZONE`) |
| VTK legacy ASCII | `.vtk` | **Working** (structured grid + unstructured grid subset) |
| VTK XML | `.vtu`, `.vtp`, … | **Stub** — clear `Unimplemented` error |
| CGNS | `.cgns` | **Stub** — documents FFI via system `libcgns` / future `cgns-sys`; optional feature `cgns` |
| Fluent case/data | `.cas`, `.dat` | **Stub** — documents public section layout; binary returns `Unimplemented` |

**Not in scope (no reverse engineering):** proprietary Tecplot binary (`.plt`) and Fluent binary case/data decoding. Public format specifications / ASCII layouts only.

## Crate layout

- `src/dataset.rs` — data model
- `src/readers/` — `Reader` trait + Tecplot / VTK / CGNS / Fluent modules
- `src/contour.rs` — scalar contour → PNG
- `src/nondim.rs` — reference scales + name suggestions
- `src/pathlines.rs` — velocity-field particle traces
- `src/field.rs` — ordered-plane extract + bilinear sample
- `src/bin/flowplot.rs` — CLI
- `gui/` — first tkinter layout shell + SVG wordmark
- `tests/fixtures/` — synthetic Tecplot / VTK ASCII (incl. `flow.dat` for contour/pathlines)

## License

MIT OR Apache-2.0
