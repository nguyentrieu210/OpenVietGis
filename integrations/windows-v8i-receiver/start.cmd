@echo off
setlocal
cd /d "%~dp0"
where py >nul 2>nul
if %ERRORLEVEL% EQU 0 (
  py -3 "%~dp0gui.py"
) else (
  python "%~dp0gui.py"
)
if errorlevel 1 pause