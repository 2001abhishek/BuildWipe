@echo off
rem Builds BuildWipe locally with update signing, like the GitHub Release workflow.
rem
rem The key and password are read from %USERPROFILE%\.tauri, never stored here,
rem so this file is safe to commit. Extra arguments go to `tauri build`, e.g.:
rem   build-signed.bat --bundles nsis

setlocal
cd /d "%~dp0"

set "KEY_DIR=%USERPROFILE%\.tauri"
set "TAURI_SIGNING_PRIVATE_KEY=%KEY_DIR%\buildwipe-updater.key"

if not exist "%TAURI_SIGNING_PRIVATE_KEY%" (
  echo Signing key not found: %TAURI_SIGNING_PRIVATE_KEY%
  echo Restore it from your backup, or build unsigned with:
  echo   npm run tauri build -- --config src-tauri/tauri.ci.conf.json
  exit /b 1
)

if exist "%KEY_DIR%\buildwipe-updater.password.txt" (
  set /p TAURI_SIGNING_PRIVATE_KEY_PASSWORD=<"%KEY_DIR%\buildwipe-updater.password.txt"
) else (
  rem No password file: ask for it without showing it on screen.
  for /f "usebackq delims=" %%P in (`powershell -NoProfile -Command "$s = Read-Host 'Signing key password' -AsSecureString; [Runtime.InteropServices.Marshal]::PtrToStringBSTR([Runtime.InteropServices.Marshal]::SecureStringToBSTR($s))"`) do set "TAURI_SIGNING_PRIVATE_KEY_PASSWORD=%%P"
)

if not defined TAURI_SIGNING_PRIVATE_KEY_PASSWORD (
  echo No signing key password given.
  exit /b 1
)

call npm run tauri build -- %*
set "RESULT=%ERRORLEVEL%"

if "%RESULT%"=="0" (
  echo.
  echo Signed installers are in src-tauri\target\release\bundle
)
endlocal & exit /b %RESULT%
