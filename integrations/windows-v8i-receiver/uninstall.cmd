@echo off
setlocal
set "TARGET=%LOCALAPPDATA%\OpenVietGIS\Connector"
echo This removes only the installed OpenVietGIS Windows Connector.
echo Received data under %LOCALAPPDATA%\OpenVietGIS\Incoming will NOT be deleted.
choice /C YN /M "Uninstall the Connector application"
if errorlevel 2 exit /b 0
if not exist "%TARGET%" exit /b 0
cd /d "%USERPROFILE%"
rmdir /S /Q "%TARGET%"
if exist "%TARGET%" (
  echo Could not uninstall; close Connector and retry.
  exit /b 1
)
echo Connector removed. Received data preserved.