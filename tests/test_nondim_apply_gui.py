#!/usr/bin/env python3
"""GUI Apply nondim fills empty scales and redraws without a JSON info dialog."""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path
from unittest import mock

os.environ.setdefault("DISPLAY", ":0")

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "gui"))

from flowplot_gui import FlowplotGui  # noqa: E402


def main() -> None:
    fixture = ROOT / "tests" / "fixtures" / "flow.dat"
    assert fixture.is_file()

    app = FlowplotGui()
    app.withdraw()
    app.dataset_path = fixture
    app._source_dataset_path = fixture
    app._set_variables(["X", "Y", "U", "V", "P", "T"])
    app.var_name.set("U")
    app.layer_contour.set(True)
    app.scale_vars["U_ref"].set("1000")
    for key in ("Lx", "Ly", "Lz", "T_ref"):
        app.scale_vars[key].set("")

    probe_payload = {
        "status": "ok",
        "command": "nondim",
        "applied": True,
        "nondim_suggestions": [
            {"variable": "X", "kind": "LengthX", "message": "set --lx"},
            {"variable": "Y", "kind": "LengthY", "message": "set --ly"},
            {"variable": "U", "kind": "Velocity", "message": "set --u-ref"},
            {"variable": "V", "kind": "Velocity", "message": "set --u-ref"},
            {"variable": "T", "kind": "Temperature", "message": "set --t-ref"},
        ],
        "fields": [
            {"variable": "X", "min": 0.0, "max": 1.0},
            {"variable": "Y", "min": 0.0, "max": 1.0},
            {"variable": "U", "min": 0.001, "max": 0.001},
            {"variable": "V", "min": 0.0, "max": 0.0},
            {"variable": "P", "min": 0.0, "max": 2.0},
            {"variable": "T", "min": 300.0, "max": 320.0},
        ],
    }
    apply_payload = {
        **probe_payload,
        "applied": True,
    }

    calls: list[list[str]] = []

    def fake_run_cli(args: list[str]) -> subprocess.CompletedProcess[str]:
        calls.append(list(args))
        # First call = probe (may already include --u-ref); second = full apply.
        body = probe_payload if len(calls) == 1 else apply_payload
        return subprocess.CompletedProcess(args, 0, stdout=json.dumps(body) + "\n", stderr="")

    redraws = {"n": 0}

    def fake_redraw(*_a, **_k) -> None:
        redraws["n"] += 1

    with (
        mock.patch.object(app, "_run_cli", side_effect=fake_run_cli),
        mock.patch.object(app, "action_redraw", side_effect=fake_redraw),
        mock.patch("flowplot_gui.messagebox.showinfo") as showinfo,
        mock.patch("flowplot_gui.messagebox.showerror") as showerror,
        mock.patch("flowplot_gui.messagebox.showwarning") as showwarning,
    ):
        app.action_nondim()

    assert showinfo.call_count == 0, "must not dump JSON in showinfo"
    assert showerror.call_count == 0
    assert showwarning.call_count == 0
    assert app.scale_vars["U_ref"].get() == "1000"
    assert app.scale_vars["Lx"].get() == "1"
    assert app.scale_vars["Ly"].get() == "1"
    assert app.scale_vars["T_ref"].get() == "300"
    assert redraws["n"] == 1
    assert app.layer_contour.get() is True

    # Final apply CLI must carry filled flags.
    assert len(calls) >= 2
    final = calls[-1]
    assert "nondim" in final
    joined = " ".join(final)
    assert "--u-ref" in final and "1000" in final
    assert "--lx" in final and "--ly" in final and "--t-ref" in final
    assert "300" in joined

    app.destroy()
    print("OK nondim-apply-gui")


if __name__ == "__main__":
    main()
