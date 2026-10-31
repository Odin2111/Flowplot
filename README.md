# flowplot

Rust CFD post-processing library with a zone/variable data model:

**Dataset → Zone → named Variables**, with ordered (I / IJ / IJK) or finite-element connectivity, and nodal or cell-centered values.

This slice adds **mesh (wireframe) PNGs**, **scalar contour PNGs**, **reference-scale nondimensionalization** (CLI/config), and **pathlines** (streamlines of the velocity field). The **GUI** Plot panel defaults to Mesh on, Contour optional (`gui/`).

## Build

```bash
cargo build
cargo test
cargo run --bin flowplot -- tests/fixtures/simple.dat
cargo run --bin flowplot -- mesh tests/fixtures/flow.dat -o /tmp/mesh.png
cargo run --bin flowplot -- contour tests/fixtures/flow.dat --var P -o /tmp/p.png
```

Requires a recent stable Rust toolchain (`edition = "2021"`).

## Windows installer

Cross-build a 64-bit MSI (MinGW + [wixl](https://wiki.gnome.org/msitools)):

```bash
./packaging/windows/build-msi.sh
# → packaging/windows/Flowplot-Setup.msi
```

Installs under `C:\Program Files\Flowplot\`, Start Menu shortcuts, optional PATH via `add-to-path.bat`. See `packaging/windows/README.txt`.

## GUI (Plot panel)

```bash
cargo build
python3 gui/flowplot_gui.py
```

**File → Open** runs `info`, fills the variable list, enables **Mesh**, leaves **Contour** off, and fills the Viewport (no letterbox waste). Turn on Contour (and pick e.g. `Pressure`) to overlay. Contour toolbar under the menu: **＋/－** levels, **Abc** labels, **✎ Probe** (click prints values on the plot), **Details…** → Contour & Multi-Coloring Details. **XYZ** buttons and viewport drag rotate; **Cut / Track** sets the plane. **Plot → Zone Style…** / **Plot → Contour…** for zone and contour details. See [docs/gui-layout.md](docs/gui-layout.md).

## CLI

```bash
# Summary + name-based nondim suggestions (agent entry: load --json)
flowplot load <path> [--json]
flowplot <path> [--json]          # same as load
flowplot info <path> [--json]

# Mesh / zone grid (equal aspect by default; --fill for viewport fill)
flowplot mesh <path> -o mesh.png [--zone 0] [--plane k=0] \
  [--view auto|+x|-x|+y|-y|+z|-z|az=35,el=55] \
  [--width 800] [--height 600] [--style default|jfm] [--fill] [--margin 4] [--json]

# Contour any scalar to PNG (banded/continuous, colormap, legend)
flowplot contour <path> --var P -o out.png [--plane k=0] \
  [--view +z] [--vmin 0] [--vmax 1] \
  [--width 512] [--height 512] [--zone 0] [--style default|jfm] \
  [--coloring continuous|banded] [--levels 16] [--colormap default|turbo|jfm|coolwarm|grayscale] \
  [--legend|--no-legend] [--margin 4] \
  [--u-ref U] [--lx Lx] [--ly Ly] [--lz Lz] [--t-ref T] \
  [--mesh] [--pathlines] [--pathlines-out lines.png] [--seed x,y] [--u-var U] [--v-var V] [--dt 0.02] \
  [--json]

# Probe scalar at world or pixel coords (Fieldplot)
flowplot probe <path> --var P --x X --y Y [--json]
flowplot probe <path> --var P --px PX --py PY --width W --height H [-o stamped.png] [--json]

# Nondim suggestions / apply scales (no invented physics)
flowplot nondim <path> [--u-ref U] [--lx Lx] [--ly Ly] [--lz Lz] [--t-ref T] \
  [--x-var X] [--y-var Y] [--z-var Z] [--json]

# Pathlines (particle traces / streamlines)
flowplot pathlines <path> -o lines.png [--var P] [--seed x,y] [--u-var U] [--v-var V] \
  [--style default|jfm] [--json]

# Field sanity / positivity (lehar validation ideas on the loaded file)
flowplot validate <path> [--positivity] [--json]

# Centerline / profile extract (optional --ref CSV for RMSE)
flowplot profile <path> --var P [--line mid-j] [--ref ref.csv] [--json]

# CFD post-processing menu (lehar agents ↔ Flowplot)
flowplot menu
flowplot menu show field-sanity
flowplot menu run field-sanity -- tests/fixtures/flow.dat --json
flowplot menu run bow-shock-field   # stub: requires agent + planned flags
```

`--pathlines` overlays traces on the contour PNG; `--pathlines-out` writes a second PNG.
`--style jfm` uses a restrained coolwarm map, white exterior, black frame, and (if size left at 512×512) a single-column 700×525 raster.

GUI: **Agents** runs validation/correction (field sanity, positivity, profiles, shock polar) with side charts (`gui/agents_panel.py`). Top menus: File … Agents … Help.

Agents (lehar-agent / MCP): prefer `--json` or `FLOWPLOT_BIN=… python3 agents/flowplot_mcp.py` — see [`docs/agent-bridge.md`](docs/agent-bridge.md) and [`docs/postprocess-menu.md`](docs/postprocess-menu.md).

## Format support

| Format | Extensions | Status |
|--------|------------|--------|
| PLT ASCII | `.dat`, `.tp`, ASCII `.plt` | **Working** (minimal: `TITLE` / `VARIABLES` / ordered or FE `ZONE`) |
| PLT binary | `.plt` (`#!TDV111` / `#!TDV112`) | **Working** — public Data Format Guide layout (block packing; ordered + basic FE) |
| VTK legacy ASCII | `.vtk` | **Working** (structured grid + unstructured grid subset) |
| VTK XML | `.vtu`, `.vtp`, … | **Stub** — clear `Unimplemented` error |
| CGNS | `.cgns`, `.cgz` | **Working** when system `libcgns` is found at build; else clear stub (`apt install libcgns-dev`, `--features cgns`) |
| Fluent case/data | `.cas`, `.dat` | **Stub** — documents public section layout; binary returns `Unimplemented` |

**Not in scope (no reverse engineering):** PLT SZL (`.szplt`) and Fluent binary case/data decoding.

## Crate layout

- `src/dataset.rs` — data model
- `src/readers/` — `Reader` trait + PLT / VTK / CGNS / Fluent modules
- `src/mesh.rs` — wireframe zone grid → PNG (equal aspect)
- `src/contour.rs` — scalar contour → PNG
- `src/nondim.rs` — reference scales + name suggestions
- `src/pathlines.rs` — velocity-field particle traces
- `src/validate.rs` — field sanity / positivity checks
- `src/profile.rs` — line / centerline extracts (+ optional ref RMSE)
- `src/field.rs` — ordered-plane extract + bilinear sample
- `src/bin/flowplot.rs` — CLI (`load` / `info` / `mesh` / `contour` / `nondim` / `pathlines` / `validate` / `profile` / `menu`, `--json`, `--style`)
- `src/menu.rs` — postprocess menu loader (embedded YAML)
- `agents/postprocess-menu.yaml` — machine-readable CFD postprocess menu
- `agents/flowplot_mcp.py` — stdio MCP wrapper for lehar agents
- `docs/agent-bridge.md` — lehar ↔ Flowplot contract
- `docs/postprocess-menu.md` — human-facing menu (groups ↔ lehar ↔ CLI/MCP)
- `gui/` — menus + Plot panel (mesh-first) + Analyze charts + SVG wordmark
- `packaging/windows/` — MSI packaging (MinGW + wixl)
- `tests/fixtures/` — synthetic PLT / VTK ASCII (incl. `flow.dat` for contour/pathlines)

## License

MIT OR Apache-2.0
