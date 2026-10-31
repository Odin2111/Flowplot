@echo off
REM Fieldplot (WSL GUI) — DESKTOP-BMQ9321 double-click launcher
REM PowerShell Start-Process → wsl.exe env + absolute launch-fieldplot.sh.
REM Do NOT use bash -lc with nested quotes (cmd empties ROOT → /mnt/c/Windows).
REM Do NOT use: start "Fieldplot" /min wsl … (title quoting → "cannot find \Fieldplot\").
REM Tip: cursor/contour-var-desktop-launch-207d (stacked on #24).

cd /d "%USERPROFILE%\Desktop" >nul 2>&1

powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -Command ^
  "Start-Process -FilePath 'wsl.exe' -ArgumentList @('-d','Ubuntu-24.04','--','env','FIELDPLOT_SKIP_GIT_SYNC=1','FIELDPLOT_TIP_BRANCH=cursor/contour-var-desktop-launch-207d','bash','/home/nasu/software/Flowplot-agents-wt/scripts/launch-fieldplot.sh') -WindowStyle Hidden"

if errorlevel 1 (
  powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -Command ^
    "Start-Process -FilePath 'wsl.exe' -ArgumentList @('-d','Ubuntu-24.04','--','env','FIELDPLOT_SKIP_GIT_SYNC=1','bash','/home/nasu/software/Flowplot/scripts/launch-fieldplot.sh') -WindowStyle Hidden"
)
exit /b 0
