@echo off
setlocal
set "ROOT=%~dp0"
set "PATH=%ROOT%;%PATH%"

where py >nul 2>&1
if %ERRORLEVEL%==0 (
  py -3 "%ROOT%gui\flowplot_gui.py" %*
  exit /b %ERRORLEVEL%
)

where python >nul 2>&1
if %ERRORLEVEL%==0 (
  python "%ROOT%gui\flowplot_gui.py" %*
  exit /b %ERRORLEVEL%
)

echo Flowplot GUI needs Python 3 with tkinter on PATH.
echo Install from https://www.python.org/downloads/ and re-run.
pause
exit /b 1
