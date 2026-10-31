#!/usr/bin/env python3
"""Apply nondim: fill empty Lx/Ly/T_ref from suggestion field extents."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "gui"))

from flowplot_gui import (  # noqa: E402
    proposed_scales_from_nondim_payload,
    scale_value_from_extents,
)


def test_scale_value_from_extents() -> None:
    assert scale_value_from_extents("LengthX", 0.0, 2.5) == 2.5
    assert scale_value_from_extents("LengthY", -1.0, 1.0) == 2.0
    assert scale_value_from_extents("Velocity", -10.0, 1000.0) == 1000.0
    assert scale_value_from_extents("Temperature", 300.0, 320.0) == 300.0
    assert scale_value_from_extents("Temperature", -5.0, -1.0) == 5.0
    assert scale_value_from_extents("ThermalOther", 1.0, 2.0) is None
    assert scale_value_from_extents("LengthX", float("nan"), 1.0) is None


def test_proposed_scales_from_payload() -> None:
    payload = {
        "status": "ok",
        "applied": True,
        "nondim_suggestions": [
            {"variable": "X", "kind": "LengthX", "message": "set --lx"},
            {"variable": "Y", "kind": "LengthY", "message": "set --ly"},
            {"variable": "U", "kind": "Velocity", "message": "set --u-ref"},
            {"variable": "V", "kind": "Velocity", "message": "set --u-ref"},
            {"variable": "Temperature", "kind": "Temperature", "message": "set --t-ref"},
        ],
        "fields": [
            {"variable": "X", "min": 0.0, "max": 0.12},
            {"variable": "Y", "min": -0.05, "max": 0.05},
            {"variable": "U", "min": 884.0, "max": 1000.0},
            {"variable": "V", "min": -20.0, "max": 30.0},
            {"variable": "Temperature", "min": 288.0, "max": 350.0},
        ],
    }
    got = proposed_scales_from_nondim_payload(payload)
    assert got["Lx"] == 0.12
    assert got["Ly"] == 0.1
    assert got["U_ref"] == 1000.0  # peak across U and V
    assert got["T_ref"] == 288.0


def main() -> None:
    test_scale_value_from_extents()
    test_proposed_scales_from_payload()
    print("OK nondim-apply-fill")


if __name__ == "__main__":
    main()
