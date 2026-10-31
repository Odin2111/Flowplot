"""Open CFD dataset dialog with Name | Size | Modified columns.

Native tk filedialogs on Linux often hide mtime/size; this details list makes
the newest `.plt` obvious.
"""

from __future__ import annotations

import os
import tkinter as tk
from datetime import datetime
from pathlib import Path
from tkinter import ttk
from typing import Iterable

from windowing import stay_above

BG = "#eceef0"
PANEL = "#e2e5e8"
HEADER_BG = "#d5d9de"
FG = "#1f2a33"
MUTED = "#5c6b76"
ACCENT = "#3d5a6c"

CFD_GLOBS = (
    "*.plt",
    "*.dat",
    "*.tp",
    "*.cgns",
    "*.cgz",
    "*.vtk",
    "*.vtu",
    "*.vtp",
    "*.vti",
    "*.vts",
    "*.vtr",
    "*.cas",
)

FILTERS: list[tuple[str, tuple[str, ...]]] = [
    ("CFD datasets", CFD_GLOBS),
    ("PLT / DAT", ("*.plt", "*.dat", "*.tp")),
    ("CGNS", ("*.cgns", "*.cgz")),
    ("VTK", ("*.vtk", "*.vtu", "*.vtp", "*.vti", "*.vts", "*.vtr")),
    ("Fluent", ("*.cas", "*.dat")),
    ("All files", ("*",)),
]


def _fmt_size(n: int) -> str:
    if n < 1024:
        return f"{n} B"
    if n < 1024**2:
        return f"{n / 1024:.1f} KB"
    if n < 1024**3:
        return f"{n / 1024**2:.1f} MB"
    return f"{n / 1024**3:.2f} GB"


def _fmt_mtime(ts: float) -> str:
    return datetime.fromtimestamp(ts).strftime("%Y-%m-%d %H:%M:%S")


def _match_globs(name: str, globs: Iterable[str]) -> bool:
    name_l = name.lower()
    for g in globs:
        if g == "*":
            return True
        if g.startswith("*.") and name_l.endswith(g[1:].lower()):
            return True
        if g == "*.*" and "." in name:
            return True
    return False


class OpenDatasetDialog(tk.Toplevel):
    """Details-view file picker: Name | Size | Modified."""

    def __init__(self, master: tk.Misc, *, initialdir: str | Path | None = None) -> None:
        super().__init__(master)
        self.title("Open CFD dataset")
        self.geometry("760x480")
        self.minsize(560, 360)
        self.configure(bg=BG)
        self.result: str | None = None

        start = Path(initialdir) if initialdir else Path.cwd()
        if not start.is_dir():
            start = start.parent if start.parent.is_dir() else Path.cwd()
        self._dir = start.resolve()
        self._filter_idx = tk.IntVar(value=0)
        self._sort_col = "modified"
        self._sort_desc = True  # newest first by default

        self._build()
        stay_above(self, master, modal=True)
        self.protocol("WM_DELETE_WINDOW", self._cancel)
        self.bind("<Escape>", lambda _e: self._cancel())
        self.bind("<Return>", lambda _e: self._ok())
        self.after(1, self._refresh)

    def _build(self) -> None:
        hdr = tk.Frame(self, bg=HEADER_BG)
        hdr.pack(side=tk.TOP, fill=tk.X)
        tk.Label(
            hdr,
            text="Open CFD dataset",
            bg=HEADER_BG,
            fg=FG,
            font=("Helvetica", 11, "bold"),
        ).pack(side=tk.LEFT, padx=12, pady=8)
        tk.Label(
            hdr,
            text="Name · Size · Modified (newest first)",
            bg=HEADER_BG,
            fg=MUTED,
            font=("Helvetica", 8),
        ).pack(side=tk.LEFT, padx=4)

        path_row = ttk.Frame(self, padding=(10, 8))
        path_row.pack(side=tk.TOP, fill=tk.X)
        ttk.Label(path_row, text="Folder").pack(side=tk.LEFT)
        self.path_var = tk.StringVar(value=str(self._dir))
        ent = ttk.Entry(path_row, textvariable=self.path_var)
        ent.pack(side=tk.LEFT, fill=tk.X, expand=True, padx=6)
        ent.bind("<Return>", lambda _e: self._go_path())
        ttk.Button(path_row, text="Up", width=4, command=self._go_up).pack(side=tk.LEFT, padx=2)
        ttk.Button(path_row, text="Go", width=4, command=self._go_path).pack(side=tk.LEFT)

        mid = ttk.Frame(self, padding=(10, 0))
        mid.pack(side=tk.TOP, fill=tk.BOTH, expand=True)
        cols = ("name", "size", "modified")
        self.tree = ttk.Treeview(
            mid,
            columns=cols,
            show="headings",
            selectmode="browse",
        )
        self.tree.heading("name", text="Name", command=lambda: self._sort_by("name"))
        self.tree.heading("size", text="Size", command=lambda: self._sort_by("size"))
        self.tree.heading(
            "modified", text="Modified", command=lambda: self._sort_by("modified")
        )
        self.tree.column("name", width=360, anchor="w")
        self.tree.column("size", width=100, anchor="e")
        self.tree.column("modified", width=160, anchor="w")
        vsb = ttk.Scrollbar(mid, orient=tk.VERTICAL, command=self.tree.yview)
        self.tree.configure(yscrollcommand=vsb.set)
        self.tree.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        vsb.pack(side=tk.RIGHT, fill=tk.Y)
        self.tree.bind("<Double-1>", self._on_double)
        self.tree.bind("<<TreeviewSelect>>", self._on_select)

        filt = ttk.Frame(self, padding=(10, 6))
        filt.pack(side=tk.TOP, fill=tk.X)
        ttk.Label(filt, text="Filter").pack(side=tk.LEFT)
        self.filter_combo = ttk.Combobox(
            filt,
            state="readonly",
            width=28,
            values=[f"{label} ({', '.join(globs)})" for label, globs in FILTERS],
        )
        self.filter_combo.current(0)
        self.filter_combo.pack(side=tk.LEFT, padx=6)
        self.filter_combo.bind("<<ComboboxSelected>>", lambda _e: self._refresh())

        file_row = ttk.Frame(self, padding=(10, 0))
        file_row.pack(side=tk.TOP, fill=tk.X)
        ttk.Label(file_row, text="File").pack(side=tk.LEFT)
        self.file_var = tk.StringVar(value="")
        ttk.Entry(file_row, textvariable=self.file_var).pack(
            side=tk.LEFT, fill=tk.X, expand=True, padx=6
        )

        btns = ttk.Frame(self, padding=(10, 10))
        btns.pack(side=tk.BOTTOM, fill=tk.X)
        ttk.Button(btns, text="Cancel", command=self._cancel).pack(side=tk.RIGHT, padx=4)
        ttk.Button(btns, text="Open", command=self._ok).pack(side=tk.RIGHT)

    def _globs(self) -> tuple[str, ...]:
        idx = self.filter_combo.current()
        if idx < 0 or idx >= len(FILTERS):
            idx = 0
        return FILTERS[idx][1]

    def _go_up(self) -> None:
        self._dir = self._dir.parent
        self.path_var.set(str(self._dir))
        self._refresh()

    def _go_path(self) -> None:
        p = Path(self.path_var.get().strip() or ".").expanduser()
        if p.is_file():
            self._dir = p.parent.resolve()
            self.file_var.set(p.name)
            self.path_var.set(str(self._dir))
        elif p.is_dir():
            self._dir = p.resolve()
            self.path_var.set(str(self._dir))
        self._refresh()

    def _sort_by(self, col: str) -> None:
        if self._sort_col == col:
            self._sort_desc = not self._sort_desc
        else:
            self._sort_col = col
            self._sort_desc = col == "modified"
        self._refresh()

    def _refresh(self) -> None:
        for iid in self.tree.get_children():
            self.tree.delete(iid)
        self.path_var.set(str(self._dir))
        globs = self._globs()
        rows: list[tuple[str, str, str, str, float, int, bool]] = []
        # dirs first (navigate), then matching files
        try:
            entries = list(self._dir.iterdir())
        except OSError as exc:
            self.tree.insert("", tk.END, iid="err", values=(f"(cannot list: {exc})", "", ""))
            return
        for ent in entries:
            try:
                st = ent.stat()
            except OSError:
                continue
            is_dir = ent.is_dir()
            if is_dir:
                if ent.name.startswith("."):
                    continue
                rows.append(
                    (
                        ent.name + "/",
                        "—",
                        _fmt_mtime(st.st_mtime),
                        ent.name,
                        st.st_mtime,
                        -1,
                        True,
                    )
                )
            else:
                if not _match_globs(ent.name, globs):
                    continue
                rows.append(
                    (
                        ent.name,
                        _fmt_size(st.st_size),
                        _fmt_mtime(st.st_mtime),
                        ent.name,
                        st.st_mtime,
                        st.st_size,
                        False,
                    )
                )

        def key(r: tuple[str, str, str, str, float, int, bool]):
            name, _sz, _mt, _raw, mtime, size, is_dir = r
            # Directories always before files when sorting by name; for mtime/size
            # keep dirs grouped at top for navigation.
            if self._sort_col == "name":
                return (0 if is_dir else 1, name.lower())
            if self._sort_col == "size":
                return (0 if is_dir else 1, size)
            return (0 if is_dir else 1, mtime)

        rows.sort(key=key, reverse=self._sort_desc and self._sort_col != "name")
        if self._sort_col == "name" and self._sort_desc:
            rows.sort(key=lambda r: (0 if r[6] else 1, r[0].lower()), reverse=True)

        # Default: dirs A→Z, files by Modified newest first when sort is modified.
        if self._sort_col == "modified":
            dirs = [r for r in rows if r[6]]
            files = [r for r in rows if not r[6]]
            dirs.sort(key=lambda r: r[0].lower())
            files.sort(key=lambda r: r[4], reverse=self._sort_desc)
            rows = dirs + files

        for i, (disp, size_s, mtime_s, raw, _mt, _sz, is_dir) in enumerate(rows):
            tag = "dir" if is_dir else "file"
            self.tree.insert(
                "",
                tk.END,
                iid=f"{'d' if is_dir else 'f'}{i}:{raw}",
                values=(disp, size_s, mtime_s),
                tags=(tag,),
            )
        self.tree.tag_configure("dir", foreground=ACCENT)
        # Update heading arrows
        for col, label in (
            ("name", "Name"),
            ("size", "Size"),
            ("modified", "Modified"),
        ):
            mark = ""
            if col == self._sort_col:
                mark = " ▼" if self._sort_desc else " ▲"
            self.tree.heading(col, text=label + mark)

    def _on_select(self, _event: tk.Event | None = None) -> None:
        sel = self.tree.selection()
        if not sel:
            return
        iid = sel[0]
        if iid.startswith("d"):
            return
        # fN:name
        name = iid.split(":", 1)[-1]
        self.file_var.set(name)

    def _on_double(self, _event: tk.Event | None = None) -> None:
        sel = self.tree.selection()
        if not sel:
            return
        iid = sel[0]
        name = iid.split(":", 1)[-1]
        if iid.startswith("d"):
            self._dir = (self._dir / name).resolve()
            self.file_var.set("")
            self._refresh()
            return
        self.file_var.set(name)
        self._ok()

    def _ok(self) -> None:
        name = self.file_var.get().strip()
        if not name:
            sel = self.tree.selection()
            if sel and sel[0].startswith("f"):
                name = sel[0].split(":", 1)[-1]
        if not name:
            return
        path = (self._dir / name).resolve()
        if path.is_dir():
            self._dir = path
            self.file_var.set("")
            self._refresh()
            return
        if not path.is_file():
            return
        self.result = str(path)
        self.destroy()

    def _cancel(self) -> None:
        self.result = None
        self.destroy()


def ask_open_dataset(
    master: tk.Misc,
    *,
    initialdir: str | Path | None = None,
) -> str | None:
    """Modal Open CFD dataset dialog; returns path or None."""
    # Prefer last dataset folder when available.
    if initialdir is None:
        initialdir = os.environ.get("FIELDPLOT_LAST_DIR") or os.getcwd()
    dlg = OpenDatasetDialog(master, initialdir=initialdir)
    master.wait_window(dlg)
    return dlg.result
