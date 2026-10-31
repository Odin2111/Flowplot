# Fieldplot (WSL GUI) launcher for Windows Desktop / PowerShell.
# Starts Ubuntu-24.04 WSL with scripts/launch-fieldplot.sh (WSLg DISPLAY).
# Preference: no MSI / no interactive UI automation.
# IMPORTANT: call the .sh by absolute path — do not wrap in bash -lc with
# nested \"$VAR\" quotes (cmd.exe breaks ROOT and cds to /mnt/c/Windows).

param(
    [string]$Distro = "Ubuntu-24.04",
    [string]$WslScript = "/home/nasu/software/Flowplot-agents-wt/scripts/launch-fieldplot.sh",
    [string]$TipBranch = "cursor/desktop-click-blank-load-8052",
    [string]$FallbackScript = "/home/nasu/software/Flowplot/scripts/launch-fieldplot.sh",
    [switch]$ShowConsole
)

$ErrorActionPreference = "Stop"

if (-not (Get-Command wsl.exe -ErrorAction SilentlyContinue)) {
    Write-Error "wsl.exe not found. Install WSL / Ubuntu-24.04 first."
    exit 1
}

# Absolute-path exec only — no bash -lc string interpolation.
$script = $WslScript
$probe = & wsl.exe -d $Distro -- bash -c "test -x '$script' && echo OK || echo MISSING"
if ($probe -notmatch 'OK') {
    $script = $FallbackScript
}

$envPrefix = if ($TipBranch) { "FIELDPLOT_TIP_BRANCH='$TipBranch' " } else { "" }
# Use env + absolute script; bash reads the file (no nested Windows quoting).
$wslArgs = @("-d", $Distro, "--", "bash", "-c", "${envPrefix}exec bash '$script'")

if ($ShowConsole) {
    & wsl.exe @wslArgs
    exit $LASTEXITCODE
}

Start-Process -FilePath "wsl.exe" -ArgumentList $wslArgs -WindowStyle Minimized
exit 0
