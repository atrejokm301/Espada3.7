#Requires -Version 5.1
<#
.SYNOPSIS
  Build the Rust e-Sword sidecar + run the WinUI 3 shell (feature/winui3-shell).

.DESCRIPTION
  1. cargo build --bin adc-api  (debug by default)
  2. dotnet run the WinUI host which loads ui/index.html over WebView2
     with Desktop Acrylic glass and talks to adc-api on 127.0.0.1:17865
#>
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path (Join-Path $Root "ui\index.html"))) {
  $Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
}

Write-Host "==> Repo: $Root" -ForegroundColor Cyan

$env:Path = [System.Environment]::GetEnvironmentVariable("Path", "Machine") + ";" +
            [System.Environment]::GetEnvironmentVariable("Path", "User")

$tauri = Join-Path $Root "src-tauri"
$winui = Join-Path $Root "src-winui\AsignacionDelCielo.WinUI"

if (-not (Test-Path $winui)) {
  throw "WinUI project not found at $winui"
}

$Configuration = if ($args -contains "--release") { "release" } else { "debug" }
$DotNetConfig = if ($Configuration -eq "release") { "Release" } else { "Debug" }

Write-Host "==> Building adc-api ($Configuration)…" -ForegroundColor Cyan
Push-Location $tauri
try {
  if ($Configuration -eq "release") {
    cargo build --bin adc-api --release
  } else {
    cargo build --bin adc-api
  }
} finally {
  Pop-Location
}

$apiExe = Join-Path $tauri "target\$Configuration\adc-api.exe"
if (-not (Test-Path $apiExe)) {
  throw "adc-api.exe missing after build: $apiExe"
}

$env:ADC_API_EXE = $apiExe
$env:ADC_UI_DIR = Join-Path $Root "ui"
$env:ADC_API_PORT = "17865"

Write-Host "==> API: $apiExe" -ForegroundColor DarkGray
Write-Host "==> UI : $($env:ADC_UI_DIR)" -ForegroundColor DarkGray
Write-Host "==> Building WinUI 3 host ($DotNetConfig | x64, self-contained)…" -ForegroundColor Cyan

$dotnet = "C:\Program Files\dotnet\dotnet.exe"
Push-Location $winui
try {
  & $dotnet build -c $DotNetConfig -p:Platform=x64
  if ($LASTEXITCODE -ne 0) { throw "WinUI build failed" }

  $exe = Get-ChildItem -Recurse -Path (Join-Path $winui "bin") -Filter "AsignacionDelCielo.WinUI.exe" |
    Where-Object { $_.FullName -match [regex]::Escape($DotNetConfig) } |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1

  if (-not $exe) { throw "WinUI exe not found under bin\" }

  Write-Host "==> Launching $($exe.FullName)" -ForegroundColor Cyan
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $exe.FullName
  $psi.WorkingDirectory = $exe.DirectoryName
  $psi.UseShellExecute = $false
  $psi.EnvironmentVariables["ADC_API_EXE"] = $apiExe
  $psi.EnvironmentVariables["ADC_UI_DIR"] = $env:ADC_UI_DIR
  $psi.EnvironmentVariables["ADC_API_PORT"] = "17865"
  $proc = [System.Diagnostics.Process]::Start($psi)
  Write-Host "==> WinUI started (PID $($proc.Id)). Close the window when done." -ForegroundColor Green
  # Don't wait forever if launched interactively from an agent; wait if console attached
  if ([Environment]::UserInteractive -and -not [Console]::IsInputRedirected) {
    $proc.WaitForExit()
  }
} finally {
  Pop-Location
}
