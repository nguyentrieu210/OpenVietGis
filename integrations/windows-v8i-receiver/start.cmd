@echo off
setlocal
cd /d "%~dp0"
if exist "%~dp0OpenVietGISConnector.exe" goto EXE
where py >nul 2>nul
if %ERRORLEVEL% EQU 0 (
  py -3 "%~dp0gui.py"
) else (
  python "%~dp0gui.py"
)
if errorlevel 1 pause
exit /b %ERRORLEVEL%

:EXE
"%~dp0OpenVietGISConnector.exe"
exit /b %ERRORLEVEL%