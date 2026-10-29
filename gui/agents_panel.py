"""Analyze panel — validation / correction agents with side charts (not raw JSON).

Invokes `flowplot menu run <id>` against the loaded dataset. Field sanity,
positivity, and profile extracts render Tk/matplotlib charts; shock polar draws
an analytic θ–β–M curve (see docs). Raw JSON stays behind a Details toggle.
"""

from __future__ import annotations

import json
import os
import subprocess
import tempfile
import tkinter as tk
from pathlib import Path
from tkinter import messagebox, ttk
from typing import Any, Callable

from shock_polar import plot_polar_on_axes, plot_polar_png
from windowing import stay_above

BG = "#eceef0"
PANEL = "#e2e5e8"
MENU_BG = "#d5d9de"
ACCENT = "#3d5a6c"
FG = "#1f2a33"
MUTED = "#5c6b76"
BTN_BG = "#f4f5f6"
BTN_ACTIVE = "#cfd5db"
VIEWPORT_BG = "#f7f8f9"

COORD_NAMES = {"x", "y", "z", "r", "theta", "th", "i", "j", "k"}

POSITIVITY_NAMES = {
    "rho",
    "density",
    "dens",
    "p",
    "pressure",
    "press",
    "t",
    "temp",
    "temperature",
    "tt",
    "static_pressure",
    "static_temperature",
}


def first_scalar(variables: list[str]) -> str:
    for name in variables:
        if name.strip().lower() not in COORD_NAMES:
            return name
    return variables[0] if variables else ""


def guess_positivity_var(variables: list[str]) -> str:
    lower = {v.lower(): v for v in variables}
    for key in (
        "density",
        "rho",
        "pressure",
        "p",
        "temperature",
        "temp",
        "t",
    ):
        if key in lower:
            return lower[key]
    for v in variables:
        n = v.lower()
        if n in POSITIVITY_NAMES or "pressure" in n or n.startswith("rho") or "temp" in n:
            return v
    return first_scalar(variables)


def menu_yaml_path(repo_root: Path) -> Path:
    env = os.environ.get("FLOWPLOT_MENU")
    if env:
        return Path(env)
    return repo_root / "agents" / "postprocess-menu.yaml"


class AgentsPanel(tk.Toplevel):
    """Browse Analyze actions and show charts for validation / polar / profiles."""

    def __init__(
        self,
        master: tk.Misc,
        *,
        flowplot_bin: str,
        dataset_path: Path | None,
        variables: list[str],
        var_name: str,
        nondim_args: list[str],
        plot_style: str = "jfm",
        on_png: Callable[[Path], None] | None = None,
        repo_root: Path | None = None,
        title: str = "Analyze",
        focus_group: str | None = None,
    ) -> None:
        super().__init__(master)
        self.title(title)
        self.geometry("980x680")
        self.minsize(760, 520)
        self.configure(bg=BG)

        self.flowplot = flowplot_bin
        self.dataset_path = dataset_path
        self.variables = list(variables)
        self.var_name = tk.StringVar(value=var_name or first_scalar(self.variables))
        self.nondim_args = list(nondim_args)
        self.plot_style = plot_style
        self.on_png = on_png
        self.repo_root = repo_root or Path(__file__).resolve().parent.parent
        self.focus_group = focus_group
        self._items: dict[str, dict[str, Any]] = {}
        self._selected_id: str | None = None
        self._last_payload: dict[str, Any] | None = None
        self._fig_canvas = None
        self._mpl_fig = None

        self._style()
        self._build()
        self._load_menu()
        self.protocol("WM_DELETE_WINDOW", self.destroy)
        stay_above(self, master, modal=False)

    def _style(self) -> None:
        style = ttk.Style(self)
        try:
            style.theme_use("clam")
        except tk.TclError:
            pass
        style.configure("Agents.TFrame", background=PANEL)
        style.configure("Agents.TLabel", background=PANEL, foreground=FG)
        style.configure("AgentsMuted.TLabel", background=PANEL, foreground=MUTED)
        style.configure(
            "Agents.TButton",
            background=BTN_BG,
            foreground=FG,
            padding=(10, 4),
        )
        style.map("Agents.TButton", background=[("active", BTN_ACTIVE)])

    def _build(self) -> None:
        header = ttk.Frame(self, style="Agents.TFrame")
        header.pack(side=tk.TOP, fill=tk.X, padx=8, pady=8)
        ttk.Label(
            header,
            text="Analyze — validation & correction",
            style="Agents.TLabel",
            font=("Helvetica", 12, "bold"),
        ).pack(side=tk.LEFT)
        ds = str(self.dataset_path) if self.dataset_path else "(no file loaded)"
        self.ds_label = ttk.Label(header, text=ds, style="AgentsMuted.TLabel")
        self.ds_label.pack(side=tk.RIGHT)

        body = ttk.Frame(self)
        body.pack(side=tk.TOP, fill=tk.BOTH, expand=True, padx=8, pady=(0, 8))

        left = ttk.Frame(body, style="Agents.TFrame", width=300)
        left.pack(side=tk.LEFT, fill=tk.Y, padx=(0, 8))
        left.pack_propagate(False)

        ttk.Label(left, text="Actions", style="Agents.TLabel", font=("Helvetica", 10, "bold")).pack(
            anchor="w", padx=10, pady=(10, 4)
        )
        tree_wrap = ttk.Frame(left, style="Agents.TFrame")
        tree_wrap.pack(fill=tk.BOTH, expand=True, padx=8, pady=(0, 8))
        self.tree = ttk.Treeview(
            tree_wrap,
            columns=("status",),
            show="tree headings",
            selectmode="browse",
        )
        self.tree.heading("#0", text="Action")
        self.tree.heading("status", text="Status")
        self.tree.column("#0", width=190, stretch=True)
        self.tree.column("status", width=70, stretch=False, anchor="center")
        scroll = ttk.Scrollbar(tree_wrap, orient=tk.VERTICAL, command=self.tree.yview)
        self.tree.configure(yscrollcommand=scroll.set)
        self.tree.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        scroll.pack(side=tk.RIGHT, fill=tk.Y)
        self.tree.bind("<<TreeviewSelect>>", self._on_select)

        right = ttk.Frame(body)
        right.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)

        detail = ttk.Frame(right, style="Agents.TFrame")
        detail.pack(fill=tk.X, pady=(0, 8))
        ttk.Label(
            detail, text="Selection", style="Agents.TLabel", font=("Helvetica", 10, "bold")
        ).pack(anchor="w", padx=10, pady=(10, 2))
        self.detail = tk.Text(
            detail,
            height=5,
            wrap=tk.WORD,
            bg="#fafbfc",
            fg=FG,
            relief=tk.FLAT,
            highlightthickness=1,
            highlightbackground="#b8c0c8",
            font=("DejaVu Sans Mono", 9),
        )
        self.detail.pack(fill=tk.X, padx=10, pady=(0, 8))
        self.detail.configure(state=tk.DISABLED)

        opts = ttk.Frame(right, style="Agents.TFrame")
        opts.pack(fill=tk.X, pady=(0, 8))
        ttk.Label(opts, text="Variable", style="AgentsMuted.TLabel").pack(
            side=tk.LEFT, padx=(10, 4), pady=8
        )
        self.var_combo = ttk.Combobox(
            opts,
            textvariable=self.var_name,
            values=self.variables,
            state="readonly" if self.variables else "disabled",
            width=18,
        )
        self.var_combo.pack(side=tk.LEFT, padx=(0, 8), pady=8)
        ttk.Button(opts, text="Run", style="Agents.TButton", command=self.run_selected).pack(
            side=tk.LEFT, padx=4, pady=8
        )
        ttk.Button(opts, text="Refresh", style="Agents.TButton", command=self._load_menu).pack(
            side=tk.LEFT, padx=4, pady=8
        )
        ttk.Button(
            opts, text="JSON details", style="Agents.TButton", command=self._toggle_json
        ).pack(side=tk.LEFT, padx=4, pady=8)

        # Chart + summary (main UX)
        self.chart_frame = ttk.Frame(right, style="Agents.TFrame")
        self.chart_frame.pack(fill=tk.BOTH, expand=True, pady=(0, 4))
        self.summary = ttk.Label(
            self.chart_frame,
            text="Select Field sanity, Positivity, Extract profile, or Shock polar → Run.",
            style="AgentsMuted.TLabel",
            wraplength=520,
            justify=tk.LEFT,
        )
        self.summary.pack(anchor="w", padx=10, pady=(8, 4))
        self.plot_host = tk.Frame(self.chart_frame, bg=VIEWPORT_BG, height=320)
        self.plot_host.pack(fill=tk.BOTH, expand=True, padx=8, pady=(0, 8))

        self.json_frame = ttk.Frame(right)
        self.results = tk.Text(
            self.json_frame,
            height=8,
            wrap=tk.WORD,
            bg=VIEWPORT_BG,
            fg=FG,
            relief=tk.FLAT,
            highlightthickness=1,
            highlightbackground="#9aa3ab",
            font=("DejaVu Sans Mono", 8),
        )
        self.results.pack(fill=tk.BOTH, expand=True)
        self._json_visible = False

    def _toggle_json(self) -> None:
        if self._json_visible:
            self.json_frame.pack_forget()
            self._json_visible = False
        else:
            self.json_frame.pack(fill=tk.X, pady=(0, 4))
            self._json_visible = True

    def _set_detail(self, text: str) -> None:
        self.detail.configure(state=tk.NORMAL)
        self.detail.delete("1.0", tk.END)
        self.detail.insert(tk.END, text)
        self.detail.configure(state=tk.DISABLED)

    def _log(self, msg: str) -> None:
        self.results.insert(tk.END, msg.rstrip() + "\n")
        self.results.see(tk.END)

    def _clear_results(self) -> None:
        self.results.delete("1.0", tk.END)

    def _clear_chart(self) -> None:
        if self._fig_canvas is not None:
            try:
                self._fig_canvas.get_tk_widget().destroy()
            except tk.TclError:
                pass
            self._fig_canvas = None
            self._mpl_fig = None
        for child in self.plot_host.winfo_children():
            child.destroy()

    def _show_mpl(self, draw_fn) -> None:
        self._clear_chart()
        try:
            from matplotlib.backends.backend_tkagg import FigureCanvasTkAgg
            from matplotlib.figure import Figure
        except ImportError:
            self.summary.configure(text="matplotlib missing — install for Analyze charts.")
            return
        fig = Figure(figsize=(5.4, 3.4), dpi=100)
        ax = fig.add_subplot(111)
        draw_fn(ax)
        fig.tight_layout()
        canvas = FigureCanvasTkAgg(fig, master=self.plot_host)
        canvas.draw()
        canvas.get_tk_widget().pack(fill=tk.BOTH, expand=True)
        self._fig_canvas = canvas
        self._mpl_fig = fig

    def _run_cli(self, args: list[str], timeout: int = 180) -> subprocess.CompletedProcess[str]:
        cmd = [self.flowplot, *args]
        self._log("$ " + " ".join(cmd))
        self.update_idletasks()
        return subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
            cwd=str(self.repo_root),
        )

    def _load_menu(self) -> None:
        self.tree.delete(*self.tree.get_children())
        self._items.clear()
        self._selected_id = None
        try:
            proc = self._run_cli(["--json", "menu", "list"])
        except (OSError, subprocess.TimeoutExpired) as exc:
            self._log(f"error loading menu: {exc}")
            return
        if proc.returncode != 0:
            self._log((proc.stderr or proc.stdout or "menu list failed").strip())
            self._load_menu_yaml_fallback()
            return
        try:
            payload = json.loads(proc.stdout.strip().splitlines()[-1])
        except (json.JSONDecodeError, IndexError) as exc:
            self._log(f"bad menu JSON: {exc}")
            self._load_menu_yaml_fallback()
            return
        groups = payload.get("groups") or []
        preferred = self.focus_group or "validation"
        for group in groups:
            gid = group.get("id") or "group"
            # Prefer validation + shocks for Analyze; still list the rest collapsed.
            open_default = gid in (preferred, "validation", "shocks", "nondim")
            gnode = self.tree.insert(
                "",
                tk.END,
                iid=f"g:{gid}",
                text=group.get("title") or gid,
                values=("",),
                open=open_default,
            )
            for item in group.get("items") or []:
                iid = item.get("id") or ""
                if not iid:
                    continue
                self._items[iid] = item
                status = item.get("status") or "?"
                mark = "ready" if status == "ready" else "stub"
                self.tree.insert(
                    gnode,
                    tk.END,
                    iid=f"i:{iid}",
                    text=f"{item.get('title') or iid}",
                    values=(mark,),
                )
        self._log(f"Loaded {len(self._items)} menu items.")

    def _load_menu_yaml_fallback(self) -> None:
        path = menu_yaml_path(self.repo_root)
        try:
            import yaml  # type: ignore
        except ImportError:
            self._log("PyYAML missing; cannot fall back to YAML parse.")
            return
        if not path.is_file():
            self._log(f"menu YAML not found: {path}")
            return
        try:
            data = yaml.safe_load(path.read_text(encoding="utf-8"))
        except OSError as exc:
            self._log(f"failed to read {path}: {exc}")
            return
        for group in data.get("groups") or []:
            gid = group.get("id") or "group"
            gnode = self.tree.insert(
                "",
                tk.END,
                iid=f"g:{gid}",
                text=group.get("title") or gid,
                values=("",),
                open=gid in ("validation", "shocks"),
            )
            for item in group.get("items") or []:
                iid = item.get("id") or ""
                if not iid:
                    continue
                self._items[iid] = item
                status = item.get("status") or "?"
                self.tree.insert(
                    gnode,
                    tk.END,
                    iid=f"i:{iid}",
                    text=item.get("title") or iid,
                    values=("ready" if status == "ready" else "stub",),
                )

    def _on_select(self, _event: tk.Event | None = None) -> None:
        sel = self.tree.selection()
        if not sel:
            return
        nid = sel[0]
        if not nid.startswith("i:"):
            self._selected_id = None
            self._set_detail("Select an action (leaf), not a group.")
            return
        iid = nid[2:]
        self._selected_id = iid
        item = self._items.get(iid, {})
        lehar = item.get("lehar") or {}
        fp = item.get("flowplot") or {}
        lines = [
            f"id: {iid}",
            f"title: {item.get('title', '')}",
            f"status: {item.get('status', '')}",
            f"summary: {item.get('summary', '')}",
        ]
        if lehar.get("agent"):
            lines.append(f"lehar_agent: {lehar['agent']}")
        lines.append(f"flowplot: {fp.get('command', '')}")
        if fp.get("notes"):
            lines.append(f"notes: {str(fp['notes']).strip()}")
        self._set_detail("\n".join(lines))

    def _require_dataset(self) -> Path | None:
        if self.dataset_path and Path(self.dataset_path).is_file():
            return Path(self.dataset_path)
        messagebox.showerror(
            "No dataset",
            "Load a CFD file first (File → Open).\n"
            "Analyze runs against the currently open path only.",
            parent=self,
        )
        self._log("error: no dataset loaded — refusing to invent a path.")
        return None

    def _build_args(self, item_id: str, item: dict[str, Any]) -> list[str] | None:
        ds = self._require_dataset()
        if not ds:
            return None
        status = item.get("status") or ""
        fp = item.get("flowplot") or {}
        command = fp.get("command") or ""
        required = list(fp.get("required_args") or [])

        if item_id == "shock-polar-field":
            # Handled in run_selected (matplotlib polar); no CLI args needed.
            return [str(ds)]

        if status != "ready":
            return [str(ds)]

        args: list[str] = [str(ds)]
        var = self.var_name.get().strip()
        if item_id == "positivity-field" or (
            "var" in required and not var and self.variables
        ):
            if item_id in ("positivity-field", "positivity-check"):
                var = guess_positivity_var(self.variables) or var
                self.var_name.set(var)
            elif not var:
                var = first_scalar(self.variables)
                self.var_name.set(var)

        needs_var = "var" in required or command in ("contour", "profile")
        needs_out = "out" in required or command in ("contour", "pathlines", "mesh")

        if needs_var:
            if not var:
                messagebox.showwarning(
                    "Variable",
                    "Select a variable for this action.",
                    parent=self,
                )
                return None
            args.extend(["--var", var])

        out_path: Path | None = None
        if needs_out:
            out_path = Path(tempfile.gettempdir()) / f"flowplot_agent_{item_id}.png"
            args.extend(["-o", str(out_path)])

        if command == "contour":
            args.extend(["--style", self.plot_style])
            args.extend(self.nondim_args)
            if item_id == "contour-pathlines":
                args.append("--pathlines")
        elif command == "pathlines":
            args.extend(["--style", self.plot_style])
            args.extend(self.nondim_args)
        elif command == "nondim":
            args.extend(self.nondim_args)
        elif command == "profile":
            args.extend(["--line", "mid-j"])
            json_out = Path(tempfile.gettempdir()) / f"flowplot_agent_{item_id}.json"
            args.extend(["-o", str(json_out)])

        self._last_out = out_path
        return args

    def _render_validation_chart(self, payload: dict[str, Any], *, positivity: bool) -> None:
        report = payload.get("report") or payload
        fields = report.get("fields") or []
        if not fields:
            self.summary.configure(text=report.get("summary") or payload.get("summary") or "No fields.")
            self._clear_chart()
            return

        def draw(ax) -> None:
            names = []
            mins = []
            maxs = []
            flags = []
            for f in fields[:24]:
                name = str(f.get("name") or "?")
                if name.lower() in COORD_NAMES:
                    continue
                names.append(name)
                mins.append(float(f["min"]) if f.get("min") is not None else 0.0)
                maxs.append(float(f["max"]) if f.get("max") is not None else 0.0)
                bad = (not f.get("ok", True)) or (
                    positivity and int(f.get("negative_count") or 0) > 0
                )
                flags.append(bad)
            if not names:
                ax.text(0.5, 0.5, "No scalar fields", ha="center", va="center")
                return
            y = range(len(names))
            colors = ["#c00000" if b else "#3d5a6c" for b in flags]
            ax.barh(list(y), [mx - mn for mn, mx in zip(mins, maxs)], left=mins, color=colors)
            ax.set_yticks(list(y), labels=names, fontsize=8)
            ax.set_xlabel("min → max")
            ax.set_title("Positivity fields" if positivity else "Field extrema")
            ax.grid(True, axis="x", alpha=0.3)

        ok = report.get("ok", payload.get("ok"))
        summary = report.get("summary") or payload.get("summary") or ""
        issues = report.get("issue_count", payload.get("issue_count", 0))
        self.summary.configure(
            text=f"{'OK' if ok else 'ISSUES'} — {summary}  (issues={issues})"
        )
        self._show_mpl(draw)

    def _render_profile_chart(self, payload: dict[str, Any]) -> None:
        profile = payload.get("profile") or payload
        samples = profile.get("samples") or []
        if not samples:
            # try nested under result
            for key in ("result", "data"):
                nested = payload.get(key) or {}
                if isinstance(nested, dict) and nested.get("samples"):
                    samples = nested["samples"]
                    profile = nested
                    break
        if not samples:
            self.summary.configure(text="Profile returned no samples.")
            self._clear_chart()
            return
        s_vals = [float(p.get("s", i)) for i, p in enumerate(samples)]
        v_vals = [float(p.get("value", 0.0)) for p in samples]
        var = profile.get("variable") or self.var_name.get()

        def draw(ax) -> None:
            ax.plot(s_vals, v_vals, color="#c00000", lw=1.8)
            ax.set_xlabel("s (arc / index)")
            ax.set_ylabel(str(var))
            ax.set_title(f"Profile · {var}")
            ax.grid(True, alpha=0.35)

        self.summary.configure(
            text=f"Extracted {len(samples)} samples of {var} along mid-j."
        )
        self._show_mpl(draw)

    def _run_shock_polar(self) -> None:
        self._clear_results()
        self._log("Analyze shock polar — analytic θ–β–M (γ-law)")
        mach = 2.0
        gamma = 1.4
        # Prefer a Mach variable name as freestream guess if present.
        for name in self.variables:
            if name.lower() in ("mach", "m", "mach_number"):
                # Still analytic — document that CFD Mach is not auto-fit here.
                break

        def draw(ax) -> None:
            plot_polar_on_axes(ax, mach=mach, gamma=gamma)

        out = Path(tempfile.gettempdir()) / "flowplot_shock_polar.png"
        curve = plot_polar_png(str(out), mach=mach, gamma=gamma)
        self.summary.configure(
            text=(
                f"Shock polar M∞={curve.mach:g}, γ={curve.gamma:g}, "
                f"θ_max≈{curve.theta_max_deg:.2f}°. "
                "Analytic ideal-gas curve in Flowplot; CFD-coupled polars → lehar shock_polar."
            )
        )
        self._show_mpl(draw)
        self._log(
            json.dumps(
                {
                    "status": "ok",
                    "command": "shock-polar",
                    "mach": curve.mach,
                    "gamma": curve.gamma,
                    "theta_max_deg": curve.theta_max_deg,
                    "out": str(out),
                    "note": "analytic θ–β–M; lehar shock_polar for CFD-coupled",
                },
                indent=2,
            )
        )
        if self.on_png is not None and out.is_file():
            try:
                self.on_png(out)
            except Exception as exc:  # noqa: BLE001
                self._log(f"viewport refresh failed: {exc}")

    def run_selected(self) -> None:
        if not self._selected_id:
            messagebox.showinfo("Analyze", "Select a menu action first.", parent=self)
            return
        item = self._items.get(self._selected_id)
        if not item:
            return

        if self._selected_id == "shock-polar-field":
            self._run_shock_polar()
            return

        args = self._build_args(self._selected_id, item)
        if args is None:
            return

        self._clear_results()
        self._log(f"Running {self._selected_id} on {args[0]}")
        try:
            proc = self._run_cli(
                ["--json", "menu", "run", self._selected_id, "--", *args]
            )
        except subprocess.TimeoutExpired:
            self._log("error: timed out")
            return
        except OSError as exc:
            self._log(f"error: {exc}")
            return

        out = (proc.stdout or "").strip()
        err = (proc.stderr or "").strip()
        payload: dict[str, Any] | None = None
        if out:
            try:
                payload = json.loads(out.splitlines()[-1])
                self._last_payload = payload
                self._log(json.dumps(payload, indent=2))
                if payload.get("status") == "stub":
                    self.summary.configure(
                        text="Stub — needs fuller lehar wiring; see JSON details."
                    )
                    self._clear_chart()
            except json.JSONDecodeError:
                self._log(out if len(out) < 8000 else out[:8000] + "…")
        if err:
            self._log("stderr:\n" + err)
        self._log(f"\nexit {proc.returncode}")

        if payload and proc.returncode == 0:
            iid = self._selected_id
            if iid in ("field-sanity", "positivity-check"):
                # menu run may nest under "result" / "report"
                body = payload.get("result") or payload.get("report") or payload
                if isinstance(body, dict):
                    self._render_validation_chart(
                        body if "fields" in body else payload,
                        positivity=(iid == "positivity-check"),
                    )
            elif iid == "extract-profile":
                self._render_profile_chart(payload.get("result") or payload)
            elif iid in ("plot-agent-scales", "nondim-suggest"):
                sugg = payload.get("suggestions") or payload.get("result") or payload
                self.summary.configure(
                    text="Nondim suggestions ready — apply scales in the left Plot panel."
                )
                self._clear_chart()
                self._log(json.dumps(sugg, indent=2) if not isinstance(sugg, str) else sugg)

        out_png = getattr(self, "_last_out", None)
        if (
            proc.returncode == 0
            and out_png
            and Path(out_png).is_file()
            and self.on_png is not None
        ):
            self._log(f"Refreshing viewport with {out_png}")
            try:
                self.on_png(Path(out_png))
            except Exception as exc:  # noqa: BLE001
                self._log(f"viewport refresh failed: {exc}")
