@echo off
setlocal EnableExtensions
set "ROOT=%~dp0"
if "%ROOT:~-1%"=="\" set "ROOT=%ROOT:~0,-1%"

echo Adding to user PATH:
echo   %ROOT%

powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$add = '%ROOT%';" ^
  "$p = [Environment]::GetEnvironmentVariable('Path','User');" ^
  "if ([string]::IsNullOrEmpty($p)) {" ^
  "  [Environment]::SetEnvironmentVariable('Path',$add,'User');" ^
  "  Write-Host 'PATH set.'" ^
  "} elseif (("';'+$p+';').ToLower().Contains((';'+$add+';').ToLower())) {" ^
  "  Write-Host 'Already on user PATH.'" ^
  "} else {" ^
  "  [Environment]::SetEnvironmentVariable('Path', $p + ';' + $add, 'User');" ^
  "  Write-Host 'Appended to user PATH.'" ^
  "}"

echo.
echo Open a new terminal for PATH changes to take effect.
pause
