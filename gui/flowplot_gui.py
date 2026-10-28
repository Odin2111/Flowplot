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
from pathlib import Path
from tkinter import filedialog, messagebox, ttk

from agents_panel import AgentsPanel
from contour_details_dialog import ContourDetailsDialog, ContourDetailsState
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
    """Prefer PATH, then cargo debug/release next to this repo."""
    found = shutil.which("flowplot")
    if found:
        return found
    repo = ROOT.parent
    for rel in ("target/debug/flowplot", "target/release/flowplot"):
        cand = repo / rel
        if cand.is_file() and os.access(cand, os.X_OK):
            return str(cand)
    return None


def first_scalar(variables: list[str]) -> str:
    """Default contour variable: first non-coordinate name (Tecplot-like)."""
    for name in variables:
        if name.strip().lower() not in COORD_NAMES:
            return name
    return variables[0] if variables else ""


class FlowplotGui(tk.Tk):
    def __init__(self) -> None:
        super().__init__()
        self.title("Flowplot")
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
        self.contour_details = ContourDetailsState()
        self.tool_mode = tk.StringVar(value="orbit")  # orbit | probe | label
        self.probe_marks: list[tuple[int, int, str]] = []
        self._preview_offset: tuple[int, int] = (0, 0)
        self._preview_size: tuple[int, int] = (0, 0)
        self._preview_native: tuple[int, int] = (0, 0)

        # Tecplot Show Zone Layers — Mesh ON, Contour OFF by default.
        self.layer_mesh = tk.BooleanVar(value=True)
        self.layer_contour = tk.BooleanVar(value=False)
        self.layer_shade = tk.BooleanVar(value=False)
        self.layer_vector = tk.BooleanVar(value=False)
        self.layer_edge = tk.BooleanVar(value=False)
        self.layer_scatter = tk.BooleanVar(value=False)
        self.layer_pathlines = tk.BooleanVar(value=False)

        # Cut / track plane + Tecplot-like view orientation.
        self.plane_axis = tk.StringVar(value="auto")
        self.plane_index = tk.IntVar(value=0)
        self.view_mode = tk.StringVar(value="auto")  # auto|+x|-x|+y|-y|+z|-z|free
        self.azimuth_deg = 0.0
        self.elevation_deg = 90.0
        self._drag_last: tuple[int, int] | None = None
        self._dragging = False
        self._redraw_after: str | None = None
        self._fullres_after: str | None = None
        self._settle_after: str | None = None
        self._cli_proc: subprocess.Popen[str] | None = None
        self._render_cache: dict[str, Path] = {}
        self._render_gen = 0
        # Tecplot-like display-list: last settled frame + angles for cheap drag warp.
        self._settle_image = None  # PIL.Image.Image | None
        self._settle_az = 0.0
        self._settle_el = 90.0
        self._settle_plane: tuple[str, int] | None = None
        self._warp_photo = None
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
        self._build_menu()
        self._build_menubar_row()
        self._build_contour_toolbar()
        self._build_chrome()
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
        style.configure("Scale.TEntry", fieldbackground="#ffffff", foreground=FG)
        style.configure("TLabelframe", background=PANEL, foreground=FG)
        style.configure("TLabelframe.Label", background=PANEL, foreground=ACCENT, font=("Helvetica", 9, "bold"))
        style.configure("TCheckbutton", background=PANEL, foreground=FG)
        style.configure("TCombobox", fieldbackground="#ffffff", foreground=FG)

    def _plot_menu_items(self, menu: tk.Menu) -> None:
        """Tecplot Plot menu order (stubs where not yet implemented)."""
        menu.add_command(label="Axis…", command=lambda: self.stub_plot("Axis"))
        menu.add_command(label="Assign XY…", command=lambda: self.stub_plot("Assign XY"))
        menu.add_command(label="Zone Style…", command=self.action_zone_style)
        menu.add_separator()
        menu.add_command(label="Pathlines…", command=self.action_pathlines_dialog)
        menu.add_command(label="Contour…", command=self.action_contour_details)
        menu.add_command(label="Vector…", command=lambda: self.stub_plot("Vector"))
        menu.add_command(label="Scatter…", command=lambda: self.stub_plot("Scatter"))
        menu.add_separator()
        menu.add_command(label="Blanking…", command=lambda: self.stub_plot("Blanking"))
        menu.add_command(label="Label Points…", command=lambda: self.stub_plot("Label Points"))
        menu.add_separator()
        menu.add_command(label="Redraw", command=self.action_redraw)
        menu.add_command(label="Apply nondim scales", command=self.action_nondim)

    def _build_menu(self) -> None:
        menubar = tk.Menu(self, tearoff=0, bg=MENU_BG, fg=FG, activebackground=ACCENT, activeforeground="#fff")
        file_m = tk.Menu(menubar, tearoff=0)
        file_m.add_command(label="Open…", command=self.action_load, accelerator="Ctrl+O")
        file_m.add_command(label="Export PNG…", command=self.action_export_png)
        file_m.add_separator()
        file_m.add_command(label="Quit", command=self.destroy)
        menubar.add_cascade(label="File", menu=file_m)

        plot_m = tk.Menu(menubar, tearoff=0)
        self._plot_menu_items(plot_m)
        menubar.add_cascade(label="Plot", menu=plot_m)

        style_m = tk.Menu(menubar, tearoff=0)
        style_m.add_radiobutton(label="Default", variable=self.plot_style, value="default")
        style_m.add_radiobutton(label="JFM", variable=self.plot_style, value="jfm")
        menubar.add_cascade(label="Style", menu=style_m)

        agents_m = tk.Menu(menubar, tearoff=0)
        agents_m.add_command(label="Post-process agents…", command=self.open_agents_panel)
        menubar.add_cascade(label="Agents", menu=agents_m)

        help_m = tk.Menu(menubar, tearoff=0)
        help_m.add_command(label="About Flowplot", command=self.show_about)
        menubar.add_cascade(label="Help", menu=help_m)

        self.config(menu=menubar)
        self.bind("<Control-o>", lambda _e: self.action_load())

    def _build_menubar_row(self) -> None:
        """Visible menu strip so names stay obvious on X11 / screenshots."""
        row = tk.Frame(self, bg=MENU_BG, highlightthickness=0)
        row.pack(side=tk.TOP, fill=tk.X)

        def menubutton(label: str) -> tk.Menubutton:
            b = tk.Menubutton(
                row,
                text=label,
                bg=MENU_BG,
                fg=FG,
                activebackground=ACCENT,
                activeforeground="#ffffff",
                relief=tk.FLAT,
                font=("Helvetica", 9),
                padx=8,
                pady=3,
            )
            b.pack(side=tk.LEFT)
            return b

        mb_file = menubutton("File")
        file_m = tk.Menu(mb_file, tearoff=0)
        file_m.add_command(label="Open…", command=self.action_load)
        file_m.add_command(label="Export PNG…", command=self.action_export_png)
        file_m.add_separator()
        file_m.add_command(label="Quit", command=self.destroy)
        mb_file.configure(menu=file_m)

        mb_plot = menubutton("Plot")
        plot_m = tk.Menu(mb_plot, tearoff=0)
        self._plot_menu_items(plot_m)
        mb_plot.configure(menu=plot_m)

        mb_style = menubutton("Style")
        style_m = tk.Menu(mb_style, tearoff=0)
        style_m.add_radiobutton(label="Default", variable=self.plot_style, value="default")
        style_m.add_radiobutton(label="JFM", variable=self.plot_style, value="jfm")
        mb_style.configure(menu=style_m)

        mb_agents = menubutton("Agents")
        agents_m = tk.Menu(mb_agents, tearoff=0)
        agents_m.add_command(label="Post-process agents…", command=self.open_agents_panel)
        mb_agents.configure(menu=agents_m)

        mb_help = menubutton("Help")
        help_m = tk.Menu(mb_help, tearoff=0)
        help_m.add_command(label="About Flowplot", command=self.show_about)
        mb_help.configure(menu=help_m)

        tk.Frame(self, bg="#b8c0c8", height=1).pack(side=tk.TOP, fill=tk.X)

    def _build_contour_toolbar(self) -> None:
        """Always-visible Contour ± / Label / Probe / Details strip under the menu."""
        bar = tk.Frame(self, bg="#cfd5db", highlightthickness=0)
        bar.pack(side=tk.TOP, fill=tk.X)
        tk.Label(
            bar,
            text="Contour tools",
            bg="#cfd5db",
            fg=ACCENT,
            font=("Helvetica", 9, "bold"),
        ).pack(side=tk.LEFT, padx=(10, 8), pady=5)

        def tool_btn(text: str, cmd) -> ttk.Button:
            b = ttk.Button(bar, text=text, style="Tool.TButton", command=cmd)
            b.pack(side=tk.LEFT, padx=2, pady=4)
            return b

        tool_btn("＋ Level", self.action_contour_add_level)
        tool_btn("－ Level", self.action_contour_remove_level)
        tool_btn("Abc Label", self.action_toggle_contour_labels)
        self._probe_btn = tool_btn("✎ Probe", self.action_toggle_probe)
        ttk.Separator(bar, orient=tk.VERTICAL).pack(side=tk.LEFT, fill=tk.Y, padx=8, pady=6)
        tool_btn("Details…", self.action_contour_details)
        self.levels_label = tk.Label(
            bar,
            text=self._levels_status_text(),
            bg="#cfd5db",
            fg=FG,
            font=("Helvetica", 9),
        )
        self.levels_label.pack(side=tk.LEFT, padx=10)
        tk.Frame(self, bg="#9aa3ab", height=1).pack(side=tk.TOP, fill=tk.X)

    def _levels_status_text(self) -> str:
        cd = self.contour_details
        mode = "banded" if cd.cli_coloring() == "banded" else "continuous"
        probe = " · probe" if self.tool_mode.get() == "probe" else ""
        labels = " · labels" if cd.show_labels else ""
        return f"{cd.nlevels} levels · {mode} · {cd.colormap}{labels}{probe}"

    def _refresh_levels_label(self) -> None:
        if hasattr(self, "levels_label"):
            self.levels_label.configure(text=self._levels_status_text())

    def _build_chrome(self) -> None:
        chrome = ttk.Frame(self, style="MenuBar.TFrame")
        chrome.pack(side=tk.TOP, fill=tk.X)

        logo_frame = ttk.Frame(chrome, style="MenuBar.TFrame")
        logo_frame.pack(side=tk.LEFT, padx=10, pady=6)
        logo = tk.Canvas(
            logo_frame, width=200, height=32, bg=MENU_BG, highlightthickness=0, bd=0
        )
        logo.pack()
        logo.create_line(6, 24, 14, 10, 22, 10, 30, 24, smooth=True, fill=ACCENT, width=2)
        logo.create_line(10, 26, 18, 14, 26, 14, 34, 26, smooth=True, fill=ACCENT, width=1)
        logo.create_oval(32, 15, 36, 19, fill=ACCENT, outline=ACCENT)
        logo.create_text(44, 16, text="Flowplot", anchor="w", fill=FG, font=("Helvetica", 14, "bold"))
        self._logo_canvas = logo
        self._wordmark_path = WORDMARK

        tools = ttk.Frame(chrome, style="MenuBar.TFrame")
        tools.pack(side=tk.LEFT, padx=8, pady=4)
        for label, cmd in (
            ("Open", self.action_load),
            ("Redraw", self.action_redraw),
            ("Export PNG", self.action_export_png),
        ):
            ttk.Button(tools, text=label, style="Tool.TButton", command=cmd).pack(
                side=tk.LEFT, padx=3
            )

    def _build_body(self) -> None:
        body = ttk.Frame(self)
        body.pack(side=tk.TOP, fill=tk.BOTH, expand=True, padx=4, pady=4)

        # Left Plot sidebar (scrollable). Nondim lives here; no right column.
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
        self._build_plot_panel(left)

        # Viewport takes the full remaining width.
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
        tip = ttk.Label(
            view_wrap,
            text="Contour toolbar above · Drag = orbit · ✎ Probe click prints values · Viewport full width",
            font=("Helvetica", 8),
            foreground=MUTED,
        )
        tip.pack(anchor="w", pady=(2, 0))
        self._log("Open a dataset — Contour toolbar always visible; Viewport uses full width.")

    def _build_plot_panel(self, parent: ttk.Frame) -> None:
        ttk.Label(parent, text="Plot", style="Panel.TLabel", font=("Helvetica", 11, "bold")).pack(
            anchor="w", padx=12, pady=(12, 6)
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
        for label, var in (
            ("Mesh", self.layer_mesh),
            ("Contour", self.layer_contour),
            ("Shade", self.layer_shade),
            ("Vector", self.layer_vector),
            ("Edge", self.layer_edge),
            ("Scatter", self.layer_scatter),
        ):
            ttk.Checkbutton(
                layers,
                text=label,
                variable=var,
                command=self.action_redraw if label in ("Mesh", "Contour") else self._layer_stub(label),
            ).pack(anchor="w", pady=1)

        derived = ttk.LabelFrame(parent, text="Show Derived Objects", padding=(8, 6))
        derived.pack(fill=tk.X, padx=10, pady=(0, 8))
        ttk.Checkbutton(
            derived,
            text="Pathlines",
            variable=self.layer_pathlines,
            command=self.action_redraw,
        ).pack(anchor="w")

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

        # Cut / track plane (I/J/K index).
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

        # Tecplot-like XYZ orientation + rotate.
        view_box = ttk.LabelFrame(parent, text="View (XYZ)", padding=(8, 6))
        view_box.pack(fill=tk.X, padx=10, pady=(0, 8))
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
        rot_row = ttk.Frame(view_box, style="Panel.TFrame")
        rot_row.pack(fill=tk.X, pady=(6, 0))
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
        ttk.Button(rot_row, text="Reset", style="Tool.TButton", command=self._reset_view).pack(
            side=tk.LEFT, padx=(6, 0)
        )
        self.view_label = ttk.Label(view_box, text="view: auto", style="Muted.TLabel")
        self.view_label.pack(anchor="w", pady=(4, 0))

        ttk.Button(parent, text="Zone Style…", style="Tool.TButton", command=self.action_zone_style).pack(
            padx=12, pady=(2, 4), anchor="w"
        )
        ttk.Button(parent, text="Contour Details…", style="Tool.TButton", command=self.action_contour_details).pack(
            padx=12, pady=(2, 4), anchor="w"
        )
        ttk.Button(parent, text="Redraw layers", style="Tool.TButton", command=self.action_redraw).pack(
            padx=12, pady=(4, 8), anchor="w"
        )

        # Collapsible Nondim (was a right column — Viewport now full width).
        self.nondim_expanded = tk.BooleanVar(value=False)
        self._nondim_hdr = ttk.Frame(parent, style="Panel.TFrame")
        self._nondim_hdr.pack(fill=tk.X, padx=10, pady=(4, 0))
        ttk.Checkbutton(
            self._nondim_hdr,
            text="Nondim scales ▸",
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
            ttk.Entry(row, textvariable=var, width=12, style="Scale.TEntry").pack(
                side=tk.LEFT, fill=tk.X, expand=True
            )
        ttk.Button(
            self.nondim_body, text="Apply nondim", style="Tool.TButton", command=self.action_nondim
        ).pack(pady=(4, 2), anchor="w")

        self._log_label = ttk.Label(parent, text="Log", style="Muted.TLabel")
        self._log_label.pack(anchor="w", padx=12, pady=(8, 0))
        self.log = tk.Text(
            parent,
            height=8,
            width=26,
            bg="#fafbfc",
            fg=FG,
            relief=tk.FLAT,
            highlightthickness=1,
            highlightbackground="#b8c0c8",
            font=("DejaVu Sans Mono", 8),
            wrap=tk.WORD,
        )
        self.log.pack(fill=tk.BOTH, expand=True, padx=10, pady=(2, 10))

        ttk.Label(
            parent,
            text="Mesh first. Contour toolbar = ± / Label / Probe / Details.\n"
            "Nondim is optional above. Viewport uses full width.",
            style="Muted.TLabel",
            justify=tk.LEFT,
        ).pack(anchor="w", padx=12, pady=(0, 12))

    def _toggle_nondim_panel(self) -> None:
        for child in self._nondim_hdr.winfo_children():
            if isinstance(child, ttk.Checkbutton):
                child.configure(
                    text=("Nondim scales ▾" if self.nondim_expanded.get() else "Nondim scales ▸")
                )
        if self.nondim_expanded.get():
            self.nondim_body.pack(fill=tk.X, padx=10, pady=(2, 4), after=self._nondim_hdr)
        else:
            self.nondim_body.pack_forget()

    def _layer_stub(self, name: str):
        def _cb() -> None:
            # Keep unchecked unimplemented layers from sticking on.
            var_map = {
                "Shade": self.layer_shade,
                "Vector": self.layer_vector,
                "Edge": self.layer_edge,
                "Scatter": self.layer_scatter,
            }
            v = var_map.get(name)
            if v is not None and v.get():
                v.set(False)
                self.stub_plot(name)
        return _cb

    def _build_status(self) -> None:
        bar = ttk.Frame(self, style="MenuBar.TFrame")
        bar.pack(side=tk.BOTTOM, fill=tk.X)
        ttk.Label(bar, textvariable=self.status_var, style="Title.TLabel", font=("Helvetica", 9)).pack(
            side=tk.LEFT, padx=10, pady=4
        )

    def _on_viewport_configure(self, _event: tk.Event | None = None) -> None:
        if self._dragging and self._settle_image is not None:
            self._warp_last_frame()
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
        mode = self.view_mode.get().strip().lower()
        if mode in ("", "auto"):
            return []
        if mode == "free":
            return ["--view", f"az={self.azimuth_deg:.2f},el={self.elevation_deg:.2f}"]
        return ["--view", mode]

    def _update_view_label(self) -> None:
        mode = self.view_mode.get()
        if mode == "free":
            text = f"view: az={self.azimuth_deg:.0f}° el={self.elevation_deg:.0f}°"
        else:
            text = f"view: {mode}"
        if hasattr(self, "view_label"):
            self.view_label.configure(text=text)

    def _set_axis_view(self, tag: str) -> None:
        self.view_mode.set(tag)
        # Match Rust ViewSpec::to_free_angles for drag handoff.
        angles = {
            "+x": (0.0, 0.0),
            "-x": (180.0, 0.0),
            "+y": (90.0, 0.0),
            "-y": (-90.0, 0.0),
            "+z": (0.0, 90.0),
            "-z": (180.0, -90.0),
        }
        self.azimuth_deg, self.elevation_deg = angles.get(tag, (0.0, 90.0))
        self._update_view_label()
        # Axis snaps need a real redraw (warp alone cannot invent a new orthographic face).
        self._schedule_settle(delay_ms=20)

    def _reset_view(self) -> None:
        self.view_mode.set("auto")
        self.azimuth_deg = 0.0
        self.elevation_deg = 90.0
        self._update_view_label()
        self._schedule_settle(delay_ms=20)

    def _nudge_view(self, daz: float, del_: float) -> None:
        self.view_mode.set("free")
        self.azimuth_deg = (self.azimuth_deg + daz) % 360.0
        self.elevation_deg = max(-90.0, min(90.0, self.elevation_deg + del_))
        self._update_view_label()
        # Tecplot feel: warp last frame immediately, CLI only after settle.
        self._warp_last_frame()
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
        for attr in ("_redraw_after", "_fullres_after", "_settle_after"):
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
        if self._dragging or self._settle_running:
            return
        self._settle_running = True
        try:
            self.action_redraw(interactive=True)
            if self._dragging:
                return
            # Second tier — full fill (cancelable if user starts dragging again).
            self._fullres_after = self.after(30, self._do_settle_full)
        finally:
            self._settle_running = False

    def _do_settle_full(self) -> None:
        self._fullres_after = None
        if self._dragging or self._in_redraw:
            return
        self.action_redraw(interactive=False)

    def _on_view_press(self, event: tk.Event) -> None:
        if self.tool_mode.get() == "probe":
            self._probe_at_canvas(event.x, event.y)
            return
        self._drag_last = (event.x, event.y)
        self._dragging = True
        # Cancel any settle / CLI — drag uses display-list warp only.
        self._cancel_redraw_timers()
        self._kill_cli()
        self._render_gen += 1

    def _on_view_drag(self, event: tk.Event) -> None:
        if self.tool_mode.get() == "probe" or self._drag_last is None:
            return
        dx = event.x - self._drag_last[0]
        dy = event.y - self._drag_last[1]
        self._drag_last = (event.x, event.y)
        if self.view_mode.get() != "free":
            self.view_mode.set("free")
        self.azimuth_deg = (self.azimuth_deg + dx * 0.4) % 360.0
        self.elevation_deg = max(-90.0, min(90.0, self.elevation_deg - dy * 0.4))
        self._update_view_label()
        # Tecplot / GPU display-list style: transform last frame — no CLI.
        self._warp_last_frame()

    def _on_view_release(self, _event: tk.Event) -> None:
        if self.tool_mode.get() == "probe":
            return
        self._drag_last = None
        self._dragging = False
        self._schedule_settle(delay_ms=50)

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

    def _warp_last_frame(self) -> None:
        """Cheap screen-space transform of the last settled PNG (no CLI)."""
        if self._settle_image is None:
            self._set_status("Rotating… (no settle frame yet)")
            return
        try:
            from PIL import Image, ImageTk
        except ImportError:
            return

        daz = self.azimuth_deg - self._settle_az
        # Shortest-path wrap so a 350° jump does not spin the bitmap the long way.
        if daz > 180.0:
            daz -= 360.0
        elif daz < -180.0:
            daz += 360.0
        del_ = self.elevation_deg - self._settle_el

        base = self._settle_image
        fill = (0xF7, 0xF8, 0xF9, 255)
        # Rotate in screen space (view azimuth) — display-list feel.
        warped = base.rotate(-daz, resample=Image.Resampling.BILINEAR, expand=False, fillcolor=fill)
        # Elevation: foreshorten + light vertical shear (billboard approximation).
        w, h = warped.size
        scale_y = max(0.35, abs(math.cos(math.radians(del_ * 0.55))))
        new_h = max(8, int(round(h * scale_y)))
        if new_h != h:
            warped = warped.resize((w, new_h), Image.Resampling.BILINEAR)
            canvas = Image.new("RGBA", (w, h), fill)
            canvas.paste(warped, (0, (h - new_h) // 2), warped)
            warped = canvas
        shear = math.tan(math.radians(max(-25.0, min(25.0, del_ * 0.35))))
        if abs(shear) > 1e-3:
            # AFFINE: x' = x + shear*(y - cy), y' = y
            cy = h * 0.5
            warped = warped.transform(
                (w, h),
                Image.Transform.AFFINE,
                (1, shear, -shear * cy, 0, 1, 0),
                resample=Image.Resampling.BILINEAR,
                fillcolor=fill,
            )

        cw = max(self.viewport.winfo_width(), 2)
        ch = max(self.viewport.winfo_height(), 2)
        max_w = max(cw - 16, 16)
        max_h = max(ch - 16, 16)
        show = warped.copy()
        show.thumbnail((max_w, max_h), Image.Resampling.BILINEAR)
        self.viewport.delete("placeholder")
        self.viewport.delete("preview")
        self._warp_photo = ImageTk.PhotoImage(show)
        self._preview_photo = self._warp_photo
        self.viewport.create_image(cw // 2, ch // 2, image=self._warp_photo, tags="preview")
        self._set_status(f"Warping az={self.azimuth_deg:.0f}° el={self.elevation_deg:.0f}°")

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
        # Exact Viewport fill: prefer 1:1 (CLI rendered at canvas size).
        target_w = max(cw - 2, 16)
        target_h = max(ch - 2, 16)
        if abs(img.width - target_w) <= 2 and abs(img.height - target_h) <= 2:
            show = img
        else:
            sx = target_w / max(img.width, 1)
            sy = target_h / max(img.height, 1)
            scale = max(sx, sy)  # cover — fill Viewport
            new_w = max(1, int(round(img.width * scale)))
            new_h = max(1, int(round(img.height * scale)))
            show = img.resize((new_w, new_h), Image.Resampling.LANCZOS)
            left = max(0, (new_w - target_w) // 2)
            top = max(0, (new_h - target_h) // 2)
            show = show.crop((left, top, left + target_w, top + target_h))
        if show.width < 4 or show.height < 4:
            self._log(
                f"Preview image too small after fit ({show.width}×{show.height}); "
                "check plane/coords (thin domain)."
            )
        self._preview_photo = ImageTk.PhotoImage(show)
        ox = cw // 2
        oy = ch // 2
        self.viewport.create_image(ox, oy, image=self._preview_photo, tags="preview")
        self._preview_offset = (ox - show.width // 2, oy - show.height // 2)
        self._preview_size = (show.width, show.height)
        self._preview_native = native
        self._draw_probe_overlays()
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
            self._log(
                "Plot type 3D Cartesian selected on effectively 2D data — "
                "falling back to 2D Cartesian view (mesh/contour plane)."
            )
            self.plot_type.set("2D Cartesian")
            messagebox.showinfo(
                "3D Cartesian",
                "This dataset is effectively 2D (unit-thickness or planar).\n"
                "Flowplot will use 2D Cartesian for mesh/contour.",
            )
        self.action_redraw()

    def _on_var_changed(self) -> None:
        if self.layer_contour.get():
            self.action_redraw()

    # --- Actions ---

    def action_load(self) -> None:
        path = filedialog.askopenfilename(
            title="Open CFD dataset",
            filetypes=[
                (
                    "CFD datasets",
                    "*.plt *.dat *.tp *.cgns *.cgz *.vtk *.vtu *.vtp *.vti *.vts *.vtr *.cas",
                ),
                ("Tecplot", "*.plt *.dat *.tp"),
                ("CGNS", "*.cgns *.cgz"),
                ("VTK", "*.vtk *.vtu *.vtp *.vti *.vts *.vtr"),
                ("Fluent", "*.cas *.dat"),
                ("All files", "*.*"),
            ],
        )
        if not path:
            return
        self.dataset_path = Path(path)
        self._log(f"Loaded path: {self.dataset_path}")
        self._set_status(str(self.dataset_path))

        # Reset layers to Tecplot defaults: Mesh on, Contour off.
        self.layer_mesh.set(True)
        self.layer_contour.set(False)
        self.layer_pathlines.set(False)
        self.layer_shade.set(False)
        self.layer_vector.set(False)
        self.layer_edge.set(False)
        self.layer_scatter.set(False)
        self.plane_axis.set("auto")
        self.plane_index.set(0)
        self.active_zone = 0
        self.mesh_color = DEFAULT_MESH_COLOR
        self.zone_styles = []
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
                        "Dataset is effectively 2D — using 2D Cartesian "
                        "(3D Cartesian would not add a volume view here)."
                    )
                    self.plot_type.set("2D Cartesian")
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

        self.action_redraw()

    def _reset_view_state(self) -> None:
        self.view_mode.set("auto")
        self.azimuth_deg = 0.0
        self.elevation_deg = 90.0
        self._settle_image = None
        self._settle_az = 0.0
        self._settle_el = 90.0
        self._settle_plane = None
        self._update_view_label()

    def action_redraw(self, interactive: bool = False) -> None:
        # Re-entrancy guard: _run_cli pumps Tk events; avoid nested redraws.
        if self._in_redraw:
            return
        ds = self._require_dataset()
        if not ds:
            return
        # While rotating/cutting, never spawn CLI — warp last frame instead.
        if self._dragging:
            self._warp_last_frame()
            return
        if self.plot_type.get() == "3D Cartesian" and self.effectively_2d:
            self._log("3D Cartesian on 2D data → 2D Cartesian fallback")
            self.plot_type.set("2D Cartesian")
        if self.plot_type.get() not in ("2D Cartesian", "3D Cartesian"):
            self._log(f"plot type {self.plot_type.get()} not drawable yet")
            return

        want_mesh = self.layer_mesh.get()
        want_contour = self.layer_contour.get()
        want_pathlines = self.layer_pathlines.get()
        zone_idx = self._active_zone_index()
        zone_args = ["--zone", str(zone_idx)]
        mesh_color_args = self._mesh_color_cli_args()

        if not want_mesh and not want_contour and not want_pathlines:
            self.last_png = None
            self._settle_image = None
            self._draw_viewport_placeholder()
            self._set_status("No layers enabled")
            return

        # Exact Viewport raster → 1:1 display (fonts stay crisp; no letterbox).
        cw = max(self.viewport.winfo_width(), 64)
        ch = max(self.viewport.winfo_height(), 64)
        if interactive:
            width, height = max(cw // 2, 320), max(ch // 2, 240)
            quality = "preview"
        else:
            width, height = min(max(cw, 64), 1920), min(max(ch, 64), 1440)
            quality = "full"

        out = Path(tempfile.gettempdir()) / f"flowplot_gui_view_{quality}.png"
        style = self.plot_style.get()
        nondim = self._nondim_args()
        plane = self._plane_cli_arg()
        view = self._view_cli_arg()
        var = self.var_name.get().strip()
        size = ["--width", str(width), "--height", str(height)]
        cd = self.contour_details
        contour_args = self._contour_cli_args()

        cache_key = "|".join(
            [
                str(ds),
                f"m={want_mesh}",
                f"c={want_contour}",
                f"p={want_pathlines}",
                f"z={zone_idx}",
                f"mc={','.join(mesh_color_args)}",
                f"cd={','.join(contour_args)}",
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
            if want_contour:
                layers.append(f"Contour({var})")
            if want_pathlines and not interactive:
                layers.append("Pathlines")
            tag = "preview" if quality == "preview" else "full"
            zname = self._zone_label(zone_idx)
            self._set_status(
                (" + ".join(layers) if layers else "Done") + f" · zone {zname} [{tag}]"
            )

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
        interactive: bool,
    ) -> subprocess.CompletedProcess[str] | None:
        if want_contour:
            if not var:
                messagebox.showwarning("Contour", "Select a contour variable (e.g. Pressure).")
                return None
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
                *nondim,
                *self._contour_cli_args(),
            ]
            if want_mesh:
                args.append("--mesh")
                args.extend(mesh_color_args)
            if want_pathlines and not interactive:
                args.append("--pathlines")
            return self._run_cli(args)
        if want_pathlines:
            args = [
                "pathlines",
                str(ds),
                "-o",
                str(out),
                "--style",
                style,
                *zone_args,
                *size,
                *plane,
                *view,
                *nondim,
            ]
            if var:
                args.extend(["--var", var])
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
            *nondim,
        ]
        return self._run_cli(args)

    def _contour_cli_args(self) -> list[str]:
        cd = self.contour_details
        args = [
            "--coloring",
            cd.cli_coloring(),
            "--levels",
            str(max(2, int(cd.nlevels))),
            "--colormap",
            cd.cli_colormap(),
            "--margin",
            "2",
        ]
        if cd.legend:
            args.append("--legend")
        else:
            args.append("--no-legend")
        return args

    def action_enable_contour(self) -> None:
        self.layer_contour.set(True)
        if not self.var_name.get().strip() and self.variables:
            self.var_name.set(first_scalar(self.variables))
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
            existing.lift()
            existing.focus_force()
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
        self.contour_details.nlevels = min(64, int(self.contour_details.nlevels) + 1)
        if self.contour_details.coloring.lower().startswith("cont"):
            self.contour_details.coloring = "Banded"
        self._refresh_levels_label()
        if self.layer_contour.get():
            self._render_cache.clear()
            self.action_redraw()

    def action_contour_remove_level(self) -> None:
        self.contour_details.nlevels = max(2, int(self.contour_details.nlevels) - 1)
        if self.contour_details.coloring.lower().startswith("cont"):
            self.contour_details.coloring = "Banded"
        self._refresh_levels_label()
        if self.layer_contour.get():
            self._render_cache.clear()
            self.action_redraw()

    def action_toggle_contour_labels(self) -> None:
        self.contour_details.show_labels = not self.contour_details.show_labels
        self._refresh_levels_label()
        self._log(
            "Contour labels "
            + ("on (placement stub — use probe for values)" if self.contour_details.show_labels else "off")
        )

    def action_toggle_probe(self) -> None:
        if self.tool_mode.get() == "probe":
            self.tool_mode.set("orbit")
            self.viewport.configure(cursor="fleur")
            self._set_status("Orbit")
        else:
            self.tool_mode.set("probe")
            self.viewport.configure(cursor="crosshair")
            self._set_status("Probe — click the Viewport to print values")
        self._refresh_levels_label()

    def _draw_probe_overlays(self) -> None:
        self.viewport.delete("probe")
        for cx, cy, text in self.probe_marks:
            self.viewport.create_line(cx - 6, cy, cx + 6, cy, fill="#1f2a33", width=1, tags="probe")
            self.viewport.create_line(cx, cy - 6, cx, cy + 6, fill="#1f2a33", width=1, tags="probe")
            self.viewport.create_rectangle(
                cx + 8,
                cy - 18,
                cx + 8 + max(70, 7 * len(text)),
                cy + 2,
                fill="#fafbfc",
                outline="#5c6b76",
                tags="probe",
            )
            self.viewport.create_text(
                cx + 12,
                cy - 8,
                text=text,
                anchor="w",
                fill="#1f2a33",
                font=("DejaVu Sans", 9),
                tags="probe",
            )

    def _probe_at_canvas(self, cx: int, cy: int) -> None:
        ds = self._require_dataset()
        if not ds:
            return
        var = self.var_name.get().strip() or first_scalar(self.variables)
        if not var:
            messagebox.showwarning("Probe", "Select a contour variable first.")
            return
        ox, oy = self._preview_offset
        pw, ph = self._preview_size
        nw, nh = self._preview_native
        if pw < 2 or ph < 2 or nw < 2 or nh < 2:
            self._log("Probe: no preview mapped yet — redraw first")
            return
        # Canvas → displayed image → native raster pixels.
        lx = cx - ox
        ly = cy - oy
        if lx < 0 or ly < 0 or lx >= pw or ly >= ph:
            self._log("Probe: click outside the plot image")
            return
        px = int(lx * nw / pw)
        py = int(ly * nh / ph)
        args = [
            "probe",
            str(ds),
            "--var",
            var,
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
        result = self._run_cli(args)
        if not result or result.returncode != 0:
            return
        label = f"{var}=?"
        try:
            payload = json.loads(result.stdout.strip().splitlines()[-1])
            if payload.get("status") == "ok":
                label = str(payload.get("label") or label)
                self._log(
                    f"probe {label} @ world=({payload.get('x')}, {payload.get('y')}) "
                    f"px=({payload.get('px')}, {payload.get('py')})"
                )
            else:
                self._log(result.stdout.strip())
                return
        except (json.JSONDecodeError, IndexError, TypeError):
            # Plain-text fallback.
            for line in result.stdout.strip().splitlines():
                if "→" in line or "probe" in line.lower():
                    label = line.strip()
                    break
            self._log(label)
        self.probe_marks.append((cx, cy, label))
        if len(self.probe_marks) > 12:
            self.probe_marks = self.probe_marks[-12:]
        self._draw_probe_overlays()
        self._set_status(label)

    def action_pathlines_dialog(self) -> None:
        self.layer_pathlines.set(True)
        self.action_redraw()

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
            existing.lift()
            existing.focus_force()
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
        self.action_redraw()

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
        dest = filedialog.asksaveasfilename(
            title="Export PNG",
            defaultextension=".png",
            filetypes=[("PNG", "*.png")],
            initialfile="flowplot.png",
        )
        if not dest:
            return
        # Force a redraw into the chosen path via temporary swap.
        prev = Path(tempfile.gettempdir()) / "flowplot_gui_view.png"
        self.action_redraw()
        if self.last_png and self.last_png.is_file():
            shutil.copyfile(self.last_png, dest)
            self._log(f"Exported {dest}")
            self._show_png_preview(Path(dest))
        else:
            self._log(f"export failed (no preview); last expected {prev}")

    def stub_plot(self, name: str) -> None:
        messagebox.showinfo(
            name,
            f"Plot → {name} matches Tecplot’s menu label.\n"
            "Not implemented in this slice (stub).",
        )

    def open_agents_panel(self) -> None:
        bin_path = self._require_cli()
        if not bin_path:
            return
        # Reuse an existing panel if still open.
        existing = getattr(self, "_agents_panel", None)
        if existing is not None and existing.winfo_exists():
            existing.dataset_path = self.dataset_path
            existing.variables = list(self.variables)
            existing.var_name.set(self.var_name.get() or first_scalar(self.variables))
            existing.nondim_args = self._nondim_args()
            existing.ds_label.configure(
                text=str(self.dataset_path) if self.dataset_path else "(no file loaded)"
            )
            existing.var_combo.configure(
                values=self.variables,
                state="readonly" if self.variables else "disabled",
            )
            existing.lift()
            existing.focus_force()
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
        )
        self._agents_panel = panel

    def show_about(self) -> None:
        messagebox.showinfo(
            "About Flowplot",
            "Flowplot — CFD post-processor\n"
            "Tecplot-like Plot panel: Mesh first, Contour optional.\n"
            f"Wordmark: {self._wordmark_path}\n"
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
                            if app.plot_type.get() == "3D Cartesian" and app.effectively_2d:
                                app.plot_type.set("2D Cartesian")
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
