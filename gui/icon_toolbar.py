"""icon toolbar under the menubar (canvas-drawn icons)."""

from __future__ import annotations

import tkinter as tk
from collections.abc import Callable
from dataclasses import dataclass


MENU_BG = "#d5d9de"
BTN_BG = "#f4f5f6"
BTN_ACTIVE = "#cfd5db"
BTN_BORDER = "#9aa3ab"
FG = "#1f2a33"
ACCENT = "#3d5a6c"
MUTED = "#5c6b76"
ICON = 22


@dataclass
class IconSpec:
    key: str
    tip: str
    draw: Callable[[tk.Canvas], None]
    command: Callable[[], None]


def draw_open(c: tk.Canvas) -> None:
    c.create_rectangle(4, 9, 18, 18, outline=ACCENT, width=1)
    c.create_rectangle(4, 7, 11, 10, fill=ACCENT, outline=ACCENT)


def draw_save(c: tk.Canvas) -> None:
    c.create_rectangle(5, 4, 17, 18, outline=ACCENT, width=1)
    c.create_rectangle(7, 4, 15, 9, fill=BTN_BG, outline=ACCENT)
    c.create_rectangle(8, 12, 14, 17, fill=ACCENT, outline=ACCENT)


def draw_print(c: tk.Canvas) -> None:
    c.create_rectangle(6, 4, 16, 8, outline=ACCENT, width=1)
    c.create_rectangle(4, 8, 18, 14, fill=ACCENT, outline=ACCENT)
    c.create_rectangle(7, 14, 15, 18, outline=ACCENT, width=1)


def draw_rotate(c: tk.Canvas) -> None:
    c.create_arc(4, 4, 18, 18, start=40, extent=220, style=tk.ARC, outline=ACCENT, width=2)
    c.create_polygon(16, 5, 19, 8, 14, 9, fill=ACCENT, outline=ACCENT)


def draw_pan(c: tk.Canvas) -> None:
    # Four-way translate arrows (Pan).
    c.create_line(11, 4, 11, 18, fill=ACCENT, width=2, arrow=tk.BOTH)
    c.create_line(4, 11, 18, 11, fill=ACCENT, width=2, arrow=tk.BOTH)


def make_axis_drawer(text: str) -> Callable[[tk.Canvas], None]:
    def _draw(c: tk.Canvas) -> None:
        c.create_line(4, 18, 17, 5, fill=ACCENT, width=1)
        c.create_oval(15, 3, 19, 7, fill=ACCENT, outline=ACCENT)
        c.create_text(8, 9, text=text, fill=FG, font=("Helvetica", 6, "bold"))

    return _draw


def draw_probe(c: tk.Canvas) -> None:
    c.create_line(11, 4, 11, 18, fill=ACCENT, width=1)
    c.create_line(4, 11, 18, 11, fill=ACCENT, width=1)
    c.create_oval(8, 8, 14, 14, outline="#c00000", width=1)


def draw_slice(c: tk.Canvas) -> None:
    c.create_rectangle(5, 5, 17, 17, outline=ACCENT, width=1)
    c.create_line(5, 11, 17, 11, fill="#c00000", width=2)


def draw_levels_minus(c: tk.Canvas) -> None:
    c.create_rectangle(4, 4, 18, 18, outline=ACCENT, width=1)
    c.create_line(7, 11, 15, 11, fill=FG, width=2)


def draw_levels_plus(c: tk.Canvas) -> None:
    c.create_rectangle(4, 4, 18, 18, outline=ACCENT, width=1)
    c.create_line(7, 11, 15, 11, fill=FG, width=2)
    c.create_line(11, 7, 11, 15, fill=FG, width=2)


def draw_labels(c: tk.Canvas) -> None:
    c.create_text(11, 11, text="Abc", fill=ACCENT, font=("Helvetica", 8, "bold"))


def draw_pathlines(c: tk.Canvas) -> None:
    c.create_line(4, 16, 8, 10, 12, 14, 18, 5, smooth=True, fill=ACCENT, width=2)
    c.create_oval(16, 3, 20, 7, fill="#c00000", outline="#c00000")


class IconToolbar(tk.Frame):
    """Horizontal icon strip."""

    def __init__(self, master: tk.Misc, **kw) -> None:
        super().__init__(master, bg=MENU_BG, highlightthickness=0, **kw)
        self._canvases: dict[str, tk.Canvas] = {}
        self.status = tk.Label(
            self,
            text="",
            bg=MENU_BG,
            fg=MUTED,
            font=("Helvetica", 8),
            anchor="w",
        )

    def add_icon(self, spec: IconSpec) -> None:
        wrap = tk.Frame(self, bg=MENU_BG, bd=0, highlightthickness=0)
        wrap.pack(side=tk.LEFT, padx=1, pady=2)
        canvas = tk.Canvas(
            wrap,
            width=ICON,
            height=ICON,
            bg=BTN_BG,
            highlightthickness=1,
            highlightbackground=BTN_BORDER,
            cursor="hand2",
        )
        canvas.pack(padx=1, pady=1)
        spec.draw(canvas)

        def on_enter(_e: tk.Event) -> None:
            canvas.configure(bg=BTN_ACTIVE)
            self.status.configure(text=spec.tip)

        def on_leave(_e: tk.Event) -> None:
            canvas.configure(bg=BTN_BG)
            self.status.configure(text="")

        def on_click(_e: tk.Event) -> None:
            spec.command()

        for w in (canvas, wrap):
            w.bind("<Enter>", on_enter)
            w.bind("<Leave>", on_leave)
            w.bind("<Button-1>", on_click)
        self._canvases[spec.key] = canvas

    def add_separator(self) -> None:
        sep = tk.Frame(self, bg="#9aa3ab", width=1, height=ICON + 4)
        sep.pack(side=tk.LEFT, padx=4, pady=2)

    def finish(self) -> None:
        self.status.pack(side=tk.LEFT, fill=tk.X, expand=True, padx=(8, 6))

    def set_active_tools(self, active_keys: set[str] | None = None) -> None:
        """Highlight exclusive interaction tools (rotate / pan / probe / pathlines)."""
        active = active_keys or set()
        tool_keys = {"rotate", "pan", "probe", "pathlines"}
        for key, canvas in self._canvases.items():
            if key not in tool_keys:
                continue
            if key in active:
                canvas.configure(
                    bg=BTN_ACTIVE,
                    highlightbackground=ACCENT,
                    highlightthickness=2,
                )
            else:
                canvas.configure(
                    bg=BTN_BG,
                    highlightbackground=BTN_BORDER,
                    highlightthickness=1,
                )


def build_fieldplot_icon_toolbar(parent: tk.Misc, gui) -> IconToolbar:
    """Wire icons to FieldplotGui actions."""
    bar = IconToolbar(parent)
    logo = tk.Canvas(bar, width=100, height=ICON, bg=MENU_BG, highlightthickness=0, bd=0)
    logo.pack(side=tk.LEFT, padx=(6, 4), pady=2)
    logo.create_line(2, 16, 7, 6, 12, 6, 17, 16, smooth=True, fill=ACCENT, width=2)
    logo.create_oval(16, 8, 20, 12, fill=ACCENT, outline=ACCENT)
    logo.create_text(24, 11, text="Fieldplot", anchor="w", fill=FG, font=("Helvetica", 10, "bold"))

    for spec in (
        IconSpec("open", "Open dataset", draw_open, gui.action_load),
        IconSpec("save", "Redraw / refresh view", draw_save, gui.action_redraw),
        IconSpec("print", "Export PNG", draw_print, gui.action_export_png),
    ):
        bar.add_icon(spec)
    bar.add_separator()

    bar.add_icon(
        IconSpec(
            "rotate",
            "Rotate — left-drag orbits az/el",
            draw_rotate,
            gui.action_rotate_tool,
        )
    )
    bar.add_icon(
        IconSpec(
            "pan",
            "Pan / translate — left-drag moves the domain",
            draw_pan,
            gui.action_pan_tool,
        )
    )
    for tag, tip, label in (
        ("+x", "Align +X", "+X"),
        ("-x", "Align −X", "−X"),
        ("+y", "Align +Y", "+Y"),
        ("-y", "Align −Y", "−Y"),
        ("+z", "Align +Z", "+Z"),
        ("-z", "Align −Z", "−Z"),
    ):
        bar.add_icon(
            IconSpec(
                f"look_{tag}",
                tip,
                make_axis_drawer(label),
                lambda t=tag: gui._set_axis_view(t),
            )
        )
    bar.add_separator()
    bar.add_icon(IconSpec("probe", "Probe tool", draw_probe, gui.action_toggle_probe))
    bar.add_icon(IconSpec("slice", "Slices / cut plane", draw_slice, gui.action_slices_focus))
    bar.add_separator()
    bar.add_icon(
        IconSpec("levels_minus", "Fewer contour levels", draw_levels_minus, gui.action_contour_remove_level)
    )
    bar.add_icon(
        IconSpec("levels_plus", "More contour levels", draw_levels_plus, gui.action_contour_add_level)
    )
    bar.add_icon(
        IconSpec("labels", "Contour labels", draw_labels, gui.action_toggle_contour_labels)
    )
    bar.add_icon(
        IconSpec("pathlines", "Pathlines rake", draw_pathlines, gui.action_pathlines_tool)
    )
    bar.finish()
    return bar
