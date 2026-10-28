"""Tecplot-like Zone Style dialog for Flowplot.

Tabs mirror Tecplot 360: Mesh, Contour, Vector, Scatter, Shade, Edge, Points,
Surfaces, Volume, Effects. Mesh + Contour rows drive redraw; other tabs are
labeled stubs for now.
"""

from __future__ import annotations

import copy
import tkinter as tk
from dataclasses import dataclass
from tkinter import colorchooser, messagebox, ttk
from typing import Callable


# Match Flowplot GUI palette (restrained scientific slate).
PANEL = "#e2e5e8"
BG = "#eceef0"
FG = "#1f2a33"
MUTED = "#5c6b76"
ACCENT = "#3d5a6c"
HEADER_BG = "#d5d9de"
ROW_ALT = "#f4f5f6"
BTN_BG = "#f4f5f6"

MESH_TYPES = ("Wire Frame", "Overlay", "Points")
LINE_PATTERNS = ("Solid", "Dashed", "Dotted")
DEFAULT_MESH_COLOR = "#22A03C"

ZONE_STYLE_TABS = (
    "Mesh",
    "Contour",
    "Vector",
    "Scatter",
    "Shade",
    "Edge",
    "Points",
    "Surfaces",
    "Volume",
    "Effects",
)


@dataclass
class ZoneStyleState:
    """Per-zone style row (Tecplot Zone Style table)."""

    index: int
    name: str
    group: int = 0
    show_zone: bool = True
    show_mesh: bool = True
    mesh_type: str = "Wire Frame"
    mesh_color: str = DEFAULT_MESH_COLOR
    line_pattern: str = "Solid"
    show_contour: bool = False
    # Stubs kept for forward-compatible Apply sync.
    show_vector: bool = False
    show_scatter: bool = False
    show_shade: bool = False
    show_edge: bool = False


def zone_styles_from_info(zones: list[dict]) -> list[ZoneStyleState]:
    """Build default Zone Style rows from `flowplot --json info` zones."""
    rows: list[ZoneStyleState] = []
    for z in zones:
        if not isinstance(z, dict):
            continue
        try:
            idx = int(z.get("index", len(rows)))
        except (TypeError, ValueError):
            idx = len(rows)
        name = str(z.get("name") or f"Zone {idx + 1}")
        group = 0
        try:
            if z.get("group") is not None:
                group = int(z["group"])
        except (TypeError, ValueError):
            group = 0
        rows.append(
            ZoneStyleState(
                index=idx,
                name=name,
                group=group,
                show_zone=True,
                show_mesh=True,
                show_contour=False,
            )
        )
    if not rows:
        rows.append(ZoneStyleState(index=0, name="Zone 1"))
    return rows


def parse_hex_rgb(color: str) -> tuple[int, int, int] | None:
    """Parse `#RRGGBB` or `R,G,B` into 0–255 RGB."""
    s = (color or "").strip()
    if not s:
        return None
    if s.startswith("#") and len(s) == 7:
        try:
            return int(s[1:3], 16), int(s[3:5], 16), int(s[5:7], 16)
        except ValueError:
            return None
    parts = [p.strip() for p in s.split(",")]
    if len(parts) == 3:
        try:
            r, g, b = (int(parts[0]), int(parts[1]), int(parts[2]))
            if all(0 <= v <= 255 for v in (r, g, b)):
                return r, g, b
        except ValueError:
            return None
    return None


def rgb_cli(color: str) -> str:
    """CLI `--mesh-color` value `R,G,B`."""
    rgb = parse_hex_rgb(color) or parse_hex_rgb(DEFAULT_MESH_COLOR)
    assert rgb is not None
    return f"{rgb[0]},{rgb[1]},{rgb[2]}"


class ZoneStyleDialog(tk.Toplevel):
    """Modal-ish Zone Style window (Apply / Close like Tecplot)."""

    def __init__(
        self,
        master: tk.Misc,
        styles: list[ZoneStyleState],
        *,
        on_apply: Callable[[list[ZoneStyleState]], None] | None = None,
        initial_tab: str = "Mesh",
    ) -> None:
        super().__init__(master)
        self.title("Zone Style")
        self.configure(bg=BG)
        self.geometry("920x420")
        self.minsize(720, 320)
        self.transient(master.winfo_toplevel())

        self._on_apply = on_apply
        self.styles: list[ZoneStyleState] = copy.deepcopy(styles) or [
            ZoneStyleState(index=0, name="Zone 1")
        ]
        self._vars: list[dict] = []
        self._table_frame: ttk.Frame | None = None
        self._active_tab = tk.StringVar(value=initial_tab if initial_tab in ZONE_STYLE_TABS else "Mesh")

        self._build()
        self.protocol("WM_DELETE_WINDOW", self.destroy)
        self.after(30, self._focus)

    def _focus(self) -> None:
        try:
            self.lift()
            self.focus_force()
        except tk.TclError:
            pass

    def _build(self) -> None:
        tip = ttk.Label(
            self,
            text="Per-zone Show Zone / Mesh / Contour (Apply redraws). Other tabs are stubs.",
            foreground=MUTED,
            background=BG,
        )
        tip.pack(anchor="w", padx=10, pady=(8, 4))

        nb = ttk.Notebook(self)
        nb.pack(fill=tk.BOTH, expand=True, padx=8, pady=4)
        self._notebook = nb

        self._tab_bodies: dict[str, ttk.Frame] = {}
        for name in ZONE_STYLE_TABS:
            page = ttk.Frame(nb, padding=4)
            nb.add(page, text=name)
            self._tab_bodies[name] = page
            if name in ("Mesh", "Contour"):
                # Shared table rebuilt on tab change so columns match the active tab.
                pass
            else:
                stub = ttk.Label(
                    page,
                    text=(
                        f"{name} zone attributes match Tecplot’s Zone Style tab.\n"
                        "Not wired in this slice (stub)."
                    ),
                    foreground=MUTED,
                    justify=tk.LEFT,
                )
                stub.pack(anchor="w", padx=8, pady=12)

        # One shared working table hosted under Mesh; Contour reuses the same
        # widgets when that tab is selected (columns swap).
        self._host = ttk.Frame(self._tab_bodies["Mesh"])
        self._host.pack(fill=tk.BOTH, expand=True)
        self._build_table(self._host, "Mesh")

        # Contour page shows the same table via raise/reparent on tab change.
        self._contour_host = ttk.Frame(self._tab_bodies["Contour"])
        self._contour_host.pack(fill=tk.BOTH, expand=True)

        nb.bind("<<NotebookTabChanged>>", self._on_tab_changed)

        btn_row = ttk.Frame(self)
        btn_row.pack(fill=tk.X, padx=10, pady=(4, 10))
        ttk.Button(btn_row, text="Apply", command=self._apply).pack(side=tk.RIGHT, padx=4)
        ttk.Button(btn_row, text="Close", command=self.destroy).pack(side=tk.RIGHT, padx=4)
        ttk.Button(btn_row, text="Select All Zones", command=lambda: self._set_all_show(True)).pack(
            side=tk.LEFT, padx=2
        )
        ttk.Button(btn_row, text="Clear Show Zone", command=lambda: self._set_all_show(False)).pack(
            side=tk.LEFT, padx=2
        )

    def _on_tab_changed(self, _event: object | None = None) -> None:
        try:
            tab_id = self._notebook.select()
            name = self._notebook.tab(tab_id, "text")
        except tk.TclError:
            return
        self._active_tab.set(name)
        if name not in ("Mesh", "Contour"):
            return
        self._harvest()
        parent = self._host if name == "Mesh" else self._contour_host
        for child in parent.winfo_children():
            child.destroy()
        # Move/rebuild table into the active working tab.
        self._build_table(parent, name)

    def _build_table(self, parent: ttk.Frame, tab: str) -> None:
        wrap = ttk.Frame(parent)
        wrap.pack(fill=tk.BOTH, expand=True)

        canvas = tk.Canvas(wrap, bg=BG, highlightthickness=0)
        vsb = ttk.Scrollbar(wrap, orient="vertical", command=canvas.yview)
        canvas.configure(yscrollcommand=vsb.set)
        vsb.pack(side=tk.RIGHT, fill=tk.Y)
        canvas.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)

        inner = ttk.Frame(canvas)
        win = canvas.create_window((0, 0), window=inner, anchor="nw")

        def _on_configure(_event: tk.Event | None = None) -> None:
            canvas.configure(scrollregion=canvas.bbox("all"))
            canvas.itemconfigure(win, width=canvas.winfo_width())

        inner.bind("<Configure>", _on_configure)
        canvas.bind("<Configure>", lambda e: canvas.itemconfigure(win, width=e.width))

        headers = ["Zone Number", "Zone Name", "Group", "Show Zone"]
        if tab == "Mesh":
            headers += ["Show Mesh", "Mesh Type", "Mesh Color", "Line Pattern"]
        elif tab == "Contour":
            headers += ["Show Contour"]

        header = tk.Frame(inner, bg=HEADER_BG)
        header.pack(fill=tk.X)
        for i, h in enumerate(headers):
            tk.Label(
                header,
                text=h,
                bg=HEADER_BG,
                fg=FG,
                font=("Helvetica", 9, "bold"),
                padx=6,
                pady=4,
            ).grid(row=0, column=i, sticky="nsew")
            header.grid_columnconfigure(i, weight=1 if i in (1, 5, 6, 7) else 0)

        self._vars = []
        for r, zs in enumerate(self.styles):
            bg = ROW_ALT if r % 2 else "#ffffff"
            row = tk.Frame(inner, bg=bg)
            row.pack(fill=tk.X)
            show_zone = tk.BooleanVar(value=zs.show_zone)
            show_mesh = tk.BooleanVar(value=zs.show_mesh)
            show_contour = tk.BooleanVar(value=zs.show_contour)
            mesh_type = tk.StringVar(value=zs.mesh_type if zs.mesh_type in MESH_TYPES else "Wire Frame")
            mesh_color = tk.StringVar(value=zs.mesh_color or DEFAULT_MESH_COLOR)
            line_pat = tk.StringVar(
                value=zs.line_pattern if zs.line_pattern in LINE_PATTERNS else "Solid"
            )
            group = tk.StringVar(value=str(zs.group))

            tk.Label(row, text=str(zs.index + 1), bg=bg, fg=FG, width=10, anchor="w", padx=6).grid(
                row=0, column=0, sticky="w"
            )
            tk.Label(row, text=zs.name, bg=bg, fg=FG, anchor="w", padx=6).grid(
                row=0, column=1, sticky="ew"
            )
            tk.Entry(row, textvariable=group, width=6, bg="#ffffff", fg=FG).grid(
                row=0, column=2, padx=4, pady=2
            )
            tk.Checkbutton(row, variable=show_zone, bg=bg, activebackground=bg).grid(
                row=0, column=3
            )

            col = 4
            if tab == "Mesh":
                tk.Checkbutton(row, variable=show_mesh, bg=bg, activebackground=bg).grid(
                    row=0, column=col
                )
                col += 1
                ttk.Combobox(
                    row,
                    textvariable=mesh_type,
                    values=MESH_TYPES,
                    state="readonly",
                    width=12,
                ).grid(row=0, column=col, padx=2)
                col += 1
                color_fr = tk.Frame(row, bg=bg)
                color_fr.grid(row=0, column=col, padx=2)
                swatch = tk.Label(
                    color_fr,
                    text="  ",
                    bg=mesh_color.get() if parse_hex_rgb(mesh_color.get()) else DEFAULT_MESH_COLOR,
                    width=3,
                    relief=tk.GROOVE,
                )
                swatch.pack(side=tk.LEFT, padx=(0, 4))

                def _pick(
                    var: tk.StringVar = mesh_color,
                    sw: tk.Label = swatch,
                ) -> None:
                    rgb = parse_hex_rgb(var.get())
                    initial = "#%02x%02x%02x" % rgb if rgb else DEFAULT_MESH_COLOR
                    picked = colorchooser.askcolor(color=initial, parent=self, title="Mesh Color")
                    if picked and picked[1]:
                        var.set(picked[1])
                        sw.configure(bg=picked[1])

                tk.Button(
                    color_fr,
                    text="Color…",
                    command=_pick,
                    bg=BTN_BG,
                    fg=FG,
                    relief=tk.GROOVE,
                    padx=4,
                ).pack(side=tk.LEFT)
                col += 1
                ttk.Combobox(
                    row,
                    textvariable=line_pat,
                    values=LINE_PATTERNS,
                    state="readonly",
                    width=10,
                ).grid(row=0, column=col, padx=2)
            elif tab == "Contour":
                tk.Checkbutton(row, variable=show_contour, bg=bg, activebackground=bg).grid(
                    row=0, column=col
                )

            row.grid_columnconfigure(1, weight=1)
            self._vars.append(
                {
                    "index": zs.index,
                    "name": zs.name,
                    "show_zone": show_zone,
                    "show_mesh": show_mesh,
                    "show_contour": show_contour,
                    "mesh_type": mesh_type,
                    "mesh_color": mesh_color,
                    "line_pattern": line_pat,
                    "group": group,
                    "show_vector": zs.show_vector,
                    "show_scatter": zs.show_scatter,
                    "show_shade": zs.show_shade,
                    "show_edge": zs.show_edge,
                }
            )

        self._table_frame = inner

    def _harvest(self) -> None:
        """Pull widget values back into self.styles."""
        if not self._vars:
            return
        by_idx = {s.index: s for s in self.styles}
        for v in self._vars:
            zs = by_idx.get(v["index"])
            if zs is None:
                continue
            zs.show_zone = bool(v["show_zone"].get())
            zs.show_mesh = bool(v["show_mesh"].get())
            zs.show_contour = bool(v["show_contour"].get())
            zs.mesh_type = v["mesh_type"].get()
            zs.mesh_color = v["mesh_color"].get()
            zs.line_pattern = v["line_pattern"].get()
            try:
                zs.group = int(str(v["group"].get()).strip() or "0")
            except ValueError:
                zs.group = 0

    def _set_all_show(self, value: bool) -> None:
        self._harvest()
        for zs in self.styles:
            zs.show_zone = value
        tab = self._active_tab.get()
        if tab in ("Mesh", "Contour"):
            parent = self._host if tab == "Mesh" else self._contour_host
            for child in parent.winfo_children():
                child.destroy()
            self._build_table(parent, tab)

    def _apply(self) -> None:
        self._harvest()
        if self._on_apply is not None:
            self._on_apply(copy.deepcopy(self.styles))
        else:
            messagebox.showinfo("Zone Style", "Applied (no redraw callback).", parent=self)
