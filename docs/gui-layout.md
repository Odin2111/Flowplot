# Flowplot GUI layout (Tecplot-like Plot panel)

Native **tkinter** shell under `gui/flowplot_gui.py`. The viewport previews PNGs from the `flowplot` CLI (Pillow optional). **File → Open** uses a details list (**Name | Size | Modified**, newest first) so competing `.plt` files are easy to pick, then draws the **Mesh** layer first. Contour is an optional layer.

## Layout

| Region | Contents |
|--------|----------|
| Top | Menus + **Tecplot-like icon toolbar**: Open / Save(Redraw) / Export PNG · Rotate · ±X/Y/Z · Probe · Slice · ± levels / Abc / Pathlines |
| Left | **Plot** (scrollable): dataset/zones, plot type, zone layers, derived objects, contour var, Cut/Track, Zone Style / Contour Details |
| Center | **Viewport**. CLI raster sized 1:1; mesh `--fill`. **Left-drag = pan**, mid/right = orbit → **settle redraw** (`--pan-x/--pan-y`); **F = fit**; Probe; Pathlines rake; XYZ triad |
| Right | **View** (scrollable): XYZ snaps, rotate/zoom, Fit · **Nondim scales** · **Log** |

Missing contour/iso variables never raise a blocking modal — Fieldplot auto-picks the first scalar or quietly disables Contour/Iso and falls back to Mesh.

CLI discovery picks the newest of PATH / `target/release` / `target/debug` (override with `FLOWPLOT_BIN`). A stale debug binary that lacked `--pan-x` used to dump clap `--help` into Log and break Open redraw.

## Defaults (match Tecplot)

- Mesh **on**, Contour **off** after load
- Variable combo filled from dataset; default = first non-coordinate scalar (e.g. cone20 → `Pressure`, not `P`)
- Plot menu: Axis…, Assign XY…, **Zone Style…** (Mesh + Contour working; other tabs stub), Pathlines…, **Contour…** (Contour Details), Vector…, Scatter…, Blanking…, Label Points… (stubs where not yet implemented; Pathlines = Tecplot “Streamtraces”)
- Zone Style Apply → per-zone Show Zone / Show Mesh / Contour + mesh color → CLI `--zone` / `--mesh-color`
- Contour Details (Levels and Color / Bands / Lines / Labels / Legend): working slice = levels, colormap, banded vs continuous, legend; other tabs stub
- Annotation HUD uses native fontdue AA at 1:1 Viewport size (no stretch); integer baseline snap (no jitter); colorbar labels sized to fit
- **F** / Frame → Fit to Viewport / View → Fit whole domain clears pan and recenters
- Pathlines: after enable, click-drag draws a rake/seed line; pathlines emit from that segment (exclusive vs Probe)
- Product UI name is **Fieldplot** (window title / About / logo); CLI crate binary remains `flowplot`
- Zone layers: Vector (U/V/W picker + arrows), Edge (boundary), Scatter (subsample), Iso-Surfaces (isolines)
- Pathlines rake persists as canvas overlay + PNG traces after drag

## Run

```bash
cargo build
python3 gui/flowplot_gui.py
```

CLI mesh (no contour):

```bash
flowplot mesh path/to/file.plt -o mesh.png
```

**Agents → Post-process agents…** opens an interactive panel (not a stub): browse `agents/postprocess-menu.yaml`, run ready actions against the **currently loaded** file via `flowplot menu run`, and show JSON/stdout in a results pane. Look is restrained scientific slate/gray — no purple chrome.
