@echo off
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0user-reload.ps1" -Edit
pause
