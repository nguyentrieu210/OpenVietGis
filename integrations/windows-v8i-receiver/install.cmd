@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "SOURCE=%~dp0"
set "TARGET=%LOCALAPPDATA%\OpenVietGIS\Connector"
if not exist "%SOURCE%OpenVietGISConnector.exe" if exist "%TARGET%\OpenVietGISConnector.exe" (
  echo Existing EXE installed but this package is source-only.
  echo Use an updated Windows EXE bundle or uninstall the old one first.
  exit /b 1
)
if not exist "%TARGET%" mkdir "%TARGET%"
if not exist "%TARGET%" (
  echo Cannot create installation folder.
  pause
  exit /b 1
)
for %%F in (receiver.py v8i_probe.py gui.py start.cmd OpenVietGISConnector.bas README.md outbound.py sync_transport.py windows_com.py bridge_contract.py diagnostics.py selftest.py uninstall.cmd) do (
  copy /Y "%SOURCE%%%F" "%TARGET%\%%F" >nul
  if errorlevel 1 (
    echo Installation failed while copying %%F
    pause
    exit /b 1
  )
)
if exist "%SOURCE%OpenVietGISConnector.exe" (
  copy /Y "%SOURCE%OpenVietGISConnector.exe" "%TARGET%\OpenVietGISConnector.exe" >nul
  if errorlevel 1 exit /b 1
  if exist "%SOURCE%_internal\" (
    if not exist "%TARGET%\_internal" mkdir "%TARGET%\_internal"
    xcopy /E /I /Q /Y "%SOURCE%_internal\*" "%TARGET%\_internal\" >nul
    if errorlevel 1 exit /b 1
  ) else (
    echo Missing executable dependencies: _internal\
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
if exist "%TARGET%\OpenVietGISConnector.exe" (
  echo Portable EXE installed; no Python setup required.
) else (
  echo Source install: Python 3.10+ required; pywin32 for COM.
)
echo Start the Connector directly using: %TARGET%\start.cmd
echo.
pause