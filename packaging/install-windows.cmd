@echo off
REM Akasha OS Preview — bypass Restricted execution policy for unsigned install.ps1
cd /d "%~dp0"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1" %*
if errorlevel 1 exit /b %ERRORLEVEL%
