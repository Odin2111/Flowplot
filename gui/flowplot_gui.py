#!/usr/bin/env python3
"""Flowplot GUI — Tecplot-like Plot panel (mesh first, contours optional).

Thin Python front-end that shells out to the `flowplot` CLI. On File→Open the
default view is the zone **Mesh** (wireframe crate) with Contour off. Contour
and Pathlines are optional layers, matching Tecplot’s Show Zone Layers /
Show Derived Objects order (Tecplot’s “Streamtraces” → Flowplot “Pathlines”).
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import tkinter as tk
from pathlib import Path
from tkinter import filedialog, messagebox, ttk

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
        self.flowplot = find_flowplot_bin()
        self.status_var = tk.StringVar(value=self._status_idle())
        self.var_name = tk.StringVar(value="")
        self.plot_style = tk.StringVar(value="jfm")
        self.plot_type = tk.StringVar(value="2D Cartesian")

        # Tecplot Show Zone Layers — Mesh ON, Contour OFF by default.
        self.layer_mesh = tk.BooleanVar(value=True)
        self.layer_contour = tk.BooleanVar(value=False)
        self.layer_shade = tk.BooleanVar(value=False)
        self.layer_vector = tk.BooleanVar(value=False)
        self.layer_edge = tk.BooleanVar(value=False)
        self.layer_scatter = tk.BooleanVar(value=False)
        self.layer_pathlines = tk.BooleanVar(value=False)

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
        self._build_chrome()
        self._build_body()
        self._build_status()

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
        menu.add_command(label="Zone Style…", command=lambda: self.stub_plot("Zone Style"))
        menu.add_separator()
        menu.add_command(label="Pathlines…", command=self.action_pathlines_dialog)
        menu.add_command(label="Contour…", command=self.action_enable_contour)
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
        agents_m.add_command(label="Post-process agents…", command=self.stub_agents)
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
        agents_m.add_command(label="Post-process agents…", command=self.stub_agents)
        mb_agents.configure(menu=agents_m)

        mb_help = menubutton("Help")
        help_m = tk.Menu(mb_help, tearoff=0)
        help_m.add_command(label="About Flowplot", command=self.show_about)
        mb_help.configure(menu=help_m)

        tk.Frame(self, bg="#b8c0c8", height=1).pack(side=tk.TOP, fill=tk.X)

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
        body.pack(side=tk.TOP, fill=tk.BOTH, expand=True, padx=8, pady=6)

        # Left: Tecplot-like Plot sidebar
        left = ttk.Frame(body, style="Panel.TFrame", width=240)
        left.pack(side=tk.LEFT, fill=tk.Y, padx=(0, 8))
        left.pack_propagate(False)
        self._build_plot_panel(left)

        # Center viewport
        view_wrap = ttk.Frame(body)
        view_wrap.pack(side=tk.LEFT, fill=tk.BOTH, expand=True, padx=(0, 8))
        view_label = ttk.Label(
            view_wrap, text="Viewport", font=("Helvetica", 9, "bold"), foreground=MUTED
        )
        view_label.pack(anchor="w", pady=(0, 4))

        self.viewport = tk.Canvas(
            view_wrap,
            bg=VIEWPORT_BG,
            highlightthickness=1,
            highlightbackground="#9aa3ab",
        )
        self.viewport.pack(fill=tk.BOTH, expand=True)
        self.viewport.bind("<Configure>", self._on_viewport_configure)

        # Right nondim column
        right = ttk.Frame(body, style="Panel.TFrame", width=200)
        right.pack(side=tk.RIGHT, fill=tk.Y)
        right.pack_propagate(False)

        ttk.Label(right, text="Nondim scales", style="Panel.TLabel", font=("Helvetica", 10, "bold")).pack(
            anchor="w", padx=12, pady=(12, 4)
        )
        ttk.Label(
            right,
            text="--u-ref / --lx / --ly / --lz / --t-ref",
            style="Muted.TLabel",
            justify=tk.LEFT,
        ).pack(anchor="w", padx=12, pady=(0, 10))

        form = ttk.Frame(right, style="Panel.TFrame")
        form.pack(fill=tk.X, padx=12)
        for key, var in self.scale_vars.items():
            row = ttk.Frame(form, style="Panel.TFrame")
            row.pack(fill=tk.X, pady=3)
            ttk.Label(row, text=key, width=7, style="Panel.TLabel").pack(side=tk.LEFT)
            ttk.Entry(row, textvariable=var, width=12, style="Scale.TEntry").pack(
                side=tk.LEFT, fill=tk.X, expand=True
            )

        ttk.Button(right, text="Apply nondim", style="Tool.TButton", command=self.action_nondim).pack(
            padx=12, pady=14, anchor="w"
        )

        self.log = tk.Text(
            right,
            height=12,
            width=24,
            bg="#fafbfc",
            fg=FG,
            relief=tk.FLAT,
            highlightthickness=1,
            highlightbackground="#b8c0c8",
            font=("DejaVu Sans Mono", 8),
            wrap=tk.WORD,
        )
        self.log.pack(fill=tk.BOTH, expand=True, padx=12, pady=(0, 12))
        self._log("Open a dataset — Mesh layer on by default (crate / wireframe).")

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

        ttk.Button(parent, text="Redraw layers", style="Tool.TButton", command=self.action_redraw).pack(
            padx=12, pady=(4, 8), anchor="w"
        )

        ttk.Label(
            parent,
            text="Mesh on = grid first.\nContour is optional.",
            style="Muted.TLabel",
            justify=tk.LEFT,
        ).pack(anchor="w", padx=12, pady=(4, 12))

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

    def _run_cli(self, args: list[str]) -> subprocess.CompletedProcess[str] | None:
        bin_path = self._require_cli()
        if not bin_path:
            return None
        cmd = [bin_path, *args]
        self._log("$ " + " ".join(cmd))
        self._set_status("Running…")
        self.update_idletasks()
        try:
            proc = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=180,
                check=False,
            )
        except subprocess.TimeoutExpired:
            self._log("error: flowplot timed out")
            self._set_status("Timed out")
            return None
        except OSError as exc:
            self._log(f"error: {exc}")
            self._set_status("Failed to launch CLI")
            return None
        out = (proc.stdout or "").strip()
        err = (proc.stderr or "").strip()
        if out:
            self._log(out if len(out) < 2000 else out[:2000] + "…")
        if err:
            self._log(err)
        if proc.returncode != 0:
            self._set_status(f"CLI exit {proc.returncode}")
        else:
            self._set_status("Done")
        return proc

    def _show_png_preview(self, path: Path) -> None:
        """Best-effort PNG preview in the viewport (Pillow optional)."""
        try:
            from PIL import Image, ImageTk
        except ImportError:
            self._log(f"Wrote {path} (install Pillow for in-window preview)")
            return
        try:
            img = Image.open(path).convert("RGBA")
        except OSError as exc:
            self._log(f"Could not open PNG: {exc}")
            return
        self.viewport.delete("placeholder")
        self.viewport.delete("preview")
        cw = max(self.viewport.winfo_width(), 2)
        ch = max(self.viewport.winfo_height(), 2)
        img.thumbnail((cw - 16, ch - 16), Image.Resampling.LANCZOS)
        self._preview_photo = ImageTk.PhotoImage(img)
        self.viewport.create_image(cw // 2, ch // 2, image=self._preview_photo, tags="preview")
        self.last_png = path

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

        proc = self._run_cli(["--json", "info", str(self.dataset_path)])
        if proc and proc.returncode == 0 and proc.stdout:
            try:
                payload = json.loads(proc.stdout.strip().splitlines()[-1])
                vars_ = payload.get("variables") or []
                if isinstance(vars_, list):
                    self._set_variables([str(v) for v in vars_])
            except json.JSONDecodeError:
                self._log("warning: could not parse info JSON; variable list empty")
                self._set_variables([])
        else:
            self._set_variables([])

        self.action_redraw()

    def action_redraw(self) -> None:
        ds = self._require_dataset()
        if not ds:
            return
        if self.plot_type.get() not in ("2D Cartesian", "3D Cartesian"):
            self._log(f"plot type {self.plot_type.get()} not drawable yet")
            return

        want_mesh = self.layer_mesh.get()
        want_contour = self.layer_contour.get()
        want_pathlines = self.layer_pathlines.get()

        if not want_mesh and not want_contour and not want_pathlines:
            self.last_png = None
            self._draw_viewport_placeholder()
            self._set_status("No layers enabled")
            return

        out = Path(tempfile.gettempdir()) / "flowplot_gui_view.png"
        style = self.plot_style.get()
        nondim = self._nondim_args()
        var = self.var_name.get().strip()

        if want_contour:
            if not var:
                messagebox.showwarning("Contour", "Select a contour variable (e.g. Pressure).")
                return
            args = [
                "contour",
                str(ds),
                "--var",
                var,
                "-o",
                str(out),
                "--style",
                style,
                *nondim,
            ]
            if want_mesh:
                args.append("--mesh")
            if want_pathlines:
                args.append("--pathlines")
            proc = self._run_cli(args)
        elif want_pathlines:
            args = [
                "pathlines",
                str(ds),
                "-o",
                str(out),
                "--style",
                style,
                *nondim,
            ]
            if var:
                args.extend(["--var", var])
            proc = self._run_cli(args)
        else:
            # Mesh only — equal-aspect wireframe crate.
            args = [
                "mesh",
                str(ds),
                "-o",
                str(out),
                "--style",
                style,
                *nondim,
            ]
            proc = self._run_cli(args)

        if proc and proc.returncode == 0 and out.is_file():
            self._show_png_preview(out)
            layers = []
            if want_mesh:
                layers.append("Mesh")
            if want_contour:
                layers.append(f"Contour({var})")
            if want_pathlines:
                layers.append("Pathlines")
            self._set_status(" + ".join(layers) if layers else "Done")

    def action_enable_contour(self) -> None:
        self.layer_contour.set(True)
        if not self.var_name.get().strip() and self.variables:
            self.var_name.set(first_scalar(self.variables))
        self.action_redraw()

    def action_pathlines_dialog(self) -> None:
        self.layer_pathlines.set(True)
        self.action_redraw()

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

    def stub_agents(self) -> None:
        messagebox.showinfo(
            "Agents",
            "Agent post-process menu is wired in the lehar ↔ Flowplot bridge (PR #3).\n"
            "This GUI entry is a stub until the interactive agent panel lands.",
        )

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
        app = FlowplotGui()
        app.update_idletasks()
        app.update()
        app.after(200, lambda: _capture_and_quit(app, out))
        app.mainloop()
        return 0 if out.is_file() else 1

    app = FlowplotGui()
    app.mainloop()
    return 0


def _capture_and_quit(app: FlowplotGui, out: Path) -> None:
    app.update_idletasks()
    app.update()
    out.parent.mkdir(parents=True, exist_ok=True)
    try:
        from PIL import ImageGrab
    except ImportError:
        app._log("Pillow missing; cannot screenshot")
        app.destroy()
        return
    x = app.winfo_rootx()
    y = app.winfo_rooty()
    w = app.winfo_width()
    h = app.winfo_height()
    img = ImageGrab.grab(bbox=(x, y, x + w, y + h))
    img.save(out)
    app.destroy()


if __name__ == "__main__":
    sys.exit(main())
