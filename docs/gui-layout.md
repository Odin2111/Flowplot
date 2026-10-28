# Flowplot GUI layout (Tecplot-like Plot panel)

Native **tkinter** shell under `gui/flowplot_gui.py`. The viewport previews PNGs from the `flowplot` CLI (Pillow optional). **File → Open** loads the dataset and draws the **Mesh** layer first (wireframe crate, equal aspect). Contour is an optional layer.

## Layout

| Region | Contents |
|--------|----------|
| Top | Menus **File / Plot / Style / Agents / Help**, always-visible Contour toolbar **＋ Level / － Level / Abc Label / ✎ Probe / Details…**, wordmark + **Open · Redraw · Export PNG** |
| Left | **Plot** panel (scrollable): layers, variable, cut/view, Zone Style / Contour Details, collapsible **Nondim scales ▸**, Log |
| Center | **Viewport full width** (no right column). CLI raster sized 1:1 to canvas; mesh `--fill`. **✎ Probe** click prints values; drag = orbit |
| Right | *(removed — Nondim moved under Plot)* |

## Defaults (match Tecplot)

- Mesh **on**, Contour **off** after load
- Variable combo filled from dataset; default = first non-coordinate scalar (e.g. cone20 → `Pressure`, not `P`)
- Plot menu: Axis…, Assign XY…, **Zone Style…** (Mesh + Contour working; other tabs stub), Pathlines…, **Contour…** (Contour Details), Vector…, Scatter…, Blanking…, Label Points… (stubs where not yet implemented; Pathlines = Tecplot “Streamtraces”)
- Zone Style Apply → per-zone Show Zone / Show Mesh / Contour + mesh color → CLI `--zone` / `--mesh-color`
- Contour Details (Levels and Color / Bands / Lines / Labels / Legend): working slice = levels, colormap, banded vs continuous, legend; other tabs stub
- Annotation HUD uses 2× supersampled DejaVu Sans (no stretch artifacts); colorbar labels sized to fit

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
