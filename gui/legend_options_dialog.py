"""Tecplot-like Legend Options (show, alignment, font, colorbar)."""

from __future__ import annotations

import tkinter as tk
from dataclasses import dataclass, replace
from tkinter import ttk
from typing import Callable

from windowing import stay_above


BG = "#eceef0"
PANEL = "#e2e5e8"
HEADER_BG = "#d5d9de"
FG = "#1f2a33"
MUTED = "#5c6b76"
ACCENT = "#3d5a6c"

ALIGNS = ("Right", "Left")


@dataclass
class LegendOptionsState:
    """Contour Details → Legend working slice."""

    show: bool = True
    align: str = "Right"  # Right | Left
    font_px: float = 0.0  # 0 = auto
    colorbar: bool = True  # same as show for this slice

    def cli_align(self) -> str:
        return "left" if self.align.lower().startswith("left") else "right"


class LegendOptionsDialog(tk.Toplevel):
    def __init__(
        self,
        master: tk.Misc,
        state: LegendOptionsState,
        on_apply: Callable[[LegendOptionsState], None],
    ) -> None:
        super().__init__(master)
        self.title("Legend Options")
        self.configure(bg=BG)
        self.geometry("400x320")
        self.minsize(340, 280)
        self.state = replace(state)
        self._on_apply = on_apply

        self.show_var = tk.BooleanVar(value=self.state.show)
        self.align_var = tk.StringVar(value=self.state.align)
        self.font_var = tk.DoubleVar(value=float(self.state.font_px or 0.0))
        self.colorbar_var = tk.BooleanVar(value=self.state.colorbar)

        self._build()
        self.protocol("WM_DELETE_WINDOW", self.destroy)
        stay_above(self, master, modal=True)

    def _build(self) -> None:
        hdr = tk.Frame(self, bg=HEADER_BG)
        hdr.pack(side=tk.TOP, fill=tk.X)
        tk.Label(
            hdr,
            text="Legend Options",
            bg=HEADER_BG,
            fg=FG,
            font=("Helvetica", 11, "bold"),
        ).pack(side=tk.LEFT, padx=12, pady=8)
        tk.Label(
            hdr,
            text="Tecplot Contour Details → Legend",
            bg=HEADER_BG,
            fg=MUTED,
            font=("Helvetica", 8),
        ).pack(side=tk.LEFT, padx=4)

        body = tk.Frame(self, bg=PANEL)
        body.pack(side=tk.TOP, fill=tk.BOTH, expand=True, padx=10, pady=10)

        ttk.Checkbutton(body, text="Show legend", variable=self.show_var).pack(
            anchor="w", padx=8, pady=(8, 4)
        )
        ttk.Checkbutton(
            body, text="Show colorbar (same strip)", variable=self.colorbar_var
        ).pack(anchor="w", padx=8, pady=4)

        row = tk.Frame(body, bg=PANEL)
        row.pack(anchor="w", fill=tk.X, padx=8, pady=10)
        tk.Label(row, text="Alignment", bg=PANEL, fg=FG).pack(side=tk.LEFT)
        ttk.Combobox(
            row,
            textvariable=self.align_var,
            values=ALIGNS,
            state="readonly",
            width=10,
        ).pack(side=tk.LEFT, padx=8)

        row2 = tk.Frame(body, bg=PANEL)
        row2.pack(anchor="w", fill=tk.X, padx=8, pady=6)
        tk.Label(row2, text="Font size (px, 0=auto)", bg=PANEL, fg=FG).pack(side=tk.LEFT)
        ttk.Spinbox(
            row2, from_=0, to=28, increment=1, textvariable=self.font_var, width=6
        ).pack(side=tk.LEFT, padx=8)

        tk.Label(
            body,
            text="Apply redraws Contour with the colorbar on the chosen side.\n"
            "Font 0 keeps the VisIt-quality auto size from raster width.",
            bg=PANEL,
            fg=MUTED,
            justify=tk.LEFT,
            font=("Helvetica", 8),
        ).pack(anchor="w", padx=8, pady=12)

        btns = tk.Frame(self, bg=BG)
        btns.pack(side=tk.BOTTOM, fill=tk.X, padx=10, pady=8)
        ttk.Button(btns, text="Close", command=self.destroy).pack(side=tk.RIGHT, padx=4)
        ttk.Button(btns, text="Apply", command=self._apply).pack(side=tk.RIGHT, padx=4)

    def _apply(self) -> None:
        try:
            font = float(self.font_var.get())
        except (tk.TclError, ValueError, TypeError):
            font = 0.0
        show = bool(self.show_var.get())
        colorbar = bool(self.colorbar_var.get()) and show
        self.state = LegendOptionsState(
            show=show,
            align=self.align_var.get() or "Right",
            font_px=max(0.0, font),
            colorbar=colorbar,
        )
        self._on_apply(self.state)
