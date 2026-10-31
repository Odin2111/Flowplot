# Flowplot GUI layout (Plot panel)

Native **tkinter** shell under `gui/flowplot_gui.py`. The viewport previews PNGs from the `flowplot` CLI (Pillow optional). **File → Open** uses a details list (**Name | Size | Modified**, newest first) so competing `.plt` files are easy to pick, then draws the **Wireframe** layer first. Flood is an optional layer.

## Layout

| Region | Contents |
|--------|----------|
| Top | Menus + **icon toolbar**: Open / Save(Redraw) / Export PNG · Orbit · Align ±X/Y/Z · Sample · Cut · ± levels / Abc / Pathlines |
| Left | **Plot** (scrollable): dataset/zones, plot type, zone layers, derived objects, flood var, Cut/Track, Zone Style / Flood Details |
| Center | **Viewport**. CLI raster sized 1:1; mesh `--fill`. **Left-drag = pan**, mid/right = orbit → **settle redraw** (`--pan-x/--pan-y`); **F = fit**; Probe; Pathlines rake; XYZ triad |
| Right | **View** (scrollable): XYZ snaps, rotate/zoom, Fit · **Nondim scales** · **Log** |

Missing flood/iso variables never raise a blocking modal — Fieldplot auto-picks the first scalar or quietly disables Flood/Iso and falls back to Wireframe.

CLI discovery picks the newest of PATH / `target/release` / `target/debug` (override with `FLOWPLOT_BIN`). A stale debug binary that lacked `--pan-x` used to dump clap `--help` into Log and break Open redraw.

## Defaults

- Wireframe **on**, Flood **off** after load
- Variable combo filled from dataset; default = first non-coordinate scalar (e.g. cone20 → `Pressure`, not `P`)
- Plot menu: Axis…, Assign XY…, **Zone Style…** (Wireframe + Flood working; other tabs stub), Pathlines…, **Flood Details…**, Glyphs…, Sample points…, Blanking…, Label Points… (stubs where not yet implemented)
- Zone Style Apply → per-zone Show Zone / Show Wireframe / Flood + wireframe color → CLI `--zone` / `--mesh-color`
- Flood Details (Levels and Color / Bands / Lines / Labels / Legend): **flood variable** (all zone scalars) + levels, colormap, banded vs continuous, legend; other tabs stub
- Annotation HUD uses native fontdue AA at 1:1 Viewport size (no stretch); integer baseline snap (no jitter); colorbar labels sized to fit
- **F** / Frame → Fit to Viewport / View → Fit whole domain clears pan and recenters
- Pathlines: after enable, click-drag draws a rake/seed line; pathlines emit from that segment (exclusive vs Probe)
- Product UI name is **Fieldplot** (window title / About / logo); CLI crate binary remains `flowplot`
- Zone layers: each Show Zone Layers row has a **gear** → Wireframe (Zone Style), Flood (Flood Details), Solid fill / Glyphs / Boundary / Sample points Details; Glyphs also uses U/V/W + `--overlay-skip`, Boundary perimeter, Sample points subsample, Iso-Surfaces (isolines)
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
