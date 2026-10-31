# Fieldplot (WSL GUI) launcher for Windows Desktop / PowerShell.
# Starts Ubuntu-24.04 WSL with scripts/launch-fieldplot.sh (WSLg DISPLAY).
# Preference: no MSI / no interactive UI automation.

param(
    [string]$Distro = "Ubuntu-24.04",
    [string]$WslScript = "/home/nasu/software/Flowplot-agents-wt/scripts/launch-fieldplot.sh",
    [string]$TipBranch = "cursor/hud-optical-agents-fix-e488",
    [switch]$ShowConsole
)

$ErrorActionPreference = "Stop"

if (-not (Get-Command wsl.exe -ErrorAction SilentlyContinue)) {
    Write-Error "wsl.exe not found. Install WSL / Ubuntu-24.04 first."
    exit 1
}

$envArgs = @()
if ($TipBranch) {
    # Pass tip override into the shell script.
    $bash = "export FIELDPLOT_TIP_BRANCH='$TipBranch'; if [ -x '$WslScript' ]; then exec bash '$WslScript'; else export DISPLAY=`${DISPLAY:-:0}; cd ~/software/Flowplot-agents-wt && exec python3 gui/flowplot_gui.py; fi"
} else {
    $bash = "if [ -x '$WslScript' ]; then exec bash '$WslScript'; else export DISPLAY=`${DISPLAY:-:0}; cd ~/software/Flowplot-agents-wt && exec python3 gui/flowplot_gui.py; fi"
}

$wslArgs = @("-d", $Distro, "--", "bash", "-lc", $bash)

if ($ShowConsole) {
    & wsl.exe @wslArgs
    exit $LASTEXITCODE
}

# Minimized console so double-click does not leave a lingering cmd in front.
Start-Process -FilePath "wsl.exe" -ArgumentList $wslArgs -WindowStyle Minimized
exit 0
