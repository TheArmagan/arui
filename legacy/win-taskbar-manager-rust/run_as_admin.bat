@echo off
echo Windows Taskbar Manager - Yonetici Yetkisiyle Calistiriliyor...
echo.

REM Admin kontrolü
net session >nul 2>&1
if %errorLevel% == 0 (
    echo ✅ Yonetici yetkisi var, devam ediliyor...
    echo.
) else (
    echo ❌ Bu uygulama yonetici yetkisi gerektirir!
    echo Lutfen bu batch dosyasini sag tikla "Yonetici olarak calistir" ile acin.
    echo.
    pause
    exit /b 1
)

REM Uygulamayı çalıştır
echo 🚀 Taskbar Manager baslatiliyor...
echo Cikmak icin Ctrl+C kullanin.
echo.

cargo run --release

echo.
echo Program sonlandi.
pause
