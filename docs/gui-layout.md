# Flowplot GUI layout (Tecplot-like Plot panel)

Native **tkinter** shell under `gui/flowplot_gui.py`. The viewport previews PNGs from the `flowplot` CLI (Pillow optional). **File → Open** loads the dataset and draws the **Mesh** layer first (wireframe crate, equal aspect). Contour is an optional layer.

## Layout

| Region | Contents |
|--------|----------|
| Top | Wordmark, menus **File / Plot / Style / Agents / Help**, tool strip **Open · Redraw · Export PNG** |
| Left | **Plot** panel: plot type (2D/3D Cartesian…), **Show Zone Layers** (Mesh / Contour / Shade / Vector / Edge / Scatter), **Show Derived Objects** (Streamtraces), contour variable combo from `flowplot info` |
| Center | Viewport — Mesh PNG by default; Contour / Streamtraces when those layers are on |
| Right | Nondim scales **U_ref, Lx, Ly, Lz, T_ref** → CLI flags, plus a short log |

## Defaults (match Tecplot)

- Mesh **on**, Contour **off** after load
- Variable combo filled from dataset; default = first non-coordinate scalar (e.g. cone20 → `Pressure`, not `P`)
- Plot menu: Axis…, Assign XY…, Zone Style…, Streamtraces…, Contour…, Vector…, Scatter…, Blanking…, Label Points… (stubs where not yet implemented)

## Run

```bash
cargo build
python3 gui/flowplot_gui.py
```

CLI mesh (no contour):

```bash
flowplot mesh path/to/file.plt -o mesh.png
```

Agents menu is a stub pointing at the lehar ↔ Flowplot bridge. Look is restrained scientific slate/gray — no purple chrome.
