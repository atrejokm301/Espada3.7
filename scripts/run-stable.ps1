#Requires -Version 5.1
<#
.SYNOPSIS
  Start adc-api detached + open Edge app. Double-click Start-ADC-Stable.bat is preferred.
#>
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$bat = Join-Path $Root "Start-ADC-Stable.bat"
if (-not (Test-Path $bat)) { throw "missing $bat" }
Write-Host "Launching $bat …" -ForegroundColor Cyan
Start-Process -FilePath $bat -WorkingDirectory $Root
Write-Host "If the window stays on boot, press Ctrl+R after a second." -ForegroundColor Green
