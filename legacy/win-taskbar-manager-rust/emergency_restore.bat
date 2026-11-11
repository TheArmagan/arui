@echo off
echo ============================================
echo     EMERGENCY TASKBAR RESTORE SCRIPT
echo ============================================
echo.
echo Bu script taskbar'i geri açar eğer program çökerse
echo.

echo Method 1: Direct ShowWindow çağrısı...
powershell -Command "Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class Win32 { [DllImport(\"user32.dll\")] public static extern IntPtr FindWindow(string lpClassName, string lpWindowName); [DllImport(\"user32.dll\")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow); }'; $taskbar = [Win32]::FindWindow('Shell_TrayWnd', $null); if($taskbar -ne [IntPtr]::Zero) { [Win32]::ShowWindow($taskbar, 5); Write-Host 'Taskbar restore edildi!' } else { Write-Host 'Taskbar bulunamadi!' }"

timeout /t 2 >nul

echo.
echo Method 2: Explorer restart...
echo Eğer yukarıdaki işe yaramadıysa explorer'ı yeniden başlatıyoruz...

taskkill /f /im explorer.exe >nul 2>&1
timeout /t 2 >nul
start explorer.exe

echo.
echo ✅ Taskbar restore işlemi tamamlandı!
echo Program şimdi normal şekilde çalışmalı.
echo.
pause
