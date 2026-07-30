#Requires -Version 5.1
<#
.SYNOPSIS
  Stable shell: adc-api + Microsoft Edge app window (NO WinUI / embedded WebView2).

  Use this when the WinUI host crashes. Same UI + same e-Sword backend.
#>
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path (Join-Path $Root "ui\index.html"))) {
  throw "ui/index.html not found under $Root"
}

$env:Path = [System.Environment]::GetEnvironmentVariable("Path", "Machine") + ";" +
            [System.Environment]::GetEnvironmentVariable("Path", "User")

$tauri = Join-Path $Root "src-tauri"
$ui = Join-Path $Root "ui"
$port = 17865
$url = "http://127.0.0.1:$port/"

Write-Host "==> Building adc-api…" -ForegroundColor Cyan
Push-Location $tauri
try { cargo build --bin adc-api } finally { Pop-Location }

$apiExe = Join-Path $tauri "target\debug\adc-api.exe"
if (-not (Test-Path $apiExe)) { throw "missing $apiExe" }

# Kill previous API on this port path
Get-Process -Name "adc-api" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 400

$env:ADC_UI_DIR = $ui
$env:ADC_API_PORT = "$port"

Write-Host "==> Starting API + UI server on $url" -ForegroundColor Cyan
$api = Start-Process -FilePath $apiExe -PassThru -WindowStyle Hidden
Start-Sleep -Seconds 1

$healthy = $false
for ($i = 0; $i -lt 30; $i++) {
  try {
    $h = Invoke-RestMethod "$url`health" -TimeoutSec 1
    if ($h.ok) { $healthy = $true; break }
  } catch { Start-Sleep -Milliseconds 200 }
}
if (-not $healthy) {
  Stop-Process -Id $api.Id -Force -ErrorAction SilentlyContinue
  throw "adc-api did not become healthy"
}

# Prefer Edge app window; fall back to default browser
$edgeCandidates = @(
  "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe",
  "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe",
  "$env:LocalAppData\Microsoft\Edge\Application\msedge.exe"
)
$edge = $edgeCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1

if ($edge) {
  Write-Host "==> Opening Edge app window: $url" -ForegroundColor Green
  Start-Process -FilePath $edge -ArgumentList @(
    "--app=$url",
    "--new-window",
    "--disable-features=msEdgeSidebar"
  )
} else {
  Write-Host "==> Edge not found; opening default browser: $url" -ForegroundColor Yellow
  Start-Process $url
}

Write-Host ""
Write-Host "Stable shell is up." -ForegroundColor Green
Write-Host "  UI:  $url"
Write-Host "  API: adc-api PID $($api.Id)"
Write-Host "  Close the browser window when done; stop API with: Stop-Process -Name adc-api"
Write-Host ""
