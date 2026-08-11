@echo off
REM Release launcher: optimized Tauri .exe (NOT debug).
REM Embeds ui\ at cargo --release build time.
setlocal
title Espada 3.7 - Windows RELEASE
set "ROOT=%~dp0"
set "EXE=%ROOT%src-tauri\target\release\asignacion-del-cielo-bible.exe"

echo.
echo  Espada 3.7 — Windows RELEASE
echo  ===================================
echo  UI: ui\  (embedded at release build)
echo  Profile: cargo --release (opt-level 3, LTO)
echo.

REM Free locks: Tauri release + experimental WinUI shell (WinUI is what WER crashes)
taskkill /F /IM asignacion-del-cielo-bible.exe >nul 2>&1
taskkill /F /IM AsignacionDelCielo.WinUI.exe >nul 2>&1
taskkill /F /IM adc-api.exe >nul 2>&1
timeout /t 1 /nobreak >nul 2>&1

cd /d "%ROOT%src-tauri"
echo Building RELEASE (can take several minutes with fat LTO)...
cargo build --release --bin asignacion-del-cielo-bible
if errorlevel 1 (
  echo.
  echo BUILD FAILED. Need Rust + MSVC Build Tools.
  echo   winget install Rustlang.Rustup
  pause
  exit /b 1
)

if not exist "%EXE%" (
  echo EXE not found: %EXE%
  pause
  exit /b 1
)

start "" "%EXE%"
echo.
echo Launched RELEASE app:
echo   %EXE%
echo.
timeout /t 3 /nobreak >nul
endlocal
exit /b 0
