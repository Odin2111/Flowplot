"""Show Zone Layers gear buttons + layer Details dialogs.

Each zone-layer row is checkbox + right-aligned gear (canvas icon). Contour
opens Contour Details; Mesh opens Zone Style; Shade / Vector / Edge / Scatter
have dedicated Details dialogs matching that UX.
"""

from __future__ import annotations

import math
import tkinter as tk
from dataclasses import dataclass, replace
from tkinter import colorchooser, ttk
from typing import Callable

from windowing import stay_above


PANEL = "#e2e5e8"
BG = "#eceef0"
FG = "#1f2a33"
MUTED = "#5c6b76"
ACCENT = "#3d5a6c"
HEADER_BG = "#d5d9de"
BTN_BG = "#f4f5f6"
BTN_BORDER = "#9aa3ab"
BTN_ACTIVE = "#cfd5db"
GEAR = "#2a6fbb"
ICON = 22

SHADE_COLORS = ("Use Contour Coloring", "RGB")
EDGE_PATTERNS = ("Solid", "Dashed", "Dotted")
DEFAULT_EDGE_COLOR = "#1a1a1a"
DEFAULT_SCATTER_COLOR = "#c00000"
DEFAULT_VECTOR_COLOR = "#1f4e79"


@dataclass
class ShadeDetailsState:
    """Shade layer — continuous flood under mesh."""

    color_mode: str = "Use Contour Coloring"
    rgb: str = "#808080"
    lighting: bool = False  # stub
    translucency: float = 0.0  # 0–1 stub
    use_active_var: bool = True


@dataclass
class VectorDetailsState:
    """Vector layer — U/V/W + subsample stride."""

    skip: int = 4
    scale: float = 1.0  # stub
    color: str = DEFAULT_VECTOR_COLOR  # stub visual


@dataclass
class EdgeDetailsState:
    """Edge layer — boundary wire style."""

    color: str = DEFAULT_EDGE_COLOR
    line_pattern: str = "Solid"
    line_thickness: float = 1.0  # stub


@dataclass
class ScatterDetailsState:
    """Scatter layer — nodal markers + stride."""

    skip: int = 4
    color: str = DEFAULT_SCATTER_COLOR
    symbol_size: float = 2.0  # stub


def _draw_gear(c: tk.Canvas, cx: float = 6.5, cy: float = 6.5, r: float = 3.2) -> None:
    """Small gear badge in the upper-left of the layer icon."""
    c.create_oval(cx - r, cy - r, cx + r, cy + r, outline=GEAR, width=1.5)
    for ang in (20, 80, 140, 200, 260, 320):
        rad = math.radians(ang)
        x0 = cx + (r - 0.4) * math.cos(rad)
        y0 = cy + (r - 0.4) * math.sin(rad)
        x1 = cx + (r + 1.6) * math.cos(rad)
        y1 = cy + (r + 1.6) * math.sin(rad)
        c.create_line(x0, y0, x1, y1, fill=GEAR, width=1.5)
    c.create_oval(cx - 1.1, cy - 1.1, cx + 1.1, cy + 1.1, fill=BTN_BG, outline=GEAR)


def draw_mesh_gear(c: tk.Canvas) -> None:
    c.create_rectangle(8, 8, 20, 20, outline=ACCENT, width=1)
    c.create_line(8, 14, 20, 14, fill=ACCENT)
    c.create_line(14, 8, 14, 20, fill=ACCENT)
    _draw_gear(c)


def draw_contour_gear(c: tk.Canvas) -> None:
    c.create_line(8, 17, 11, 12, 14, 15, 17, 9, 20, 12, smooth=True, fill="#3a7fc1", width=2)
    c.create_line(8, 19, 12, 14, 16, 17, 20, 11, smooth=True, fill="#7eb6e8", width=1)
    _draw_gear(c)


def draw_shade_gear(c: tk.Canvas) -> None:
    # Flood swatch in the lower-right; gear badge upper-left (same as Contour/Vector).
    for i, col in enumerate(("#c8d8e8", "#7aa0c4", "#3d5a6c", "#2a4a5c")):
        y0 = 10 + i * 2.6
        c.create_rectangle(10, y0, 20, y0 + 2.6, fill=col, outline=col)
    _draw_gear(c)


def draw_vector_gear(c: tk.Canvas) -> None:
    for y, x0 in ((10, 9), (14, 8), (18, 10)):
        c.create_line(x0, y, x0 + 9, y, fill=GEAR, width=1, arrow=tk.LAST)
    _draw_gear(c)


def draw_edge_gear(c: tk.Canvas) -> None:
    c.create_line(9, 9, 9, 19, 19, 19, fill="#7eb6e8", width=2)
    c.create_line(9, 9, 19, 9, fill="#a8c8e0", width=1)
    _draw_gear(c)


def draw_scatter_gear(c: tk.Canvas) -> None:
    for x, y in ((10, 11), (15, 9), (18, 14), (12, 17), (16, 18)):
        c.create_oval(x - 1.4, y - 1.4, x + 1.4, y + 1.4, fill=GEAR, outline=GEAR)
    _draw_gear(c)


LAYER_GEAR_DRAWERS: dict[str, Callable[[tk.Canvas], None]] = {
    "Wireframe": draw_mesh_gear,
    "Flood": draw_contour_gear,
    "Solid fill": draw_shade_gear,
    "Glyphs": draw_vector_gear,
    "Boundary": draw_edge_gear,
    "Sample points": draw_scatter_gear,
}


def add_layer_row(
    parent: tk.Misc,
    *,
    label: str,
    variable: tk.BooleanVar,
    on_toggle: Callable[[], None],
    on_settings: Callable[[], None],
) -> tk.Frame:
    """Checkbox + right-aligned gear button for one zone layer."""
    row = ttk.Frame(parent)
    row.pack(fill=tk.X, pady=1)
    ttk.Checkbutton(row, text=label, variable=variable, command=on_toggle).pack(
        side=tk.LEFT, anchor="w"
    )
    drawer = LAYER_GEAR_DRAWERS.get(label, draw_mesh_gear)
    canvas = tk.Canvas(
        row,
        width=ICON,
        height=ICON,
        bg=BTN_BG,
        highlightthickness=1,
        highlightbackground=BTN_BORDER,
        cursor="hand2",
    )
    canvas.pack(side=tk.RIGHT, padx=(4, 0))
    drawer(canvas)

    def on_enter(_e: tk.Event) -> None:
        canvas.configure(bg=BTN_ACTIVE)

    def on_leave(_e: tk.Event) -> None:
        canvas.configure(bg=BTN_BG)

    def on_click(_e: tk.Event) -> None:
        on_settings()

    canvas.bind("<Enter>", on_enter)
    canvas.bind("<Leave>", on_leave)
    canvas.bind("<Button-1>", on_click)
    return row


class _LayerDetailsBase(tk.Toplevel):
    """Shared chrome for Shade / Vector / Edge / Scatter Details."""

    title_text = "Layer Details"
    subtitle = ""

    def __init__(self, master: tk.Misc) -> None:
        super().__init__(master)
        self.title(self.title_text)
        self.configure(bg=BG)
        self.resizable(False, False)
        self.geometry("420x360")
        self.minsize(380, 320)
        self.protocol("WM_DELETE_WINDOW", self.destroy)
        stay_above(self, master, modal=False)

        hdr = tk.Frame(self, bg=HEADER_BG)
        hdr.pack(side=tk.TOP, fill=tk.X)
        tk.Label(
            hdr,
            text=self.title_text,
            bg=HEADER_BG,
            fg=FG,
            font=("Helvetica", 11, "bold"),
        ).pack(side=tk.LEFT, padx=12, pady=8)
        if self.subtitle:
            tk.Label(
                hdr,
                text=self.subtitle,
                bg=HEADER_BG,
                fg=MUTED,
                font=("Helvetica", 8),
            ).pack(side=tk.LEFT, padx=4)

        self.body = tk.Frame(self, bg=PANEL)
        self.body.pack(side=tk.TOP, fill=tk.BOTH, expand=True, padx=10, pady=8)

        btns = tk.Frame(self, bg=BG)
        btns.pack(side=tk.BOTTOM, fill=tk.X, padx=10, pady=8)
        ttk.Button(btns, text="Close", command=self.destroy).pack(side=tk.RIGHT, padx=4)
        ttk.Button(btns, text="Apply", command=self._apply).pack(side=tk.RIGHT, padx=4)

    def _apply(self) -> None:  # pragma: no cover - overridden
        raise NotImplementedError


class ShadeDetailsDialog(_LayerDetailsBase):
    title_text = "Solid fill Details"
    subtitle = "Flood under wireframe"

    def __init__(
        self,
        master: tk.Misc,
        state: ShadeDetailsState,
        on_apply: Callable[[ShadeDetailsState], None],
        variables: list[str] | None = None,
        active_var: str = "",
    ) -> None:
        super().__init__(master)
        self.geometry("440x400")
        self.state = replace(state)
        self._on_apply = on_apply
        self.color_mode = tk.StringVar(value=self.state.color_mode)
        self.rgb = tk.StringVar(value=self.state.rgb)
        self.lighting = tk.BooleanVar(value=self.state.lighting)
        self.translucency = tk.DoubleVar(value=float(self.state.translucency))
        self.use_active = tk.BooleanVar(value=self.state.use_active_var)
        self.active_var = tk.StringVar(value=active_var or "")
        self._variables = variables or []
        self._build_body()

    def _build_body(self) -> None:
        pad = {"padx": 12, "pady": 6}
        tk.Label(
            self.body,
            text="Color",
            bg=PANEL,
            fg=ACCENT,
            font=("Helvetica", 9, "bold"),
        ).pack(anchor="w", **pad)
        ttk.Combobox(
            self.body,
            textvariable=self.color_mode,
            values=SHADE_COLORS,
            state="readonly",
            width=28,
        ).pack(anchor="w", padx=12)

        rgb_row = tk.Frame(self.body, bg=PANEL)
        rgb_row.pack(anchor="w", fill=tk.X, padx=12, pady=6)
        self._swatch = tk.Label(
            rgb_row, text="  ", bg=self.rgb.get() or "#808080", width=3, relief=tk.GROOVE
        )
        self._swatch.pack(side=tk.LEFT, padx=(0, 6))

        def _pick() -> None:
            picked = colorchooser.askcolor(
                color=self.rgb.get() or "#808080", parent=self, title="Solid fill RGB"
            )
            if picked and picked[1]:
                self.rgb.set(picked[1])
                self._swatch.configure(bg=picked[1])
                self.color_mode.set("RGB")

        tk.Button(
            rgb_row,
            text="RGB…",
            command=_pick,
            bg=BTN_BG,
            fg=FG,
            relief=tk.GROOVE,
            padx=6,
        ).pack(side=tk.LEFT)
        tk.Label(
            rgb_row,
            text="(solid fill when not using Flood coloring)",
            bg=PANEL,
            fg=MUTED,
            font=("Helvetica", 8),
        ).pack(side=tk.LEFT, padx=8)

        ttk.Checkbutton(
            self.body,
            text="Use active Flood variable for flood",
            variable=self.use_active,
        ).pack(anchor="w", padx=12, pady=4)
        if self._variables:
            row = tk.Frame(self.body, bg=PANEL)
            row.pack(anchor="w", fill=tk.X, padx=12, pady=4)
            tk.Label(row, text="Variable", bg=PANEL, fg=FG).pack(side=tk.LEFT)
            ttk.Combobox(
                row,
                textvariable=self.active_var,
                values=self._variables,
                width=22,
            ).pack(side=tk.LEFT, padx=8)

        ttk.Checkbutton(
            self.body,
            text="Lighting (stub — flat flood in this slice)",
            variable=self.lighting,
        ).pack(anchor="w", padx=12, pady=8)
        trow = tk.Frame(self.body, bg=PANEL)
        trow.pack(anchor="w", fill=tk.X, padx=12, pady=4)
        tk.Label(trow, text="Translucency (0–1, stub)", bg=PANEL, fg=FG).pack(side=tk.LEFT)
        ttk.Spinbox(
            trow, from_=0.0, to=1.0, increment=0.05, textvariable=self.translucency, width=6
        ).pack(side=tk.LEFT, padx=8)
        tk.Label(
            self.body,
            text="Solid fill alone forces continuous flood under Wireframe.\n"
            "Flood Details coloring wins when Flood is also on.",
            bg=PANEL,
            fg=MUTED,
            justify=tk.LEFT,
            font=("Helvetica", 8),
        ).pack(anchor="w", padx=12, pady=10)

    def _apply(self) -> None:
        try:
            t = float(self.translucency.get())
        except (tk.TclError, ValueError, TypeError):
            t = 0.0
        self.state = ShadeDetailsState(
            color_mode=self.color_mode.get() or "Use Contour Coloring",
            rgb=self.rgb.get() or "#808080",
            lighting=bool(self.lighting.get()),
            translucency=max(0.0, min(1.0, t)),
            use_active_var=bool(self.use_active.get()),
        )
        if self.active_var.get().strip():
            # Caller may sync active Contour variable from the dialog.
            pass
        self._on_apply(self.state)


class VectorDetailsDialog(_LayerDetailsBase):
    title_text = "Glyphs Details"
    subtitle = "U / V / W components"

    def __init__(
        self,
        master: tk.Misc,
        *,
        u_var: tk.StringVar,
        v_var: tk.StringVar,
        w_var: tk.StringVar,
        state: VectorDetailsState,
        variables: list[str],
        on_apply: Callable[[VectorDetailsState], None],
    ) -> None:
        super().__init__(master)
        self.geometry("420x300")
        self.u_var = u_var
        self.v_var = v_var
        self.w_var = w_var
        self.state = replace(state)
        self._on_apply = on_apply
        self.skip = tk.IntVar(value=max(1, int(self.state.skip)))
        self.scale = tk.DoubleVar(value=float(self.state.scale))
        self._variables = variables or [""]
        self._build_body()

    def _build_body(self) -> None:
        tk.Label(
            self.body,
            text="Velocity components",
            bg=PANEL,
            fg=ACCENT,
            font=("Helvetica", 9, "bold"),
        ).pack(anchor="w", padx=12, pady=(8, 4))
        for label, var in (
            ("U / Ux", self.u_var),
            ("V / Uy", self.v_var),
            ("W / Uz", self.w_var),
        ):
            row = tk.Frame(self.body, bg=PANEL)
            row.pack(fill=tk.X, padx=12, pady=3)
            tk.Label(row, text=label, bg=PANEL, fg=FG, width=10, anchor="w").pack(side=tk.LEFT)
            ttk.Combobox(row, textvariable=var, values=self._variables, width=22).pack(
                side=tk.LEFT, fill=tk.X, expand=True
            )
        row = tk.Frame(self.body, bg=PANEL)
        row.pack(anchor="w", fill=tk.X, padx=12, pady=10)
        tk.Label(row, text="Skip (subsample)", bg=PANEL, fg=FG).pack(side=tk.LEFT)
        ttk.Spinbox(row, from_=1, to=64, textvariable=self.skip, width=6).pack(side=tk.LEFT, padx=8)
        row2 = tk.Frame(self.body, bg=PANEL)
        row2.pack(anchor="w", fill=tk.X, padx=12, pady=4)
        tk.Label(row2, text="Scale (stub)", bg=PANEL, fg=FG).pack(side=tk.LEFT)
        ttk.Spinbox(
            row2, from_=0.1, to=10.0, increment=0.1, textvariable=self.scale, width=6
        ).pack(side=tk.LEFT, padx=8)

    def _apply(self) -> None:
        try:
            skip = int(self.skip.get())
        except (tk.TclError, ValueError, TypeError):
            skip = 4
        try:
            scale = float(self.scale.get())
        except (tk.TclError, ValueError, TypeError):
            scale = 1.0
        self.state = VectorDetailsState(skip=max(1, skip), scale=max(0.01, scale))
        self._on_apply(self.state)


class EdgeDetailsDialog(_LayerDetailsBase):
    title_text = "Boundary Details"
    subtitle = "Domain boundary"

    def __init__(
        self,
        master: tk.Misc,
        state: EdgeDetailsState,
        on_apply: Callable[[EdgeDetailsState], None],
    ) -> None:
        super().__init__(master)
        self.geometry("400x260")
        self.state = replace(state)
        self._on_apply = on_apply
        self.color = tk.StringVar(value=self.state.color)
        self.pattern = tk.StringVar(value=self.state.line_pattern)
        self.thickness = tk.DoubleVar(value=float(self.state.line_thickness))
        self._build_body()

    def _build_body(self) -> None:
        row = tk.Frame(self.body, bg=PANEL)
        row.pack(anchor="w", fill=tk.X, padx=12, pady=12)
        tk.Label(row, text="Boundary color", bg=PANEL, fg=FG).pack(side=tk.LEFT)
        self._swatch = tk.Label(
            row, text="  ", bg=self.color.get() or DEFAULT_EDGE_COLOR, width=3, relief=tk.GROOVE
        )
        self._swatch.pack(side=tk.LEFT, padx=8)

        def _pick() -> None:
            picked = colorchooser.askcolor(
                color=self.color.get() or DEFAULT_EDGE_COLOR, parent=self, title="Boundary Color"
            )
            if picked and picked[1]:
                self.color.set(picked[1])
                self._swatch.configure(bg=picked[1])

        tk.Button(
            row, text="Color…", command=_pick, bg=BTN_BG, fg=FG, relief=tk.GROOVE, padx=6
        ).pack(side=tk.LEFT)

        row2 = tk.Frame(self.body, bg=PANEL)
        row2.pack(anchor="w", fill=tk.X, padx=12, pady=8)
        tk.Label(row2, text="Line pattern", bg=PANEL, fg=FG).pack(side=tk.LEFT)
        ttk.Combobox(
            row2,
            textvariable=self.pattern,
            values=EDGE_PATTERNS,
            state="readonly",
            width=12,
        ).pack(side=tk.LEFT, padx=8)

        row3 = tk.Frame(self.body, bg=PANEL)
        row3.pack(anchor="w", fill=tk.X, padx=12, pady=8)
        tk.Label(row3, text="Thickness (stub)", bg=PANEL, fg=FG).pack(side=tk.LEFT)
        ttk.Spinbox(
            row3, from_=0.5, to=8.0, increment=0.5, textvariable=self.thickness, width=6
        ).pack(side=tk.LEFT, padx=8)
        tk.Label(
            self.body,
            text="Boundary draws the ordered-grid perimeter (--edges).\n"
            "Color/pattern are stored for the next overlay pass.",
            bg=PANEL,
            fg=MUTED,
            justify=tk.LEFT,
            font=("Helvetica", 8),
        ).pack(anchor="w", padx=12, pady=10)

    def _apply(self) -> None:
        try:
            th = float(self.thickness.get())
        except (tk.TclError, ValueError, TypeError):
            th = 1.0
        self.state = EdgeDetailsState(
            color=self.color.get() or DEFAULT_EDGE_COLOR,
            line_pattern=self.pattern.get() or "Solid",
            line_thickness=max(0.5, th),
        )
        self._on_apply(self.state)


class ScatterDetailsDialog(_LayerDetailsBase):
    title_text = "Sample points Details"
    subtitle = "Nodal markers"

    def __init__(
        self,
        master: tk.Misc,
        state: ScatterDetailsState,
        on_apply: Callable[[ScatterDetailsState], None],
    ) -> None:
        super().__init__(master)
        self.geometry("400x260")
        self.state = replace(state)
        self._on_apply = on_apply
        self.skip = tk.IntVar(value=max(1, int(self.state.skip)))
        self.color = tk.StringVar(value=self.state.color)
        self.size = tk.DoubleVar(value=float(self.state.symbol_size))
        self._build_body()

    def _build_body(self) -> None:
        row = tk.Frame(self.body, bg=PANEL)
        row.pack(anchor="w", fill=tk.X, padx=12, pady=12)
        tk.Label(row, text="Skip (subsample)", bg=PANEL, fg=FG).pack(side=tk.LEFT)
        ttk.Spinbox(row, from_=1, to=64, textvariable=self.skip, width=6).pack(side=tk.LEFT, padx=8)

        row2 = tk.Frame(self.body, bg=PANEL)
        row2.pack(anchor="w", fill=tk.X, padx=12, pady=8)
        tk.Label(row2, text="Symbol color", bg=PANEL, fg=FG).pack(side=tk.LEFT)
        self._swatch = tk.Label(
            row2,
            text="  ",
            bg=self.color.get() or DEFAULT_SCATTER_COLOR,
            width=3,
            relief=tk.GROOVE,
        )
        self._swatch.pack(side=tk.LEFT, padx=8)

        def _pick() -> None:
            picked = colorchooser.askcolor(
                color=self.color.get() or DEFAULT_SCATTER_COLOR,
                parent=self,
                title="Sample points Color",
            )
            if picked and picked[1]:
                self.color.set(picked[1])
                self._swatch.configure(bg=picked[1])

        tk.Button(
            row2, text="Color…", command=_pick, bg=BTN_BG, fg=FG, relief=tk.GROOVE, padx=6
        ).pack(side=tk.LEFT)

        row3 = tk.Frame(self.body, bg=PANEL)
        row3.pack(anchor="w", fill=tk.X, padx=12, pady=8)
        tk.Label(row3, text="Symbol size (stub)", bg=PANEL, fg=FG).pack(side=tk.LEFT)
        ttk.Spinbox(
            row3, from_=1.0, to=12.0, increment=0.5, textvariable=self.size, width=6
        ).pack(side=tk.LEFT, padx=8)

    def _apply(self) -> None:
        try:
            skip = int(self.skip.get())
        except (tk.TclError, ValueError, TypeError):
            skip = 4
        try:
            size = float(self.size.get())
        except (tk.TclError, ValueError, TypeError):
            size = 2.0
        self.state = ScatterDetailsState(
            skip=max(1, skip),
            color=self.color.get() or DEFAULT_SCATTER_COLOR,
            symbol_size=max(0.5, size),
        )
        self._on_apply(self.state)
