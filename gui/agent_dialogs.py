"""Fieldplot Agents input dialogs for Fieldplot Agents.

Dialogs collect refs / variables / freestream / line specs before an agent run.
Solver-agnostic: values apply to whatever CFD dataset is loaded.
"""

from __future__ import annotations

import tkinter as tk
from tkinter import ttk
from typing import Any

from windowing import stay_above

BG = "#eceef0"
PANEL = "#e2e5e8"
FG = "#1f2a33"
MUTED = "#5c6b76"
BTN_BG = "#f4f5f6"


def _combo(parent: tk.Misc, var: tk.Variable, values: list[str], width: int = 18) -> ttk.Combobox:
    state = "readonly" if values else "normal"
    box = ttk.Combobox(parent, textvariable=var, values=values, state=state, width=width)
    return box


class _BaseDialog(tk.Toplevel):
    result: dict[str, Any] | None = None

    def __init__(self, master: tk.Misc, title: str) -> None:
        super().__init__(master)
        self.title(title)
        self.configure(bg=BG)
        self.resizable(False, False)
        self.result = None
        stay_above(self, master, modal=True)
        self.protocol("WM_DELETE_WINDOW", self._cancel)

    def _ok(self) -> None:
        self.destroy()

    def _cancel(self) -> None:
        self.result = None
        self.destroy()

    def _buttons(self, parent: tk.Misc) -> None:
        row = ttk.Frame(parent)
        row.pack(fill=tk.X, padx=12, pady=(4, 12))
        ttk.Button(row, text="Cancel", command=self._cancel).pack(side=tk.RIGHT, padx=4)
        ttk.Button(row, text="OK", command=self._ok).pack(side=tk.RIGHT, padx=4)


def ask_variable(
    master: tk.Misc,
    *,
    variables: list[str],
    initial: str = "",
    title: str = "Select variable",
    prompt: str = "Variable",
) -> str | None:
    dlg = _BaseDialog(master, title)
    body = ttk.Frame(dlg, padding=12)
    body.pack(fill=tk.BOTH, expand=True)
    ttk.Label(body, text=prompt, foreground=MUTED).pack(anchor="w")
    var = tk.StringVar(value=initial or (variables[0] if variables else ""))
    _combo(body, var, variables, width=28).pack(anchor="w", pady=6)

    def ok() -> None:
        dlg.result = {"var": var.get().strip()}
        dlg.destroy()

    dlg._ok = ok  # type: ignore[method-assign]
    dlg._buttons(dlg)
    dlg.wait_window()
    if not dlg.result:
        return None
    return str(dlg.result.get("var") or "") or None


def ask_profile(
    master: tk.Misc,
    *,
    variables: list[str],
    initial_var: str = "",
    initial_line: str = "mid-j",
) -> dict[str, str] | None:
    dlg = _BaseDialog(master, "Extract profile")
    body = ttk.Frame(dlg, padding=12)
    body.pack(fill=tk.BOTH, expand=True)
    ttk.Label(body, text="Sample a scalar along a grid line (any CFD file).").pack(anchor="w")
    ttk.Label(body, text="Variable", foreground=MUTED).pack(anchor="w", pady=(10, 0))
    var = tk.StringVar(value=initial_var or (variables[0] if variables else ""))
    _combo(body, var, variables, width=28).pack(anchor="w", pady=4)
    ttk.Label(body, text="Line", foreground=MUTED).pack(anchor="w", pady=(8, 0))
    line = tk.StringVar(value=initial_line)
    _combo(body, line, ["mid-j", "mid-i", "j=0", "i=0"], width=28).pack(anchor="w", pady=4)
    ttk.Label(
        body,
        text="Optional reference CSV for RMSE is set via CLI --ref.",
        foreground=MUTED,
        wraplength=360,
    ).pack(anchor="w", pady=(8, 0))

    def ok() -> None:
        dlg.result = {"var": var.get().strip(), "line": line.get().strip() or "mid-j"}
        dlg.destroy()

    dlg._ok = ok  # type: ignore[method-assign]
    dlg._buttons(dlg)
    dlg.wait_window()
    return dlg.result  # type: ignore[return-value]


def ask_reference_values(
    master: tk.Misc,
    *,
    initial: dict[str, str] | None = None,
) -> dict[str, str] | None:
    init = initial or {}
    dlg = _BaseDialog(master, "Reference scales")
    body = ttk.Frame(dlg, padding=12)
    body.pack(fill=tk.BOTH, expand=True)
    ttk.Label(
        body,
        text="Freestream / length / time refs (apply only when set).",
        wraplength=380,
    ).pack(anchor="w")
    fields = [
        ("u_ref", "U_ref", init.get("u_ref", "")),
        ("lx", "L_x", init.get("lx", "")),
        ("ly", "L_y", init.get("ly", "")),
        ("lz", "L_z", init.get("lz", "")),
        ("t_ref", "T_ref", init.get("t_ref", "")),
    ]
    vars_: dict[str, tk.StringVar] = {}
    for key, label, val in fields:
        ttk.Label(body, text=label, foreground=MUTED).pack(anchor="w", pady=(8, 0))
        sv = tk.StringVar(value=val)
        vars_[key] = sv
        ttk.Entry(body, textvariable=sv, width=28).pack(anchor="w", pady=2)

    def ok() -> None:
        out: dict[str, str] = {}
        for key, sv in vars_.items():
            text = sv.get().strip()
            if text:
                out[key] = text
        dlg.result = out
        dlg.destroy()

    dlg._ok = ok  # type: ignore[method-assign]
    dlg._buttons(dlg)
    dlg.wait_window()
    return dlg.result  # type: ignore[return-value]


def ask_shock_polar(
    master: tk.Misc,
    *,
    mach: float = 2.0,
    gamma: float = 1.4,
) -> dict[str, float] | None:
    dlg = _BaseDialog(master, "Shock polar (θ–β–M)")
    body = ttk.Frame(dlg, padding=12)
    body.pack(fill=tk.BOTH, expand=True)
    ttk.Label(
        body,
        text="Ideal-gas freestream for analytic θ–β–M (any hypersonic / shock case).",
        wraplength=380,
    ).pack(anchor="w")
    ttk.Label(body, text="M∞", foreground=MUTED).pack(anchor="w", pady=(10, 0))
    m_var = tk.StringVar(value=f"{mach:g}")
    ttk.Entry(body, textvariable=m_var, width=18).pack(anchor="w", pady=2)
    ttk.Label(body, text="γ", foreground=MUTED).pack(anchor="w", pady=(8, 0))
    g_var = tk.StringVar(value=f"{gamma:g}")
    ttk.Entry(body, textvariable=g_var, width=18).pack(anchor="w", pady=2)

    def ok() -> None:
        try:
            m = float(m_var.get().strip())
            g = float(g_var.get().strip())
        except ValueError:
            return
        if m <= 1.0 or g <= 1.0:
            return
        dlg.result = {"mach": m, "gamma": g}
        dlg.destroy()

    dlg._ok = ok  # type: ignore[method-assign]
    dlg._buttons(dlg)
    dlg.wait_window()
    return dlg.result  # type: ignore[return-value]


def ask_contour_var(
    master: tk.Misc,
    *,
    variables: list[str],
    initial: str = "",
    title: str = "Contour scalar",
    hint: str = "",
) -> str | None:
    dlg = _BaseDialog(master, title)
    body = ttk.Frame(dlg, padding=12)
    body.pack(fill=tk.BOTH, expand=True)
    if hint:
        ttk.Label(body, text=hint, wraplength=380).pack(anchor="w")
    ttk.Label(body, text="Variable", foreground=MUTED).pack(anchor="w", pady=(8, 0))
    var = tk.StringVar(value=initial or (variables[0] if variables else ""))
    _combo(body, var, variables, width=28).pack(anchor="w", pady=4)

    def ok() -> None:
        dlg.result = {"var": var.get().strip()}
        dlg.destroy()

    dlg._ok = ok  # type: ignore[method-assign]
    dlg._buttons(dlg)
    dlg.wait_window()
    if not dlg.result:
        return None
    return str(dlg.result.get("var") or "") or None
