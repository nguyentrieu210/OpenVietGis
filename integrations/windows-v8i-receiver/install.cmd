@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "SOURCE=%~dp0"
set "TARGET=%LOCALAPPDATA%\OpenVietGIS\Connector"
if not exist "%TARGET%" mkdir "%TARGET%"
if not exist "%TARGET%" (
  echo Cannot create installation folder.
  pause
  exit /b 1
)
for %%F in (receiver.py v8i_probe.py gui.py start.cmd OpenVietGISConnector.bas README.md outbound.py sync_transport.py windows_com.py bridge_contract.py) do (
  copy /Y "%SOURCE%%%F" "%TARGET%\%%F" >nul
  if errorlevel 1 (
    echo Installation failed while copying %%F
    pause
    exit /b 1
  )
)
echo.
echo OpenVietGIS Windows Connector installed in:
echo %TARGET%
echo.
echo NEXT: import OpenVietGISConnector.bas into a MicroStation V8i VBA project.
echo Then run the OpenVietGIS_Connect macro from MicroStation.
echo.
echo Python 3.10+ required. pywin32 optional for COM detection.
echo Start the Connector directly using: %TARGET%\start.cmd
echo.
pause