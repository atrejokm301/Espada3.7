@echo off
setlocal
title Asignacion del Cielo - Stable
cd /d "%~dp0"

set "API=%~dp0src-tauri\target\debug\adc-api.exe"
set "UI=%~dp0ui"
set "PORT=17865"
set "URL=http://127.0.0.1:%PORT%/"

if not exist "%API%" (
  echo Building adc-api first time...
  pushd "%~dp0src-tauri"
  cargo build --bin adc-api
  popd
)
if not exist "%API%" (
  echo ERROR: adc-api.exe not found. Install Rust and run: cargo build --bin adc-api
  pause
  exit /b 1
)

echo Stopping old adc-api if any...
"%SystemRoot%\System32\taskkill.exe" /F /IM adc-api.exe >nul 2>&1

echo Starting adc-api on %URL%
REM Detach with env vars via a tiny helper cmd so the process keeps running.
set "ADC_UI_DIR=%UI%"
set "ADC_API_PORT=%PORT%"
"%SystemRoot%\System32\cmd.exe" /c "set ADC_UI_DIR=%UI%&& set ADC_API_PORT=%PORT%&& start `"ADC-API`" /MIN `"%API%`""

echo Waiting for server...
set /a n=0
:wait
set /a n+=1
powershell -NoProfile -Command "try { $r=Invoke-RestMethod '%URL%health' -TimeoutSec 1; if($r.ok){exit 0}else{exit 1} } catch { exit 1 }"
if %errorlevel%==0 goto ready
if %n% geq 40 goto fail
timeout /t 1 /nobreak >nul
goto wait

:fail
echo ERROR: adc-api did not start. Is port %PORT% free?
pause
exit /b 1

:ready
echo Server OK. Opening app...
set "EDGE1=%ProgramFiles(x86)%\Microsoft\Edge\Application\msedge.exe"
set "EDGE2=%ProgramFiles%\Microsoft\Edge\Application\msedge.exe"
if exist "%EDGE1%" (
  start "" "%EDGE1%" --app=%URL% --new-window
) else if exist "%EDGE2%" (
  start "" "%EDGE2%" --app=%URL% --new-window
) else (
  start "" "%URL%"
)

echo.
echo App opened. Leave adc-api running in the background.
echo To stop later: taskkill /F /IM adc-api.exe
echo.
timeout /t 4 /nobreak >nul
endlocal
