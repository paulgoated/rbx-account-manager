@echo off
rem Builds dist\RBXAccountManager.exe. See build.ps1 for details.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0build.ps1"
if errorlevel 1 (echo. & echo BUILD FAILED & pause & exit /b 1)
