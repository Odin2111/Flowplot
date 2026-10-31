"""Export PNG dialog with style presets (JFM Crisp from Tecplot macro pack)."""

from __future__ import annotations

import tkinter as tk
from pathlib import Path
from tkinter import filedialog, ttk
from typing import Any

from windowing import stay_above

BG = "#eceef0"
MUTED = "#5c6b76"


def ask_export_png(
    master: tk.Misc,
    *,
    initial_style: str = "jfm",
    initialfile: str = "fieldplot.png",
) -> dict[str, Any] | None:
    """Return {path, preset} or None if cancelled.

    Presets:
      - current: keep interactive Contour Details / style
      - jfm-crisp: banded flood, no mesh, JFM paper (JFM_Crisp_2D_Contour macro)
    """
    dlg = tk.Toplevel(master)
    dlg.title("Export PNG")
    dlg.configure(bg=BG)
    dlg.resizable(False, False)
    result: dict[str, Any] | None = {"_alive": True}
    stay_above(dlg, master, modal=True)

    body = ttk.Frame(dlg, padding=12)
    body.pack(fill=tk.BOTH, expand=True)
    ttk.Label(body, text="Export style preset").pack(anchor="w")
    preset = tk.StringVar(value="jfm-crisp" if initial_style == "jfm" else "current")
    ttk.Radiobutton(
        body,
        text="Current Viewport (interactive Contour Details)",
        variable=preset,
        value="current",
    ).pack(anchor="w", pady=2)
    ttk.Radiobutton(
        body,
        text="JFM Crisp 2D contour (banded flood, no mesh, white paper)",
        variable=preset,
        value="jfm-crisp",
    ).pack(anchor="w", pady=2)
    ttk.Label(
        body,
        text="JFM Crisp mirrors the Tecplot JFM_Crisp_2D_Contour macro: clean flood,\n"
        "no mesh/edge/shade, Times-like paper look via --style jfm.",
        foreground=MUTED,
        justify=tk.LEFT,
    ).pack(anchor="w", pady=(8, 4))

    path_var = tk.StringVar(value="")

    def browse() -> None:
        dest = filedialog.asksaveasfilename(
            parent=dlg,
            title="Export PNG",
            defaultextension=".png",
            filetypes=[("PNG", "*.png")],
            initialfile=initialfile,
        )
        if dest:
            path_var.set(dest)

    row = ttk.Frame(body)
    row.pack(fill=tk.X, pady=(8, 4))
    ttk.Entry(row, textvariable=path_var, width=42).pack(side=tk.LEFT, fill=tk.X, expand=True)
    ttk.Button(row, text="Browse…", command=browse).pack(side=tk.LEFT, padx=4)

    def ok() -> None:
        dest = path_var.get().strip()
        if not dest:
            browse()
            dest = path_var.get().strip()
        if not dest:
            return
        result.clear()
        result.update({"path": Path(dest), "preset": preset.get()})
        dlg.destroy()

    def cancel() -> None:
        result.clear()
        dlg.destroy()

    btns = ttk.Frame(body)
    btns.pack(fill=tk.X, pady=(12, 0))
    ttk.Button(btns, text="Cancel", command=cancel).pack(side=tk.RIGHT, padx=4)
    ttk.Button(btns, text="Export", command=ok).pack(side=tk.RIGHT, padx=4)
    dlg.protocol("WM_DELETE_WINDOW", cancel)
    dlg.wait_window()
    if not result or "path" not in result:
        return None
    return result
