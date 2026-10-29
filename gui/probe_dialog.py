"""Tecplot-like Probe window: Variable Values / Cell Center / Zone info.

Markers on the Viewport are owned by the main GUI; this dialog clears them
on Close or Clear.
"""

from __future__ import annotations

import tkinter as tk
from tkinter import ttk
from typing import Any, Callable

from windowing import stay_above


BG = "#eceef0"
PANEL = "#e2e5e8"
HEADER_BG = "#d5d9de"
FG = "#1f2a33"
MUTED = "#5c6b76"
ACCENT = "#3d5a6c"
VIEWPORT_BG = "#fafbfc"


class ProbeDialog(tk.Toplevel):
    """Side panel listing all probed variables at the last click."""

    def __init__(
        self,
        master: tk.Misc,
        *,
        on_clear: Callable[[], None] | None = None,
        on_close: Callable[[], None] | None = None,
    ) -> None:
        super().__init__(master)
        self.title("Probe")
        self.geometry("360x480")
        self.minsize(300, 360)
        self.configure(bg=BG)
        self._on_clear = on_clear
        self._on_close = on_close
        self._last: dict[str, Any] = {}
        stay_above(self, master, modal=False)

        hdr = tk.Frame(self, bg=HEADER_BG)
        hdr.pack(side=tk.TOP, fill=tk.X)
        tk.Label(
            hdr,
            text="Probe",
            bg=HEADER_BG,
            fg=FG,
            font=("Helvetica", 11, "bold"),
        ).pack(side=tk.LEFT, padx=12, pady=8)
        tk.Label(
            hdr,
            text="Variable Values · Cell · Zone",
            bg=HEADER_BG,
            fg=MUTED,
            font=("Helvetica", 8),
        ).pack(side=tk.LEFT, padx=4)

        self.nb = ttk.Notebook(self)
        self.nb.pack(side=tk.TOP, fill=tk.BOTH, expand=True, padx=8, pady=8)

        self.tab_vars = tk.Frame(self.nb, bg=PANEL)
        self.tab_cell = tk.Frame(self.nb, bg=PANEL)
        self.tab_zone = tk.Frame(self.nb, bg=PANEL)
        self.nb.add(self.tab_vars, text="Variable Values")
        self.nb.add(self.tab_cell, text="Cell Center")
        self.nb.add(self.tab_zone, text="Zone info")

        self.vars_text = self._make_text(self.tab_vars)
        self.cell_text = self._make_text(self.tab_cell)
        self.zone_text = self._make_text(self.tab_zone)

        btns = tk.Frame(self, bg=BG)
        btns.pack(side=tk.BOTTOM, fill=tk.X, padx=8, pady=8)
        ttk.Button(btns, text="Clear", command=self.clear).pack(side=tk.LEFT, padx=4)
        ttk.Button(btns, text="Close", command=self._close).pack(side=tk.RIGHT, padx=4)

        self.protocol("WM_DELETE_WINDOW", self._close)
        self._set_placeholder()

    def _make_text(self, parent: tk.Frame) -> tk.Text:
        t = tk.Text(
            parent,
            wrap=tk.NONE,
            bg=VIEWPORT_BG,
            fg=FG,
            relief=tk.FLAT,
            highlightthickness=1,
            highlightbackground="#b8c0c8",
            font=("DejaVu Sans Mono", 9),
        )
        scroll_y = ttk.Scrollbar(parent, orient=tk.VERTICAL, command=t.yview)
        scroll_x = ttk.Scrollbar(parent, orient=tk.HORIZONTAL, command=t.xview)
        t.configure(yscrollcommand=scroll_y.set, xscrollcommand=scroll_x.set)
        t.pack(side=tk.TOP, fill=tk.BOTH, expand=True, padx=6, pady=6)
        scroll_y.pack(side=tk.RIGHT, fill=tk.Y)
        scroll_x.pack(side=tk.BOTTOM, fill=tk.X)
        t.configure(state=tk.DISABLED)
        return t

    def _write(self, widget: tk.Text, text: str) -> None:
        widget.configure(state=tk.NORMAL)
        widget.delete("1.0", tk.END)
        widget.insert(tk.END, text)
        widget.configure(state=tk.DISABLED)

    def _set_placeholder(self) -> None:
        tip = "Click the Viewport with Probe mode on to sample all variables."
        self._write(self.vars_text, tip)
        self._write(self.cell_text, tip)
        self._write(self.zone_text, tip)

    def update_probe(self, payload: dict[str, Any]) -> None:
        """Refresh tabs from a `flowplot probe --json` payload."""
        self._last = dict(payload)
        values = payload.get("values") or {}
        if isinstance(values, dict) and values:
            lines = [f"{name:16s}  {self._fmt(val)}" for name, val in values.items()]
            self._write(self.vars_text, "\n".join(lines))
        else:
            var = payload.get("var")
            val = payload.get("value")
            self._write(self.vars_text, f"{var}  =  {self._fmt(val)}")

        cell_lines = [
            f"X = {self._fmt(payload.get('x'))}",
            f"Y = {self._fmt(payload.get('y'))}",
            f"px = {payload.get('px')}",
            f"py = {payload.get('py')}",
            f"primary = {payload.get('label') or ''}",
        ]
        bounds = payload.get("bounds") or {}
        if isinstance(bounds, dict) and bounds:
            cell_lines.append("")
            cell_lines.append("Plane bounds:")
            for k in ("xmin", "xmax", "ymin", "ymax"):
                if k in bounds:
                    cell_lines.append(f"  {k} = {self._fmt(bounds[k])}")
        self._write(self.cell_text, "\n".join(cell_lines))

        zone_lines = [
            f"zone index = {payload.get('zone', 0)}",
            f"zone name  = {payload.get('zone_name', '')}",
            f"ni × nj    = {payload.get('ni', '?')} × {payload.get('nj', '?')}",
            f"path       = {payload.get('path', '')}",
        ]
        self._write(self.zone_text, "\n".join(zone_lines))

    @staticmethod
    def _fmt(val: Any) -> str:
        if val is None:
            return "n/a"
        try:
            f = float(val)
            if not (f == f):  # NaN
                return "nan"
            a = abs(f)
            if a != 0.0 and (a >= 1e4 or a < 1e-3):
                return f"{f:.6e}"
            return f"{f:.8g}"
        except (TypeError, ValueError):
            return str(val)

    def clear(self) -> None:
        self._last = {}
        self._set_placeholder()
        if self._on_clear is not None:
            self._on_clear()

    def _close(self) -> None:
        if self._on_close is not None:
            self._on_close()
        self.destroy()
