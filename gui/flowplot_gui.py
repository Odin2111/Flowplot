#!/usr/bin/env python3
"""Flowplot GUI shell — first layout (not a full renderer).

Thin Python front-end that shells out to the `flowplot` CLI for load /
contour / nondim / pathlines / PNG export. Unimplemented actions stay stubs.
"""

from __future__ import annotations

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


class FlowplotGui(tk.Tk):
    def __init__(self) -> None:
        super().__init__()
        self.title("Flowplot")
        self.geometry("1100x680")
        self.minsize(900, 560)
        self.configure(bg=BG)

        self.dataset_path: Path | None = None
        self.last_png: Path | None = None
        self.flowplot = find_flowplot_bin()
        self.status_var = tk.StringVar(value=self._status_idle())
        self.var_name = tk.StringVar(value="P")
        self.plot_style = tk.StringVar(value="jfm")

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

    def _build_menu(self) -> None:
        menubar = tk.Menu(self, tearoff=0, bg=MENU_BG, fg=FG, activebackground=ACCENT, activeforeground="#fff")
        file_m = tk.Menu(menubar, tearoff=0)
        file_m.add_command(label="Open…", command=self.action_load, accelerator="Ctrl+O")
        file_m.add_command(label="Export PNG…", command=self.action_export_png)
        file_m.add_separator()
        file_m.add_command(label="Quit", command=self.destroy)
        menubar.add_cascade(label="File", menu=file_m)

        plot_m = tk.Menu(menubar, tearoff=0)
        plot_m.add_command(label="Contour…", command=self.action_contour)
        plot_m.add_command(label="Pathlines…", command=self.action_pathlines)
        plot_m.add_command(label="Apply nondim scales", command=self.action_nondim)
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
        """Visible menu strip (File / Plot / Style / Agents / Help).

        Complements the native ``tk.Menu`` so the names stay obvious on X11 /
        screenshots (window managers sometimes hide the system menubar).
        """
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
        plot_m.add_command(label="Contour…", command=self.action_contour)
        plot_m.add_command(label="Pathlines…", command=self.action_pathlines)
        plot_m.add_command(label="Apply nondim scales", command=self.action_nondim)
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
        # Tk cannot render SVG natively; draw a compact wordmark on a canvas.
        logo = tk.Canvas(
            logo_frame, width=200, height=32, bg=MENU_BG, highlightthickness=0, bd=0
        )
        logo.pack()
        # Pathline glyph
        logo.create_line(6, 24, 14, 10, 22, 10, 30, 24, smooth=True, fill=ACCENT, width=2)
        logo.create_line(10, 26, 18, 14, 26, 14, 34, 26, smooth=True, fill=ACCENT, width=1)
        logo.create_oval(32, 15, 36, 19, fill=ACCENT, outline=ACCENT)
        logo.create_text(44, 16, text="Flowplot", anchor="w", fill=FG, font=("Helvetica", 14, "bold"))
        self._logo_canvas = logo
        self._wordmark_path = WORDMARK  # shipped SVG for docs / web use

        tools = ttk.Frame(chrome, style="MenuBar.TFrame")
        tools.pack(side=tk.LEFT, padx=8, pady=4)
        for label, cmd in (
            ("Load", self.action_load),
            ("Contour", self.action_contour),
            ("Nondim", self.action_nondim),
            ("Pathlines", self.action_pathlines),
            ("Export PNG", self.action_export_png),
        ):
            ttk.Button(tools, text=label, style="Tool.TButton", command=cmd).pack(
                side=tk.LEFT, padx=3
            )

        ttk.Label(chrome, text="var", style="Title.TLabel", font=("Helvetica", 9)).pack(
            side=tk.LEFT, padx=(16, 4)
        )
        ttk.Entry(chrome, textvariable=self.var_name, width=8).pack(side=tk.LEFT)

    def _build_body(self) -> None:
        body = ttk.Frame(self)
        body.pack(side=tk.TOP, fill=tk.BOTH, expand=True, padx=8, pady=6)

        # Center viewport placeholder
        view_wrap = ttk.Frame(body)
        view_wrap.pack(side=tk.LEFT, fill=tk.BOTH, expand=True, padx=(0, 8))
        view_label = ttk.Label(view_wrap, text="Viewport", font=("Helvetica", 9, "bold"), foreground=MUTED)
        view_label.pack(anchor="w", pady=(0, 4))

        self.viewport = tk.Canvas(
            view_wrap,
            bg=VIEWPORT_BG,
            highlightthickness=1,
            highlightbackground="#9aa3ab",
        )
        self.viewport.pack(fill=tk.BOTH, expand=True)
        self.viewport.bind("<Configure>", self._draw_viewport_placeholder)

        # Right nondim column
        right = ttk.Frame(body, style="Panel.TFrame", width=220)
        right.pack(side=tk.RIGHT, fill=tk.Y)
        right.pack_propagate(False)

        ttk.Label(right, text="Nondim scales", style="Panel.TLabel", font=("Helvetica", 10, "bold")).pack(
            anchor="w", padx=12, pady=(12, 4)
        )
        ttk.Label(
            right,
            text="Passed to flowplot as\n--u-ref / --lx / --ly / --lz / --t-ref",
            style="Muted.TLabel",
            justify=tk.LEFT,
        ).pack(anchor="w", padx=12, pady=(0, 10))

        form = ttk.Frame(right, style="Panel.TFrame")
        form.pack(fill=tk.X, padx=12)
        for key, var in self.scale_vars.items():
            row = ttk.Frame(form, style="Panel.TFrame")
            row.pack(fill=tk.X, pady=3)
            ttk.Label(row, text=key, width=7, style="Panel.TLabel").pack(side=tk.LEFT)
            ttk.Entry(row, textvariable=var, width=14, style="Scale.TEntry").pack(
                side=tk.LEFT, fill=tk.X, expand=True
            )

        ttk.Button(right, text="Apply nondim", style="Tool.TButton", command=self.action_nondim).pack(
            padx=12, pady=14, anchor="w"
        )

        self.log = tk.Text(
            right,
            height=12,
            width=28,
            bg="#fafbfc",
            fg=FG,
            relief=tk.FLAT,
            highlightthickness=1,
            highlightbackground="#b8c0c8",
            font=("DejaVu Sans Mono", 8),
            wrap=tk.WORD,
        )
        self.log.pack(fill=tk.BOTH, expand=True, padx=12, pady=(0, 12))
        self._log("Idle layout. Load a Tecplot/VTK ASCII file to begin.")

    def _build_status(self) -> None:
        bar = ttk.Frame(self, style="MenuBar.TFrame")
        bar.pack(side=tk.BOTTOM, fill=tk.X)
        ttk.Label(bar, textvariable=self.status_var, style="Title.TLabel", font=("Helvetica", 9)).pack(
            side=tk.LEFT, padx=10, pady=4
        )

    def _draw_viewport_placeholder(self, _event: tk.Event | None = None) -> None:
        c = self.viewport
        c.delete("placeholder")
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
            h // 2 + 16,
            text="Contour / pathline render goes here\n(PNG preview after Contour or Export)",
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
        messagebox.showwarning("No dataset", "Load a CFD file first (File → Open or Load).")
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
                timeout=120,
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
            self._log(out)
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

    # --- Actions (wire to CLI; stubs where needed) ---

    def action_load(self) -> None:
        path = filedialog.askopenfilename(
            title="Open CFD dataset",
            filetypes=[
                ("CFD / Tecplot / VTK", "*.dat *.tp *.vtk"),
                ("All files", "*.*"),
            ],
        )
        if not path:
            return
        self.dataset_path = Path(path)
        self._log(f"Loaded path: {self.dataset_path}")
        self._set_status(str(self.dataset_path))
        self._run_cli(["info", str(self.dataset_path)])

    def action_contour(self) -> None:
        ds = self._require_dataset()
        if not ds:
            return
        out = Path(tempfile.gettempdir()) / "flowplot_gui_contour.png"
        args = [
            "contour",
            str(ds),
            "--var",
            self.var_name.get().strip() or "P",
            "-o",
            str(out),
            "--style",
            self.plot_style.get(),
            *self._nondim_args(),
        ]
        proc = self._run_cli(args)
        if proc and proc.returncode == 0 and out.is_file():
            self._show_png_preview(out)

    def action_nondim(self) -> None:
        ds = self._require_dataset()
        if not ds:
            return
        self._run_cli(["nondim", str(ds), *self._nondim_args()])

    def action_pathlines(self) -> None:
        ds = self._require_dataset()
        if not ds:
            return
        out = Path(tempfile.gettempdir()) / "flowplot_gui_pathlines.png"
        args = [
            "pathlines",
            str(ds),
            "-o",
            str(out),
            "--style",
            self.plot_style.get(),
            *self._nondim_args(),
        ]
        var = self.var_name.get().strip()
        if var:
            args.extend(["--var", var])
        proc = self._run_cli(args)
        if proc and proc.returncode == 0 and out.is_file():
            self._show_png_preview(out)

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
        args = [
            "contour",
            str(ds),
            "--var",
            self.var_name.get().strip() or "P",
            "-o",
            dest,
            "--style",
            self.plot_style.get(),
            *self._nondim_args(),
        ]
        proc = self._run_cli(args)
        if proc and proc.returncode == 0:
            self._show_png_preview(Path(dest))
            self._log(f"Exported {dest}")

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
            "First GUI layout (viewport placeholder; CLI-backed actions).\n"
            f"Wordmark: {self._wordmark_path}\n"
            f"CLI: {self.flowplot or '(not found)'}",
        )


def main() -> int:
    if os.environ.get("FLOWPLOT_GUI_SCREENSHOT"):
        # Headless capture path used by tools/screenshot_gui.py
        out = Path(os.environ["FLOWPLOT_GUI_SCREENSHOT"])
        app = FlowplotGui()
        app.update_idletasks()
        app.update()
        # Give geometry a moment under Xvfb
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
        # Fallback: blank canvas note
        app._log("Pillow missing; cannot screenshot")
        app.destroy()
        return
    x = app.winfo_rootx()
    y = app.winfo_rooty()
    w = app.winfo_width()
    h = app.winfo_height()
    # Slight pad so window chrome is included
    img = ImageGrab.grab(bbox=(x, y, x + w, y + h))
    img.save(out)
    app.destroy()


if __name__ == "__main__":
    sys.exit(main())
