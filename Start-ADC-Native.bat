@echo off
REM Primary launcher: native Tauri desktop app (offline e-Sword via IPC).
REM Loads the live ui/ folder embedded at cargo/tauri build time.
setlocal
title Espada 3.7 - Windows (Tauri)
set "ROOT=%~dp0"
set "EXE=%ROOT%src-tauri\target\debug\asignacion-del-cielo-bible.exe"

echo.
echo  Espada 3.7 — Windows (Tauri native)
echo  ==========================================
echo  UI: ui\   Backend: Rust IPC (no Edge / no adc-api required)
echo.

REM Free file lock if a previous instance is open
taskkill /F /IM asignacion-del-cielo-bible.exe >nul 2>&1
timeout /t 1 /nobreak >nul 2>&1

cd /d "%ROOT%src-tauri"
echo Building native shell (embeds latest ui\)...
cargo build
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

REM Detached start — no stdin/stdout pipes (avoids Windows 0x800700e8)
start "" "%EXE%"
echo.
echo Launched native app:
echo   %EXE%
echo.
timeout /t 3 /nobreak >nul
endlocal
exit /b 0
