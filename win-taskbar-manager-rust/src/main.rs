mod taskbar;

use env_logger::Env;
use log::error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use taskbar::{TaskbarEvent, TaskbarManager};
use tokio::time;

/// Acil durum taskbar restore fonksiyonu
fn emergency_taskbar_restore() {
    // EXPLORER RESTART - En basit ve etkili çözüm
    let _ = std::process::Command::new("taskkill")
        .args(&["/F", "/IM", "explorer.exe"])
        .output();

    std::thread::sleep(std::time::Duration::from_millis(500));

    let _ = std::process::Command::new("explorer.exe").spawn();
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Logger'ı başlat
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    // TaskbarManager'ı oluştur
    let (mut taskbar_manager, mut event_receiver) = match TaskbarManager::new() {
        Ok((manager, receiver)) => (manager, receiver),
        Err(e) => {
            emergency_taskbar_restore();
            return Err(e);
        }
    };

    // Taskbar'ı gizle
    if let Err(e) = taskbar_manager.hide_taskbar() {
        emergency_taskbar_restore();
        return Err(e);
    }

    // Event listener task
    let _event_task = tokio::spawn(async move {
        while let Some(event) = event_receiver.recv().await {
            print_event_json(&event);
        }
    });

    // Background taskbar guardian - TÜM MONİTÖRLERDEKİ taskbar'ları sürekli gizli tut
    let _guardian_task = tokio::spawn(async move {
        let mut guardian_interval = time::interval(Duration::from_millis(200));
        loop {
            guardian_interval.tick().await;

            unsafe {
                use windows::Win32::UI::WindowsAndMessaging::*;

                // Ana taskbar'ı kontrol et
                if let Ok(taskbar_hwnd) = windows::Win32::UI::WindowsAndMessaging::FindWindowW(
                    windows::core::w!("Shell_TrayWnd"),
                    None,
                ) {
                    if IsWindowVisible(taskbar_hwnd).as_bool() {
                        println!("{{\"event_type\":\"auto_hide\",\"reason\":\"main_taskbar_became_visible\"}}");
                        let _ = ShowWindow(taskbar_hwnd, SW_HIDE);
                    }
                }

                // İkincil taskbar'ları kontrol et (çoklu monitör)
                let secondary_classes = [
                    windows::core::w!("Shell_SecondaryTrayWnd"),
                    windows::core::w!("WorkerW"),
                ];

                for class_name in &secondary_classes {
                    if let Ok(mut current_hwnd) =
                        windows::Win32::UI::WindowsAndMessaging::FindWindowW(*class_name, None)
                    {
                        loop {
                            if IsWindowVisible(current_hwnd).as_bool() {
                                println!("{{\"event_type\":\"auto_hide\",\"reason\":\"secondary_taskbar_became_visible\"}}");
                                let _ = ShowWindow(current_hwnd, SW_HIDE);
                            }

                            // Sonraki pencereyi ara
                            match FindWindowExW(None, current_hwnd, *class_name, None) {
                                Ok(next_hwnd) => current_hwnd = next_hwnd,
                                Err(_) => break,
                            }
                        }
                    }
                }
            }
        }
    });

    // Ctrl+C handler kurulumu
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let shutdown_flag_clone = shutdown_flag.clone();

    ctrlc::set_handler(move || {
        shutdown_flag_clone.store(true, Ordering::SeqCst);
    })?;

    // Ana event loop
    loop {
        let mut interval = time::interval(Duration::from_millis(100));

        tokio::select! {
            _ = interval.tick() => {
                if shutdown_flag.load(Ordering::SeqCst) {
                    break;
                }
                taskbar_manager.check_mouse_events();
            }
            _ = tokio::signal::ctrl_c() => {
                break;
            }
        }
    }

    // Program sonlanırken taskbar'ı geri göster
    match taskbar_manager.restore_taskbar() {
        Ok(_) => {}
        Err(_) => {
            emergency_taskbar_restore();
        }
    }

    Ok(())
}

fn print_event_json(event: &TaskbarEvent) {
    match serde_json::to_string(event) {
        Ok(json) => {
            println!("{}", json);
        }
        Err(e) => {
            error!("JSON serialization hatası: {}", e);
        }
    }
}
