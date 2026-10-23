@echo off
rem PYIME TSF dev kit: register/unregister PYIME.dll (x64 + x86), start/stop pyime-server.exe.
rem Usage: tsf-dev.bat install | uninstall | start | stop
rem install/uninstall ask for administrator rights (regsvr32 writes HKLM);
rem the server is always started without elevation (from this non-elevated window).
setlocal
set "KIT=%~dp0"
if /i "%~1"=="install" goto install
if /i "%~1"=="uninstall" goto uninstall
if /i "%~1"=="start" goto start
if /i "%~1"=="stop" goto stop
if /i "%~1"=="_register" goto register
if /i "%~1"=="_unregister" goto unregister
echo Usage: %~nx0 install ^| uninstall ^| start ^| stop
exit /b 1

:install
call :stopserver
call :elevate _register || goto failed
call :startserver
echo Installed. Add "PYIME" in Settings - Time and language - Language - Chinese - Keyboards if it is not listed.
exit /b 0

:uninstall
call :stopserver
call :elevate _unregister || goto failed
echo Uninstalled.
exit /b 0

:start
call :startserver
exit /b 0

:stop
call :stopserver
exit /b 0

:register
"%SystemRoot%\System32\regsvr32.exe" /s "%KIT%x64\PYIME.dll" || exit /b 1
"%SystemRoot%\SysWOW64\regsvr32.exe" /s "%KIT%x86\PYIME.dll" || exit /b 1
exit /b 0

:unregister
"%SystemRoot%\System32\regsvr32.exe" /u /s "%KIT%x64\PYIME.dll"
"%SystemRoot%\SysWOW64\regsvr32.exe" /u /s "%KIT%x86\PYIME.dll"
exit /b 0

:elevate
powershell -NoProfile -Command "exit (Start-Process -FilePath '%~f0' -ArgumentList '%1' -Verb RunAs -Wait -PassThru).ExitCode"
exit /b %errorlevel%

:startserver
start "" "%KIT%pyime-server.exe" --supervise
exit /b 0

:stopserver
if exist "%KIT%pyime-server.exe" "%KIT%pyime-server.exe" --quit
taskkill /F /IM pyime-server.exe >nul 2>&1
exit /b 0

:failed
echo Failed (administrator rights refused or regsvr32 error).
exit /b 1
