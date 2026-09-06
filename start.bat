@echo off
title Hay Star - 1-Click Launch by Ashraf Morningstar
color 0B

echo =============================================================================
echo    🌾 HAY STAR - 1-CLICK AUTONOMOUS LAUNCHER
echo    Created by Ashraf Morningstar
echo    GitHub: https://github.com/AshrafMorningstar/hay-star
echo =============================================================================
echo.

:: Check if python is available
python --version >nul 2>&1
if %errorlevel% equ 0 (
    echo [INFO] Starting Autonomous Supervisor Engine...
    python supervisor.py
) else (
    echo [WARN] Python not found in PATH. Launching native hay-star.exe directly...
    if exist "hay-star.exe" (
        hay-star.exe
    ) else (
        echo [ERROR] Neither Python nor hay-star.exe was found!
        echo Please run setup.bat or compile from source.
        pause
    )
)

pause
