@echo off
REM Clean debug launch — avoids pipe error 0x800700e8 from cargo run / attached shells.
setlocal
set "ROOT=%~dp0"
set "EXE=%ROOT%src-tauri\target\debug\asignacion-del-cielo-bible.exe"

REM Stop previous instance (and free the .exe lock)
taskkill /F /IM asignacion-del-cielo-bible.exe >nul 2>&1
timeout /t 1 /nobreak >nul 2>&1

cd /d "%ROOT%src-tauri"
cargo build
if errorlevel 1 (
  echo Build failed.
  exit /b 1
)

if not exist "%EXE%" (
  echo EXE not found: %EXE%
  exit /b 1
)

REM Detached start — no stdin/stdout pipes (no 0x800700e8)
start "" "%EXE%"
echo Launched.
exit /b 0
