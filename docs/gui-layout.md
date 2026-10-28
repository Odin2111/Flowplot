# Flowplot GUI layout (Tecplot-like Plot panel)

Native **tkinter** shell under `gui/flowplot_gui.py`. The viewport previews PNGs from the `flowplot` CLI (Pillow optional). **File → Open** loads the dataset and draws the **Mesh** layer first (wireframe crate, equal aspect). Contour is an optional layer.

## Layout

| Region | Contents |
|--------|----------|
| Top | Menus **File / Plot / Style / Agents / Help**, Contour toolbar **＋/－ · Abc · ✎ Probe · Details…**, wordmark + **Open · Redraw · Export PNG** |
| Left | **Plot** panel: plot type (2D/3D Cartesian…), **Show Zone Layers** (Mesh / Contour / Shade / Vector / Edge / Scatter), **Show Derived Objects** (Pathlines), contour variable combo from `flowplot info`, **Zone Style…** |
| Center | Viewport fills available space (CLI raster sized to canvas; `--fill` mesh). **Drag** warps last frame; **✎ Probe** click prints values on the plot; **settle** = preview then full fill |
| Right | Nondim scales **U_ref, Lx, Ly, Lz, T_ref** → CLI flags, plus a short log |

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
