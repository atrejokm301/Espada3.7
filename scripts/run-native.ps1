#Requires -Version 5.1
<#
.SYNOPSIS
  Build and launch the native Tauri desktop app (primary product path).
  Prefer Start-ADC-Native.bat for double-click; this script is for PowerShell / npm run native.
#>
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path (Join-Path $Root "src-tauri"))) {
  $Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
}
$Exe = Join-Path $Root "src-tauri\target\debug\asignacion-del-cielo-bible.exe"
$SrcTauri = Join-Path $Root "src-tauri"
$Ui = Join-Path $Root "ui"

if (-not (Test-Path (Join-Path $Ui "index.html"))) {
  throw "Missing study UI: $Ui\index.html"
}

Write-Host "Espada 3.7 — Windows (Tauri)" -ForegroundColor Cyan
Write-Host "  UI: $Ui" -ForegroundColor DarkGray
Write-Host "  Host: WebView2 + Rust IPC (offline e-Sword)" -ForegroundColor DarkGray

Get-Process -Name "asignacion-del-cielo-bible" -ErrorAction SilentlyContinue |
  Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 800

Push-Location $SrcTauri
try {
  Write-Host "cargo build (embeds ui\)…" -ForegroundColor Yellow
  cargo build
  if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
} finally {
  Pop-Location
}

if (-not (Test-Path $Exe)) { throw "EXE missing: $Exe" }

# Detached process, no redirected pipes
Start-Process -FilePath $Exe -WorkingDirectory $SrcTauri
Write-Host "Launched native app: $Exe" -ForegroundColor Green
