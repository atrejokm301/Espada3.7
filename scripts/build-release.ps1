#Requires -Version 5.1
<#
.SYNOPSIS
  Optimized Windows release build for Espada 3.7 (Tauri).

  Profile: -O3, fat LTO, strip, panic=abort, native CPU, single codegen unit.
  Output:  installer(s) + release .exe under src-tauri\target\release\
#>
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path (Join-Path $Root "src-tauri"))) {
  $Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
}
$SrcTauri = Join-Path $Root "src-tauri"
$Ui = Join-Path $Root "ui"

Write-Host ""
Write-Host "  Espada 3.7 (Windows) — RELEASE BUILD" -ForegroundColor Cyan
Write-Host "  =====================================" -ForegroundColor Cyan
Write-Host "  opt-level=3  LTO=fat  codegen-units=1  strip  panic=abort" -ForegroundColor DarkGray
Write-Host "  RUSTFLAGS: target-cpu=native (+ embed-bitcode)" -ForegroundColor DarkGray
Write-Host "  UI: $Ui" -ForegroundColor DarkGray
Write-Host ""

if (-not (Test-Path (Join-Path $Ui "index.html"))) {
  throw "Missing UI: $Ui\index.html"
}

# Free locks
Get-Process -Name "asignacion-del-cielo-bible" -ErrorAction SilentlyContinue |
  Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500

# Max performance for THIS machine. For broader CPU support use x86-64-v2.
$env:RUSTFLAGS = @(
  "-C target-cpu=native",
  "-C embed-bitcode=yes"
) -join " "

# Prefer release profile always
$env:CARGO_PROFILE_RELEASE_OPT_LEVEL = "3"
$env:CARGO_PROFILE_RELEASE_LTO = "fat"
$env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS = "1"
$env:CARGO_PROFILE_RELEASE_STRIP = "symbols"
$env:CARGO_PROFILE_RELEASE_PANIC = "abort"
$env:CARGO_PROFILE_RELEASE_DEBUG = "0"
$env:CARGO_PROFILE_RELEASE_INCREMENTAL = "false"

# MSVC flags for C deps (bundled SQLite via rusqlite). Avoid /GL here — it
# fights Cargo's linker; Rust fat LTO already covers our crates.
$env:CFLAGS = "/O2 /fp:fast /DNDEBUG"
$env:CXXFLAGS = "/O2 /fp:fast /DNDEBUG"

Write-Host "RUSTFLAGS=$env:RUSTFLAGS" -ForegroundColor Yellow
Write-Host "Building (this can take several minutes with fat LTO)…" -ForegroundColor Yellow
Write-Host ""

Push-Location $Root
try {
  # Prefer tauri CLI (embeds ui/, produces NSIS/MSI)
  $tauriOk = $false
  if (Get-Command npx -ErrorAction SilentlyContinue) {
    npx --yes tauri build
    if ($LASTEXITCODE -eq 0) { $tauriOk = $true }
  }
  if (-not $tauriOk) {
    Write-Host "tauri build failed or unavailable — cargo release binary only" -ForegroundColor Yellow
    Push-Location $SrcTauri
    try {
      cargo build --release --bin asignacion-del-cielo-bible
      if ($LASTEXITCODE -ne 0) { throw "cargo build --release failed ($LASTEXITCODE)" }
    } finally {
      Pop-Location
    }
  }
} finally {
  Pop-Location
}

$rel = Join-Path $SrcTauri "target\release"
$exe = Join-Path $rel "asignacion-del-cielo-bible.exe"
$bundle = Join-Path $rel "bundle"

Write-Host ""
if (Test-Path $exe) {
  $fi = Get-Item $exe
  Write-Host "  EXE:  $($fi.FullName)" -ForegroundColor Green
  Write-Host "  Size: $([math]::Round($fi.Length / 1MB, 2)) MB" -ForegroundColor Green
  Write-Host "  Time: $($fi.LastWriteTime)" -ForegroundColor Green
} else {
  Write-Host "  WARNING: release exe not found at $exe" -ForegroundColor Red
}

if (Test-Path $bundle) {
  Write-Host "  Bundle folder: $bundle" -ForegroundColor Green
  Get-ChildItem $bundle -Recurse -Include *.exe,*.msi -ErrorAction SilentlyContinue |
    ForEach-Object {
      Write-Host ("    - {0}  ({1:N2} MB)" -f $_.FullName, ($_.Length / 1MB)) -ForegroundColor Cyan
    }
}

Write-Host ""
Write-Host "Done." -ForegroundColor Cyan
