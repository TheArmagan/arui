use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskbarEvent {
    pub event_type: String,
    pub timestamp: DateTime<Utc>,
    pub mouse_position: MousePosition,
    pub taskbar_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MousePosition {
    pub x: i32,
    pub y: i32,
}

pub struct TaskbarManager {
    taskbar_hwnd: HWND,
    is_hidden: bool,
    event_sender: mpsc::UnboundedSender<TaskbarEvent>,
    last_mouse_in_taskbar: bool,
}

impl TaskbarManager {
    pub fn new() -> std::result::Result<
        (Self, mpsc::UnboundedReceiver<TaskbarEvent>),
        Box<dyn std::error::Error>,
    > {
        let (event_sender, event_receiver) = mpsc::unbounded_channel();

        // Taskbar pencere handle'ını bul
        let taskbar_hwnd = unsafe { FindWindowW(w!("Shell_TrayWnd"), None).ok() };

        let hwnd = match taskbar_hwnd {
            Some(hwnd) => hwnd,
            None => return Err("Taskbar window not found".into()),
        };

        let manager = TaskbarManager {
            taskbar_hwnd: hwnd,
            is_hidden: false,
            event_sender,
            last_mouse_in_taskbar: false,
        };

        Ok((manager, event_receiver))
    }

    pub fn hide_taskbar(&mut self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::*;

            // Ana taskbar'ı gizle
            let _result = ShowWindow(self.taskbar_hwnd, SW_HIDE);

            // Ana taskbar'ın pozisyonunu ve boyutunu değiştir - AGRESİF YAKLAŞIM
            let _ = SetWindowPos(
                self.taskbar_hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_HIDEWINDOW,
            );

            // TÜM MONİTÖRLERDEKİ TASKBAR'LARI GİZLE
            // Secondary taskbar'lar (çoklu monitör sistemi)
            let secondary_taskbar_classes = [
                w!("Shell_SecondaryTrayWnd"), // Windows 10/11 ikincil taskbar
                w!("WorkerW"),                // Bazı durumlarda kullanılan
            ];

            for class_name in &secondary_taskbar_classes {
                // İlk pencereyi bul
                if let Ok(mut current_hwnd) = FindWindowW(*class_name, None) {
                    loop {
                        let _ = ShowWindow(current_hwnd, SW_HIDE);
                        let _ = SetWindowPos(
                            current_hwnd,
                            None,
                            0,
                            0,
                            0,
                            0,
                            SWP_NOZORDER | SWP_NOACTIVATE | SWP_HIDEWINDOW,
                        );

                        // Sonraki pencereyi ara
                        match FindWindowExW(None, current_hwnd, *class_name, None) {
                            Ok(next_hwnd) => current_hwnd = next_hwnd,
                            Err(_) => break, // Başka pencere yok
                        }
                    }
                }
            }

            // Çalışma alanını güncelle - AGRESİF APPBAR MESSAGE
            let _ = windows::Win32::UI::Shell::SHAppBarMessage(
                windows::Win32::UI::Shell::ABM_SETSTATE,
                &mut windows::Win32::UI::Shell::APPBARDATA {
                    cbSize: std::mem::size_of::<windows::Win32::UI::Shell::APPBARDATA>() as u32,
                    hWnd: self.taskbar_hwnd,
                    uCallbackMessage: 0,
                    uEdge: windows::Win32::UI::Shell::ABE_BOTTOM,
                    rc: windows::Win32::Foundation::RECT {
                        left: 0,
                        top: 0,
                        right: 0,
                        bottom: 0,
                    },
                    lParam: windows::Win32::Foundation::LPARAM(
                        windows::Win32::UI::Shell::ABS_AUTOHIDE as isize,
                    ),
                    ..Default::default()
                },
            );

            // FULL EKRAN İÇİN: Çalışma alanını tam ekran yap
            let screen_width = GetSystemMetrics(SM_CXSCREEN);
            let screen_height = GetSystemMetrics(SM_CYSCREEN);

            let mut full_work_area = windows::Win32::Foundation::RECT {
                left: 0,
                top: 0,
                right: screen_width,
                bottom: screen_height, // TAM EKRAN: taskbar yok, tüm alan kullanılabilir
            };

            let _ = SystemParametersInfoW(
                SPI_SETWORKAREA,
                0,
                Some(&mut full_work_area as *mut _ as *mut _),
                SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
            );

            self.is_hidden = true;

            // Event gönder
            let event = TaskbarEvent {
                event_type: "taskbar_hidden".to_string(),
                timestamp: Utc::now(),
                mouse_position: self.get_mouse_position(),
                taskbar_state: "hidden_aggressive_mode_all_monitors".to_string(),
            };

            let _ = self.event_sender.send(event);
            Ok(())
        }
    }

    pub fn show_taskbar(&mut self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        unsafe {
            let _result = ShowWindow(self.taskbar_hwnd, SW_SHOW);

            self.is_hidden = false;

            let event = TaskbarEvent {
                event_type: "taskbar_shown".to_string(),
                timestamp: Utc::now(),
                mouse_position: self.get_mouse_position(),
                taskbar_state: "visible".to_string(),
            };

            let _ = self.event_sender.send(event);
            Ok(())
        }
    }

    pub fn get_mouse_position(&self) -> MousePosition {
        unsafe {
            let mut point = POINT { x: 0, y: 0 };
            let _ = GetCursorPos(&mut point);
            MousePosition {
                x: point.x,
                y: point.y,
            }
        }
    }

    pub fn is_mouse_in_taskbar_area(&self) -> bool {
        let pos = self.get_mouse_position();
        let screen_height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        let taskbar_threshold = screen_height - 50; // Taskbar alanı eşiği

        pos.y >= taskbar_threshold
    }

    pub fn check_mouse_events(&mut self) {
        let is_in_taskbar = self.is_mouse_in_taskbar_area();

        // State değişikliklerini kontrol et
        if is_in_taskbar && !self.last_mouse_in_taskbar {
            // Mouse taskbar alanına girdi - SADECE LOG ET, taskbar'ı gösterme!
            let event = TaskbarEvent {
                event_type: "mouse_request_show".to_string(),
                timestamp: Utc::now(),
                mouse_position: self.get_mouse_position(),
                taskbar_state: "hidden_show_requested".to_string(),
            };

            let _ = self.event_sender.send(event);
        } else if !is_in_taskbar && self.last_mouse_in_taskbar {
            // Mouse taskbar alanından çıktı
            let event = TaskbarEvent {
                event_type: "mouse_request_hide".to_string(),
                timestamp: Utc::now(),
                mouse_position: self.get_mouse_position(),
                taskbar_state: "hidden_maintained".to_string(),
            };

            let _ = self.event_sender.send(event);
        }

        self.last_mouse_in_taskbar = is_in_taskbar;
    }

    /// Acil durum için manuel taskbar restore fonksiyonu
    pub fn emergency_restore_taskbar() -> std::result::Result<(), Box<dyn std::error::Error>> {
        // EXPLORER RESTART - En etkili emergency restore
        std::process::Command::new("taskkill")
            .args(&["/F", "/IM", "explorer.exe"])
            .output()?;

        std::thread::sleep(std::time::Duration::from_millis(500));

        std::process::Command::new("explorer.exe").spawn()?;

        Ok(())
    }

    /// Güvenli şekilde taskbar'ı restore et
    pub fn restore_taskbar(&mut self) -> std::result::Result<(), Box<dyn std::error::Error>> {
        if self.is_hidden {
            // EXPLORER RESTART - En etkili çözüm
            std::process::Command::new("taskkill")
                .args(&["/F", "/IM", "explorer.exe"])
                .output()?;

            std::thread::sleep(std::time::Duration::from_millis(500));

            std::process::Command::new("explorer.exe").spawn()?;

            self.is_hidden = false;

            let event = TaskbarEvent {
                event_type: "taskbar_restored".to_string(),
                timestamp: Utc::now(),
                mouse_position: self.get_mouse_position(),
                taskbar_state: "restored_via_explorer_restart".to_string(),
            };

            let _ = self.event_sender.send(event);
        }
        Ok(())
    }
}

impl Drop for TaskbarManager {
    fn drop(&mut self) {
        // EXPLORER RESTART - En basit ve etkili cleanup
        if self.is_hidden {
            let _ = std::process::Command::new("taskkill")
                .args(&["/F", "/IM", "explorer.exe"])
                .output();

            std::thread::sleep(std::time::Duration::from_millis(500));

            let _ = std::process::Command::new("explorer.exe").spawn();

            self.is_hidden = false;
        }
    }
}
