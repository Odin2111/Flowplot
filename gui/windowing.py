"""Keep secondary Fieldplot windows above the primary Viewport (Fieldplot)."""

from __future__ import annotations

import tkinter as tk


def stay_above(
    win: tk.Toplevel,
    master: tk.Misc,
    *,
    modal: bool = False,
    always_on_top: bool = False,
) -> None:
    """transient + lift. Sticky -topmost when always_on_top (Agents); else brief pulse."""
    try:
        root = master.winfo_toplevel()
    except tk.TclError:
        root = master
    try:
        win.transient(root)
    except tk.TclError:
        pass
    try:
        win.lift()
        win.focus_force()
    except tk.TclError:
        pass
    try:
        win.attributes("-topmost", True)
        if not always_on_top and not modal:
            # Brief topmost pulse so the dialog wins stacking, then release.
            win.after(80, lambda: _clear_topmost(win))
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
                if always_on_top:
                    win.attributes("-topmost", True)
                win.lift()
        except tk.TclError:
            pass

    win.bind("<FocusIn>", _re_lift)
    if always_on_top:
        win.bind("<Map>", _re_lift)


def _clear_topmost(win: tk.Toplevel) -> None:
    try:
        if win.winfo_exists():
            win.attributes("-topmost", False)
            win.lift()
    except tk.TclError:
        pass


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
