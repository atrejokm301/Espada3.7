#Requires -Version 5.1
<#
.SYNOPSIS
  Stable shell: keep adc-api alive + open Edge app window.
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
$apiExe = Join-Path $tauri "target\debug\adc-api.exe"
$logDir = Join-Path $env:LOCALAPPDATA "asignacion-del-cielo-bible"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$pidFile = Join-Path $logDir "adc-api.pid"

function Test-ApiHealthy {
  try {
    $h = Invoke-RestMethod -Uri ("{0}health" -f $url) -TimeoutSec 2
    return [bool]$h.ok
  } catch {
    return $false
  }
}

Write-Host "==> Repo: $Root" -ForegroundColor Cyan

if (-not (Test-Path $apiExe)) {
  Write-Host "==> Building adc-api…" -ForegroundColor Cyan
  Push-Location $tauri
  try { cargo build --bin adc-api } finally { Pop-Location }
}
if (-not (Test-Path $apiExe)) { throw "adc-api.exe missing after build" }

if (Test-ApiHealthy) {
  Write-Host "==> adc-api already healthy" -ForegroundColor DarkGray
} else {
  Get-Process -Name "adc-api" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep -Milliseconds 400

  Write-Host "==> Starting adc-api…" -ForegroundColor Cyan
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $apiExe
  $psi.WorkingDirectory = $tauri
  $psi.UseShellExecute = $false
  $psi.CreateNoWindow = $true
  # Critical: do not redirect stdout/stderr without a reader (deadlocks the process).
  $psi.EnvironmentVariables["ADC_UI_DIR"] = $ui
  $psi.EnvironmentVariables["ADC_API_PORT"] = "$port"
  $proc = New-Object System.Diagnostics.Process
  $proc.StartInfo = $psi
  if (-not $proc.Start()) { throw "Failed to start adc-api" }

  Set-Content -Path $pidFile -Value $proc.Id -Encoding ascii
  Write-Host "    PID $($proc.Id)"

  $ok = $false
  for ($i = 1; $i -le 50; $i++) {
    Start-Sleep -Milliseconds 200
    if ($proc.HasExited) {
      throw "adc-api exited immediately (code $($proc.ExitCode))"
    }
    if (Test-ApiHealthy) { $ok = $true; break }
  }
  if (-not $ok) { throw "adc-api did not become healthy on $url" }
}

Write-Host "==> Healthy: $url" -ForegroundColor Green

$edge = @(
  "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe",
  "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe",
  "$env:LocalAppData\Microsoft\Edge\Application\msedge.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1

if ($edge) {
  Write-Host "==> Opening Edge app…" -ForegroundColor Cyan
  Start-Process -FilePath $edge -ArgumentList @("--app=$url", "--new-window")
} else {
  Start-Process $url
}

Write-Host ""
Write-Host "Listo. Si sigue en 'Iniciando…', pulsa Ctrl+R en la ventana." -ForegroundColor Green
Write-Host "  $url"
Write-Host "  Stop: Stop-Process -Name adc-api"
Write-Host ""
