# Clean debug launch — avoids Windows error 0x800700e8 (pipe closed) from cargo run.
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path (Join-Path $Root "src-tauri"))) {
  $Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
}
$Exe = Join-Path $Root "src-tauri\target\debug\asignacion-del-cielo-bible.exe"
$SrcTauri = Join-Path $Root "src-tauri"

Get-Process -Name "asignacion-del-cielo-bible" -ErrorAction SilentlyContinue |
  Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 800

Push-Location $SrcTauri
try {
  cargo build
  if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
} finally {
  Pop-Location
}

if (-not (Test-Path $Exe)) { throw "EXE missing: $Exe" }

# Detached process, no redirected pipes
Start-Process -FilePath $Exe -WorkingDirectory $SrcTauri
Write-Host "Launched: $Exe"
