#!/usr/bin/env python3
"""Contour variable must enable for all zone scalars after Open (no sticky disable)."""

from __future__ import annotations

import os
import sys
from pathlib import Path

os.environ.setdefault("DISPLAY", ":0")

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "gui"))

from flowplot_gui import FlowplotGui  # noqa: E402


def main() -> None:
    app = FlowplotGui()
    app.withdraw()

    # Pre-Open: Contour Details opened empty → combo disabled.
    app.action_contour_details()
    dlg = app._contour_details_dialog
    assert dlg is not None and dlg.winfo_exists()
    assert str(dlg._var_combo.cget("state")) == "disabled"

    vars_ = ["X", "Y", "Density", "U", "V", "Pressure", "Mach"]
    app._set_variables(vars_)

    assert list(app.var_combo.cget("values")) == vars_
    assert str(app.var_combo.cget("state")) == "readonly"
    assert app.var_name.get() == "Density"

    # Open dialog must refresh (was disabled) to readonly with all zone scalars.
    assert list(dlg._var_combo.cget("values")) == vars_
    assert str(dlg._var_combo.cget("state")) == "readonly"

    # Sticky-disable regression: force disabled, sync again.
    dlg._var_combo.configure(state="disabled", values=())
    app._sync_contour_variable_widgets()
    assert str(dlg._var_combo.cget("state")) == "readonly"
    assert list(dlg._var_combo.cget("values")) == vars_

    app.destroy()
    print("OK contour-var-after-open")


if __name__ == "__main__":
    main()
