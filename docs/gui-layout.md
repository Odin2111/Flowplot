# Flowplot GUI layout (first shell)

Native **tkinter** shell under `gui/flowplot_gui.py`. Not a full renderer — the center is a viewport placeholder; Contour / Pathlines / Export PNG call the existing `flowplot` CLI and preview the PNG when Pillow is available.

## Layout

| Region | Contents |
|--------|----------|
| Top | Wordmark (canvas + shipped SVG `gui/assets/flowplot-wordmark.svg`), menus **File / Plot / Style / Agents / Help**, tool strip **Load · Contour · Nondim · Pathlines · Export PNG**, variable name entry |
| Center | Empty viewport placeholder (future contour / pathline canvas) |
| Right | Nondim scales **U_ref, Lx, Ly, Lz, T_ref** → CLI `--u-ref` / `--lx` / `--ly` / `--lz` / `--t-ref`, plus a short log |

## Run

```bash
cargo build
python3 gui/flowplot_gui.py
```

Agents menu is a stub pointing at the lehar ↔ Flowplot bridge. Look is restrained scientific slate/gray — no purple chrome.
