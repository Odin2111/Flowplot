#!/usr/bin/env bash
# Install / refresh Windows Desktop Fieldplot.bat + .lnk + .ico (DESKTOP-BMQ9321).
# Keeps the Fieldplot icon; PowerShell Start-Process + absolute .sh (no bash -lc).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WIN_USER="${FIELDPLOT_WIN_USER:-CHC-PC}"
DESKTOP="/mnt/c/Users/${WIN_USER}/Desktop"
ICO_SRC="$ROOT/assets/icons/Fieldplot.ico"
LAUNCH_SH="$ROOT/scripts/launch-fieldplot.sh"
PS1="$ROOT/scripts/launch-fieldplot.ps1"

if [[ ! -d "$DESKTOP" ]]; then
  echo "Desktop not found: $DESKTOP" >&2
  exit 1
fi
if [[ ! -f "$ICO_SRC" ]]; then
  echo "Icon missing: $ICO_SRC" >&2
  exit 1
fi

cp -f "$ICO_SRC" "$DESKTOP/Fieldplot.ico"

# PowerShell Start-Process + env + absolute .sh — no bash -lc, no start "title".
# Confirmed: Start-Process wsl … bash /abs/launch-fieldplot.sh keeps the GUI alive.
cat >"$DESKTOP/Fieldplot.bat" <<'EOF'
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
EOF

mkdir -p "$ROOT/packaging/windows"
cp -f "$DESKTOP/Fieldplot.bat" "$ROOT/packaging/windows/Fieldplot.desktop.bat"
cp -f "$ICO_SRC" "$ROOT/packaging/windows/Fieldplot.ico"

powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "
\$desk = [Environment]::GetFolderPath('Desktop')
\$bat = Join-Path \$desk 'Fieldplot.bat'
\$ico = Join-Path \$desk 'Fieldplot.ico'
\$lnkPath = Join-Path \$desk 'Fieldplot.lnk'
\$w = New-Object -ComObject WScript.Shell
\$s = \$w.CreateShortcut(\$lnkPath)
\$s.TargetPath = \$bat
\$s.WorkingDirectory = \$desk
\$s.WindowStyle = 7
\$s.IconLocation = \"\$ico,0\"
\$s.Description = 'Fieldplot CFD post-processor (WSL)'
\$s.Save()
Write-Output \"lnk=\$lnkPath icon=\$ico target=\$bat\"
"

chmod +x "$LAUNCH_SH" "$PS1" || true
echo "Installed Desktop Fieldplot.bat / .lnk / .ico → $DESKTOP"
