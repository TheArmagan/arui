use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

fn main() {
    println!("ACIL DURUM TASKBAR RESTORE BAŞLIYOR...");

    unsafe {
        // Tüm taskbar'ları bul ve göster
        let taskbar_classes = ["Shell_TrayWnd", "Shell_SecondaryTrayWnd"];

        for class_name in &taskbar_classes {
            let class_name_wide: Vec<u16> = class_name
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            if let Ok(hwnd) = FindWindowW(windows::core::PCWSTR(class_name_wide.as_ptr()), None) {
                println!("Taskbar bulundu: {}", class_name);

                // Taskbar'ı göster
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = ShowWindow(hwnd, SW_RESTORE);

                // Normal pozisyona getir
                let screen_width = GetSystemMetrics(SM_CXSCREEN);
                let screen_height = GetSystemMetrics(SM_CYSCREEN);
                let taskbar_height = 40;

                let _ = SetWindowPos(
                    hwnd,
                    None,
                    0,
                    screen_height - taskbar_height,
                    screen_width,
                    taskbar_height,
                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );

                println!("Taskbar restore edildi: {}", class_name);
            }
        }

        // Workspace alanını normale döndür
        let screen_width = GetSystemMetrics(SM_CXSCREEN);
        let screen_height = GetSystemMetrics(SM_CYSCREEN);
        let taskbar_height = 40;

        let mut work_area = RECT {
            left: 0,
            top: 0,
            right: screen_width,
            bottom: screen_height - taskbar_height,
        };

        let result = SystemParametersInfoW(
            SPI_SETWORKAREA,
            0,
            Some(&mut work_area as *mut _ as *mut _),
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        );

        if result.is_ok() {
            println!("Workspace alanı normale döndürüldü");
        } else {
            println!("Workspace restore hatası!");
        }
    }

    // Explorer restart
    println!("Explorer restart ediliyor...");
    let _ = std::process::Command::new("taskkill")
        .args(&["/F", "/IM", "explorer.exe"])
        .output();

    std::thread::sleep(std::time::Duration::from_millis(1000));

    let _ = std::process::Command::new("explorer.exe").spawn();

    println!("ACIL DURUM RESTORE TAMAMLANDI!");
    println!("Taskbar görünmüyorsa bilgisayarı yeniden başlatın.");
}
