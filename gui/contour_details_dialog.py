"""Tecplot-like Contour & Multi-Coloring Details dialog.

Tabs mirror Tecplot 360 Contour Details: Levels and Color, Bands, Lines,
Labels, Legend. Working slice: levels count, colormap, banded vs continuous,
legend on/off. Other controls are labeled stubs.
"""

from __future__ import annotations

import tkinter as tk
from dataclasses import dataclass, replace
from tkinter import ttk
from typing import Callable


PANEL = "#e2e5e8"
BG = "#eceef0"
FG = "#1f2a33"
MUTED = "#5c6b76"
ACCENT = "#3d5a6c"
HEADER_BG = "#d5d9de"
BTN_BG = "#f4f5f6"

COLORINGS = ("Continuous", "Banded")
COLORMAPS = ("Default", "Turbo", "JFM", "Coolwarm", "Grayscale")
DETAIL_TABS = ("Levels and Color", "Bands", "Lines", "Labels", "Legend")


@dataclass
class ContourDetailsState:
    """Contour Details working slice (Tecplot Contour & Multi-Coloring)."""

    coloring: str = "Continuous"  # Continuous | Banded
    nlevels: int = 16
    colormap: str = "Default"
    legend: bool = True
    legend_align: str = "Right"  # Right | Left
    legend_font_px: float = 0.0  # 0 = auto
    show_lines: bool = False  # stub
    show_labels: bool = False  # toolbar Contour Abc Label
    label_skip: int = 1  # stub
    band_min: float | None = None  # stub
    band_max: float | None = None  # stub

    def cli_coloring(self) -> str:
        return "banded" if self.coloring.lower().startswith("band") else "continuous"

    def cli_colormap(self) -> str:
        m = {
            "Default": "default",
            "Turbo": "turbo",
            "JFM": "jfm",
            "Coolwarm": "coolwarm",
            "Grayscale": "grayscale",
        }
        return m.get(self.colormap, "default")

    def cli_legend_align(self) -> str:
        return "left" if self.legend_align.lower().startswith("left") else "right"


class ContourDetailsDialog(tk.Toplevel):
    """Side/modal Contour Details notebook."""

    def __init__(
        self,
        master: tk.Misc,
        state: ContourDetailsState,
        on_apply: Callable[[ContourDetailsState], None],
    ) -> None:
        super().__init__(master)
        self.title("Contour & Multi-Coloring Details")
        self.configure(bg=BG)
        self.resizable(True, True)
        self.geometry("520x420")
        self.minsize(440, 360)
        self.transient(master)
        self.state = replace(state)
        self._on_apply = on_apply

        self.coloring_var = tk.StringVar(value=self.state.coloring)
        self.nlevels_var = tk.IntVar(value=max(2, int(self.state.nlevels)))
        self.colormap_var = tk.StringVar(value=self.state.colormap)
        self.legend_var = tk.BooleanVar(value=self.state.legend)
        self.legend_align_var = tk.StringVar(value=self.state.legend_align)
        self.legend_font_var = tk.DoubleVar(value=float(self.state.legend_font_px or 0.0))
        self.lines_var = tk.BooleanVar(value=self.state.show_lines)
        self.labels_var = tk.BooleanVar(value=self.state.show_labels)
        self.label_skip_var = tk.IntVar(value=max(1, int(self.state.label_skip)))

        self._build()
        self.protocol("WM_DELETE_WINDOW", self.destroy)
        try:
            self.grab_set()
        except tk.TclError:
            pass

    def _build(self) -> None:
        hdr = tk.Frame(self, bg=HEADER_BG)
        hdr.pack(side=tk.TOP, fill=tk.X)
        tk.Label(
            hdr,
            text="Contour Details",
            bg=HEADER_BG,
            fg=FG,
            font=("Helvetica", 11, "bold"),
        ).pack(side=tk.LEFT, padx=12, pady=8)
        tk.Label(
            hdr,
            text="Tecplot-like Levels · Color · Legend",
            bg=HEADER_BG,
            fg=MUTED,
            font=("Helvetica", 8),
        ).pack(side=tk.LEFT, padx=4)

        nb = ttk.Notebook(self)
        nb.pack(side=tk.TOP, fill=tk.BOTH, expand=True, padx=10, pady=8)

        self._tab_levels = tk.Frame(nb, bg=PANEL)
        self._tab_bands = tk.Frame(nb, bg=PANEL)
        self._tab_lines = tk.Frame(nb, bg=PANEL)
        self._tab_labels = tk.Frame(nb, bg=PANEL)
        self._tab_legend = tk.Frame(nb, bg=PANEL)
        nb.add(self._tab_levels, text="Levels and Color")
        nb.add(self._tab_bands, text="Bands")
        nb.add(self._tab_lines, text="Lines")
        nb.add(self._tab_labels, text="Labels")
        nb.add(self._tab_legend, text="Legend")

        self._build_levels_tab(self._tab_levels)
        self._build_stub_tab(
            self._tab_bands,
            "Bands",
            "Band min/max and custom level list — stub in this slice.\n"
            "Use Levels and Color → Banded + level count for discrete flood.",
        )
        self._build_lines_tab(self._tab_lines)
        self._build_labels_tab(self._tab_labels)
        self._build_legend_tab(self._tab_legend)

        btns = tk.Frame(self, bg=BG)
        btns.pack(side=tk.BOTTOM, fill=tk.X, padx=10, pady=8)
        ttk.Button(btns, text="Close", command=self.destroy).pack(side=tk.RIGHT, padx=4)
        ttk.Button(btns, text="Apply", command=self._apply).pack(side=tk.RIGHT, padx=4)

    def _build_levels_tab(self, parent: tk.Frame) -> None:
        pad = {"padx": 12, "pady": 6}
        tk.Label(
            parent,
            text="Coloring",
            bg=PANEL,
            fg=ACCENT,
            font=("Helvetica", 9, "bold"),
        ).pack(anchor="w", **pad)
        coloring = ttk.Combobox(
            parent,
            textvariable=self.coloring_var,
            values=COLORINGS,
            state="readonly",
            width=18,
        )
        coloring.pack(anchor="w", padx=12)

        row = tk.Frame(parent, bg=PANEL)
        row.pack(anchor="w", fill=tk.X, padx=12, pady=10)
        tk.Label(row, text="Number of levels", bg=PANEL, fg=FG).pack(side=tk.LEFT)
        sp = ttk.Spinbox(row, from_=2, to=64, textvariable=self.nlevels_var, width=6)
        sp.pack(side=tk.LEFT, padx=8)

        tk.Label(
            parent,
            text="Colormap",
            bg=PANEL,
            fg=ACCENT,
            font=("Helvetica", 9, "bold"),
        ).pack(anchor="w", **pad)
        cmap = ttk.Combobox(
            parent,
            textvariable=self.colormap_var,
            values=COLORMAPS,
            state="readonly",
            width=18,
        )
        cmap.pack(anchor="w", padx=12)
        tk.Label(
            parent,
            text="Continuous = smooth flood. Banded = discrete contour bands.\n"
            "Toolbar Contour + / − adjusts the level count.",
            bg=PANEL,
            fg=MUTED,
            justify=tk.LEFT,
            font=("Helvetica", 8),
        ).pack(anchor="w", padx=12, pady=12)

    def _build_lines_tab(self, parent: tk.Frame) -> None:
        ttk.Checkbutton(
            parent,
            text="Show contour lines (stub — flood only in this slice)",
            variable=self.lines_var,
        ).pack(anchor="w", padx=12, pady=12)
        tk.Label(
            parent,
            text="Line pattern / color controls arrive with iso-line marching.",
            bg=PANEL,
            fg=MUTED,
            font=("Helvetica", 8),
        ).pack(anchor="w", padx=12)

    def _build_labels_tab(self, parent: tk.Frame) -> None:
        ttk.Checkbutton(
            parent,
            text="Show contour labels (iso-value stamps)",
            variable=self.labels_var,
        ).pack(anchor="w", padx=12, pady=12)
        row = tk.Frame(parent, bg=PANEL)
        row.pack(anchor="w", padx=12)
        tk.Label(row, text="Label every Nth level (spacing hint)", bg=PANEL, fg=FG).pack(
            side=tk.LEFT
        )
        ttk.Spinbox(row, from_=1, to=16, textvariable=self.label_skip_var, width=5).pack(
            side=tk.LEFT, padx=8
        )
        tk.Label(
            parent,
            text="Toolbar Abc Label toggles stamps along a mid-plane scan of the flood.",
            bg=PANEL,
            fg=MUTED,
            font=("Helvetica", 8),
        ).pack(anchor="w", padx=12, pady=10)

    def _build_legend_tab(self, parent: tk.Frame) -> None:
        ttk.Checkbutton(
            parent,
            text="Show legend (colorbar)",
            variable=self.legend_var,
        ).pack(anchor="w", padx=12, pady=12)
        row = tk.Frame(parent, bg=PANEL)
        row.pack(anchor="w", fill=tk.X, padx=12, pady=6)
        tk.Label(row, text="Alignment", bg=PANEL, fg=FG).pack(side=tk.LEFT)
        ttk.Combobox(
            row,
            textvariable=self.legend_align_var,
            values=("Right", "Left"),
            state="readonly",
            width=10,
        ).pack(side=tk.LEFT, padx=8)
        row2 = tk.Frame(parent, bg=PANEL)
        row2.pack(anchor="w", fill=tk.X, padx=12, pady=6)
        tk.Label(row2, text="Font size (px, 0=auto)", bg=PANEL, fg=FG).pack(side=tk.LEFT)
        ttk.Spinbox(
            row2, from_=0, to=28, increment=1, textvariable=self.legend_font_var, width=6
        ).pack(side=tk.LEFT, padx=8)
        tk.Label(
            parent,
            text="Matches Tecplot Contour Details → Legend. Toolbar also has Legend Options…",
            bg=PANEL,
            fg=MUTED,
            font=("Helvetica", 8),
        ).pack(anchor="w", padx=12, pady=10)

    def _build_stub_tab(self, parent: tk.Frame, title: str, body: str) -> None:
        tk.Label(
            parent,
            text=title,
            bg=PANEL,
            fg=ACCENT,
            font=("Helvetica", 10, "bold"),
        ).pack(anchor="w", padx=12, pady=(12, 4))
        tk.Label(
            parent,
            text=body,
            bg=PANEL,
            fg=MUTED,
            justify=tk.LEFT,
            font=("Helvetica", 9),
        ).pack(anchor="w", padx=12, pady=4)

    def _apply(self) -> None:
        try:
            n = int(self.nlevels_var.get())
        except (tk.TclError, ValueError, TypeError):
            n = 16
        try:
            font = float(self.legend_font_var.get())
        except (tk.TclError, ValueError, TypeError):
            font = 0.0
        self.state = ContourDetailsState(
            coloring=self.coloring_var.get() or "Continuous",
            nlevels=max(2, min(64, n)),
            colormap=self.colormap_var.get() or "Default",
            legend=bool(self.legend_var.get()),
            legend_align=self.legend_align_var.get() or "Right",
            legend_font_px=max(0.0, font),
            show_lines=bool(self.lines_var.get()),
            show_labels=bool(self.labels_var.get()),
            label_skip=max(1, int(self.label_skip_var.get() or 1)),
        )
        self._on_apply(self.state)
