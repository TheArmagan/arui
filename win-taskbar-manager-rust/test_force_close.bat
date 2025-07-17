@echo off
echo ===============================================
echo     TASK MANAGER FORCE CLOSE TEST SCRIPT
echo ===============================================
echo.
echo Bu script taskbar manager'i zorla kapatmaya calisir
echo ve taskbar'in otomatik restore edilip edilmedigini test eder.
echo.

echo 1. Normal Ctrl+C test...
echo Taskbar Manager'i baslatin ve Ctrl+C ile kapatin.
echo Taskbar restore edildi mi?
echo.
pause

echo.
echo 2. Task Manager force close test...
echo a) Taskbar Manager'i baslatin
echo b) Task Manager'i acin (Ctrl+Shift+Esc)
echo c) "win-taskbar-manager.exe" procesini bulun
echo d) "End Task" ile zorla kapatin
echo e) Taskbar'in hemen restore edildigini kontrol edin
echo.
pause

echo.
echo 3. Process kill test...
echo Taskbar Manager'i baslatin ve asagidaki komutu calistirin:
echo taskkill /f /im win-taskbar-manager.exe
echo.
echo Taskbar restore edildi mi?
echo.
pause

echo.
echo ✅ Tüm testleri geçtiyse güvenlik sistemi çalışıyor!
echo ❌ Herhangi bir testte taskbar kaybolursa emergency_restore.bat çalıştırın.
echo.
pause
