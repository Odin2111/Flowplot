"""Interactive Agents panel — browse postprocess-menu.yaml and run against the loaded file.

Invokes `flowplot menu run <id> -- <args…> --json` so agents share the same
dataset path / CLI model as Mesh, Contour, and Pathlines.
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

# Match main GUI palette.
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
    """Browse the post-process menu and run selected actions on the loaded dataset."""

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
    ) -> None:
        super().__init__(master)
        self.title("Flowplot Agents")
        self.geometry("920x620")
        self.minsize(720, 480)
        self.configure(bg=BG)

        self.flowplot = flowplot_bin
        self.dataset_path = dataset_path
        self.variables = list(variables)
        self.var_name = tk.StringVar(value=var_name or first_scalar(self.variables))
        self.nondim_args = list(nondim_args)
        self.plot_style = plot_style
        self.on_png = on_png
        self.repo_root = repo_root or Path(__file__).resolve().parent.parent
        self._items: dict[str, dict[str, Any]] = {}
        self._selected_id: str | None = None

        self._style()
        self._build()
        self._load_menu()
        self.protocol("WM_DELETE_WINDOW", self.destroy)

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
            text="Post-process agents",
            style="Agents.TLabel",
            font=("Helvetica", 12, "bold"),
        ).pack(side=tk.LEFT)
        ds = str(self.dataset_path) if self.dataset_path else "(no file loaded)"
        self.ds_label = ttk.Label(header, text=ds, style="AgentsMuted.TLabel")
        self.ds_label.pack(side=tk.RIGHT)

        body = ttk.Frame(self)
        body.pack(side=tk.TOP, fill=tk.BOTH, expand=True, padx=8, pady=(0, 8))

        left = ttk.Frame(body, style="Agents.TFrame", width=320)
        left.pack(side=tk.LEFT, fill=tk.Y, padx=(0, 8))
        left.pack_propagate(False)

        ttk.Label(left, text="Menu", style="Agents.TLabel", font=("Helvetica", 10, "bold")).pack(
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
        self.tree.column("#0", width=200, stretch=True)
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
            height=8,
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
        ttk.Button(opts, text="Run selected", style="Agents.TButton", command=self.run_selected).pack(
            side=tk.LEFT, padx=4, pady=8
        )
        ttk.Button(opts, text="Refresh menu", style="Agents.TButton", command=self._load_menu).pack(
            side=tk.LEFT, padx=4, pady=8
        )

        ttk.Label(
            right, text="Results", style="Agents.TLabel", font=("Helvetica", 10, "bold")
        ).pack(anchor="w")
        self.results = tk.Text(
            right,
            wrap=tk.WORD,
            bg=VIEWPORT_BG,
            fg=FG,
            relief=tk.FLAT,
            highlightthickness=1,
            highlightbackground="#9aa3ab",
            font=("DejaVu Sans Mono", 9),
        )
        self.results.pack(fill=tk.BOTH, expand=True, pady=(4, 0))
        self._log("Select a ready action and click Run selected.")
        if not self.dataset_path:
            self._log("No dataset loaded — File → Open a CFD file first.")

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
            # Fallback: parse YAML if CLI menu fails.
            self._load_menu_yaml_fallback()
            return
        try:
            payload = json.loads(proc.stdout.strip().splitlines()[-1])
        except (json.JSONDecodeError, IndexError) as exc:
            self._log(f"bad menu JSON: {exc}")
            self._load_menu_yaml_fallback()
            return
        groups = payload.get("groups") or []
        for group in groups:
            gid = group.get("id") or "group"
            gnode = self.tree.insert(
                "",
                tk.END,
                iid=f"g:{gid}",
                text=group.get("title") or gid,
                values=("",),
                open=True,
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
                open=True,
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
        self._log(f"Loaded {len(self._items)} items from {path.name}.")

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
        if lehar.get("surface"):
            lines.append(f"lehar_surface: {lehar['surface']}")
        lines.append(f"flowplot: {fp.get('command', '')}")
        if fp.get("template"):
            lines.append(f"template: {fp['template']}")
        if fp.get("notes"):
            lines.append(f"notes: {str(fp['notes']).strip()}")
        self._set_detail("\n".join(lines))

    def _require_dataset(self) -> Path | None:
        if self.dataset_path and Path(self.dataset_path).is_file():
            return Path(self.dataset_path)
        messagebox.showerror(
            "No dataset",
            "Load a CFD file first (File → Open).\n"
            "Agents run against the currently open path only.",
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

        # Stubs: still invoke menu run so JSON documents the required agent.
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
            if var:
                # already have --var from needs_var? pathlines uses --var as underlay
                pass
        elif command == "nondim":
            args.extend(self.nondim_args)
        elif command == "profile":
            args.extend(["--line", "mid-j"])
            # JSON profile to a sidecar (--json also prints to stdout via menu run).
            json_out = Path(tempfile.gettempdir()) / f"flowplot_agent_{item_id}.json"
            args.extend(["-o", str(json_out)])

        # stash for refresh
        self._last_out = out_path
        return args

    def run_selected(self) -> None:
        if not self._selected_id:
            messagebox.showinfo("Agents", "Select a menu action first.", parent=self)
            return
        item = self._items.get(self._selected_id)
        if not item:
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
        if out:
            try:
                # pretty-print last JSON object if possible
                payload = json.loads(out.splitlines()[-1])
                self._log(json.dumps(payload, indent=2))
                status = payload.get("status")
                if status == "stub":
                    self._log(
                        "\n(stub) This action still needs fuller lehar wiring; "
                        "template and planned flags are shown above."
                    )
            except json.JSONDecodeError:
                self._log(out if len(out) < 8000 else out[:8000] + "…")
        if err:
            self._log("stderr:\n" + err)
        self._log(f"\nexit {proc.returncode}")

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
            except Exception as exc:  # noqa: BLE001 — show in panel
                self._log(f"viewport refresh failed: {exc}")
