@echo off
setlocal

rem Delete only this project's Rust/Tauri build output.
rem The target path is resolved relative to this .bat file.
set "TARGET=%~dp0my-ssh-frontend\src-tauri\target"

echo.
echo Cleaning Rust build cache:
echo %TARGET%
echo.

if not exist "%TARGET%" (
  echo target directory does not exist. Nothing to clean.
  echo.
  pause
  exit /b 0
)

rmdir /s /q "%TARGET%"

if exist "%TARGET%" (
  echo Failed to remove target directory.
) else (
  echo Rust build cache removed.
)

echo.
pause
