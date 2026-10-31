# Fieldplot (WSL GUI) launcher for Windows Desktop / PowerShell.
# Starts Ubuntu-24.04 WSL with scripts/launch-fieldplot.sh (WSLg DISPLAY).
# Absolute .sh path + env vars — never bash -lc with nested Windows quotes.

param(
    [string]$Distro = "Ubuntu-24.04",
    [string]$WslScript = "/home/nasu/software/Flowplot-agents-wt/scripts/launch-fieldplot.sh",
    [string]$TipBranch = "cursor/contour-var-desktop-launch-207d",
    [string]$FallbackScript = "/home/nasu/software/Flowplot/scripts/launch-fieldplot.sh",
    [switch]$ShowConsole
)

$ErrorActionPreference = "Stop"

if (-not (Get-Command wsl.exe -ErrorAction SilentlyContinue)) {
    Write-Error "wsl.exe not found. Install WSL / Ubuntu-24.04 first."
    exit 1
}

$script = $WslScript
$probe = & wsl.exe -d $Distro -- bash -c "test -f '$script' && echo OK || echo MISSING"
if ($probe -notmatch 'OK') {
    $script = $FallbackScript
}

# env VAR=… bash /abs/script — no bash -lc string (Start-Process-safe).
$wslArgs = @(
    "-d", $Distro,
    "--",
    "env",
    "FIELDPLOT_SKIP_GIT_SYNC=1",
    "FIELDPLOT_TIP_BRANCH=$TipBranch",
    "bash",
    $script
)

if ($ShowConsole) {
    & wsl.exe @wslArgs
    exit $LASTEXITCODE
}

Start-Process -FilePath "wsl.exe" -ArgumentList $wslArgs -WindowStyle Hidden
exit 0
