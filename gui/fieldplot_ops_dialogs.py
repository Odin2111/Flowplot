"""Fieldplot dialogs for Derived fields, Line sample, Operators, Block filter.

UI copy uses Fieldplot glossary names only (never VisIt / Tecplot / Silo brands).
"""

from __future__ import annotations

import tkinter as tk
from tkinter import filedialog, messagebox, ttk
from typing import Any, Callable

from windowing import stay_above

BG = "#eceef0"
PANEL = "#e2e5e8"
FG = "#1f2a33"
MUTED = "#5c6b76"


class _Base(tk.Toplevel):
    result: dict[str, Any] | None = None

    def __init__(self, master: tk.Misc, title: str, *, geom: str = "420x320") -> None:
        super().__init__(master)
        self.title(title)
        self.geometry(geom)
        self.configure(bg=BG)
        self.result = None
        stay_above(self, master, modal=True)
        self.protocol("WM_DELETE_WINDOW", self._cancel)

    def _cancel(self) -> None:
        self.result = None
        self.destroy()

    def _buttons(self, parent: tk.Misc, ok_label: str = "Apply") -> None:
        row = ttk.Frame(parent)
        row.pack(fill=tk.X, padx=12, pady=(8, 12))
        ttk.Button(row, text="Cancel", command=self._cancel).pack(side=tk.RIGHT, padx=4)
        ttk.Button(row, text=ok_label, command=self._ok).pack(side=tk.RIGHT, padx=4)

    def _ok(self) -> None:
        self.destroy()


def ask_derived_field(
    master: tk.Misc,
    *,
    variables: list[str],
    existing: list[dict[str, str]] | None = None,
) -> dict[str, Any] | None:
    """Define / apply a Derived field expression."""
    dlg = _Base(master, "Derived fields", geom="460x420")
    body = ttk.Frame(dlg, padding=12)
    body.pack(fill=tk.BOTH, expand=True)
    ttk.Label(
        body,
        text="Create a scalar from loaded variables (expression field).",
        foreground=MUTED,
    ).pack(anchor="w")
    ttk.Label(body, text="Name", foreground=MUTED).pack(anchor="w", pady=(10, 0))
    name = tk.StringVar(value="Speed")
    ttk.Entry(body, textvariable=name, width=28).pack(anchor="w", pady=2)
    ttk.Label(body, text="Expression", foreground=MUTED).pack(anchor="w", pady=(8, 0))
    expr = tk.StringVar(value="hypot(U,V)" if "U" in variables else ("P*1" if variables else "0"))
    ttk.Entry(body, textvariable=expr, width=40).pack(anchor="w", pady=2)
    ttk.Label(
        body,
        text="Ops: + − * /  ·  fn: sqrt abs hypot/mag min max  ·  vars: "
        + (", ".join(variables[:12]) or "(none)"),
        foreground=MUTED,
        wraplength=400,
    ).pack(anchor="w", pady=(6, 0))

    presets = ttk.Frame(body)
    presets.pack(fill=tk.X, pady=(8, 0))

    def set_preset(n: str, e: str) -> None:
        name.set(n)
        expr.set(e)

    ttk.Button(
        presets, text="Velocity mag", command=lambda: set_preset("Speed", "hypot(U,V)")
    ).pack(side=tk.LEFT, padx=2)
    ttk.Button(
        presets, text="|P|", command=lambda: set_preset("AbsP", "abs(P)")
    ).pack(side=tk.LEFT, padx=2)

    list_fr = ttk.LabelFrame(body, text="Session derived fields", padding=6)
    list_fr.pack(fill=tk.BOTH, expand=True, pady=(10, 0))
    lb = tk.Listbox(list_fr, height=5, bg="#fafbfc", fg=FG)
    lb.pack(fill=tk.BOTH, expand=True)
    for item in existing or []:
        lb.insert(tk.END, f"{item.get('name')}: {item.get('expression')}")

    write_out = tk.BooleanVar(value=True)

    def ok() -> None:
        n = name.get().strip()
        e = expr.get().strip()
        if not n or not e:
            messagebox.showwarning("Derived fields", "Name and expression are required.", parent=dlg)
            return
        dlg.result = {"name": n, "expression": e, "write": bool(write_out.get())}
        dlg.destroy()

    dlg._ok = ok  # type: ignore[method-assign]
    ttk.Checkbutton(
        body, text="Write temporary dataset with new field", variable=write_out
    ).pack(anchor="w", pady=(6, 0))
    dlg._buttons(dlg, ok_label="Apply")
    dlg.wait_window()
    return dlg.result


def ask_line_sample(
    master: tk.Misc,
    *,
    variables: list[str],
    initial_var: str = "",
    bounds: tuple[float, float, float, float] | None = None,
) -> dict[str, Any] | None:
    """Line sample endpoints + CSV path."""
    dlg = _Base(master, "Line sample", geom="440x360")
    body = ttk.Frame(dlg, padding=12)
    body.pack(fill=tk.BOTH, expand=True)
    ttk.Label(
        body,
        text="Sample a scalar along a world-space segment and export CSV.",
        foreground=MUTED,
    ).pack(anchor="w")
    ttk.Label(body, text="Variable", foreground=MUTED).pack(anchor="w", pady=(10, 0))
    var = tk.StringVar(value=initial_var or (variables[0] if variables else ""))
    ttk.Combobox(body, textvariable=var, values=variables, width=28).pack(anchor="w", pady=2)

    xmin, xmax, ymin, ymax = bounds or (0.0, 1.0, 0.0, 1.0)
    ttk.Label(body, text="From (x,y)", foreground=MUTED).pack(anchor="w", pady=(8, 0))
    fro = tk.StringVar(value=f"{xmin},{0.5 * (ymin + ymax)}")
    ttk.Entry(body, textvariable=fro, width=28).pack(anchor="w", pady=2)
    ttk.Label(body, text="To (x,y)", foreground=MUTED).pack(anchor="w", pady=(8, 0))
    to = tk.StringVar(value=f"{xmax},{0.5 * (ymin + ymax)}")
    ttk.Entry(body, textvariable=to, width=28).pack(anchor="w", pady=2)
    ttk.Label(body, text="Samples", foreground=MUTED).pack(anchor="w", pady=(8, 0))
    n = tk.StringVar(value="64")
    ttk.Entry(body, textvariable=n, width=10).pack(anchor="w", pady=2)

    csv_path = tk.StringVar(value="")

    def browse() -> None:
        path = filedialog.asksaveasfilename(
            parent=dlg,
            title="Save Line sample CSV",
            defaultextension=".csv",
            filetypes=[("CSV", "*.csv"), ("All files", "*")],
        )
        if path:
            csv_path.set(path)

    row = ttk.Frame(body)
    row.pack(fill=tk.X, pady=(10, 0))
    ttk.Label(row, text="CSV", foreground=MUTED).pack(side=tk.LEFT)
    ttk.Entry(row, textvariable=csv_path).pack(side=tk.LEFT, fill=tk.X, expand=True, padx=6)
    ttk.Button(row, text="…", width=3, command=browse).pack(side=tk.LEFT)

    def ok() -> None:
        try:
            nn = max(2, int(n.get().strip() or "64"))
        except ValueError:
            messagebox.showwarning("Line sample", "Samples must be an integer ≥ 2.", parent=dlg)
            return
        dlg.result = {
            "var": var.get().strip(),
            "from": fro.get().strip(),
            "to": to.get().strip(),
            "n": nn,
            "csv": csv_path.get().strip(),
        }
        dlg.destroy()

    dlg._ok = ok  # type: ignore[method-assign]
    dlg._buttons(dlg, ok_label="Sample")
    dlg.wait_window()
    return dlg.result


def ask_operator(
    master: tk.Misc,
    *,
    variables: list[str],
    initial_var: str = "",
    initial_kind: str = "cut",
) -> dict[str, Any] | None:
    """Add Cut / Threshold / Level set to the operator stack."""
    dlg = _Base(master, "Operators", geom="420x380")
    body = ttk.Frame(dlg, padding=12)
    body.pack(fill=tk.BOTH, expand=True)
    ttk.Label(
        body,
        text="Operator stack: Cut is live; Threshold and Level set are stubs.",
        foreground=MUTED,
        wraplength=380,
    ).pack(anchor="w")
    kind0 = (initial_kind or "cut").strip().lower()
    if kind0 not in ("cut", "threshold", "level-set"):
        kind0 = "cut"
    kind = tk.StringVar(value=kind0)
    for label, val in (
        ("Cut", "cut"),
        ("Threshold", "threshold"),
        ("Level set", "level-set"),
    ):
        ttk.Radiobutton(body, text=label, variable=kind, value=val).pack(anchor="w", pady=2)

    ttk.Label(body, text="Cut plane (e.g. k=0 / auto)", foreground=MUTED).pack(
        anchor="w", pady=(10, 0)
    )
    plane = tk.StringVar(value="auto")
    ttk.Entry(body, textvariable=plane, width=20).pack(anchor="w", pady=2)

    ttk.Label(body, text="Variable (Threshold / Level set)", foreground=MUTED).pack(
        anchor="w", pady=(8, 0)
    )
    var = tk.StringVar(value=initial_var or (variables[0] if variables else ""))
    ttk.Combobox(body, textvariable=var, values=variables, width=24).pack(anchor="w", pady=2)

    ttk.Label(body, text="Threshold min / max (blank = open)", foreground=MUTED).pack(
        anchor="w", pady=(8, 0)
    )
    row = ttk.Frame(body)
    row.pack(anchor="w")
    tmin = tk.StringVar(value="")
    tmax = tk.StringVar(value="")
    ttk.Entry(row, textvariable=tmin, width=10).pack(side=tk.LEFT)
    ttk.Label(row, text=" … ").pack(side=tk.LEFT)
    ttk.Entry(row, textvariable=tmax, width=10).pack(side=tk.LEFT)

    ttk.Label(body, text="Level set value", foreground=MUTED).pack(anchor="w", pady=(8, 0))
    value = tk.StringVar(value="0")
    ttk.Entry(body, textvariable=value, width=12).pack(anchor="w", pady=2)

    def ok() -> None:
        k = kind.get()
        out: dict[str, Any] = {"kind": k}
        if k == "cut":
            out["plane"] = plane.get().strip() or "auto"
        elif k == "threshold":
            out["var"] = var.get().strip()
            out["min"] = tmin.get().strip()
            out["max"] = tmax.get().strip()
        else:
            out["var"] = var.get().strip()
            out["value"] = value.get().strip()
        dlg.result = out
        dlg.destroy()

    dlg._ok = ok  # type: ignore[method-assign]
    dlg._buttons(dlg, ok_label="Add")
    dlg.wait_window()
    return dlg.result


def ask_block_filter(
    master: tk.Misc,
    *,
    zones: list[tuple[int, str]],
    selected: list[int] | None = None,
) -> list[int] | None:
    """Block filter — subset of zones (SIL/subset analogue)."""
    dlg = _Base(master, "Block filter", geom="360x380")
    body = ttk.Frame(dlg, padding=12)
    body.pack(fill=tk.BOTH, expand=True)
    ttk.Label(
        body,
        text="Show only selected blocks (zones).",
        foreground=MUTED,
    ).pack(anchor="w")
    lb = tk.Listbox(body, selectmode=tk.MULTIPLE, height=12, bg="#fafbfc", fg=FG)
    lb.pack(fill=tk.BOTH, expand=True, pady=8)
    selected_set = set(selected or [z[0] for z in zones])
    for idx, name in zones:
        lb.insert(tk.END, f"[{idx}] {name}")
        if idx in selected_set:
            lb.selection_set(lb.size() - 1)

    def ok() -> None:
        picks = []
        for i in lb.curselection():
            picks.append(zones[i][0])
        dlg.result = {"zones": picks}
        dlg.destroy()

    dlg._ok = ok  # type: ignore[method-assign]
    dlg._buttons(dlg, ok_label="Apply")
    dlg.wait_window()
    if not dlg.result:
        return None
    return list(dlg.result.get("zones") or [])
