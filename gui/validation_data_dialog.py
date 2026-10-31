"""Fieldplot Agents → Validation data… dialog.

Download ERCOFTAC / NASA / arXiv case data via read-only agents into a deletable
local cache. Compare experiment (symbols only) to loaded simulation (lines only)
only when the user hits Plot compare.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import threading
import tkinter as tk
from pathlib import Path
from tkinter import messagebox, ttk
from typing import Any, Callable

from validation_cache import (
    case_dir,
    default_cache_root,
    delete_case,
    dir_size_bytes,
    ensure_cache_root,
    find_data_tables,
    format_size,
    list_cached_cases,
    load_sim_csv,
    load_xy_series,
    nasaagent_script,
    normalize_ercoftac_case,
    plotagent_script,
    purge_cache,
    read_table_columns,
    refresh_index,
    resolve_lehar_agents_root,
    slugify,
)
from windowing import stay_above

BG = "#eceef0"
PANEL = "#e2e5e8"
FG = "#1f2a33"
MUTED = "#5c6b76"
ACCENT = "#3d5a6c"


class ValidationDataDialog(tk.Toplevel):
    """Non-modal Agents dialog for archival validation data + compare."""

    def __init__(
        self,
        master: tk.Misc,
        *,
        flowplot_bin: str | None = None,
        dataset_path: Path | None = None,
        line_sample_csv: Path | None = None,
        variables: list[str] | None = None,
        on_log: Callable[[str], None] | None = None,
        on_viewport_overlay: Callable[[dict[str, Any]], None] | None = None,
        cache_root: Path | None = None,
    ) -> None:
        super().__init__(master)
        self.title("Validation data")
        self.configure(bg=BG)
        self.geometry("780x640")
        self.minsize(640, 520)
        stay_above(self, master, modal=False, always_on_top=True)
        self.protocol("WM_DELETE_WINDOW", self.destroy)

        self.flowplot_bin = flowplot_bin
        self.dataset_path = dataset_path
        self.line_sample_csv = line_sample_csv
        self.variables = list(variables or [])
        self.on_log = on_log or (lambda _m: None)
        self.on_viewport_overlay = on_viewport_overlay
        self.cache_root = ensure_cache_root(cache_root or default_cache_root())
        self._busy = False
        self._current_case: Path | None = None
        self._table_path: Path | None = None
        self._compare_win: tk.Toplevel | None = None

        self._build()
        self._refresh_cache_ui()
        self._append_status(
            f"Cache: {self.cache_root}\n"
            f"lehar-agents: {resolve_lehar_agents_root() or '(not found)'}\n"
            "Agents stay read-only until Plot compare."
        )

    # ------------------------------------------------------------------ UI
    def _build(self) -> None:
        outer = ttk.Frame(self, padding=10)
        outer.pack(fill=tk.BOTH, expand=True)

        head = ttk.Frame(outer)
        head.pack(fill=tk.X)
        ttk.Label(head, text="Validation data", font=("", 11, "bold")).pack(anchor="w")
        ttk.Label(
            head,
            text="Experiment = symbols only · Simulation = lines only · cache is ephemeral",
            foreground=MUTED,
        ).pack(anchor="w")

        self.notebook = ttk.Notebook(outer)
        self.notebook.pack(fill=tk.X, pady=(8, 4))
        self.query_vars: dict[str, tk.StringVar] = {}
        self.tab_hints = {
            "ercoftac": "ERCOFTAC case id (e.g. case025 or 25)",
            "nasa": "NASA report / dataset query (e.g. NASA-CR-194967)",
            "arxiv": "arXiv id or search (e.g. 2301.12345 or hypersonic cylinder)",
        }
        for key, title in (
            ("ercoftac", "ERCOFTAC"),
            ("nasa", "NASA"),
            ("arxiv", "arXiv"),
        ):
            frame = ttk.Frame(self.notebook, padding=8)
            self.notebook.add(frame, text=title)
            ttk.Label(frame, text=self.tab_hints[key], foreground=MUTED).pack(anchor="w")
            row = ttk.Frame(frame)
            row.pack(fill=tk.X, pady=4)
            var = tk.StringVar(value="case025" if key == "ercoftac" else "")
            self.query_vars[key] = var
            ttk.Entry(row, textvariable=var, width=48).pack(side=tk.LEFT, fill=tk.X, expand=True)

        btn_row = ttk.Frame(outer)
        btn_row.pack(fill=tk.X, pady=4)
        for label, cmd in (
            ("Download", self._on_download),
            ("Refresh", self._on_refresh),
            ("Delete downloaded", self._on_delete),
            ("Replace", self._on_replace),
            ("Open folder", self._on_open_folder),
        ):
            ttk.Button(btn_row, text=label, command=cmd).pack(side=tk.LEFT, padx=2)

        cache_fr = ttk.LabelFrame(outer, text="Cache", padding=6)
        cache_fr.pack(fill=tk.X, pady=4)
        self.cache_label = ttk.Label(cache_fr, text="", foreground=MUTED)
        self.cache_label.pack(anchor="w")
        cache_btns = ttk.Frame(cache_fr)
        cache_btns.pack(fill=tk.X, pady=(4, 0))
        ttk.Button(cache_btns, text="Purge all cache", command=self._on_purge).pack(
            side=tk.LEFT, padx=2
        )
        ttk.Button(cache_btns, text="Rescan", command=self._refresh_cache_ui).pack(
            side=tk.LEFT, padx=2
        )
        self.case_list = tk.Listbox(cache_fr, height=4, bg="#fafbfc", fg=FG)
        self.case_list.pack(fill=tk.X, pady=(4, 0))
        self.case_list.bind("<<ListboxSelect>>", self._on_case_select)

        series = ttk.LabelFrame(outer, text="Series picker (experiment → symbols)", padding=6)
        series.pack(fill=tk.X, pady=4)
        self.table_var = tk.StringVar()
        self.x_var = tk.StringVar()
        self.y_var = tk.StringVar()
        ttk.Label(series, text="Table", foreground=MUTED).grid(row=0, column=0, sticky="w")
        self.table_combo = ttk.Combobox(
            series, textvariable=self.table_var, state="readonly", width=56
        )
        self.table_combo.grid(row=0, column=1, sticky="ew", padx=4)
        self.table_combo.bind("<<ComboboxSelected>>", lambda _e: self._on_table_pick())
        ttk.Label(series, text="X column", foreground=MUTED).grid(row=1, column=0, sticky="w")
        self.x_combo = ttk.Combobox(series, textvariable=self.x_var, state="readonly", width=24)
        self.x_combo.grid(row=1, column=1, sticky="w", padx=4, pady=2)
        ttk.Label(series, text="Y column", foreground=MUTED).grid(row=2, column=0, sticky="w")
        self.y_combo = ttk.Combobox(series, textvariable=self.y_var, state="readonly", width=24)
        self.y_combo.grid(row=2, column=1, sticky="w", padx=4, pady=2)
        series.columnconfigure(1, weight=1)

        compare = ttk.LabelFrame(outer, text="Compare to loaded .plt", padding=6)
        compare.pack(fill=tk.X, pady=4)
        self.compare_target = tk.StringVar(value="matplotlib")
        ttk.Radiobutton(
            compare, text="Separate matplotlib window", variable=self.compare_target, value="matplotlib"
        ).pack(anchor="w")
        ttk.Radiobutton(
            compare, text="Overlay on Viewport", variable=self.compare_target, value="viewport"
        ).pack(anchor="w")
        self.sim_source = tk.StringVar(value="line-sample")
        sim_row = ttk.Frame(compare)
        sim_row.pack(fill=tk.X, pady=2)
        ttk.Label(sim_row, text="Simulation series", foreground=MUTED).pack(side=tk.LEFT)
        ttk.Combobox(
            sim_row,
            textvariable=self.sim_source,
            values=["line-sample", "flood-profile"],
            state="readonly",
            width=18,
        ).pack(side=tk.LEFT, padx=6)
        self.sim_var = tk.StringVar(value=(self.variables[0] if self.variables else "Pressure"))
        ttk.Label(sim_row, text="Var", foreground=MUTED).pack(side=tk.LEFT)
        ttk.Combobox(
            sim_row,
            textvariable=self.sim_var,
            values=self.variables or ["Pressure"],
            width=16,
        ).pack(side=tk.LEFT, padx=4)
        ttk.Button(compare, text="Plot compare", command=self._on_plot_compare).pack(
            anchor="w", pady=(6, 0)
        )

        status = ttk.LabelFrame(outer, text="Status / audit log", padding=6)
        status.pack(fill=tk.BOTH, expand=True, pady=4)
        self.status = tk.Text(status, height=10, wrap=tk.WORD, bg="#fafbfc", fg=FG, font=("TkFixedFont", 9))
        self.status.pack(fill=tk.BOTH, expand=True)
        self.status.configure(state=tk.DISABLED)

        foot = ttk.Frame(outer)
        foot.pack(fill=tk.X)
        ttk.Button(foot, text="Close", command=self.destroy).pack(side=tk.RIGHT)

    # ------------------------------------------------------------------ helpers
    def _active_source(self) -> str:
        idx = self.notebook.index(self.notebook.select())
        return ("ercoftac", "nasa", "arxiv")[idx]

    def _query(self) -> str:
        return self.query_vars[self._active_source()].get().strip()

    def _append_status(self, text: str) -> None:
        self.status.configure(state=tk.NORMAL)
        self.status.insert(tk.END, text.rstrip() + "\n")
        self.status.see(tk.END)
        self.status.configure(state=tk.DISABLED)
        self.on_log(text.strip())

    def _set_busy(self, busy: bool) -> None:
        self._busy = busy
        self.configure(cursor="watch" if busy else "")

    def _refresh_cache_ui(self) -> None:
        size = dir_size_bytes(self.cache_root)
        self.cache_label.configure(
            text=f"{format_size(size)}  ·  {self.cache_root}"
        )
        self.case_list.delete(0, tk.END)
        self._cached = list_cached_cases(self.cache_root)
        for item in self._cached:
            self.case_list.insert(
                tk.END, f"{item.source}/{item.case_id}  ({format_size(item.size_bytes)})"
            )
        refresh_index(self.cache_root)

    def _on_case_select(self, _event: object | None = None) -> None:
        sel = self.case_list.curselection()
        if not sel:
            return
        item = self._cached[int(sel[0])]
        self._load_case(item.path, source=item.source, case_id=item.case_id)

    def _load_case(self, path: Path, *, source: str, case_id: str) -> None:
        self._current_case = path
        # Sync tab + query
        tab_i = {"ercoftac": 0, "nasa": 1, "arxiv": 2}.get(source, 0)
        self.notebook.select(tab_i)
        self.query_vars[source].set(case_id)
        tables = find_data_tables(path)
        labels = [str(p.relative_to(path)) if p.is_relative_to(path) else str(p) for p in tables]
        self._tables = tables
        self.table_combo.configure(values=labels)
        if labels:
            self.table_var.set(labels[0])
            self._on_table_pick()
        else:
            self.table_var.set("")
            self.x_combo.configure(values=[])
            self.y_combo.configure(values=[])
        audit = path / "audit.json"
        if audit.is_file():
            try:
                payload = json.loads(audit.read_text(encoding="utf-8"))
                snippet = json.dumps(payload, indent=2)[:2500]
            except (OSError, json.JSONDecodeError) as exc:
                snippet = f"(audit error: {exc})"
            self._append_status(f"— audit.json ({source}/{case_id}) —\n{snippet}")

    def _on_table_pick(self) -> None:
        label = self.table_var.get().strip()
        if not label or not self._current_case:
            return
        path = self._current_case / label
        if not path.is_file():
            # fall back to absolute match
            for p in getattr(self, "_tables", []):
                if str(p).endswith(label) or str(p) == label:
                    path = p
                    break
        self._table_path = path if path.is_file() else None
        if not self._table_path:
            return
        try:
            header, _rows = read_table_columns(self._table_path)
        except OSError as exc:
            self._append_status(f"table read failed: {exc}")
            return
        self.x_combo.configure(values=header)
        self.y_combo.configure(values=header)
        if header:
            self.x_var.set(header[0])
            self.y_var.set(header[1] if len(header) > 1 else header[0])

    # ------------------------------------------------------------------ download
    def _on_download(self) -> None:
        self._start_agent(refresh=False)

    def _on_refresh(self) -> None:
        self._start_agent(refresh=True)

    def _on_replace(self) -> None:
        source = self._active_source()
        q = self._query()
        if not q:
            messagebox.showwarning("Validation data", "Enter a case id / query.", parent=self)
            return
        case_id = self._normalize_case_id(source, q)
        delete_case(self.cache_root, source, case_id)
        self._append_status(f"Deleted {source}/{case_id} — re-downloading…")
        self._start_agent(refresh=True)

    def _normalize_case_id(self, source: str, query: str) -> str:
        if source == "ercoftac":
            return normalize_ercoftac_case(query)
        if source == "arxiv":
            return slugify(query.replace("/", "_"))
        return slugify(query)

    def _start_agent(self, *, refresh: bool) -> None:
        if self._busy:
            return
        source = self._active_source()
        query = self._query()
        if not query:
            messagebox.showwarning("Validation data", "Enter a case id / query.", parent=self)
            return
        self._set_busy(True)
        self._append_status(f"Starting {source} download (read-only)…")

        def worker() -> None:
            try:
                path = self._run_agent(source, query, refresh=refresh)
                self.after(0, lambda: self._download_done(source, path, None))
            except Exception as exc:  # noqa: BLE001
                self.after(0, lambda: self._download_done(source, None, exc))

        threading.Thread(target=worker, daemon=True).start()

    def _download_done(self, source: str, path: Path | None, err: Exception | None) -> None:
        self._set_busy(False)
        if err is not None:
            self._append_status(f"ERROR: {err}")
            messagebox.showerror("Validation data", str(err), parent=self)
            return
        assert path is not None
        self._append_status(f"Ready: {path}")
        self._refresh_cache_ui()
        self._load_case(path, source=source, case_id=path.name)

    def _run_agent(self, source: str, query: str, *, refresh: bool) -> Path:
        out_root = self.cache_root / source
        out_root.mkdir(parents=True, exist_ok=True)
        if source == "ercoftac":
            case = normalize_ercoftac_case(query)
            script = plotagent_script()
            if script is None:
                raise RuntimeError(
                    "plotagent not found — set LEHAR_AGENTS to ~/software/lehar-agents"
                )
            cmd = [
                sys.executable,
                str(script),
                "--case",
                case,
                "--output",
                str(out_root),
                "--audit",
            ]
            if refresh:
                cmd.append("--refresh")
            # never --plots on download
            self._run_cmd(cmd)
            return out_root / case

        if source == "nasa":
            script = nasaagent_script()
            if script is None:
                raise RuntimeError(
                    "nasaagent not found — set LEHAR_AGENTS to ~/software/lehar-agents"
                )
            cmd = [
                sys.executable,
                str(script),
                "--case",
                query,
                "--output",
                str(out_root),
            ]
            if refresh:
                cmd.append("--refresh")
            before = {p.name for p in out_root.iterdir()} if out_root.is_dir() else set()
            self._run_cmd(cmd)
            # nasaagent writes slugify(case) under output — resolve by slug or newest dir
            from validation_cache import slugify as _slug

            candidate = out_root / _slug(query)
            if candidate.is_dir():
                return candidate
            after = [p for p in out_root.iterdir() if p.is_dir()]
            created = [p for p in after if p.name not in before]
            if created:
                return max(created, key=lambda p: p.stat().st_mtime)
            if after:
                return max(after, key=lambda p: p.stat().st_mtime)
            raise RuntimeError(f"nasaagent produced no case directory under {out_root}")

        # arxiv — thin Atom client (no dedicated lehar arXiv agent on DESKTOP)
        from arxiv_agent import run_arxiv_case

        return run_arxiv_case(query, out_root, refresh=refresh)

    def _run_cmd(self, cmd: list[str]) -> None:
        self.after(0, lambda: self._append_status("$ " + " ".join(cmd)))
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=600,
            env={**os.environ, "MPLCONFIGDIR": str(self.cache_root / ".mpl")},
        )
        out = (proc.stdout or "")[-2000:]
        err = (proc.stderr or "")[-1000:]
        if out:
            self.after(0, lambda: self._append_status(out))
        if err:
            self.after(0, lambda: self._append_status(err))
        if proc.returncode != 0:
            raise RuntimeError(f"agent exited {proc.returncode}: {err or out or 'no output'}")

    # ------------------------------------------------------------------ cache actions
    def _on_delete(self) -> None:
        source = self._active_source()
        q = self._query()
        if not q:
            sel = self.case_list.curselection()
            if not sel:
                messagebox.showwarning("Validation data", "Select or enter a case.", parent=self)
                return
            item = self._cached[int(sel[0])]
            source, case_id = item.source, item.case_id
        else:
            case_id = self._normalize_case_id(source, q)
        if not messagebox.askyesno(
            "Delete downloaded",
            f"Delete cache for {source}/{case_id}?",
            parent=self,
        ):
            return
        ok = delete_case(self.cache_root, source, case_id)
        self._append_status(f"Deleted {source}/{case_id}" if ok else "Nothing to delete")
        self._current_case = None
        self._refresh_cache_ui()

    def _on_purge(self) -> None:
        if not messagebox.askyesno(
            "Purge cache",
            f"Delete all validation cache under\n{self.cache_root}?",
            parent=self,
        ):
            return
        freed = purge_cache(self.cache_root)
        self._append_status(f"Purged cache ({format_size(freed)})")
        self._current_case = None
        self._refresh_cache_ui()

    def _on_open_folder(self) -> None:
        target = self._current_case or self.cache_root
        target.mkdir(parents=True, exist_ok=True)
        path = str(target)
        try:
            if sys.platform.startswith("win"):
                os.startfile(path)  # type: ignore[attr-defined]
            elif sys.platform == "darwin":
                subprocess.Popen(["open", path])
            else:
                # Prefer Windows explorer via wslpath when under WSL
                try:
                    win = subprocess.check_output(["wslpath", "-w", path], text=True).strip()
                    subprocess.Popen(["explorer.exe", win])
                except (OSError, subprocess.CalledProcessError):
                    subprocess.Popen(["xdg-open", path])
            self._append_status(f"Opened {path}")
        except OSError as exc:
            self._append_status(f"Open folder failed: {exc}")
            messagebox.showinfo("Open folder", path, parent=self)

    # ------------------------------------------------------------------ compare
    def _load_experiment_xy(self) -> tuple[list[float], list[float], str]:
        if not self._table_path or not self._table_path.is_file():
            raise RuntimeError("Pick a downloaded table and X/Y columns first")
        x_col = self.x_var.get().strip()
        y_col = self.y_var.get().strip()
        if not x_col or not y_col:
            raise RuntimeError("Select X and Y columns")
        xs, ys = load_xy_series(self._table_path, x_col, y_col)
        if len(xs) < 2:
            raise RuntimeError(f"Need ≥2 numeric points in {self._table_path.name}")
        return xs, ys, f"exp:{self._table_path.name}:{y_col}"

    def _load_simulation_xy(self) -> tuple[list[float], list[float], str]:
        source = self.sim_source.get()
        if source == "line-sample":
            csv_path = self.line_sample_csv
            if csv_path and csv_path.is_file():
                xs, ys = load_sim_csv(csv_path)
                if len(xs) >= 2:
                    return xs, ys, f"sim:line-sample:{csv_path.name}"
            # fall through to flood-profile extract
        if not self.dataset_path or not self.dataset_path.is_file():
            raise RuntimeError("Load a .plt/.dat in Fieldplot first (File → Open)")
        if not self.flowplot_bin:
            raise RuntimeError("flowplot binary not found")
        var = self.sim_var.get().strip() or "Pressure"
        tmp = self.cache_root / "_sim_profile.csv"
        cmd = [
            self.flowplot_bin,
            "profile",
            str(self.dataset_path),
            "--var",
            var,
            "--line",
            "mid-j",
            "--json",
        ]
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=180)
        if proc.returncode != 0:
            raise RuntimeError(proc.stderr or proc.stdout or "profile failed")
        # Prefer CSV if CLI wrote samples in JSON
        try:
            payload = json.loads(proc.stdout)
        except json.JSONDecodeError:
            payload = {}
        samples = payload.get("samples") or payload.get("profile", {}).get("samples")
        xs: list[float] = []
        ys: list[float] = []
        if isinstance(samples, list) and samples:
            for s in samples:
                if isinstance(s, dict):
                    xs.append(float(s.get("s", s.get("x", 0.0))))
                    ys.append(float(s.get("value", s.get("y", 0.0))))
        if len(xs) < 2:
            # try parsing CSV-ish stdout lines
            for line in (proc.stdout or "").splitlines():
                if "," not in line or line.startswith("#") or "value" in line.lower():
                    continue
                parts = line.split(",")
                try:
                    xs.append(float(parts[0]))
                    ys.append(float(parts[-1]))
                except ValueError:
                    continue
        if len(xs) < 2:
            raise RuntimeError("Could not extract simulation profile (try Data → Line sample… first)")
        tmp.write_text(
            "s,value\n" + "\n".join(f"{a},{b}" for a, b in zip(xs, ys)) + "\n",
            encoding="utf-8",
        )
        return xs, ys, f"sim:flood-profile:{var}"

    def _on_plot_compare(self) -> None:
        try:
            ex, ey, elabel = self._load_experiment_xy()
            sx, sy, slabel = self._load_simulation_xy()
        except Exception as exc:  # noqa: BLE001
            messagebox.showerror("Plot compare", str(exc), parent=self)
            self._append_status(f"Plot compare failed: {exc}")
            return
        self._append_status(
            f"Plot compare: {elabel} (symbols) vs {slabel} (lines) → {self.compare_target.get()}"
        )
        # Optional: ask agents for --plots only now (ERCOFTAC/NASA refresh with plots)
        self._maybe_agent_plots()
        target = self.compare_target.get()
        if target == "viewport" and self.on_viewport_overlay:
            self.on_viewport_overlay(
                {
                    "experiment": {"x": ex, "y": ey, "label": elabel},
                    "simulation": {"x": sx, "y": sy, "label": slabel},
                }
            )
            self._append_status("Viewport overlay sent (exp markers + sim polyline).")
            return
        self._show_matplotlib(ex, ey, elabel, sx, sy, slabel)

    def _maybe_agent_plots(self) -> None:
        """Gate --plots to Plot compare only; best-effort, never blocks compare."""
        source = self._active_source()
        if source not in {"ercoftac", "nasa"} or not self._current_case:
            return
        query = self._query()
        try:
            if source == "ercoftac":
                script = plotagent_script()
                if not script:
                    return
                case = normalize_ercoftac_case(query)
                cmd = [
                    sys.executable,
                    str(script),
                    "--case",
                    case,
                    "--output",
                    str(self.cache_root / "ercoftac"),
                    "--audit",
                    "--plots",
                ]
            else:
                script = nasaagent_script()
                if not script:
                    return
                cmd = [
                    sys.executable,
                    str(script),
                    "--case",
                    query,
                    "--output",
                    str(self.cache_root / "nasa"),
                    "--plots",
                ]
            self._append_status("$ " + " ".join(cmd) + "  # gated by Plot compare")
            threading.Thread(
                target=lambda: subprocess.run(cmd, capture_output=True, text=True, timeout=600),
                daemon=True,
            ).start()
        except Exception as exc:  # noqa: BLE001
            self._append_status(f"(agent --plots skipped: {exc})")

    def _show_matplotlib(
        self,
        ex: list[float],
        ey: list[float],
        elabel: str,
        sx: list[float],
        sy: list[float],
        slabel: str,
    ) -> None:
        try:
            from matplotlib.backends.backend_tkagg import FigureCanvasTkAgg
            from matplotlib.figure import Figure
        except ImportError:
            messagebox.showerror(
                "Plot compare", "matplotlib is required for the compare window.", parent=self
            )
            return
        if self._compare_win is not None:
            try:
                self._compare_win.destroy()
            except tk.TclError:
                pass
        win = tk.Toplevel(self)
        win.title("Validation compare")
        stay_above(win, self, modal=False, always_on_top=True)
        self._compare_win = win
        fig = Figure(figsize=(6.2, 4.2), dpi=100)
        ax = fig.add_subplot(111)
        # Experiment = symbols only (no connecting line)
        ax.plot(ex, ey, linestyle="None", marker="o", markersize=5, label=elabel, color="#1f2a33")
        # Simulation = lines only (no markers)
        ax.plot(sx, sy, linestyle="-", marker="None", linewidth=1.6, label=slabel, color="#3d5a6c")
        ax.set_xlabel("x / s")
        ax.set_ylabel("value")
        ax.legend(frameon=False, fontsize=8)
        ax.grid(True, alpha=0.25)
        fig.tight_layout()
        canvas = FigureCanvasTkAgg(fig, master=win)
        canvas.draw()
        canvas.get_tk_widget().pack(fill=tk.BOTH, expand=True)
        ttk.Button(win, text="Close", command=win.destroy).pack(anchor="e", padx=8, pady=6)


def open_validation_data_dialog(
    master: tk.Misc,
    **kwargs: Any,
) -> ValidationDataDialog:
    return ValidationDataDialog(master, **kwargs)
