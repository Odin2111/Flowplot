#!/usr/bin/env python3
"""Validation data dialog — cache, menu entry, compare style rules."""

from __future__ import annotations

import os
import sys
import tempfile
from pathlib import Path

os.environ.setdefault("DISPLAY", ":0")

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "gui"))

from flowplot_gui import FlowplotGui  # noqa: E402
from validation_cache import (  # noqa: E402
    delete_case,
    ensure_cache_root,
    find_data_tables,
    load_xy_series,
    normalize_ercoftac_case,
    purge_cache,
)


def test_cache_and_series(tmp: Path) -> None:
    root = ensure_cache_root(tmp / "validation_cache")
    case = root / "ercoftac" / "case025" / "canonical"
    case.mkdir(parents=True)
    table = case / "table_001.csv"
    table.write_text("x,U,uu\n0.0,1.0,0.1\n0.5,1.2,0.2\n1.0,1.1,0.15\n", encoding="utf-8")
    (root / "ercoftac" / "case025" / "audit.json").write_text(
        '{"status":"ok","plots_created":false}\n', encoding="utf-8"
    )
    tables = find_data_tables(root / "ercoftac" / "case025")
    assert table in tables
    xs, ys = load_xy_series(table, "x", "U")
    assert xs == [0.0, 0.5, 1.0]
    assert ys == [1.0, 1.2, 1.1]
    assert normalize_ercoftac_case("25") == "case025"
    assert delete_case(root, "ercoftac", "case025")
    assert not (root / "ercoftac" / "case025").exists()
    # recreate then purge
    case.mkdir(parents=True)
    (case / "a.csv").write_text("x,y\n0,1\n", encoding="utf-8")
    freed = purge_cache(root)
    assert freed >= 0
    assert list((root / "ercoftac").iterdir()) == []


def test_dialog_and_menu() -> None:
    app = FlowplotGui()
    app.withdraw()
    # Menu entry present
    menubar = app.nametowidget(app.cget("menu"))
    labels: list[str] = []

    def walk(menu) -> None:  # type: ignore[no-untyped-def]
        end = menu.index("end")
        if end is None:
            return
        for i in range(end + 1):
            try:
                kind = menu.type(i)
            except Exception:
                continue
            if kind == "command":
                labels.append(str(menu.entrycget(i, "label")))
            elif kind == "cascade":
                walk(menu.nametowidget(menu.entrycget(i, "menu")))

    walk(menubar)
    assert "Validation data…" in labels
    assert not any("Tecplot" in lab or "tecplot" in lab.lower() for lab in labels)

    app.open_validation_data()
    dlg = app._validation_dialog
    assert dlg is not None and dlg.winfo_exists()
    assert dlg.title() == "Validation data"
    # Tabs
    n_tabs = int(dlg.notebook.index("end"))
    tabs = [dlg.notebook.tab(i, "text") for i in range(n_tabs)]
    assert tabs == ["ERCOFTAC", "NASA", "arXiv"]
    # Seed a fake cached table and exercise series picker + matplotlib compare
    cache = dlg.cache_root
    case = cache / "ercoftac" / "case025"
    canon = case / "canonical"
    canon.mkdir(parents=True, exist_ok=True)
    table = canon / "exp.csv"
    table.write_text("s,value\n0,0\n1,1\n2,1.5\n3,1.2\n", encoding="utf-8")
    (case / "audit.json").write_text('{"status":"ok","plots_created":false}\n', encoding="utf-8")
    sim = cache / "_fixture_sim.csv"
    sim.write_text("s,value\n0,0.1\n1,0.9\n2,1.4\n3,1.1\n", encoding="utf-8")
    dlg.line_sample_csv = sim
    dlg._refresh_cache_ui()
    dlg._load_case(case, source="ercoftac", case_id="case025")
    assert dlg.table_var.get()
    dlg.compare_target.set("matplotlib")
    dlg.sim_source.set("line-sample")
    dlg._on_plot_compare()
    # Compare window should exist with matplotlib rules exercised (no crash)
    assert dlg._compare_win is not None and dlg._compare_win.winfo_exists()

    # Viewport overlay path
    dlg.compare_target.set("viewport")
    dlg._on_plot_compare()
    assert app._validation_overlay is not None
    assert "experiment" in app._validation_overlay
    assert "simulation" in app._validation_overlay

    dlg.destroy()
    app.destroy()
    print("OK validation-data-dialog")


def main() -> None:
    with tempfile.TemporaryDirectory() as td:
        test_cache_and_series(Path(td))
    test_dialog_and_menu()


if __name__ == "__main__":
    main()
