@echo off
title Hay Star - Setup & Diagnostics by Ashraf Morningstar
color 0A

echo =============================================================================
echo    🌾 HAY STAR SETUP & ENVIRONMENT CHECKER
echo    Created by Ashraf Morningstar
echo    GitHub: https://github.com/AshrafMorningstar/hay-star
echo =============================================================================
echo.

echo [1/4] Checking Python environment...
python --version >nul 2>&1
if %errorlevel% equ 0 (
    echo [OK] Python detected.
) else (
    echo [NOTICE] Python is not installed. You can still run hay-star.exe directly.
)

echo.
echo [2/4] Checking ADB and LDPlayer connectivity...
adb devices
echo.

echo [3/4] Verifying binary integrity...
if exist "hay-star.exe" (
    echo [OK] Core loader hay-star.exe found.
) else (
    echo [WARN] hay-star.exe missing. Build with: cd loader ^&^& cargo build --release
)

if exist "native\build\libmstar.so" (
    echo [OK] ARM64 native engine libmstar.so found.
) else (
    echo [WARN] libmstar.so not found in native/build.
)

echo.
echo [4/4] Setup check complete!
echo To run autonomous mode, launch: start.bat
echo To run the interactive REPL console, run: hay-star.exe
echo.
pause
