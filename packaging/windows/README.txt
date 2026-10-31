Flowplot 0.1.0 — Windows install
================================

Installed under:
  C:\Program Files\Flowplot\

Contents
--------
  flowplot.exe          CLI (PLT/VTK load, contour PNG, nondim, pathlines)
  flowplot-gui.bat      Launches the Python GUI shell
  gui\flowplot_gui.py   Thin tkinter front-end (shells out to the CLI)
  gui\assets\           Wordmark SVG
  DEPS.txt              GUI / Python notes
  README.txt            This file

Quick start
-----------
  1. Open a new Command Prompt (so PATH updates apply).
  2. Run:  flowplot --help
  3. Or:   flowplot path\to\case.dat
  4. GUI:  Start Menu → Flowplot → Flowplot GUI
           (needs Python 3 with tkinter on PATH)

Optional PATH
-------------
  Start Menu → Flowplot → Add Flowplot to PATH
  (or run add-to-path.bat in this folder). Opens a helper that appends
  this directory to your user PATH. Open a new terminal afterward.

Uninstall
---------
  Settings → Apps → Flowplot, or Start Menu → Flowplot → Uninstall Flowplot.

License: MIT OR Apache-2.0
Repo:    https://github.com/Odin2111/Flowplot
