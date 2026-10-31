#!/usr/bin/env python3
"""Flowplot GUI — Tecplot-like Plot panel (mesh first, contours optional).

Thin Python front-end that shells out to the `flowplot` CLI. On File→Open the
default view is the zone **Mesh** (wireframe crate) with Contour off. Contour
and Pathlines are optional layers, matching Tecplot’s Show Zone Layers /
Show Derived Objects order (Tecplot’s “Streamtraces” → Flowplot “Pathlines”).
"""

from __future__ import annotations

import copy
import json
import math
import os
import shutil
import subprocess
import sys
import tempfile
import time
import tkinter as tk
from dataclasses import replace
from datetime import datetime
from pathlib import Path
from tkinter import filedialog, messagebox, ttk

from agents_panel import AgentsPanel
from contour_details_dialog import ContourDetailsDialog, ContourDetailsState
from icon_toolbar import build_fieldplot_icon_toolbar
from layer_settings_dialog import (
    EdgeDetailsDialog,
    EdgeDetailsState,
    ScatterDetailsDialog,
    ScatterDetailsState,
    ShadeDetailsDialog,
    ShadeDetailsState,
    VectorDetailsDialog,
    VectorDetailsState,
    add_layer_row,
)
from legend_options_dialog import LegendOptionsDialog, LegendOptionsState
from open_dataset_dialog import ask_open_dataset
from probe_dialog import ProbeDialog
from windowing import release_grabs, stay_above
from zone_style_dialog import (
    DEFAULT_MESH_COLOR,
    ZoneStyleDialog,
    ZoneStyleState,
    rgb_cli,
    zone_styles_from_info,
)

ROOT = Path(__file__).resolve().parent
ASSETS = ROOT / "assets"
WORDMARK = ASSETS / "flowplot-wordmark.svg"

# Restrained scientific palette (no purple chrome).
BG = "#eceef0"
PANEL = "#e2e5e8"
MENU_BG = "#d5d9de"
VIEWPORT_BG = "#f7f8f9"
ACCENT = "#3d5a6c"
FG = "#1f2a33"
MUTED = "#5c6b76"
BTN_BG = "#f4f5f6"
BTN_ACTIVE = "#cfd5db"

COORD_NAMES = {"x", "y", "z", "r", "theta", "th", "i", "j", "k"}

PLOT_TYPES = (
    "2D Cartesian",
    "3D Cartesian",
    "XY Line",
    "Polar Line",
    "Sketch",
)


def find_flowplot_bin() -> str | None:
    """Locate the flowplot CLI.

    Prefer ``FLOWPLOT_BIN`` / ``FIELDPLOT_BIN``, else the newest of PATH /
    ``target/release`` / ``target/debug``. Picking stale ``debug`` first used to
    reject new flags (e.g. ``--pan-x``) and dump clap's ``--help`` tip into Log.
    """
    for key in ("FLOWPLOT_BIN", "FIELDPLOT_BIN"):
        env = os.environ.get(key, "").strip()
        if env:
            p = Path(env).expanduser()
            if p.is_file() and os.access(p, os.X_OK):
                return str(p.resolve())
    candidates: list[Path] = []
    found = shutil.which("flowplot")
    if found:
        candidates.append(Path(found))
    repo = ROOT.parent  # gui/ → Flowplot/
    for rel in ("target/release/flowplot", "target/debug/flowplot"):
        cand = repo / rel
        if cand.is_file() and os.access(cand, os.X_OK):
            candidates.append(cand)
    if not candidates:
        return None
    best = max(candidates, key=lambda p: p.stat().st_mtime)
    return str(best.resolve())


def first_scalar(variables: list[str]) -> str:
    """Default contour variable: first non-coordinate name (Tecplot-like)."""
    for name in variables:
        if name.strip().lower() not in COORD_NAMES:
            return name
    return variables[0] if variables else ""


class FlowplotGui(tk.Tk):
    def __init__(self) -> None:
        super().__init__()
        self.title("Fieldplot")
        self.geometry("1180x720")
        self.minsize(960, 600)
        self.configure(bg=BG)

        self.dataset_path: Path | None = None
        self.last_png: Path | None = None
        self.variables: list[str] = []
        self.effectively_2d: bool = True
        self.zone_ordered: dict[str, int] | None = None
        self.zone_styles: list[ZoneStyleState] = []
        self.active_zone: int = 0
        self.mesh_color: str = DEFAULT_MESH_COLOR
        self.flowplot = find_flowplot_bin()
        self.status_var = tk.StringVar(value=self._status_idle())
        self.var_name = tk.StringVar(value="")
        self.plot_style = tk.StringVar(value="jfm")
        self.plot_type = tk.StringVar(value="2D Cartesian")
        self._preview_photo = None
        self._preview_after: str | None = None
        self._zone_style_dialog: ZoneStyleDialog | None = None
        self._contour_details_dialog: ContourDetailsDialog | None = None
        self._shade_details_dialog: ShadeDetailsDialog | None = None
        self._vector_details_dialog: VectorDetailsDialog | None = None
        self._edge_details_dialog: EdgeDetailsDialog | None = None
        self._scatter_details_dialog: ScatterDetailsDialog | None = None
        self._legend_options_dialog: LegendOptionsDialog | None = None
        self._probe_dialog: ProbeDialog | None = None
        self.contour_details = ContourDetailsState()
        self.shade_details = ShadeDetailsState()
        self.vector_details = VectorDetailsState()
        self.edge_details = EdgeDetailsState()
        self.scatter_details = ScatterDetailsState()
        # Exclusive interaction tools (Tecplot-like): rotate | pan | probe | pathlines.
        # Default Rotate so left-drag orbits the domain (not pan/translate).
        self.tool_mode = tk.StringVar(value="rotate")
        self.probe_marks: list[tuple[int, int, str]] = []
        self.pathline_seeds: list[tuple[float, float]] = []
        self._pl_drag_start: tuple[int, int] | None = None
        # Last rake as fractions of the preview image (survives redraw / pan).
        self._rake_frac: tuple[tuple[float, float], tuple[float, float]] | None = None
        self._preview_offset: tuple[int, int] = (0, 0)
        self._preview_size: tuple[int, int] = (0, 0)
        self._preview_native: tuple[int, int] = (0, 0)
        # Screen-space domain pan (pixels); Key-F / Fit resets to 0.
        self.view_pan_x = 0.0
        self.view_pan_y = 0.0
        self._pan_last: tuple[int, int] | None = None
        self._panning = False
        self.vector_u = tk.StringVar(value="")
        self.vector_v = tk.StringVar(value="")
        self.vector_w = tk.StringVar(value="")

        # Tecplot Show Zone Layers — Mesh ON, Contour OFF by default.
        self.layer_mesh = tk.BooleanVar(value=True)
        self.layer_contour = tk.BooleanVar(value=False)
        self.layer_shade = tk.BooleanVar(value=False)
        self.layer_vector = tk.BooleanVar(value=False)
        self.layer_edge = tk.BooleanVar(value=False)
        self.layer_scatter = tk.BooleanVar(value=False)
        # Show Derived Objects
        self.layer_isosurfaces = tk.BooleanVar(value=False)
        self.layer_slices = tk.BooleanVar(value=False)
        self.layer_pathlines = tk.BooleanVar(value=False)

        # Cut / track plane + Tecplot-like view orientation.
        self.plane_axis = tk.StringVar(value="auto")
        self.plane_index = tk.IntVar(value=0)
        self.view_mode = tk.StringVar(value="auto")  # auto|+x|-x|+y|-y|+z|-z|free
        self.azimuth_deg = 0.0
        self.elevation_deg = 90.0
        # Principal world plane for face-on camera / edge-on clamps (xy|xz|yz).
        self.principal_plane = "xy"
        self._drag_last: tuple[int, int] | None = None
        self._dragging = False
        self._redraw_after: str | None = None
        self._fullres_after: str | None = None
        self._settle_after: str | None = None
        self._orbit_preview_after: str | None = None
        self._cli_proc: subprocess.Popen[str] | None = None
        self._render_cache: dict[str, Path] = {}
        self._render_gen = 0
        # Last settled frame metadata (geometry redraw on settle — no PNG warp).
        self._settle_image = None  # PIL.Image.Image | None
        self._settle_az = 0.0
        self._settle_el = 90.0
        self._settle_plane: tuple[str, int] | None = None
        self._in_redraw = False
        self._settle_running = False

        self.scale_vars = {
            "U_ref": tk.StringVar(value=""),
            "Lx": tk.StringVar(value=""),
            "Ly": tk.StringVar(value=""),
            "Lz": tk.StringVar(value=""),
            "T_ref": tk.StringVar(value=""),
        }

        self._style()
        self._build_menu()  # single native menubar only (no duplicate button row)
        self._build_toolbar()  # one compact row: Open + Contour tools
        self._build_body()
        self._build_status()
        self.protocol("WM_DELETE_WINDOW", self._on_close)

    def _status_idle(self) -> str:
        if self.flowplot:
            return f"Ready — CLI: {self.flowplot}"
        return "Ready — flowplot binary not found (build with cargo build)"

    def _style(self) -> None:
        style = ttk.Style(self)
        try:
            style.theme_use("clam")
        except tk.TclError:
            pass
        style.configure("TFrame", background=BG)
        style.configure("Panel.TFrame", background=PANEL)
        style.configure("MenuBar.TFrame", background=MENU_BG)
        style.configure("TLabel", background=BG, foreground=FG)
        style.configure("Panel.TLabel", background=PANEL, foreground=FG)
        style.configure("Muted.TLabel", background=PANEL, foreground=MUTED)
        style.configure("Title.TLabel", background=MENU_BG, foreground=FG, font=("Helvetica", 13, "bold"))
        style.configure(
            "Tool.TButton",
            background=BTN_BG,
            foreground=FG,
            padding=(10, 4),
            font=("Helvetica", 9),
        )
        style.map("Tool.TButton", background=[("active", BTN_ACTIVE)])
        # Compact Contour toolbar — one short row, smaller hit targets.
        style.configure(
            "ToolCompact.TButton",
            background=BTN_BG,
            foreground=FG,
            padding=(5, 1),
            font=("Helvetica", 8),
        )
        style.map("ToolCompact.TButton", background=[("active", BTN_ACTIVE)])
        style.configure("Scale.TEntry", fieldbackground="#ffffff", foreground=FG)
        style.configure("TLabelframe", background=PANEL, foreground=FG)
        style.configure("TLabelframe.Label", background=PANEL, foreground=ACCENT, font=("Helvetica", 9, "bold"))
        style.configure("TCheckbutton", background=PANEL, foreground=FG)
        style.configure("TCombobox", fieldbackground="#ffffff", foreground=FG)

    def _file_menu_items(self, menu: tk.Menu) -> None:
        menu.add_command(label="Open…", command=self.action_load, accelerator="Ctrl+O")
        menu.add_command(label="Export PNG…", command=self.action_export_png)
        menu.add_separator()
        menu.add_command(label="Quit", command=self.destroy)

    def _edit_menu_items(self, menu: tk.Menu) -> None:
        menu.add_command(label="Clear probe markers", command=self.clear_probe_marks)
        menu.add_command(label="Redraw", command=self.action_redraw)

    def _view_menu_items(self, menu: tk.Menu) -> None:
        for label in ("+X", "-X", "+Y", "-Y", "+Z", "-Z"):
            menu.add_command(
                label=f"Look {label}",
                command=lambda v=label.lower(): self._set_axis_view(v),
            )
        menu.add_separator()
        menu.add_command(label="Fit whole domain", command=self.action_fit_view, accelerator="F")
        menu.add_command(label="Reset view", command=self._reset_view)
        menu.add_separator()
        menu.add_radiobutton(label="Style · Default", variable=self.plot_style, value="default")
        menu.add_radiobutton(label="Style · JFM", variable=self.plot_style, value="jfm")

    def _plot_menu_items(self, menu: tk.Menu) -> None:
        """Tecplot-like Plot menu — only wired actions (no stub placeholders)."""
        menu.add_command(label="Zone Style…", command=self.action_zone_style)
        menu.add_separator()
        menu.add_command(label="Contour Details…", command=self.action_contour_details)
        menu.add_command(label="Legend Options…", command=self.action_legend_options)
        menu.add_command(label="Pathlines (rake)…", command=self.action_pathlines_tool)
        menu.add_command(label="Iso-Surfaces…", command=self.action_isosurfaces)
        menu.add_command(label="Slices…", command=self.action_slices_focus)
        menu.add_separator()
        menu.add_command(label="Clear pathline seeds", command=self.clear_pathline_seeds)
        menu.add_command(label="Redraw", command=self.action_redraw)
        menu.add_command(label="Apply nondim scales", command=self.action_nondim)

    def _data_menu_items(self, menu: tk.Menu) -> None:
        menu.add_command(label="Dataset info…", command=self.action_dataset_info)
        menu.add_command(label="Probe…", command=self.action_open_probe_window)
        menu.add_separator()
        menu.add_command(
            label="Extract profile…",
            command=lambda: self.open_agents_panel_select("extract-profile", "profiles"),
        )
        menu.add_command(label="Apply nondim scales", command=self.action_nondim)

    def _frame_menu_items(self, menu: tk.Menu) -> None:
        menu.add_command(label="Fit to Viewport", command=self.action_fit_view, accelerator="F")

    def _options_menu_items(self, menu: tk.Menu) -> None:
        menu.add_command(label="Legend Options…", command=self.action_legend_options)
        menu.add_command(label="Contour Details…", command=self.action_contour_details)
        menu.add_separator()
        menu.add_radiobutton(label="Plot style · Default", variable=self.plot_style, value="default")
        menu.add_radiobutton(label="Plot style · JFM", variable=self.plot_style, value="jfm")

    def _tools_menu_items(self, menu: tk.Menu) -> None:
        menu.add_command(label="Rotate (orbit)", command=self.action_rotate_tool)
        menu.add_command(label="Pan / translate", command=self.action_pan_tool)
        menu.add_command(label="Probe tool", command=self.action_toggle_probe)
        menu.add_command(label="Pathlines rake", command=self.action_pathlines_tool)
        menu.add_separator()
        menu.add_command(label="More levels", command=self.action_contour_add_level)
        menu.add_command(label="Fewer levels", command=self.action_contour_remove_level)
        menu.add_command(label="Contour labels", command=self.action_toggle_contour_labels)

    def _agents_cascade(self, parent: tk.Menu, title: str, items: list[tuple[str, str, str]]) -> None:
        """Add a categorized Agents submenu: (label, item_id, focus_group)."""
        sub = tk.Menu(parent, tearoff=0)
        for label, item_id, group in items:
            sub.add_command(
                label=label,
                command=lambda i=item_id, g=group: self.open_agents_panel_select(i, g),
            )
        parent.add_cascade(label=title, menu=sub)

    def _agents_menu_items(self, menu: tk.Menu) -> None:
        """Tecplot Analyze analogue — named Agents, categorized by CFD applicability."""
        self._agents_cascade(
            menu,
            "Positivity & sanity",
            [
                ("Field sanity…", "field-sanity", "positivity-sanity"),
                ("Positivity check…", "positivity-check", "positivity-sanity"),
                ("Positivity contour…", "positivity-field", "positivity-sanity"),
            ],
        )
        self._agents_cascade(
            menu,
            "Profiles",
            [
                ("Extract profile…", "extract-profile", "profiles"),
                ("Near-wall field contour…", "wall-physics-field", "profiles"),
            ],
        )
        self._agents_cascade(
            menu,
            "Reference values",
            [
                ("Suggest / apply scales…", "nondim-suggest", "reference-values"),
                ("Archival / ERCOFTAC scales…", "plot-agent-scales", "reference-values"),
            ],
        )
        self._agents_cascade(
            menu,
            "Calculate variables",
            [
                ("Contour scalar…", "contour", "calculate-variables"),
                ("Turbulence field…", "turbulence-field", "calculate-variables"),
            ],
        )
        self._agents_cascade(
            menu,
            "Shocks",
            [
                ("Shock polar…", "shock-polar-field", "shocks"),
                ("Shock-tracking field…", "shock-tracking-field", "shocks"),
                ("Shock-sensor / pressure…", "shock-stability-field", "shocks"),
            ],
        )
        self._agents_cascade(
            menu,
            "Hypersonic",
            [
                ("Field extrema…", "hypersonic-field-extrema", "hypersonic"),
                ("Mach / p / ρ contour…", "hypersonic-mach-contour", "hypersonic"),
                ("Bow-shock field…", "bow-shock-field", "hypersonic"),
            ],
        )
        self._agents_cascade(
            menu,
            "Grid & mesh",
            [
                ("Mesh / zone inventory…", "mesh-inventory", "grid-mesh"),
                ("Mesh wireframe…", "mesh-wireframe", "grid-mesh"),
                ("Mesh–solution QoI…", "mesh-solution-field", "grid-mesh"),
            ],
        )
        self._agents_cascade(
            menu,
            "Geometry & boundaries",
            [
                ("Zone / boundary inventory…", "zone-boundary-inventory", "geometry-boundaries"),
                ("Wall / boundary field…", "wall-boundary-field", "geometry-boundaries"),
            ],
        )
        self._agents_cascade(
            menu,
            "Integration",
            [
                ("Field mean / extrema…", "field-integrate-extrema", "integration"),
            ],
        )
        self._agents_cascade(
            menu,
            "Pathlines",
            [
                ("Trace pathlines…", "pathlines", "pathlines"),
                ("Contour + pathlines…", "contour-pathlines", "pathlines"),
            ],
        )
        self._agents_cascade(
            menu,
            "Schlieren & shadowgraph",
            [
                ("Schlieren (primary / Crisp)…", "schlieren-primary", "optical"),
                ("Schlieren (Chamarthi)…", "schlieren-chamarthi", "optical"),
                ("Shadowgraph…", "shadowgraph", "optical"),
                ("Entropy / shock-sensor…", "entropy-sensor-field", "optical"),
            ],
        )
        menu.add_separator()
        menu.add_command(label="All Agents…", command=self.open_agents_panel)

    def _help_menu_items(self, menu: tk.Menu) -> None:
        menu.add_command(label="About Fieldplot", command=self.show_about)

    def _build_menu(self) -> None:
        """Single native menubar. Stub-only menus (Insert/Animate/Scripting) omitted until wired."""
        menubar = tk.Menu(self, tearoff=0, bg=MENU_BG, fg=FG, activebackground=ACCENT, activeforeground="#fff")
        builders = (
            ("File", self._file_menu_items),
            ("Edit", self._edit_menu_items),
            ("View", self._view_menu_items),
            ("Plot", self._plot_menu_items),
            ("Data", self._data_menu_items),
            ("Frame", self._frame_menu_items),
            ("Options", self._options_menu_items),
            ("Tools", self._tools_menu_items),
            ("Agents", self._agents_menu_items),
            ("Help", self._help_menu_items),
        )
        for label, fill in builders:
            m = tk.Menu(menubar, tearoff=0)
            fill(m)
            menubar.add_cascade(label=label, menu=m)
        self.config(menu=menubar)
        self.bind("<Control-o>", lambda _e: self.action_load())
        # Tecplot-like Fit (whole domain). Bind both cases; ignore when typing in entries.
        self.bind_all("<KeyPress-f>", self._on_fit_key)
        self.bind_all("<KeyPress-F>", self._on_fit_key)

    def _build_toolbar(self) -> None:
        """Tecplot-like icon strip under the menubar."""
        self._icon_toolbar = build_fieldplot_icon_toolbar(self, self)
        self._icon_toolbar.pack(side=tk.TOP, fill=tk.X)
        # Levels status shares the icon strip's status area when idle.
        self.levels_label = self._icon_toolbar.status
        # Default exclusive tool = Rotate (curved arrow highlighted).
        self._icon_toolbar.set_active_tools({"rotate"})
        self._refresh_levels_label()
        tk.Frame(self, bg="#9aa3ab", height=1).pack(side=tk.TOP, fill=tk.X)

    def _levels_status_text(self) -> str:
        cd = self.contour_details
        mode = "banded" if cd.cli_coloring() == "banded" else "continuous"
        tool = self.tool_mode.get()
        tool_tag = f" · {tool}" if tool not in ("",) else ""
        labels = " · labels" if cd.show_labels else ""
        seeds = f" · {len(self.pathline_seeds)} seed(s)" if self.pathline_seeds else ""
        return f"{cd.nlevels} levels · {mode} · {cd.colormap}{labels}{tool_tag}{seeds}"

    def _refresh_levels_label(self) -> None:
        if hasattr(self, "levels_label"):
            # Don't overwrite live icon hover tip.
            tip = ""
            if hasattr(self, "_icon_toolbar") and self._icon_toolbar.status.cget("text"):
                # Only set levels text when tip empty or already a levels string.
                tip = str(self._icon_toolbar.status.cget("text") or "")
            if not tip or "levels" in tip or "·" in tip:
                self.levels_label.configure(text=self._levels_status_text())

    def _build_body(self) -> None:
        body = ttk.Frame(self)
        body.pack(side=tk.TOP, fill=tk.BOTH, expand=True, padx=4, pady=4)

        # Pack side columns first so the Viewport fills the remaining center.
        left_wrap = ttk.Frame(body, style="Panel.TFrame", width=252)
        left_wrap.pack(side=tk.LEFT, fill=tk.Y, padx=(0, 4))
        left_wrap.pack_propagate(False)

        left_canvas = tk.Canvas(left_wrap, bg=PANEL, highlightthickness=0, width=236)
        left_scroll = ttk.Scrollbar(left_wrap, orient=tk.VERTICAL, command=left_canvas.yview)
        left_canvas.configure(yscrollcommand=left_scroll.set)
        left_scroll.pack(side=tk.RIGHT, fill=tk.Y)
        left_canvas.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        left = ttk.Frame(left_canvas, style="Panel.TFrame")
        left_win = left_canvas.create_window((0, 0), window=left, anchor="nw")

        def _sync_left(_event: tk.Event | None = None) -> None:
            left_canvas.configure(scrollregion=left_canvas.bbox("all"))
            left_canvas.itemconfigure(left_win, width=max(left_canvas.winfo_width(), 220))

        left.bind("<Configure>", _sync_left)
        left_canvas.bind("<Configure>", _sync_left)
        self._build_left_plot_panel(left)

        right_wrap = ttk.Frame(body, style="Panel.TFrame", width=240)
        right_wrap.pack(side=tk.RIGHT, fill=tk.Y, padx=(4, 0))
        right_wrap.pack_propagate(False)

        right_canvas = tk.Canvas(right_wrap, bg=PANEL, highlightthickness=0, width=224)
        right_scroll = ttk.Scrollbar(right_wrap, orient=tk.VERTICAL, command=right_canvas.yview)
        right_canvas.configure(yscrollcommand=right_scroll.set)
        right_scroll.pack(side=tk.RIGHT, fill=tk.Y)
        right_canvas.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        right = ttk.Frame(right_canvas, style="Panel.TFrame")
        right_win = right_canvas.create_window((0, 0), window=right, anchor="nw")

        def _sync_right(_event: tk.Event | None = None) -> None:
            right_canvas.configure(scrollregion=right_canvas.bbox("all"))
            right_canvas.itemconfigure(right_win, width=max(right_canvas.winfo_width(), 210))

        right.bind("<Configure>", _sync_right)
        right_canvas.bind("<Configure>", _sync_right)
        self._build_right_view_panel(right)

        view_wrap = ttk.Frame(body)
        view_wrap.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)

        self.viewport = tk.Canvas(
            view_wrap,
            bg=VIEWPORT_BG,
            highlightthickness=1,
            highlightbackground="#9aa3ab",
            cursor="fleur",
        )
        self.viewport.pack(fill=tk.BOTH, expand=True)
        self.viewport.bind("<Configure>", self._on_viewport_configure)
        self.viewport.bind("<ButtonPress-1>", self._on_view_press)
        self.viewport.bind("<B1-Motion>", self._on_view_drag)
        self.viewport.bind("<ButtonRelease-1>", self._on_view_release)
        # Middle / right button always orbit (Probe / Pathlines never steal these).
        self.viewport.bind("<ButtonPress-2>", self._on_orbit_press)
        self.viewport.bind("<B2-Motion>", self._on_orbit_drag)
        self.viewport.bind("<ButtonRelease-2>", self._on_orbit_release)
        self.viewport.bind("<ButtonPress-3>", self._on_orbit_press)
        self.viewport.bind("<B3-Motion>", self._on_orbit_drag)
        self.viewport.bind("<ButtonRelease-3>", self._on_orbit_release)
        tip = ttk.Label(
            view_wrap,
            text="Rotate tool: left-drag orbits · Pan tool: left-drag translates · mid/right always orbit · F=fit",
            font=("Helvetica", 8),
            foreground=MUTED,
        )
        tip.pack(anchor="w", pady=(2, 0))
        bin_note = self.flowplot or "(missing — run cargo build)"
        self._log(f"CLI: {bin_note}")
        self._log("Open a dataset — Plot left · Viewport center · View/Nondim/Log right.")

    def _build_left_plot_panel(self, parent: ttk.Frame) -> None:
        """Left: dataset/zones + plot type, layers, derived objects, cut/track."""
        ttk.Label(parent, text="Dataset", style="Panel.TLabel", font=("Helvetica", 10, "bold")).pack(
            anchor="w", padx=12, pady=(10, 2)
        )
        self.dataset_label = ttk.Label(
            parent,
            text="(none)",
            style="Muted.TLabel",
            wraplength=200,
            justify=tk.LEFT,
        )
        self.dataset_label.pack(anchor="w", padx=12, pady=(0, 4))
        ttk.Button(parent, text="Open…", style="Tool.TButton", command=self.action_load).pack(
            anchor="w", padx=12, pady=(0, 6)
        )
        ttk.Label(parent, text="Zones", style="Muted.TLabel").pack(anchor="w", padx=12)
        self.zone_list = tk.Listbox(
            parent,
            height=5,
            exportselection=False,
            bg="#fafbfc",
            fg=FG,
            highlightthickness=1,
            highlightbackground="#b8c0c8",
            relief=tk.FLAT,
            font=("Helvetica", 9),
        )
        self.zone_list.pack(fill=tk.X, padx=10, pady=(2, 8))
        self.zone_list.bind("<<ListboxSelect>>", self._on_zone_list_select)

        ttk.Label(parent, text="Plot", style="Panel.TLabel", font=("Helvetica", 11, "bold")).pack(
            anchor="w", padx=12, pady=(4, 6)
        )

        ttk.Label(parent, text="Plot type", style="Muted.TLabel").pack(anchor="w", padx=12)
        type_box = ttk.Combobox(
            parent,
            textvariable=self.plot_type,
            values=PLOT_TYPES,
            state="readonly",
            width=22,
        )
        type_box.pack(anchor="w", padx=12, pady=(2, 10))
        type_box.bind("<<ComboboxSelected>>", lambda _e: self._on_plot_type())

        layers = ttk.LabelFrame(parent, text="Show Zone Layers", padding=(8, 6))
        layers.pack(fill=tk.X, padx=10, pady=(0, 8))
        # Tecplot-like: checkbox + right-aligned gear → layer Details dialog.
        for label, var, toggle, settings in (
            ("Mesh", self.layer_mesh, self.action_redraw, self.action_mesh_settings),
            ("Contour", self.layer_contour, self.action_redraw, self.action_contour_details),
            ("Shade", self.layer_shade, self.action_shade_layer, self.action_shade_settings),
            ("Vector", self.layer_vector, self.action_vector_layer, self.action_vector_settings),
            ("Edge", self.layer_edge, self.action_edge_layer, self.action_edge_settings),
            ("Scatter", self.layer_scatter, self.action_scatter_layer, self.action_scatter_settings),
        ):
            add_layer_row(
                layers,
                label=label,
                variable=var,
                on_toggle=toggle,
                on_settings=settings,
            )

        derived = ttk.LabelFrame(parent, text="Show Derived Objects", padding=(8, 6))
        derived.pack(fill=tk.X, padx=10, pady=(0, 8))
        ttk.Checkbutton(
            derived,
            text="Iso-Surfaces",
            variable=self.layer_isosurfaces,
            command=self.action_isosurfaces,
        ).pack(anchor="w")
        ttk.Checkbutton(
            derived,
            text="Slices",
            variable=self.layer_slices,
            command=self._on_slices_checkbox,
        ).pack(anchor="w")
        ttk.Checkbutton(
            derived,
            text="Pathlines",
            variable=self.layer_pathlines,
            command=self._on_pathlines_layer,
        ).pack(anchor="w")
        ttk.Button(
            derived, text="Redraw", style="Tool.TButton", command=self.action_redraw
        ).pack(anchor="w", pady=(6, 0))

        ttk.Label(parent, text="Contour variable", style="Muted.TLabel").pack(anchor="w", padx=12, pady=(4, 0))
        self.var_combo = ttk.Combobox(
            parent,
            textvariable=self.var_name,
            values=[],
            state="readonly",
            width=22,
        )
        self.var_combo.pack(anchor="w", padx=12, pady=(2, 8))
        self.var_combo.bind("<<ComboboxSelected>>", lambda _e: self._on_var_changed())

        plane_box = ttk.LabelFrame(parent, text="Cut / Track plane", padding=(8, 6))
        plane_box.pack(fill=tk.X, padx=10, pady=(0, 8))
        axis_row = ttk.Frame(plane_box, style="Panel.TFrame")
        axis_row.pack(fill=tk.X)
        ttk.Label(axis_row, text="Axis", style="Muted.TLabel").pack(side=tk.LEFT)
        self.plane_axis_combo = ttk.Combobox(
            axis_row,
            textvariable=self.plane_axis,
            values=("auto", "i", "j", "k"),
            state="readonly",
            width=8,
        )
        self.plane_axis_combo.pack(side=tk.LEFT, padx=(6, 0))
        self.plane_axis_combo.bind("<<ComboboxSelected>>", lambda _e: self._schedule_settle(delay_ms=60))
        idx_row = ttk.Frame(plane_box, style="Panel.TFrame")
        idx_row.pack(fill=tk.X, pady=(6, 0))
        ttk.Label(idx_row, text="Index", style="Muted.TLabel").pack(side=tk.LEFT)
        self.plane_spin = tk.Spinbox(
            idx_row,
            from_=0,
            to=9999,
            textvariable=self.plane_index,
            width=6,
            command=lambda: self._schedule_settle(delay_ms=80),
        )
        self.plane_spin.pack(side=tk.LEFT, padx=(6, 4))
        ttk.Button(idx_row, text="−", style="Tool.TButton", width=2, command=lambda: self._track_plane(-1)).pack(
            side=tk.LEFT, padx=1
        )
        ttk.Button(idx_row, text="+", style="Tool.TButton", width=2, command=lambda: self._track_plane(+1)).pack(
            side=tk.LEFT, padx=1
        )
        ttk.Label(
            plane_box,
            text="Track ± steps the cut index.",
            style="Muted.TLabel",
        ).pack(anchor="w", pady=(4, 0))

        ttk.Button(parent, text="Zone Style…", style="Tool.TButton", command=self.action_zone_style).pack(
            padx=12, pady=(2, 4), anchor="w"
        )
        ttk.Button(parent, text="Contour Details…", style="Tool.TButton", command=self.action_contour_details).pack(
            padx=12, pady=(2, 4), anchor="w"
        )
        ttk.Button(parent, text="Redraw layers", style="Tool.TButton", command=self.action_redraw).pack(
            padx=12, pady=(4, 8), anchor="w"
        )

    def _build_right_view_panel(self, parent: ttk.Frame) -> None:
        """Right: View XYZ, rotate/fit, Nondim scales, Log."""
        ttk.Label(parent, text="View", style="Panel.TLabel", font=("Helvetica", 11, "bold")).pack(
            anchor="w", padx=10, pady=(12, 6)
        )
        view_box = ttk.LabelFrame(parent, text="View (XYZ)", padding=(8, 6))
        view_box.pack(fill=tk.X, padx=8, pady=(0, 8))
        btn_row = ttk.Frame(view_box, style="Panel.TFrame")
        btn_row.pack(fill=tk.X)
        for label in ("+X", "-X", "+Y", "-Y", "+Z", "-Z"):
            ttk.Button(
                btn_row,
                text=label,
                style="Tool.TButton",
                width=3,
                command=lambda v=label.lower(): self._set_axis_view(v),
            ).pack(side=tk.LEFT, padx=1, pady=1)

        rot_box = ttk.LabelFrame(parent, text="Rotate / Zoom", padding=(8, 6))
        rot_box.pack(fill=tk.X, padx=8, pady=(0, 8))
        rot_row = ttk.Frame(rot_box, style="Panel.TFrame")
        rot_row.pack(fill=tk.X)
        ttk.Button(rot_row, text="↺", style="Tool.TButton", width=3, command=lambda: self._nudge_view(-15, 0)).pack(
            side=tk.LEFT, padx=1
        )
        ttk.Button(rot_row, text="↻", style="Tool.TButton", width=3, command=lambda: self._nudge_view(+15, 0)).pack(
            side=tk.LEFT, padx=1
        )
        ttk.Button(rot_row, text="↑", style="Tool.TButton", width=3, command=lambda: self._nudge_view(0, +10)).pack(
            side=tk.LEFT, padx=1
        )
        ttk.Button(rot_row, text="↓", style="Tool.TButton", width=3, command=lambda: self._nudge_view(0, -10)).pack(
            side=tk.LEFT, padx=1
        )
        ttk.Button(rot_box, text="Reset view", style="Tool.TButton", command=self._reset_view).pack(
            anchor="w", pady=(6, 0)
        )
        ttk.Button(rot_box, text="Fit whole domain (F)", style="Tool.TButton", command=self.action_fit_view).pack(
            anchor="w", pady=(4, 0)
        )
        self.view_label = ttk.Label(rot_box, text="view: auto", style="Muted.TLabel")
        self.view_label.pack(anchor="w", pady=(6, 0))
        ttk.Label(
            parent,
            text="Rotate ON → left-drag orbits\nPan ON → left-drag translates\nMid/right = orbit · F = fit",
            style="Muted.TLabel",
            justify=tk.LEFT,
        ).pack(anchor="w", padx=10, pady=(4, 8))

        self.nondim_expanded = tk.BooleanVar(value=True)
        self._nondim_hdr = ttk.Frame(parent, style="Panel.TFrame")
        self._nondim_hdr.pack(fill=tk.X, padx=8, pady=(4, 0))
        ttk.Checkbutton(
            self._nondim_hdr,
            text="Nondim scales ▾",
            variable=self.nondim_expanded,
            command=self._toggle_nondim_panel,
        ).pack(side=tk.LEFT)
        self.nondim_body = ttk.LabelFrame(parent, text="U_ref / Lx / Ly / Lz / T_ref", padding=(8, 4))
        form = ttk.Frame(self.nondim_body, style="Panel.TFrame")
        form.pack(fill=tk.X)
        for key, var in self.scale_vars.items():
            row = ttk.Frame(form, style="Panel.TFrame")
            row.pack(fill=tk.X, pady=2)
            ttk.Label(row, text=key, width=7, style="Panel.TLabel").pack(side=tk.LEFT)
            ttk.Entry(row, textvariable=var, width=10, style="Scale.TEntry").pack(
                side=tk.LEFT, fill=tk.X, expand=True
            )
        ttk.Button(
            self.nondim_body, text="Apply nondim", style="Tool.TButton", command=self.action_nondim
        ).pack(pady=(4, 2), anchor="w")
        self.nondim_body.pack(fill=tk.X, padx=8, pady=(2, 4), after=self._nondim_hdr)

        self._log_label = ttk.Label(parent, text="Log", style="Muted.TLabel")
        self._log_label.pack(anchor="w", padx=10, pady=(8, 0))
        self.log = tk.Text(
            parent,
            height=10,
            width=24,
            bg="#fafbfc",
            fg=FG,
            relief=tk.FLAT,
            highlightthickness=1,
            highlightbackground="#b8c0c8",
            font=("DejaVu Sans Mono", 8),
            wrap=tk.WORD,
        )
        self.log.pack(fill=tk.BOTH, expand=True, padx=8, pady=(2, 10))

    def _toggle_nondim_panel(self) -> None:
        for child in self._nondim_hdr.winfo_children():
            if isinstance(child, ttk.Checkbutton):
                child.configure(
                    text=("Nondim scales ▾" if self.nondim_expanded.get() else "Nondim scales ▸")
                )
        if self.nondim_expanded.get():
            self.nondim_body.pack(fill=tk.X, padx=8, pady=(2, 4), after=self._nondim_hdr)
        else:
            self.nondim_body.pack_forget()

    def _build_status(self) -> None:
        bar = ttk.Frame(self, style="MenuBar.TFrame")
        bar.pack(side=tk.BOTTOM, fill=tk.X)
        ttk.Label(bar, textvariable=self.status_var, style="Title.TLabel", font=("Helvetica", 9)).pack(
            side=tk.LEFT, padx=10, pady=4
        )

    def _on_fit_key(self, event: tk.Event) -> str | None:
        """Key F = Fit whole domain (Tecplot-like). Skip when focus is an Entry/Text."""
        w = event.widget
        try:
            cls = w.winfo_class()
        except tk.TclError:
            cls = ""
        if cls in ("Entry", "TEntry", "Text", "TCombobox", "Spinbox", "TSpinbox"):
            return None
        self.action_fit_view()
        return "break"

    def action_fit_view(self) -> None:
        """Fit whole domain into the Viewport and clear pan offset."""
        self.view_pan_x = 0.0
        self.view_pan_y = 0.0
        self._pan_last = None
        self._panning = False
        # Keep current tool (Rotate/Pan/Probe/Pathlines); only recenter the frame.
        self._apply_default_camera()
        self._update_view_label()
        self._render_cache.clear()
        self._log("Fit — whole domain (F)")
        self._schedule_settle(delay_ms=20)

    def _on_viewport_configure(self, _event: tk.Event | None = None) -> None:
        # During pan/orbit keep the last PNG; settle redraw applies the new view.
        if self._panning or self._dragging:
            return
        if self.last_png and self.last_png.is_file():
            self._show_png_preview(self.last_png)
        else:
            self._draw_viewport_placeholder()

    def _draw_viewport_placeholder(self) -> None:
        c = self.viewport
        c.delete("placeholder")
        c.delete("preview")
        w = max(c.winfo_width(), 2)
        h = max(c.winfo_height(), 2)
        margin = 24
        c.create_rectangle(
            margin,
            margin,
            w - margin,
            h - margin,
            outline="#9aa3ab",
            dash=(6, 4),
            tags="placeholder",
        )
        c.create_text(
            w // 2,
            h // 2 - 10,
            text="Viewport",
            fill=MUTED,
            font=("Helvetica", 16),
            tags="placeholder",
        )
        c.create_text(
            w // 2,
            h // 2 + 18,
            text="File → Open shows Mesh (crate)\nEnable Contour layer for Pressure etc.",
            fill="#8a97a1",
            font=("Helvetica", 10),
            justify=tk.CENTER,
            tags="placeholder",
        )

    def _log(self, msg: str) -> None:
        if not hasattr(self, "log") or self.log is None:
            return
        self.log.insert(tk.END, msg.rstrip() + "\n")
        self.log.see(tk.END)

    def _set_status(self, msg: str) -> None:
        self.status_var.set(msg)

    def _require_cli(self) -> str | None:
        if not self.flowplot:
            messagebox.showerror(
                "flowplot missing",
                "Could not find the flowplot binary.\nRun `cargo build` in the repo root.",
            )
            return None
        return self.flowplot

    def _require_dataset(self) -> Path | None:
        if self.dataset_path and self.dataset_path.is_file():
            return self.dataset_path
        messagebox.showwarning("No dataset", "Load a CFD file first (File → Open).")
        return None

    def _nondim_args(self) -> list[str]:
        mapping = {
            "U_ref": "--u-ref",
            "Lx": "--lx",
            "Ly": "--ly",
            "Lz": "--lz",
            "T_ref": "--t-ref",
        }
        args: list[str] = []
        for key, flag in mapping.items():
            raw = self.scale_vars[key].get().strip()
            if raw:
                args.extend([flag, raw])
        return args

    def _on_close(self) -> None:
        self._kill_cli()
        self.destroy()

    def _kill_cli(self) -> None:
        proc = self._cli_proc
        self._cli_proc = None
        if proc is None or proc.poll() is not None:
            return
        try:
            os.killpg(proc.pid, 15)
        except (ProcessLookupError, PermissionError, OSError):
            try:
                proc.kill()
            except OSError:
                pass

    def _run_cli(self, args: list[str]) -> subprocess.CompletedProcess[str] | None:
        bin_path = self._require_cli()
        if not bin_path:
            return None
        self._kill_cli()
        cmd = [bin_path, *args]
        self._log("$ " + " ".join(cmd))
        self._set_status("Running…")
        self.update_idletasks()
        try:
            proc = subprocess.Popen(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                start_new_session=True,
            )
            self._cli_proc = proc
            # Pump Tk so drag/press can cancel in-flight (Tecplot-like interrupt).
            deadline = time.monotonic() + 120.0
            while proc.poll() is None:
                if time.monotonic() > deadline:
                    self._log("error: flowplot timed out — killing hung CLI")
                    self._kill_cli()
                    self._set_status("Timed out (CLI killed)")
                    return None
                try:
                    self.update()
                except tk.TclError:
                    break
                if self._cli_proc is not proc:
                    # Killed / superseded by drag or newer settle.
                    return None
                time.sleep(0.02)
            if self._cli_proc is not proc:
                return None
            stdout, stderr = proc.communicate(timeout=1)
            self._cli_proc = None
            result = subprocess.CompletedProcess(cmd, proc.returncode, stdout, stderr)
        except OSError as exc:
            self._cli_proc = None
            self._log(f"error: {exc}")
            self._set_status("Failed to launch CLI")
            return None
        out = (result.stdout or "").strip()
        err = (result.stderr or "").strip()
        if out:
            self._log(out if len(out) < 2000 else out[:2000] + "…")
        if err:
            self._log(err)
        if result.returncode != 0:
            self._set_status(f"CLI exit {result.returncode}")
        else:
            self._set_status("Done")
        return result

    def _plane_cli_arg(self) -> list[str]:
        axis = self.plane_axis.get().strip().lower()
        if axis in ("", "auto"):
            return []
        try:
            idx = int(self.plane_index.get())
        except (tk.TclError, ValueError, TypeError):
            idx = 0
        return ["--plane", f"{axis}={idx}"]

    def _view_cli_arg(self) -> list[str]:
        # Always emit a concrete camera so mesh geometry rotates with az/el.
        # (Silent Auto was the jet_cyl meridian bug: Auto=XZ, Free default=+Z/XY.)
        el = self._clamp_elevation(self.elevation_deg)
        return ["--view", f"az={self.azimuth_deg:.2f},el={el:.2f}"]

    def _clamp_elevation(self, el: float) -> float:
        """Keep planar surfaces readable — never true edge-on for the principal plane."""
        el = max(-90.0, min(90.0, el))
        if not self.effectively_2d:
            return el
        min_tilt = 18.0
        plane = (self.principal_plane or "xy").lower()
        if plane == "xy":
            # XY face-on is |el|≈90; el≈0 is edge-on.
            if abs(el) < min_tilt:
                return min_tilt if el >= 0.0 else -min_tilt
            return el
        # XZ / YZ (meridian): face-on is el≈0; poles are edge-on.
        max_el = 90.0 - min_tilt
        if el > max_el:
            return max_el
        if el < -max_el:
            return -max_el
        return el

    def _update_view_label(self) -> None:
        el = self._clamp_elevation(self.elevation_deg)
        text = f"view: az={self.azimuth_deg:.0f}° el={el:.0f}° ({self.principal_plane})"
        if hasattr(self, "view_label"):
            self.view_label.configure(text=text)
        self._draw_axes_triad()

    def _set_axis_view(self, tag: str) -> None:
        tag = tag.strip().lower()
        # Leave Probe/Pathlines/Pan so left-drag orbits after an XYZ snap.
        if self.tool_mode.get() != "rotate":
            self._set_tool_mode("rotate")
        # Free angles matched to Axis projections (see ViewSpec::to_free_angles).
        ortho = {
            "+x": (180.0, 0.0),
            "-x": (0.0, 0.0),
            "+y": (-90.0, 0.0),
            "-y": (90.0, 0.0),
            "+z": (0.0, 90.0),
            "-z": (180.0, -90.0),
        }
        plane = (self.principal_plane or "xy").lower()
        # Planar: avoid true edge-on for the active principal plane.
        if plane == "xy":
            planar_tilt = {
                "+x": (180.0, 28.0),
                "-x": (0.0, 28.0),
                "+y": (-90.0, 28.0),
                "-y": (90.0, 28.0),
                "+z": (0.0, 90.0),
                "-z": (180.0, -90.0),
            }
        elif plane == "xz":
            planar_tilt = {
                "+x": (180.0, 28.0),
                "-x": (0.0, 28.0),
                "+y": (-90.0, 0.0),
                "-y": (90.0, 0.0),
                "+z": (-90.0, 28.0),
                "-z": (-90.0, -28.0),
            }
        else:  # yz
            planar_tilt = {
                "+x": (180.0, 0.0),
                "-x": (0.0, 0.0),
                "+y": (-90.0, 28.0),
                "-y": (90.0, 28.0),
                "+z": (180.0, 28.0),
                "-z": (180.0, -28.0),
            }
        self.view_mode.set("free")
        if self.effectively_2d:
            self.azimuth_deg, self.elevation_deg = planar_tilt.get(tag, ortho.get(tag, (0.0, 90.0)))
            self.elevation_deg = self._clamp_elevation(self.elevation_deg)
            self._log(
                f"View {tag} → az={self.azimuth_deg:.0f} el={self.elevation_deg:.0f} "
                f"(plane {plane})"
            )
        else:
            self.azimuth_deg, self.elevation_deg = ortho.get(tag, (0.0, 90.0))
            self._log(f"View {tag} → az={self.azimuth_deg:.0f} el={self.elevation_deg:.0f}")
        self._update_view_label()
        self._render_cache.clear()
        self._schedule_settle(delay_ms=20)

    def _reset_view(self) -> None:
        if self.tool_mode.get() != "rotate":
            self._set_tool_mode("rotate")
        self._apply_default_camera()
        self.view_pan_x = 0.0
        self.view_pan_y = 0.0
        self._pan_last = None
        self._panning = False
        self._update_view_label()
        self._render_cache.clear()
        self._schedule_settle(delay_ms=20)

    def _apply_default_camera(self, payload: dict | None = None) -> None:
        """Face-on camera for the dataset principal plane (from info JSON when present)."""
        dv = (payload or {}).get("default_view") if isinstance(payload, dict) else None
        if isinstance(dv, dict):
            try:
                self.azimuth_deg = float(dv.get("az", self.azimuth_deg))
                self.elevation_deg = float(dv.get("el", self.elevation_deg))
            except (TypeError, ValueError):
                pass
            plane = str(dv.get("plane") or "").lower()
            if plane in ("xy", "xz", "yz"):
                self.principal_plane = plane
            label = dv.get("label")
            if label:
                self._log(f"default camera {label} (plane {self.principal_plane})")
        else:
            # Fallback from principal_plane alone.
            face = {
                "xy": (0.0, 90.0),
                "xz": (-90.0, 0.0),
                "yz": (180.0, 0.0),
            }
            self.azimuth_deg, self.elevation_deg = face.get(
                self.principal_plane, (0.0, 90.0)
            )
        self.view_mode.set("free")
        self.elevation_deg = self._clamp_elevation(self.elevation_deg)

    def _nudge_view(self, daz: float, del_: float) -> None:
        if self.tool_mode.get() != "rotate":
            self._set_tool_mode("rotate")
        self.view_mode.set("free")
        self.azimuth_deg = (self.azimuth_deg + daz) % 360.0
        self.elevation_deg = self._clamp_elevation(self.elevation_deg + del_)
        self._update_view_label()
        self._render_cache.clear()
        self._schedule_settle(delay_ms=120)

    def _track_plane(self, step: int) -> None:
        axis = self.plane_axis.get().strip().lower()
        if axis == "auto":
            # Prefer K, then J, then I when starting track from Auto.
            if self.zone_ordered:
                if "k" in self.zone_ordered and self.zone_ordered["k"] > 1:
                    self.plane_axis.set("k")
                elif "j" in self.zone_ordered and self.zone_ordered["j"] > 1:
                    self.plane_axis.set("j")
                elif "i" in self.zone_ordered:
                    self.plane_axis.set("i")
                else:
                    self.plane_axis.set("k")
            else:
                self.plane_axis.set("k")
            axis = self.plane_axis.get()
        try:
            idx = int(self.plane_index.get()) + step
        except (tk.TclError, ValueError, TypeError):
            idx = 0
        lo, hi = 0, self._plane_max_index(axis)
        idx = max(lo, min(hi, idx))
        self.plane_index.set(idx)
        # Keep last frame on screen while cutting; redraw only after settle.
        self._set_status(f"Cutting {axis}={idx}…")
        self._schedule_settle(delay_ms=90)

    def _plane_max_index(self, axis: str) -> int:
        if not self.zone_ordered:
            return 9999
        n = int(self.zone_ordered.get(axis, 1))
        return max(0, n - 1)

    def _cancel_redraw_timers(self) -> None:
        for attr in ("_redraw_after", "_fullres_after", "_settle_after", "_orbit_preview_after"):
            handle = getattr(self, attr, None)
            if handle is not None:
                try:
                    self.after_cancel(handle)
                except tk.TclError:
                    pass
                setattr(self, attr, None)

    def _schedule_settle(self, *, delay_ms: int = 80) -> None:
        """After interaction stops: cancel in-flight CLI, then preview → full."""
        self._cancel_redraw_timers()
        self._kill_cli()
        self._render_gen += 1  # invalidate any in-flight result
        self._settle_after = self.after(delay_ms, self._do_settle)

    def _do_settle(self) -> None:
        """Progressive refine: cheap preview first, then full contour fill."""
        self._settle_after = None
        if self._dragging or self._panning or self._settle_running:
            return
        self._settle_running = True
        try:
            self.action_redraw(interactive=True)
            if self._dragging or self._panning:
                return
            # Second tier — full fill (cancelable if user starts dragging again).
            self._fullres_after = self.after(30, self._do_settle_full)
        finally:
            self._settle_running = False

    def _do_settle_full(self) -> None:
        self._fullres_after = None
        if self._dragging or self._panning or self._in_redraw:
            return
        self.action_redraw(interactive=False)

    def _on_view_press(self, event: tk.Event) -> None:
        mode = self.tool_mode.get()
        if mode == "probe":
            self._probe_at_canvas(event.x, event.y)
            return
        if mode == "pathlines":
            self._pl_drag_start = (event.x, event.y)
            self.viewport.delete("pathline_drag")
            return
        if mode == "pan":
            # Tecplot Translate: left-drag pans the domain.
            self._on_pan_press(event)
            return
        # Rotate tool (default): left-drag orbits az/el — domain geometry, not PNG slide.
        self._on_orbit_press(event)

    def _on_view_drag(self, event: tk.Event) -> None:
        mode = self.tool_mode.get()
        if mode == "probe":
            return
        if mode == "pathlines":
            if self._pl_drag_start is None:
                return
            x0, y0 = self._pl_drag_start
            self._draw_pathline_rake_preview(x0, y0, event.x, event.y)
            return
        if self._panning or mode == "pan":
            self._on_pan_drag(event)
            return
        self._on_orbit_drag(event)

    def _on_view_release(self, event: tk.Event) -> None:
        mode = self.tool_mode.get()
        if mode == "probe":
            return
        if mode == "pathlines":
            self._finish_pathline_drag(event.x, event.y)
            return
        if self._panning or mode == "pan":
            self._on_pan_release(event)
            return
        self._on_orbit_release(event)

    def _on_pan_press(self, event: tk.Event) -> None:
        """Begin domain pan — view offset updates; settle redraw on release."""
        self._pan_last = (event.x, event.y)
        self._panning = True
        self._dragging = False
        self._cancel_redraw_timers()
        self._kill_cli()
        self._render_gen += 1
        self._set_status(
            f"Panning… pan=({self.view_pan_x:.0f},{self.view_pan_y:.0f}) · settle on release"
        )

    def _on_pan_drag(self, event: tk.Event) -> None:
        if self._pan_last is None:
            return
        dx = event.x - self._pan_last[0]
        dy = event.y - self._pan_last[1]
        self._pan_last = (event.x, event.y)
        self.view_pan_x += float(dx)
        self.view_pan_y += float(dy)
        # Keep last PNG in place; domain pan is applied by CLI settle redraw.
        self._set_status(
            f"Panning… pan=({self.view_pan_x:.0f},{self.view_pan_y:.0f}) · settle on release"
        )

    def _on_pan_release(self, _event: tk.Event) -> None:
        self._pan_last = None
        self._panning = False
        self._set_status(
            f"Pan ({self.view_pan_x:.0f},{self.view_pan_y:.0f}) · settling…"
        )
        self._render_cache.clear()
        self._schedule_settle(delay_ms=50)

    def _preview_center(self) -> tuple[int, int]:
        """Centered preview — pan is baked into the PNG via --pan-x/--pan-y."""
        cw = max(self.viewport.winfo_width(), 2)
        ch = max(self.viewport.winfo_height(), 2)
        return cw // 2, ch // 2

    def _draw_pathline_rake_preview(self, x0: int, y0: int, x1: int, y1: int) -> None:
        """Draw the rake/seed line while dragging in Pathlines mode."""
        self.viewport.delete("pathline_drag")
        self.viewport.create_line(
            x0, y0, x1, y1, fill="#c00000", width=2, tags="pathline_drag"
        )
        dist = ((x1 - x0) ** 2 + (y1 - y0) ** 2) ** 0.5
        n = 1 if dist < 8 else max(2, min(16, int(dist / 18) + 1))
        for i in range(n):
            t = 0.0 if n == 1 else i / (n - 1)
            sx = int(round(x0 + t * (x1 - x0)))
            sy = int(round(y0 + t * (y1 - y0)))
            self.viewport.create_oval(
                sx - 3, sy - 3, sx + 3, sy + 3, outline="#c00000", fill="#ffd0d0", width=1, tags="pathline_drag"
            )

    def _on_orbit_press(self, event: tk.Event) -> None:
        """Begin orbit drag (left in Orbit mode, or always via Button-2/3)."""
        self._drag_last = (event.x, event.y)
        self._dragging = True
        self._cancel_redraw_timers()
        self._kill_cli()
        self._render_gen += 1

    def _on_orbit_drag(self, event: tk.Event) -> None:
        if self._drag_last is None:
            return
        dx = event.x - self._drag_last[0]
        dy = event.y - self._drag_last[1]
        self._drag_last = (event.x, event.y)
        self.view_mode.set("free")
        self.azimuth_deg = (self.azimuth_deg + dx * 0.4) % 360.0
        self.elevation_deg = self._clamp_elevation(self.elevation_deg - dy * 0.4)
        self._update_view_label()
        # Live mesh geometry orbit (throttled CLI) — never slide/warp the last PNG.
        self._set_status(
            f"Orbiting… az={self.azimuth_deg:.0f}° el={self.elevation_deg:.0f}° · mesh camera"
        )
        self._schedule_orbit_preview()

    def _schedule_orbit_preview(self) -> None:
        """Throttle mesh-only camera redraws while the mouse is moving."""
        if self._orbit_preview_after is not None:
            return
        self._orbit_preview_after = self.after(70, self._do_orbit_preview)

    def _do_orbit_preview(self) -> None:
        self._orbit_preview_after = None
        if not self._dragging or self._panning:
            return
        # Mesh-only, allow during drag — shows domain rotating with az/el.
        self.action_redraw(interactive=True, allow_during_drag=True, mesh_only=True)
        if self._dragging and not self._panning:
            self._orbit_preview_after = self.after(110, self._do_orbit_preview)

    def _on_orbit_release(self, _event: tk.Event) -> None:
        self._drag_last = None
        self._dragging = False
        if self._orbit_preview_after is not None:
            try:
                self.after_cancel(self._orbit_preview_after)
            except tk.TclError:
                pass
            self._orbit_preview_after = None
        self._render_cache.clear()
        self._schedule_settle(delay_ms=40)

    def _plane_key(self) -> tuple[str, int]:
        axis = self.plane_axis.get().strip().lower() or "auto"
        try:
            idx = int(self.plane_index.get())
        except (tk.TclError, ValueError, TypeError):
            idx = 0
        return (axis, idx)

    def _remember_settle_frame(self, img) -> None:
        """Store last good raster + view state for drag-time warping."""
        try:
            self._settle_image = img.copy()
        except Exception:
            self._settle_image = img
        self._settle_az = float(self.azimuth_deg)
        self._settle_el = float(self.elevation_deg)
        self._settle_plane = self._plane_key()

    def _show_png_preview(self, path: Path) -> None:
        """Best-effort PNG preview in the viewport (Pillow optional)."""
        self.last_png = path
        try:
            from PIL import Image, ImageTk
        except ImportError:
            self._log(f"Wrote {path} (install Pillow for in-window preview)")
            return
        # Viewport may still be 1×1 during first layout — defer until mapped.
        cw = self.viewport.winfo_width()
        ch = self.viewport.winfo_height()
        if cw < 32 or ch < 32:
            if self._preview_after is not None:
                try:
                    self.after_cancel(self._preview_after)
                except tk.TclError:
                    pass
            self._preview_after = self.after(50, lambda: self._show_png_preview(path))
            return
        try:
            img = Image.open(path).convert("RGBA")
        except OSError as exc:
            self._log(f"Could not open PNG: {exc}")
            return
        # Keep a private copy so temp-file rewrites cannot invalidate PhotoImage.
        preview_path = Path(tempfile.gettempdir()) / "flowplot_gui_preview.png"
        try:
            if path.resolve() != preview_path.resolve():
                shutil.copyfile(path, preview_path)
            img = Image.open(preview_path).convert("RGBA")
        except OSError:
            pass
        self.viewport.delete("placeholder")
        self.viewport.delete("preview")
        self.viewport.delete("probe")
        native = (img.width, img.height)
        # Prefer exact 1:1 — bilinear cover stretch destroys annotation glyphs.
        target_w = max(cw - 2, 16)
        target_h = max(ch - 2, 16)
        if abs(img.width - target_w) <= 2 and abs(img.height - target_h) <= 2:
            show = img
        else:
            sx = target_w / max(img.width, 1)
            sy = target_h / max(img.height, 1)
            scale = min(sx, sy)  # contain — whole domain, no crop
            new_w = max(1, int(round(img.width * scale)))
            new_h = max(1, int(round(img.height * scale)))
            # NEAREST keeps HUD/label glyphs crisp when a scale is unavoidable.
            resample = (
                Image.Resampling.NEAREST
                if abs(scale - 1.0) < 0.02 or scale > 0.85
                else Image.Resampling.BILINEAR
            )
            show = img.resize((new_w, new_h), resample)
        if show.width < 4 or show.height < 4:
            self._log(
                f"Preview image too small after fit ({show.width}×{show.height}); "
                "check plane/coords (thin domain)."
            )
        self._preview_photo = ImageTk.PhotoImage(show)
        ox, oy = self._preview_center()
        self.viewport.create_image(ox, oy, image=self._preview_photo, tags="preview")
        self._preview_offset = (ox - show.width // 2, oy - show.height // 2)
        self._preview_size = (show.width, show.height)
        self._preview_native = native
        self._draw_probe_overlays()
        self._draw_persistent_rake()
        self._draw_axes_triad()
        # Keep a full-res settle copy for drag-time warping (before thumbnail).
        try:
            settle_src = Image.open(preview_path).convert("RGBA")
            self._remember_settle_frame(settle_src)
        except OSError:
            self._remember_settle_frame(img)
        prev = getattr(self, "_preview_logged", None)
        key = (str(path), show.width, show.height, cw, ch)
        if prev != key:
            self._preview_logged = key
            self._log(
                f"Viewport: showed {path.name} ({native[0]}×{native[1]} → {show.width}×{show.height} in {cw}×{ch})"
            )

    def _set_variables(self, variables: list[str]) -> None:
        self.variables = list(variables)
        self.var_combo.configure(values=self.variables, state="readonly" if self.variables else "disabled")
        default = first_scalar(self.variables)
        if default:
            self.var_name.set(default)
        self._log(f"variables: {', '.join(self.variables) if self.variables else '(none)'}")
        self._log(f"default contour var: {default or '(none)'} (Contour layer still off)")

    def _on_plot_type(self) -> None:
        pt = self.plot_type.get()
        if pt not in ("2D Cartesian", "3D Cartesian"):
            messagebox.showinfo(
                "Plot type",
                f"{pt} is a stub in this build.\nUse 2D or 3D Cartesian for mesh/contour.",
            )
            self.plot_type.set("2D Cartesian")
            return
        if pt == "3D Cartesian" and self.effectively_2d:
            # Allow 3D Cartesian as a surface-in-3D view (planar IJ still orbits).
            self._log(
                "3D Cartesian on planar/unit-thickness data — "
                "orbiting the surface (elevation stays off edge-on)."
            )
            self.view_mode.set("free")
            self.azimuth_deg = 35.0
            self.elevation_deg = self._clamp_elevation(28.0)
            self._update_view_label()
            if self.tool_mode.get() != "rotate":
                self._set_tool_mode("rotate")
        self._render_cache.clear()
        self.action_redraw()

    def _on_var_changed(self) -> None:
        if self.layer_contour.get():
            self.action_redraw()

    # --- Actions ---

    def action_load(self) -> None:
        # Modal Contour/Legend grabs block dialogs on Linux.
        release_grabs(self)
        # Details list (Name | Size | Modified) — native Linux filedialogs hide mtime.
        initial = None
        if self.dataset_path is not None:
            initial = self.dataset_path.parent
        path = ask_open_dataset(self, initialdir=initial)
        if not path:
            self._log("Open cancelled (no file returned)")
            self._set_status("Open cancelled")
            return
        chosen = Path(path)
        if not chosen.is_file():
            messagebox.showerror("Open failed", f"File not found:\n{chosen}", parent=self)
            return
        self.dataset_path = chosen
        try:
            os.environ["FIELDPLOT_LAST_DIR"] = str(chosen.parent)
        except Exception:
            pass
        try:
            st = chosen.stat()
            self._log(
                f"Loaded path: {self.dataset_path}  "
                f"({st.st_size} bytes, mtime {datetime.fromtimestamp(st.st_mtime):%Y-%m-%d %H:%M:%S})"
            )
        except OSError:
            self._log(f"Loaded path: {self.dataset_path}")
        self._set_status(f"Opening {chosen.name}…")
        self._update_dataset_label()
        self.lift()
        self.focus_force()
        self.update_idletasks()

        # Reset layers to Tecplot defaults: Mesh on, Contour off.
        self.layer_mesh.set(True)
        self.layer_contour.set(False)
        self.layer_pathlines.set(False)
        self.layer_shade.set(False)
        self.layer_vector.set(False)
        self.layer_edge.set(False)
        self.layer_scatter.set(False)
        self.pathline_seeds.clear()
        self.clear_probe_marks()
        self._set_tool_mode("rotate")
        self.plane_axis.set("auto")
        self.plane_index.set(0)
        self.active_zone = 0
        self.mesh_color = DEFAULT_MESH_COLOR
        self.zone_styles = []
        self._refresh_zone_list()
        self._cancel_redraw_timers()
        self._kill_cli()
        self._render_cache.clear()
        self._reset_view_state()

        proc = self._run_cli(["--json", "info", str(self.dataset_path)])
        if proc and proc.returncode == 0 and proc.stdout:
            try:
                payload = json.loads(proc.stdout.strip().splitlines()[-1])
                vars_ = payload.get("variables") or []
                if isinstance(vars_, list):
                    self._set_variables([str(v) for v in vars_])
                self.effectively_2d = bool(payload.get("effectively_2d", True))
                self._apply_default_camera(payload)
                self._update_view_label()
                # True volumes → 3D Cartesian so Rotate/XYZ feel like Tecplot 3D.
                if not self.effectively_2d and self.plot_type.get() == "2D Cartesian":
                    self.plot_type.set("3D Cartesian")
                    self._log("Volume dataset — plot type → 3D Cartesian")
                self.zone_ordered = None
                zones = payload.get("zones") or []
                if isinstance(zones, list) and zones:
                    self.zone_styles = zone_styles_from_info(
                        [z for z in zones if isinstance(z, dict)]
                    )
                    self._log(
                        "zones: "
                        + ", ".join(f"{z.index}:{z.name}" for z in self.zone_styles)
                    )
                    ordered = zones[0].get("ordered") if isinstance(zones[0], dict) else None
                    if isinstance(ordered, dict):
                        self.zone_ordered = {
                            str(k).lower(): int(v)
                            for k, v in ordered.items()
                            if str(k).lower() in ("i", "j", "k")
                        }
                        self._log(
                            "ordered dims: "
                            + ", ".join(f"{k}={v}" for k, v in sorted(self.zone_ordered.items()))
                        )
                if self.plot_type.get() == "3D Cartesian" and self.effectively_2d:
                    self._log(
                        "Dataset is planar/unit-thickness — 3D Cartesian orbits the surface "
                        "(elevation stays off edge-on)."
                    )
            except json.JSONDecodeError:
                self._log("warning: could not parse info JSON; variable list empty")
                self._set_variables([])
                self.effectively_2d = True
                self.zone_ordered = None
                self.zone_styles = []
        else:
            self._set_variables([])
            self.effectively_2d = True
            self.zone_ordered = None
            self.zone_styles = []

        self._refresh_zone_list()
        self._set_status(str(self.dataset_path))
        # Defer first mesh redraw so the Open dialog can fully dismiss first.
        self.after(1, lambda: self._schedule_settle(delay_ms=10))

    def _reset_view_state(self) -> None:
        self.view_mode.set("free")
        self.principal_plane = "xy"
        self.azimuth_deg = 0.0
        self.elevation_deg = 90.0
        self.view_pan_x = 0.0
        self.view_pan_y = 0.0
        self._pan_last = None
        self._panning = False
        self._settle_image = None
        self._settle_az = 0.0
        self._settle_el = 90.0
        self._settle_plane = None
        self._update_view_label()

    def action_redraw(
        self,
        interactive: bool = False,
        allow_during_drag: bool = False,
        mesh_only: bool = False,
    ) -> None:
        # Re-entrancy guard: _run_cli pumps Tk events; avoid nested redraws.
        if self._in_redraw:
            return
        ds = self._require_dataset()
        if not ds:
            return
        # While panning, wait for settle. Orbit may request live mesh previews.
        if self._panning:
            return
        if self._dragging and not allow_during_drag:
            return
        if self.plot_type.get() not in ("2D Cartesian", "3D Cartesian"):
            self._log(f"plot type {self.plot_type.get()} not drawable yet")
            return
        self.elevation_deg = self._clamp_elevation(self.elevation_deg)

        want_mesh = True if mesh_only else self.layer_mesh.get()
        want_shade = False if mesh_only else self.layer_shade.get()
        want_vector = False if mesh_only else self.layer_vector.get()
        want_edge = False if mesh_only else self.layer_edge.get()
        want_scatter = False if mesh_only else self.layer_scatter.get()
        want_iso = False if mesh_only else self.layer_isosurfaces.get()
        # Shade/iso/vector/edge/scatter need a scalar raster base.
        want_contour = False if mesh_only else (
            self.layer_contour.get()
            or want_shade
            or want_iso
            or want_vector
            or want_edge
            or want_scatter
        )
        want_pathlines = False if mesh_only else (
            self.layer_pathlines.get() and bool(self.pathline_seeds)
        )
        zone_idx = self._active_zone_index()
        zone_args = ["--zone", str(zone_idx)]
        mesh_color_args = self._mesh_color_cli_args()

        if (
            not want_mesh
            and not want_contour
            and not want_pathlines
            and not self.layer_pathlines.get()
        ):
            self.last_png = None
            self._settle_image = None
            self._draw_viewport_placeholder()
            self._set_status("No layers enabled")
            return
        if (
            self.layer_pathlines.get()
            and not self.pathline_seeds
            and not want_contour
            and not want_mesh
        ):
            self._set_status("Pathlines — click-drag a rake/seed line")
            return

        # Exact Viewport raster → 1:1 display (annotation glyphs stay crisp).
        cw = max(self.viewport.winfo_width(), 64)
        ch = max(self.viewport.winfo_height(), 64)
        if mesh_only and interactive:
            # Live orbit: cheaper mesh camera redraw while dragging.
            width, height = min(max(cw // 2, 320), 960), min(max(ch // 2, 240), 720)
            quality = "orbit"
        else:
            # Always match canvas size (half-res interactive stretched glyphs → "broken").
            width, height = min(max(cw, 64), 1920), min(max(ch, 64), 1440)
            quality = "preview" if interactive else "full"

        out = Path(tempfile.gettempdir()) / f"flowplot_gui_view_{quality}.png"
        style = self.plot_style.get()
        nondim = self._nondim_args()
        plane = self._plane_cli_arg()
        view = self._view_cli_arg()
        var = self.var_name.get().strip()
        size = ["--width", str(width), "--height", str(height)]
        cd = self.contour_details
        contour_args = self._contour_cli_args()

        seed_key = ";".join(f"{x:.6g},{y:.6g}" for x, y in self.pathline_seeds[-24:])
        pan_args = self._pan_cli_args()
        cache_key = "|".join(
            [
                str(ds),
                f"m={want_mesh}",
                f"c={want_contour}",
                f"s={want_shade}",
                f"p={want_pathlines}",
                f"vec={want_vector}:{self.vector_u.get()}:{self.vector_v.get()}",
                f"edge={want_edge}",
                f"sc={want_scatter}",
                f"iso={want_iso}",
                f"seeds={seed_key}",
                f"z={zone_idx}",
                f"mc={','.join(mesh_color_args)}",
                f"cd={','.join(contour_args)}",
                f"pan={','.join(pan_args)}",
                var,
                style,
                *plane,
                *view,
                *size,
                *nondim,
            ]
        )
        cached = self._render_cache.get(cache_key)
        if cached and cached.is_file():
            self._show_png_preview(cached)
            self._set_status(f"Cached {quality}")
            return

        self._render_gen += 1
        gen = self._render_gen
        self._set_status("Preview…" if quality == "preview" else "Rendering…")
        self._in_redraw = True
        proc = None
        try:
            proc = self._redraw_cli(
                ds=ds,
                out=out,
                style=style,
                nondim=nondim,
                plane=plane,
                view=view,
                var=var,
                size=size,
                zone_args=zone_args,
                mesh_color_args=mesh_color_args,
                want_mesh=want_mesh,
                want_contour=want_contour,
                want_pathlines=want_pathlines,
                want_vector=want_vector,
                want_edge=want_edge,
                want_scatter=want_scatter,
                want_iso=want_iso,
                interactive=interactive,
            )
        finally:
            self._in_redraw = False

        # Drop stale results if a newer redraw started (cancel/supersede).
        if gen != self._render_gen:
            return

        if proc and proc.returncode == 0 and out.is_file():
            stable = Path(tempfile.gettempdir()) / f"flowplot_cache_{abs(hash(cache_key)) & 0xFFFFFFFF:08x}.png"
            try:
                shutil.copyfile(out, stable)
                self._render_cache[cache_key] = stable
                while len(self._render_cache) > 24:
                    self._render_cache.pop(next(iter(self._render_cache)))
            except OSError:
                stable = out
            self._show_png_preview(stable)
            layers = []
            if want_mesh:
                layers.append("Mesh")
            if want_shade and not self.layer_contour.get():
                layers.append(f"Shade({var})")
            elif self.layer_contour.get():
                layers.append(f"Contour({var})")
            if want_pathlines:
                layers.append("Pathlines")
            if want_vector:
                layers.append("Vector")
            if want_edge:
                layers.append("Edge")
            if want_scatter:
                layers.append("Scatter")
            if want_iso:
                layers.append("Iso")
            tag = "preview" if quality == "preview" else "full"
            zname = self._zone_label(zone_idx)
            n_traces = None
            if want_pathlines and proc and proc.stdout:
                try:
                    payload = json.loads(proc.stdout.strip().splitlines()[-1])
                    n_traces = payload.get("pathline_traces")
                    if n_traces is None and payload.get("command") == "pathlines":
                        n_traces = payload.get("traces")
                except (json.JSONDecodeError, IndexError, TypeError):
                    n_traces = None
            status = (" + ".join(layers) if layers else "Done") + f" · zone {zname} [{tag}]"
            if want_pathlines and n_traces is not None:
                status += f" · {n_traces} pathline(s)"
                if int(n_traces) == 0:
                    self._log(
                        "Pathlines: 0 traces — check U/V vars and seeds inside the mesh "
                        "(dt is auto from |U|)."
                    )
                else:
                    self._log(f"Pathlines: {n_traces} integrated trace(s) on settle")
            self._set_status(status)

    def _redraw_cli(
        self,
        *,
        ds: Path,
        out: Path,
        style: str,
        nondim: list[str],
        plane: list[str],
        view: list[str],
        var: str,
        size: list[str],
        zone_args: list[str],
        mesh_color_args: list[str],
        want_mesh: bool,
        want_contour: bool,
        want_pathlines: bool,
        want_vector: bool = False,
        want_edge: bool = False,
        want_scatter: bool = False,
        want_iso: bool = False,
        interactive: bool = False,
    ) -> subprocess.CompletedProcess[str] | None:
        del interactive  # always full-size raster; overlays stay on every settle.
        needs_scalar = (
            want_contour or want_pathlines or want_vector or want_edge or want_scatter or want_iso
        )
        if needs_scalar:
            var = (var or "").strip() or first_scalar(self.variables) or ""
            if var:
                self.var_name.set(var)
            else:
                # Never block the UI with a modal — quietly drop scalar layers.
                skipped = self._quietly_disable_scalar_layers()
                self._log(
                    "No scalar variable available — skipped "
                    + (", ".join(skipped) if skipped else "Contour/Iso")
                    + "; drawing Mesh only."
                )
                self._set_status("No scalar var — Mesh only")
                needs_scalar = False
                want_contour = want_pathlines = want_vector = False
                want_edge = want_scatter = want_iso = False
        if needs_scalar:
            args = [
                "contour",
                str(ds),
                "--var",
                var,
                "-o",
                str(out),
                "--style",
                style,
                *zone_args,
                *size,
                *plane,
                *view,
                *self._pan_cli_args(),
                *nondim,
                *self._contour_cli_args(),
            ]
            # Shade / Contour flood sits under mesh wires when Mesh (or Shade) is on.
            if want_mesh or self.layer_shade.get():
                args.append("--mesh")
                args.extend(mesh_color_args)
            if want_pathlines:
                args.append("--pathlines")
                args.extend(self._pathline_seed_args())
            if want_vector:
                args.append("--vectors")
                args.extend(self._vector_cli_vars())
            if want_edge:
                args.append("--edges")
            if want_scatter:
                args.append("--scatter")
            if want_iso:
                args.append("--isolines")
            if want_vector or want_scatter:
                skip = (
                    int(self.vector_details.skip)
                    if want_vector
                    else int(self.scatter_details.skip)
                )
                if want_vector and want_scatter:
                    skip = max(int(self.vector_details.skip), int(self.scatter_details.skip))
                args.extend(["--overlay-skip", str(max(1, skip))])
            # JSON so we can report pathline_traces after rake settle.
            if want_pathlines and args[:1] != ["--json"]:
                args = ["--json", *args]
            return self._run_cli(args)
        args = [
            "mesh",
            str(ds),
            "-o",
            str(out),
            "--style",
            style,
            "--fill",
            "--margin",
            "2",
            *zone_args,
            *mesh_color_args,
            *size,
            *plane,
            *view,
            *self._pan_cli_args(),
            *nondim,
        ]
        return self._run_cli(args)

    def _quietly_disable_scalar_layers(self) -> list[str]:
        """Turn off Contour/Iso/… without a modal when no scalar variable exists."""
        skipped: list[str] = []
        for label, var in (
            ("Contour", self.layer_contour),
            ("Shade", self.layer_shade),
            ("Vector", self.layer_vector),
            ("Edge", self.layer_edge),
            ("Scatter", self.layer_scatter),
            ("Iso-Surfaces", self.layer_isosurfaces),
            ("Pathlines", self.layer_pathlines),
        ):
            if var.get():
                var.set(False)
                skipped.append(label)
        if not self.layer_mesh.get():
            self.layer_mesh.set(True)
        return skipped

    def _ensure_contour_var(self) -> str:
        """Auto-pick first scalar; empty string if none (caller must not modal)."""
        var = self.var_name.get().strip()
        if var:
            return var
        var = first_scalar(self.variables) or ""
        if var:
            self.var_name.set(var)
        return var

    def _pan_cli_args(self) -> list[str]:
        """Pass domain pan into the CLI so settle redraw moves the field, not the PNG.

        Use ``--pan-y=-12`` form so negative values are not parsed as flags.
        """
        return [
            f"--pan-x={float(self.view_pan_x):.3f}",
            f"--pan-y={float(self.view_pan_y):.3f}",
        ]

    def _contour_cli_args(self) -> list[str]:
        cd = self.contour_details
        # Shade alone prefers continuous flood; Contour Details coloring wins when Contour on.
        coloring = cd.cli_coloring()
        if self.layer_shade.get() and not self.layer_contour.get():
            coloring = "continuous"
        args = [
            "--coloring",
            coloring,
            "--levels",
            str(max(2, int(cd.nlevels))),
            "--colormap",
            cd.cli_colormap(),
            "--margin",
            "2",
            "--legend-align",
            cd.cli_legend_align(),
            "--legend-font",
            str(float(cd.legend_font_px or 0.0)),
        ]
        if cd.legend:
            args.append("--legend")
        else:
            args.append("--no-legend")
        if cd.show_labels:
            args.append("--labels")
        return args

    def action_enable_contour(self) -> None:
        if not self._ensure_contour_var():
            self.layer_contour.set(False)
            self._log("Contour skipped — no scalar variable in dataset")
            self._set_status("Contour unavailable (no scalar)")
            return
        self.layer_contour.set(True)
        # Keep Zone Style Contour Show in sync for the active zone.
        zs = self._style_for_zone(self.active_zone)
        if zs is not None:
            zs.show_contour = True
            zs.show_zone = True
        self.action_redraw()

    def action_contour_details(self) -> None:
        """Plot → Contour… / toolbar Details — Tecplot Contour Details."""
        self.action_enable_contour()
        existing = self._contour_details_dialog
        if existing is not None and existing.winfo_exists():
            stay_above(existing, self, modal=False)
            return
        dlg = ContourDetailsDialog(
            self,
            self.contour_details,
            on_apply=self._apply_contour_details,
        )
        self._contour_details_dialog = dlg

    def _apply_contour_details(self, state: ContourDetailsState) -> None:
        self.contour_details = state
        self._refresh_levels_label()
        self.layer_contour.set(True)
        self._render_cache.clear()
        self.action_redraw()

    def action_contour_add_level(self) -> None:
        """Tecplot Contour ＋ Level — bump count, force Banded Contour, redraw."""
        self.contour_details.nlevels = min(64, int(self.contour_details.nlevels) + 1)
        self.contour_details.coloring = "Banded"
        self.layer_contour.set(True)
        if not self.var_name.get().strip() and self.variables:
            self.var_name.set(first_scalar(self.variables))
        self._refresh_levels_label()
        self._log(f"Contour levels → {self.contour_details.nlevels} (banded)")
        self._render_cache.clear()
        self.action_redraw()

    def action_contour_remove_level(self) -> None:
        """Tecplot Contour － Level — drop count, force Banded Contour, redraw."""
        self.contour_details.nlevels = max(2, int(self.contour_details.nlevels) - 1)
        self.contour_details.coloring = "Banded"
        self.layer_contour.set(True)
        if not self.var_name.get().strip() and self.variables:
            self.var_name.set(first_scalar(self.variables))
        self._refresh_levels_label()
        self._log(f"Contour levels → {self.contour_details.nlevels} (banded)")
        self._render_cache.clear()
        self.action_redraw()

    def action_toggle_contour_labels(self) -> None:
        self.contour_details.show_labels = not self.contour_details.show_labels
        if self.contour_details.show_labels and self.contour_details.coloring.lower().startswith(
            "cont"
        ):
            self.contour_details.coloring = "Banded"
        self._refresh_levels_label()
        self._log("Contour labels " + ("on" if self.contour_details.show_labels else "off"))
        self.layer_contour.set(True)
        self._render_cache.clear()
        self.action_redraw()

    def action_legend_options(self) -> None:
        cd = self.contour_details
        state = LegendOptionsState(
            show=cd.legend,
            align=cd.legend_align,
            font_px=cd.legend_font_px,
            colorbar=cd.legend,
        )
        existing = self._legend_options_dialog
        if existing is not None and existing.winfo_exists():
            stay_above(existing, self, modal=False)
            return

        def on_apply(st: LegendOptionsState) -> None:
            self.contour_details.legend = bool(st.show and st.colorbar)
            self.contour_details.legend_align = st.align
            self.contour_details.legend_font_px = float(st.font_px or 0.0)
            self.layer_contour.set(True)
            self._render_cache.clear()
            self.action_redraw()

        self._legend_options_dialog = LegendOptionsDialog(self, state, on_apply=on_apply)

    def _set_tool_mode(self, mode: str) -> None:
        """Exclusive tools: rotate | pan | probe | pathlines (Tecplot Rotate vs Translate)."""
        # Legacy alias from earlier "orbit" naming.
        if mode == "orbit":
            mode = "rotate"
        mode = mode if mode in ("rotate", "pan", "probe", "pathlines") else "rotate"
        prev = self.tool_mode.get()
        self.tool_mode.set(mode)
        self.viewport.delete("pathline_drag")
        self._pl_drag_start = None
        self._panning = False
        self._dragging = False
        self._pan_last = None
        self._drag_last = None
        if mode == "probe":
            self.viewport.configure(cursor="crosshair")
            self._set_status("Probe — click Viewport")
            self.action_open_probe_window()
        elif mode == "pathlines":
            self.viewport.configure(cursor="pencil")
            self.layer_pathlines.set(True)
            self._set_status("Pathlines — click-drag a rake/seed line")
        elif mode == "pan":
            self.viewport.configure(cursor="fleur")
            self._set_status("Pan / translate — left-drag moves the domain · F=fit")
        else:
            # Rotate: exchange-style cursor; left-drag orbits az/el.
            try:
                self.viewport.configure(cursor="exchange")
            except tk.TclError:
                self.viewport.configure(cursor="cross")
            self._set_status("Rotate — left-drag orbits az/el · mid/right also orbit · F=fit")
        if hasattr(self, "_icon_toolbar"):
            self._icon_toolbar.set_active_tools({mode})
        if prev != mode:
            self._log(f"Tool mode: {prev} → {mode}")
        self._refresh_levels_label()

    def action_rotate_tool(self) -> None:
        """Curved-arrow Rotate toolbar — exclusive orbit tool (left-drag az/el)."""
        self._set_tool_mode("rotate")
        self._log("Rotate tool ON — left-drag orbits the domain")

    def action_pan_tool(self) -> None:
        """Pan / translate tool — left-drag moves the domain in screen space."""
        self._set_tool_mode("pan")
        self._log("Pan tool ON — left-drag translates the domain")

    def _ensure_pathline_velocity_vars(self) -> bool:
        """Fill U/V(/W) from dataset names when empty. Returns False if U/V missing."""
        gu, gv, gw = self._guess_velocity_names()
        if not self.vector_u.get().strip() and gu:
            self.vector_u.set(gu)
        if not self.vector_v.get().strip() and gv:
            self.vector_v.set(gv)
        if not self.vector_w.get().strip() and gw:
            self.vector_w.set(gw)
        return bool(self.vector_u.get().strip() and self.vector_v.get().strip())

    def _on_pathlines_layer(self) -> None:
        """Derived Pathlines checkbox — enable rake tool; do not fight Probe."""
        if self.layer_pathlines.get():
            if not self._ensure_pathline_velocity_vars():
                self.layer_pathlines.set(False)
                messagebox.showwarning(
                    "Pathlines",
                    "No U/V velocity variables found.\n"
                    "Pathlines need velocity components on the loaded dataset.",
                    parent=self,
                )
                return
            self._set_tool_mode("pathlines")
            self._log(
                "Pathlines on — click-drag a rake/seed line "
                f"(U={self.vector_u.get()}, V={self.vector_v.get()})"
            )
            if self.pathline_seeds:
                self._render_cache.clear()
                self.action_redraw()
            return
        if self.tool_mode.get() == "pathlines":
            self._set_tool_mode("rotate")
        self._render_cache.clear()
        self.action_redraw()

    def action_toggle_probe(self) -> None:
        if self.tool_mode.get() == "probe":
            self._set_tool_mode("rotate")
        else:
            self._set_tool_mode("probe")

    def action_pathlines_tool(self) -> None:
        """Enter Pathlines rake mode (Tecplot-style click-drag seed line)."""
        if not self._ensure_pathline_velocity_vars():
            messagebox.showwarning(
                "Pathlines",
                "No U/V velocity variables found in this dataset.\n"
                "Pathlines need velocity components (UI name is Pathlines, not Streamtraces).",
                parent=self,
            )
            return
        self._set_tool_mode("pathlines")
        self._log(
            f"Pathlines tool — rake seeds (U={self.vector_u.get()}, V={self.vector_v.get()})"
        )

    def action_open_probe_window(self) -> None:
        existing = self._probe_dialog
        if existing is not None and existing.winfo_exists():
            stay_above(existing, self, modal=False)
            return
        self._probe_dialog = ProbeDialog(
            self,
            on_clear=self.clear_probe_marks,
            on_close=self._on_probe_window_close,
        )

    def _on_probe_window_close(self) -> None:
        """Clear all markers when Probe closes; leave exclusive probe mode."""
        self.clear_probe_marks()
        self._probe_dialog = None
        if self.tool_mode.get() == "probe":
            self._set_tool_mode("rotate")

    def clear_probe_marks(self) -> None:
        self.probe_marks.clear()
        self.viewport.delete("probe")
        self._log("Probe markers cleared")

    def clear_pathline_seeds(self) -> None:
        self.pathline_seeds.clear()
        self._rake_frac = None
        self.viewport.delete("pathline_drag")
        self.viewport.delete("pathline_rake")
        self._refresh_levels_label()
        self._log("Pathline seeds cleared")
        if self.layer_pathlines.get():
            self._render_cache.clear()
            self.action_redraw()

    def _draw_probe_overlays(self) -> None:
        self.viewport.delete("probe")
        for cx, cy, text in self.probe_marks:
            short = text if len(text) < 28 else text[:25] + "…"
            self.viewport.create_line(cx - 6, cy, cx + 6, cy, fill="#1f2a33", width=1, tags="probe")
            self.viewport.create_line(cx, cy - 6, cx, cy + 6, fill="#1f2a33", width=1, tags="probe")
            self.viewport.create_oval(
                cx - 3, cy - 3, cx + 3, cy + 3, outline="#c00000", width=1, tags="probe"
            )
            self.viewport.create_rectangle(
                cx + 8,
                cy - 18,
                cx + 8 + max(70, 7 * len(short)),
                cy + 2,
                fill="#fafbfc",
                outline="#5c6b76",
                tags="probe",
            )
            self.viewport.create_text(
                cx + 12,
                cy - 8,
                text=short,
                anchor="w",
                fill="#1f2a33",
                font=("DejaVu Sans", 9),
                tags="probe",
            )

    def _probe_at_canvas(self, cx: int, cy: int) -> None:
        ds = self._require_dataset()
        if not ds:
            return
        self.action_open_probe_window()
        ox, oy = self._preview_offset
        pw, ph = self._preview_size
        nw, nh = self._preview_native
        if pw < 2 or ph < 2 or nw < 2 or nh < 2:
            self._log("Probe: no preview mapped yet — redraw first")
            return
        lx = cx - ox
        ly = cy - oy
        if lx < 0 or ly < 0 or lx >= pw or ly >= ph:
            self._log("Probe: click outside the plot image")
            return
        px = int(lx * nw / pw)
        py = int(ly * nh / ph)
        # Omit --var so CLI samples every variable at the point.
        args = [
            "probe",
            str(ds),
            "--px",
            str(px),
            "--py",
            str(py),
            "--width",
            str(nw),
            "--height",
            str(nh),
            *self._plane_cli_arg(),
            *self._view_cli_arg(),
            "--zone",
            str(self._active_zone_index()),
            "--json",
        ]
        # Prefer active contour var as primary stamp when set.
        var = self.var_name.get().strip()
        if var:
            args.extend(["--var", var])
        result = self._run_cli(args)
        if not result or result.returncode != 0:
            return
        label = "probe"
        payload: dict = {}
        try:
            payload = json.loads(result.stdout.strip().splitlines()[-1])
            if payload.get("status") == "ok":
                label = str(payload.get("label") or label)
                nvals = len(payload.get("values") or {})
                self._log(
                    f"probe {label} (+{max(0, nvals - 1)} vars) @ "
                    f"world=({payload.get('x')}, {payload.get('y')})"
                )
            else:
                self._log(result.stdout.strip())
                return
        except (json.JSONDecodeError, IndexError, TypeError):
            for line in result.stdout.strip().splitlines():
                if "probe" in line.lower() or "=" in line:
                    label = line.strip()
                    break
            self._log(label)
        if payload and self._probe_dialog is not None and self._probe_dialog.winfo_exists():
            self._probe_dialog.update_probe(payload)
        self.probe_marks.append((cx, cy, label))
        if len(self.probe_marks) > 12:
            self.probe_marks = self.probe_marks[-12:]
        self._draw_probe_overlays()
        self._set_status(label)

    def action_pathlines_dialog(self) -> None:
        self.action_pathlines_tool()

    def _canvas_to_native_px(self, cx: int, cy: int) -> tuple[int, int] | None:
        ox, oy = self._preview_offset
        pw, ph = self._preview_size
        nw, nh = self._preview_native
        if pw < 2 or ph < 2 or nw < 2 or nh < 2:
            return None
        lx = cx - ox
        ly = cy - oy
        if lx < 0 or ly < 0 or lx >= pw or ly >= ph:
            return None
        return int(lx * nw / pw), int(ly * nh / ph)

    def _world_at_canvas(self, cx: int, cy: int) -> tuple[float, float] | None:
        """Map Viewport click → world (x,y) via `flowplot probe --json`."""
        ds = self.dataset_path
        if not ds or not ds.is_file():
            return None
        native = self._canvas_to_native_px(cx, cy)
        if native is None:
            return None
        px, py = native
        nw, nh = self._preview_native
        args = [
            "probe",
            str(ds),
            "--px",
            str(px),
            "--py",
            str(py),
            "--width",
            str(nw),
            "--height",
            str(nh),
            *self._plane_cli_arg(),
            *self._view_cli_arg(),
            "--zone",
            str(self._active_zone_index()),
            "--json",
        ]
        var = self.var_name.get().strip()
        if var:
            args.extend(["--var", var])
        result = self._run_cli(args)
        if not result or result.returncode != 0 or not result.stdout:
            return None
        try:
            payload = json.loads(result.stdout.strip().splitlines()[-1])
            if payload.get("status") != "ok":
                return None
            return float(payload["x"]), float(payload["y"])
        except (json.JSONDecodeError, KeyError, TypeError, ValueError, IndexError):
            return None

    def _finish_pathline_drag(self, cx: int, cy: int) -> None:
        """Emit pathlines from a rake/seed line segment (click-drag)."""
        start = self._pl_drag_start
        self._pl_drag_start = None
        if start is None:
            self.viewport.delete("pathline_drag")
            return
        x0, y0 = start
        dist = ((cx - x0) ** 2 + (cy - y0) ** 2) ** 0.5
        self._draw_pathline_rake_preview(x0, y0, cx, cy)
        self._remember_rake_frac(x0, y0, cx, cy)
        if dist < 8:
            points = [(x0, y0)]
        else:
            n = max(2, min(16, int(dist / 18) + 1))
            points = [
                (
                    int(round(x0 + (i / (n - 1)) * (cx - x0))),
                    int(round(y0 + (i / (n - 1)) * (cy - y0))),
                )
                for i in range(n)
            ]
        added = 0
        for px, py in points:
            world = self._world_at_canvas(px, py)
            if world is None:
                continue
            if any(abs(world[0] - sx) < 1e-9 and abs(world[1] - sy) < 1e-9 for sx, sy in self.pathline_seeds):
                continue
            self.pathline_seeds.append(world)
            added += 1
        if added == 0:
            self._log("Pathlines: rake outside plot or probe failed — no seeds (rake kept)")
            self._draw_persistent_rake()
            return
        self.layer_pathlines.set(True)
        self._refresh_levels_label()
        self._log(f"Pathlines rake: +{added} seed(s) on segment → {len(self.pathline_seeds)} total")
        self._render_cache.clear()
        # Full redraw so pathlines stay in the PNG (not only the transient canvas stroke).
        self.action_redraw(interactive=False)
        self._draw_persistent_rake()

    def _remember_rake_frac(self, x0: int, y0: int, x1: int, y1: int) -> None:
        ox, oy = self._preview_offset
        pw, ph = self._preview_size
        if pw < 2 or ph < 2:
            self._rake_frac = None
            return
        self._rake_frac = (
            ((x0 - ox) / pw, (y0 - oy) / ph),
            ((x1 - ox) / pw, (y1 - oy) / ph),
        )

    def _draw_persistent_rake(self) -> None:
        """Keep the rake seed line visible after redraw (canvas overlay)."""
        self.viewport.delete("pathline_drag")
        self.viewport.delete("pathline_rake")
        if self._rake_frac is None or not self.layer_pathlines.get():
            return
        ox, oy = self._preview_offset
        pw, ph = self._preview_size
        if pw < 2 or ph < 2:
            return
        (fx0, fy0), (fx1, fy1) = self._rake_frac
        x0 = int(round(ox + fx0 * pw))
        y0 = int(round(oy + fy0 * ph))
        x1 = int(round(ox + fx1 * pw))
        y1 = int(round(oy + fy1 * ph))
        self.viewport.create_line(
            x0, y0, x1, y1, fill="#c00000", width=2, tags="pathline_rake"
        )
        for sx, sy in ((x0, y0), (x1, y1)):
            self.viewport.create_oval(
                sx - 3, sy - 3, sx + 3, sy + 3,
                outline="#c00000", fill="#ffd0d0", width=1, tags="pathline_rake",
            )

    def _project_axis(self, vx: float, vy: float, vz: float) -> tuple[float, float]:
        """Project a world unit axis into screen (u right, v up) for the triad.

        Matches `ViewSpec::project` / `project_orbit` in src/view.rs.
        """
        mode = self.view_mode.get().strip().lower()
        if mode in ("+x", "-x", "+y", "-y", "+z", "-z"):
            pos = not mode.startswith("-")
            ax = mode[-1]
            if ax == "z":
                return (vx, vy) if pos else (-vx, vy)
            if ax == "y":
                return (vx, vz) if pos else (-vx, vz)
            return (-vy, vz) if pos else (vy, vz)
        # Free / 3D Cartesian — use current az/el (Auto falls through to +Z-like).
        az = math.radians(self.azimuth_deg)
        el = math.radians(self._clamp_elevation(self.elevation_deg))
        cos_el = math.cos(el)
        ex = cos_el * math.cos(az)
        ey = cos_el * math.sin(az)
        ez = math.sin(el)
        fx, fy, fz = -ex, -ey, -ez
        fl = math.sqrt(fx * fx + fy * fy + fz * fz) or 1.0
        fx, fy, fz = fx / fl, fy / fl, fz / fl
        if abs(fz) > 0.999:
            ux, uy, uz = 0.0, 1.0, 0.0
        else:
            ux, uy, uz = 0.0, 0.0, 1.0
        # right = cross(forward, world_up)
        rx = fy * uz - fz * uy
        ry = fz * ux - fx * uz
        rz = fx * uy - fy * ux
        rl = math.sqrt(rx * rx + ry * ry + rz * rz)
        if rl < 1e-30:
            rx, ry, rz = 0.0, 1.0, 0.0
        else:
            rx, ry, rz = rx / rl, ry / rl, rz / rl
        # up = cross(right, forward)
        px = ry * fz - rz * fy
        py = rz * fx - rx * fz
        pz = rx * fy - ry * fx
        u = vx * rx + vy * ry + vz * rz
        v = vx * px + vy * py + vz * pz
        return u, v

    def _draw_axes_triad(self) -> None:
        """Cartesian XYZ triad with visible origin (lower-left of Viewport)."""
        if not hasattr(self, "viewport"):
            return
        self.viewport.delete("axes_triad")
        cw = max(self.viewport.winfo_width(), 2)
        ch = max(self.viewport.winfo_height(), 2)
        if cw < 80 or ch < 80:
            return
        ox, oy = 36, ch - 36
        axis_len = 28.0
        # Soft plate
        self.viewport.create_rectangle(
            ox - 18,
            oy - 34,
            ox + 38,
            oy + 18,
            fill="#fafbfc",
            outline="#9aa3ab",
            tags="axes_triad",
        )
        self.viewport.create_oval(
            ox - 3, oy - 3, ox + 3, oy + 3, fill="#1f2a33", outline="#1f2a33", tags="axes_triad"
        )
        for (vx, vy, vz), color, label in (
            ((1.0, 0.0, 0.0), "#c82828", "X"),
            ((0.0, 1.0, 0.0), "#28a03c", "Y"),
            ((0.0, 0.0, 1.0), "#2850c8", "Z"),
        ):
            su, sv = self._project_axis(vx, vy, vz)
            norm = math.hypot(su, sv)
            if norm < 1e-6:
                self.viewport.create_text(
                    ox + 8, oy - 12, text=label, fill=color, font=("Helvetica", 9, "bold"), tags="axes_triad"
                )
                continue
            sx = su / norm * axis_len
            sy = -sv / norm * axis_len  # canvas y down
            x1 = ox + sx
            y1 = oy + sy
            self.viewport.create_line(
                ox, oy, x1, y1, fill=color, width=2, arrow=tk.LAST, tags="axes_triad"
            )
            self.viewport.create_text(
                x1 + (4 if sx >= 0 else -4),
                y1 - 8,
                text=label,
                fill=color,
                font=("Helvetica", 9, "bold"),
                tags="axes_triad",
            )

    def _vector_cli_vars(self) -> list[str]:
        args: list[str] = []
        u = self.vector_u.get().strip()
        v = self.vector_v.get().strip()
        w = self.vector_w.get().strip()
        if u:
            args.extend(["--u-var", u])
        if v:
            args.extend(["--v-var", v])
        if w:
            args.extend(["--w-var", w])
        return args

    def _guess_velocity_names(self) -> tuple[str, str, str]:
        names = list(self.variables)

        def pick(cands: list[str]) -> str:
            for c in cands:
                for n in names:
                    if n.lower() == c.lower():
                        return n
            return ""

        u = pick(["U", "Ux", "VX", "VelocityX", "x-velocity"])
        v = pick(["V", "Uy", "VY", "VelocityY", "y-velocity"])
        w = pick(["W", "Uz", "VZ", "VelocityZ", "z-velocity"])
        return u, v, w

    def _ask_vector_components(self) -> bool:
        """Tecplot-like U/V/W (or Ux/Uy/Uz) picker when enabling Vector."""
        gu, gv, gw = self._guess_velocity_names()
        if not self.vector_u.get().strip():
            self.vector_u.set(gu)
        if not self.vector_v.get().strip():
            self.vector_v.set(gv)
        if not self.vector_w.get().strip():
            self.vector_w.set(gw)
        dlg = tk.Toplevel(self)
        dlg.title("Vector variables")
        dlg.geometry("340x200")
        dlg.resizable(False, False)
        stay_above(dlg, self, modal=True)
        body = ttk.Frame(dlg, padding=12)
        body.pack(fill=tk.BOTH, expand=True)
        ttk.Label(body, text="Choose velocity components (Tecplot-like):").pack(anchor="w")
        vars_ = self.variables or [""]
        rows = [("U / Ux", self.vector_u), ("V / Uy", self.vector_v), ("W / Uz", self.vector_w)]
        for label, var in rows:
            row = ttk.Frame(body)
            row.pack(fill=tk.X, pady=4)
            ttk.Label(row, text=label, width=10).pack(side=tk.LEFT)
            cb = ttk.Combobox(row, textvariable=var, values=vars_, width=22)
            cb.pack(side=tk.LEFT, fill=tk.X, expand=True)
        result = {"ok": False}

        def ok() -> None:
            if not self.vector_u.get().strip() or not self.vector_v.get().strip():
                messagebox.showwarning("Vector", "U and V are required.", parent=dlg)
                return
            result["ok"] = True
            dlg.destroy()

        def cancel() -> None:
            dlg.destroy()

        btns = ttk.Frame(body)
        btns.pack(fill=tk.X, pady=(10, 0))
        ttk.Button(btns, text="Cancel", command=cancel).pack(side=tk.RIGHT, padx=4)
        ttk.Button(btns, text="OK", command=ok).pack(side=tk.RIGHT)
        dlg.wait_window()
        return bool(result["ok"])

    def action_mesh_settings(self) -> None:
        """Show Zone Layers → Mesh gear → Zone Style (Mesh tab)."""
        self.action_zone_style()

    def action_shade_layer(self) -> None:
        if self.layer_shade.get():
            if not self._ensure_contour_var():
                self.layer_shade.set(False)
                self._log("Shade skipped — no scalar variable in dataset")
                self._set_status("Shade unavailable (no scalar)")
                return
            self._log(f"Shade on → continuous flood ({self.var_name.get()})")
        else:
            self._log("Shade off")
        self._render_cache.clear()
        self.action_redraw()

    def action_shade_settings(self) -> None:
        """Show Zone Layers → Shade gear → Shade Details."""
        existing = self._shade_details_dialog
        if existing is not None and existing.winfo_exists():
            stay_above(existing, self, modal=False)
            return
        dlg = ShadeDetailsDialog(
            self,
            self.shade_details,
            on_apply=self._apply_shade_details,
            variables=list(self.variables),
            active_var=self.var_name.get(),
        )
        self._shade_details_dialog = dlg

    def _apply_shade_details(self, state: ShadeDetailsState) -> None:
        self.shade_details = state
        self.layer_shade.set(True)
        if state.use_active_var and not self.var_name.get().strip() and self.variables:
            self.var_name.set(first_scalar(self.variables))
        dlg = self._shade_details_dialog
        if dlg is not None and dlg.winfo_exists():
            picked = (dlg.active_var.get() or "").strip()
            if picked:
                self.var_name.set(picked)
        self._log(
            f"Shade Details: mode={state.color_mode} lighting={state.lighting} "
            f"var={self.var_name.get() or '(none)'}"
        )
        self._render_cache.clear()
        self.action_redraw()

    def action_vector_layer(self) -> None:
        if not self.layer_vector.get():
            self._render_cache.clear()
            self.action_redraw()
            return
        if not self._ask_vector_components():
            self.layer_vector.set(False)
            return
        self._log(
            f"Vector: U={self.vector_u.get()} V={self.vector_v.get()} W={self.vector_w.get() or '(none)'}"
        )
        self._render_cache.clear()
        self.action_redraw()

    def action_vector_settings(self) -> None:
        """Show Zone Layers → Vector gear → Vector Details."""
        gu, gv, gw = self._guess_velocity_names()
        if not self.vector_u.get().strip():
            self.vector_u.set(gu)
        if not self.vector_v.get().strip():
            self.vector_v.set(gv)
        if not self.vector_w.get().strip():
            self.vector_w.set(gw)
        existing = self._vector_details_dialog
        if existing is not None and existing.winfo_exists():
            stay_above(existing, self, modal=False)
            return
        dlg = VectorDetailsDialog(
            self,
            u_var=self.vector_u,
            v_var=self.vector_v,
            w_var=self.vector_w,
            state=self.vector_details,
            variables=list(self.variables) or [""],
            on_apply=self._apply_vector_details,
        )
        self._vector_details_dialog = dlg

    def _apply_vector_details(self, state: VectorDetailsState) -> None:
        self.vector_details = state
        if not self.vector_u.get().strip() or not self.vector_v.get().strip():
            messagebox.showwarning("Vector", "U and V are required.", parent=self)
            return
        self.layer_vector.set(True)
        self._log(
            f"Vector Details: U={self.vector_u.get()} V={self.vector_v.get()} "
            f"skip={state.skip}"
        )
        self._render_cache.clear()
        self.action_redraw()

    def action_edge_layer(self) -> None:
        self._log("Edge " + ("on" if self.layer_edge.get() else "off"))
        self._render_cache.clear()
        self.action_redraw()

    def action_edge_settings(self) -> None:
        """Show Zone Layers → Edge gear → Edge Details."""
        existing = self._edge_details_dialog
        if existing is not None and existing.winfo_exists():
            stay_above(existing, self, modal=False)
            return
        dlg = EdgeDetailsDialog(
            self,
            self.edge_details,
            on_apply=self._apply_edge_details,
        )
        self._edge_details_dialog = dlg

    def _apply_edge_details(self, state: EdgeDetailsState) -> None:
        self.edge_details = state
        self.layer_edge.set(True)
        self._log(f"Edge Details: color={state.color} pattern={state.line_pattern}")
        self._render_cache.clear()
        self.action_redraw()

    def action_scatter_layer(self) -> None:
        self._log("Scatter " + ("on" if self.layer_scatter.get() else "off"))
        self._render_cache.clear()
        self.action_redraw()

    def action_scatter_settings(self) -> None:
        """Show Zone Layers → Scatter gear → Scatter Details."""
        existing = self._scatter_details_dialog
        if existing is not None and existing.winfo_exists():
            stay_above(existing, self, modal=False)
            return
        dlg = ScatterDetailsDialog(
            self,
            self.scatter_details,
            on_apply=self._apply_scatter_details,
        )
        self._scatter_details_dialog = dlg

    def _apply_scatter_details(self, state: ScatterDetailsState) -> None:
        self.scatter_details = state
        self.layer_scatter.set(True)
        self._log(f"Scatter Details: skip={state.skip} color={state.color}")
        self._render_cache.clear()
        self.action_redraw()

    def _pathline_seed_args(self) -> list[str]:
        """Seeds + auto dt + U/V for Pathlines (never Streamtraces)."""
        args: list[str] = []
        for x, y in self.pathline_seeds[-24:]:
            args.extend(["--seed", f"{x:.8g},{y:.8g}"])
        # 0 = auto CFL from cell size / |U| (required for hypersonic / large-U fields).
        args.extend(["--dt", "0", "--max-steps", "1000"])
        # Ensure velocity names are known before the CLI guesses.
        gu, gv, _gw = self._guess_velocity_names()
        u = self.vector_u.get().strip() or gu
        v = self.vector_v.get().strip() or gv
        if u:
            args.extend(["--u-var", u])
        if v:
            args.extend(["--v-var", v])
        w = self.vector_w.get().strip()
        if w:
            args.extend(["--w-var", w])
        return args

    def action_isosurfaces(self) -> None:
        """Iso-Surfaces — 2D isolines of the active variable (real levels, not empty)."""
        if self.layer_isosurfaces.get():
            if not self._ensure_contour_var():
                self.layer_isosurfaces.set(False)
                self._log("Iso-Surfaces skipped — no scalar variable in dataset")
                self._set_status("Iso unavailable (no scalar)")
                return
            self.contour_details.coloring = "Banded"
            self._refresh_levels_label()
            self._render_cache.clear()
            self.action_redraw()
            self._log(
                f"Iso-Surfaces on → isolines of {self.var_name.get()} "
                f"({self.contour_details.nlevels} levels)"
            )
        else:
            self._log("Iso-Surfaces off")
            self._render_cache.clear()
            self.action_redraw()

    def action_slices_focus(self) -> None:
        """Toolbar / menu Slice — enable cut plane and focus Plot controls."""
        self.layer_slices.set(True)
        if self.plane_axis.get() == "auto":
            self.plane_axis.set("k")
        self._schedule_settle(delay_ms=60)
        self._log("Slices on — use Cut / Track plane Index ±")
        self._set_status("Slices — Cut / Track plane Index ±")

    def _on_slices_checkbox(self) -> None:
        if not self.layer_slices.get():
            self._log("Slices off")
            return
        self.action_slices_focus()

    def action_dataset_info(self) -> None:
        ds = self._require_dataset()
        if not ds:
            return
        self._run_cli(["--json", "info", str(ds)])

    def action_zone_style(self) -> None:
        """Plot → Zone Style… (Tecplot-like per-zone Mesh/Contour table)."""
        if not self.zone_styles:
            if self.dataset_path and self.dataset_path.is_file():
                # Lazy fill from a single default row if info lacked zones.
                self.zone_styles = [
                    ZoneStyleState(
                        index=0,
                        name=self.dataset_path.stem,
                        show_mesh=self.layer_mesh.get(),
                        show_contour=self.layer_contour.get(),
                        mesh_color=self.mesh_color,
                    )
                ]
            else:
                messagebox.showwarning(
                    "Zone Style",
                    "Load a CFD file first (File → Open).",
                    parent=self,
                )
                return
        existing = self._zone_style_dialog
        if existing is not None and existing.winfo_exists():
            existing.styles = copy.deepcopy(self.zone_styles)
            stay_above(existing, self, modal=False)
            return
        dlg = ZoneStyleDialog(
            self,
            self.zone_styles,
            on_apply=self._apply_zone_styles,
            initial_tab="Mesh",
        )
        self._zone_style_dialog = dlg

    def _apply_zone_styles(self, styles: list[ZoneStyleState]) -> None:
        self.zone_styles = styles
        visible = [z for z in styles if z.show_zone]
        if not visible:
            self.layer_mesh.set(False)
            self.layer_contour.set(False)
            self._log("Zone Style: no zones shown")
            self.action_redraw()
            return
        # First visible zone drives CLI --zone (multi-zone composite later).
        active = visible[0]
        self.active_zone = active.index
        self.mesh_color = active.mesh_color or DEFAULT_MESH_COLOR
        self.layer_mesh.set(any(z.show_mesh for z in visible))
        self.layer_contour.set(any(z.show_contour for z in visible))
        # Prefer ordered dims from the active zone when info listed them.
        if self.dataset_path:
            # Refresh cut limits from the chosen zone if we still have info cache.
            pass
        if self.layer_contour.get() and not self.var_name.get().strip() and self.variables:
            self.var_name.set(first_scalar(self.variables))
        self._log(
            f"Zone Style apply: zone={active.index}:{active.name} "
            f"mesh={self.layer_mesh.get()} contour={self.layer_contour.get()} "
            f"color={self.mesh_color}"
        )
        # Sync cut-plane ordered dims for the active zone when available.
        self._sync_ordered_for_zone(active.index)
        self._refresh_zone_list()
        self.action_redraw()

    def _update_dataset_label(self) -> None:
        if not hasattr(self, "dataset_label"):
            return
        if self.dataset_path is None:
            self.dataset_label.configure(text="(none)")
        else:
            self.dataset_label.configure(text=self.dataset_path.name)

    def _refresh_zone_list(self) -> None:
        if not hasattr(self, "zone_list"):
            return
        self.zone_list.delete(0, tk.END)
        if not self.zone_styles:
            if self.dataset_path is not None:
                self.zone_list.insert(tk.END, "0:(loading…)")
            return
        active = self._active_zone_index()
        sel = 0
        for i, z in enumerate(self.zone_styles):
            mark = "● " if z.show_zone else "○ "
            self.zone_list.insert(tk.END, f"{mark}{z.index}:{z.name}")
            if z.index == active:
                sel = i
        self.zone_list.selection_clear(0, tk.END)
        if self.zone_styles:
            self.zone_list.selection_set(sel)
            self.zone_list.see(sel)

    def _on_zone_list_select(self, _event: tk.Event | None = None) -> None:
        if not hasattr(self, "zone_list") or not self.zone_styles:
            return
        sel = self.zone_list.curselection()
        if not sel:
            return
        idx = int(sel[0])
        if idx < 0 or idx >= len(self.zone_styles):
            return
        z = self.zone_styles[idx]
        if self.active_zone == z.index:
            return
        self.active_zone = z.index
        for style in self.zone_styles:
            style.show_zone = style.index == z.index
        self._sync_ordered_for_zone(z.index)
        self._refresh_zone_list()
        self._log(f"Active zone → {z.index}:{z.name}")
        self._schedule_settle(delay_ms=40)

    def _active_zone_index(self) -> int:
        if self.zone_styles:
            for z in self.zone_styles:
                if z.show_zone:
                    return z.index
            return self.zone_styles[0].index
        return int(self.active_zone)

    def _zone_label(self, idx: int) -> str:
        for z in self.zone_styles:
            if z.index == idx:
                return f"{idx}:{z.name}"
        return str(idx)

    def _style_for_zone(self, idx: int) -> ZoneStyleState | None:
        for z in self.zone_styles:
            if z.index == idx:
                return z
        return None

    def _mesh_color_cli_args(self) -> list[str]:
        color = self.mesh_color or DEFAULT_MESH_COLOR
        if self.zone_styles:
            zs = self._style_for_zone(self._active_zone_index())
            if zs is not None and zs.mesh_color:
                color = zs.mesh_color
        return ["--mesh-color", rgb_cli(color)]

    def _sync_ordered_for_zone(self, zone_idx: int) -> None:
        """Best-effort: re-read info JSON ordered dims for the chosen zone."""
        ds = self.dataset_path
        if not ds or not ds.is_file():
            return
        proc = self._run_cli(["--json", "info", str(ds)])
        if not proc or proc.returncode != 0 or not proc.stdout:
            return
        try:
            payload = json.loads(proc.stdout.strip().splitlines()[-1])
        except json.JSONDecodeError:
            return
        zones = payload.get("zones") or []
        if not isinstance(zones, list):
            return
        for z in zones:
            if not isinstance(z, dict):
                continue
            try:
                zi = int(z.get("index", -1))
            except (TypeError, ValueError):
                continue
            if zi != zone_idx:
                continue
            ordered = z.get("ordered")
            if isinstance(ordered, dict):
                self.zone_ordered = {
                    str(k).lower(): int(v)
                    for k, v in ordered.items()
                    if str(k).lower() in ("i", "j", "k")
                }
            return

    def action_nondim(self) -> None:
        ds = self._require_dataset()
        if not ds:
            return
        self._run_cli(["nondim", str(ds), *self._nondim_args()])

    def action_export_png(self) -> None:
        ds = self._require_dataset()
        if not ds:
            return
        release_grabs(self)
        from export_dialog import ask_export_png

        choice = ask_export_png(self, initial_style=self.plot_style.get())
        if not choice:
            return
        dest = choice.get("path")
        preset = choice.get("preset") or "current"
        if not dest:
            return
        # Apply JFM Crisp export preset (from JFM_Crisp_2D_Contour Tecplot macro).
        prev_style = self.plot_style.get()
        prev_cd = replace(self.contour_details) if hasattr(self, "contour_details") else None
        if preset == "jfm-crisp":
            self.plot_style.set("jfm")
            self.layer_mesh.set(False)
            self.layer_contour.set(True)
            self.layer_shade.set(False)
            cd = self.contour_details
            self.contour_details = replace(
                cd,
                coloring="Banded",
                nlevels=max(16, int(cd.nlevels or 16)),
                legend=True,
                show_labels=False,
                colormap=cd.colormap if cd.colormap != "Default" else "JFM",
            )
            self._log("Export preset: JFM Crisp (banded flood, no mesh, white paper)")
        self.action_redraw()
        if self.last_png and self.last_png.is_file():
            shutil.copyfile(self.last_png, dest)
            self._log(f"Exported {dest} (preset={preset})")
            self._show_png_preview(Path(dest))
        else:
            self._log("export failed (no preview PNG)")
        # Restore interactive style unless user already on JFM.
        if preset == "jfm-crisp" and prev_style != "jfm":
            self.plot_style.set(prev_style)
        if preset == "jfm-crisp" and prev_cd is not None:
            self.contour_details = prev_cd

    def stub_plot(self, name: str) -> None:
        messagebox.showinfo(
            name,
            f"Plot → {name} matches Tecplot’s menu label.\n"
            "Not implemented in this slice (stub).",
        )

    def open_agents_panel(self) -> None:
        self.open_agents_panel_select(None, "positivity-sanity")

    def open_agents_panel_select(
        self, select_id: str | None = None, focus_group: str | None = None
    ) -> None:
        bin_path = self._require_cli()
        if not bin_path:
            return
        existing = getattr(self, "_agents_panel", None)
        if existing is not None and existing.winfo_exists():
            existing.dataset_path = self.dataset_path
            existing.variables = list(self.variables)
            existing.var_name.set(self.var_name.get() or first_scalar(self.variables))
            existing.nondim_args = self._nondim_args()
            if focus_group:
                existing.focus_group = focus_group
            existing.ds_label.configure(
                text=str(self.dataset_path) if self.dataset_path else "(no file loaded)"
            )
            existing.var_combo.configure(
                values=self.variables,
                state="readonly" if self.variables else "disabled",
            )
            stay_above(existing, self, modal=False)
            self._select_agents_item(existing, select_id)
            return
        panel = AgentsPanel(
            self,
            flowplot_bin=bin_path,
            dataset_path=self.dataset_path,
            variables=self.variables,
            var_name=self.var_name.get(),
            nondim_args=self._nondim_args(),
            plot_style=self.plot_style.get(),
            on_png=self._show_png_preview,
            repo_root=ROOT.parent,
            title="Agents",
            focus_group=focus_group or "positivity-sanity",
        )
        self._agents_panel = panel
        self._select_agents_item(panel, select_id)

    # Back-compat alias used by older call sites / docs.
    def open_analyze_panel(self, select_id: str | None = None) -> None:
        self.open_agents_panel_select(select_id, None)

    def _select_agents_item(self, panel: AgentsPanel, select_id: str | None) -> None:
        if not select_id:
            return
        iid = f"i:{select_id}"
        try:
            parent = panel.tree.parent(iid)
            if parent:
                panel.tree.item(parent, open=True)
            panel.tree.selection_set(iid)
            panel.tree.see(iid)
            panel._on_select()
        except tk.TclError:
            pass

    def show_about(self) -> None:
        messagebox.showinfo(
            "About Fieldplot",
            "Fieldplot — CFD post-processor\n"
            "Tecplot-like menus + Plot panel (Mesh first).\n"
            "Agents menu: positivity, profiles, shocks, hypersonic, grid, …\n"
            "Checks apply to any loaded CFD dataset (physics/numerics).\n"
            "CLI binary remains `flowplot` for now.\n"
            f"CLI: {self.flowplot or '(not found)'}",
        )


def main() -> int:
    if os.environ.get("FLOWPLOT_GUI_SCREENSHOT"):
        out = Path(os.environ["FLOWPLOT_GUI_SCREENSHOT"])
        open_path = os.environ.get("FLOWPLOT_GUI_OPEN")
        app = FlowplotGui()
        app.update_idletasks()
        app.update()

        def _boot() -> None:
            if open_path:
                p = Path(open_path)
                if p.is_file():
                    app.dataset_path = p
                    app.layer_mesh.set(True)
                    app.layer_contour.set(
                        os.environ.get("FLOWPLOT_GUI_CONTOUR", "").lower() in ("1", "true", "yes")
                    )
                    if app.layer_contour.get() and os.environ.get("FLOWPLOT_GUI_VAR"):
                        app.var_name.set(os.environ["FLOWPLOT_GUI_VAR"])
                    proc = app._run_cli(["--json", "info", str(p)])
                    if proc and proc.returncode == 0 and proc.stdout:
                        try:
                            payload = json.loads(proc.stdout.strip().splitlines()[-1])
                            vars_ = payload.get("variables") or []
                            if isinstance(vars_, list):
                                app._set_variables([str(v) for v in vars_])
                            app.effectively_2d = bool(payload.get("effectively_2d", True))
                            app._apply_default_camera(payload)
                            app._update_view_label()
                        except json.JSONDecodeError:
                            pass
                    if app.layer_contour.get() and not app.var_name.get():
                        app.var_name.set(first_scalar(app.variables))
                    app.action_redraw()
                    app.update_idletasks()
                    app.update()
            app.after(400, lambda: _capture_and_quit(app, out))

        app.after(100, _boot)
        app.mainloop()
        return 0 if out.is_file() else 1

    app = FlowplotGui()
    app.mainloop()
    return 0


def _capture_and_quit(app: FlowplotGui, out: Path) -> None:
    app.update_idletasks()
    app.update()
    out.parent.mkdir(parents=True, exist_ok=True)
    # Prefer compositing the last PNG into the viewport region (works headless).
    try:
        from PIL import Image, ImageDraw
    except ImportError:
        app._log("Pillow missing; cannot screenshot")
        app.destroy()
        return

    w = max(app.winfo_width(), 960)
    h = max(app.winfo_height(), 600)
    shot = Image.new("RGB", (w, h), "#eceef0")
    draw = ImageDraw.Draw(shot)
    draw.rectangle((0, 0, w, 40), fill="#d5d9de")
    draw.text((12, 12), "Flowplot — Viewport mesh preview", fill="#1f2a33")

    # Left plot panel strip
    draw.rectangle((8, 48, 248, h - 28), fill="#e2e5e8")
    draw.text((20, 60), "Plot", fill="#1f2a33")
    draw.text((20, 90), f"type: {app.plot_type.get()}", fill="#5c6b76")
    draw.text((20, 110), f"Mesh: {app.layer_mesh.get()}", fill="#5c6b76")
    draw.text((20, 130), f"Contour: {app.layer_contour.get()}", fill="#5c6b76")

    vx0, vy0, vx1, vy1 = 256, 48, w - 216, h - 28
    draw.rectangle((vx0, vy0, vx1, vy1), fill="#f7f8f9", outline="#9aa3ab")
    if app.last_png and app.last_png.is_file():
        try:
            preview = Image.open(app.last_png).convert("RGBA")
            max_w = max(vx1 - vx0 - 16, 16)
            max_h = max(vy1 - vy0 - 16, 16)
            preview.thumbnail((max_w, max_h), Image.Resampling.LANCZOS)
            px = vx0 + (vx1 - vx0 - preview.width) // 2
            py = vy0 + (vy1 - vy0 - preview.height) // 2
            shot.paste(preview, (px, py), preview)
            draw.text((vx0 + 8, vy1 - 18), f"preview: {app.last_png.name}", fill="#5c6b76")
        except OSError as exc:
            draw.text((vx0 + 20, vy0 + 20), f"PNG error: {exc}", fill="#a33")
    else:
        draw.text((vx0 + 40, (vy0 + vy1) // 2), "Viewport (no PNG)", fill="#8a97a1")

    # Try a real window grab when a display is available; fall back to composite.
    try:
        from PIL import ImageGrab

        x = app.winfo_rootx()
        y = app.winfo_rooty()
        grab = ImageGrab.grab(bbox=(x, y, x + w, y + h))
        # Keep grab only if it is not a flat empty frame.
        extrema = grab.convert("L").getextrema()
        if extrema and extrema[0] != extrema[1]:
            shot = grab
    except Exception:
        pass

    shot.save(out)
    app.destroy()


if __name__ == "__main__":
    sys.exit(main())
