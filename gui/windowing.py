"""Keep secondary Flowplot windows above the primary Viewport (Tecplot-like)."""

from __future__ import annotations

import tkinter as tk


def stay_above(win: tk.Toplevel, master: tk.Misc, *, modal: bool = False) -> None:
    """transient + lift (+ optional grab). Re-lift on FocusIn so dialogs stay on top."""
    try:
        root = master.winfo_toplevel()
    except tk.TclError:
        root = master
    try:
        win.transient(root)
    except tk.TclError:
        pass
    try:
        win.attributes("-topmost", True)
    except tk.TclError:
        pass
    try:
        win.lift()
        win.focus_force()
    except tk.TclError:
        pass
    if modal:
        try:
            win.grab_set()
        except tk.TclError:
            pass

    def _re_lift(_event: tk.Event | None = None) -> None:
        try:
            if win.winfo_exists():
                win.lift()
        except tk.TclError:
            pass

    win.bind("<FocusIn>", _re_lift)


def release_grabs(root: tk.Misc) -> None:
    """Drop any grab so filedialog / new windows can receive input."""
    try:
        current = root.grab_current()
    except tk.TclError:
        return
    if current is not None:
        try:
            current.grab_release()
        except tk.TclError:
            pass
