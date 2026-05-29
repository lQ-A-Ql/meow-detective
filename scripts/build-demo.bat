@echo off
echo ========================================
echo Forensics Workbench Demo Build Script
echo ========================================
echo.

echo [1/3] Installing frontend dependencies...
cd frontend
call pnpm install
if errorlevel 1 (
    echo Failed to install frontend dependencies!
    exit /b 1
)
echo.

echo [2/3] Building frontend...
call pnpm build
if errorlevel 1 (
    echo Failed to build frontend!
    exit /b 1
)
echo.

echo [3/3] Building Tauri application...
cd ..\apps\desktop\src-tauri
call cargo build
if errorlevel 1 (
    echo Failed to build Tauri application!
    exit /b 1
)
echo.

echo ========================================
echo Build completed successfully!
echo.
echo Output: target\debug\forensics-desktop.exe
echo.
echo To run in development mode:
echo   cargo tauri dev
echo.
echo To build for production:
echo   cargo tauri build
echo ========================================
