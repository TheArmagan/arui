use base64::{engine::general_purpose, Engine as _};
use clap::{Parser, Subcommand};
use image::{ImageBuffer, RgbaImage};
use lnk::ShellLink;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Cursor;
use std::os::windows::ffi::OsStringExt;
use std::path::Path;
use std::ptr::null_mut;
use tokio::time::{sleep, Duration};
use winapi::shared::windef::HWND;
use winapi::um::handleapi::CloseHandle;
use winapi::um::processthreadsapi::OpenProcess;
use winapi::um::psapi::{GetModuleBaseNameW, GetModuleFileNameExW};
use winapi::um::shellapi::{
    ExtractIconW, SHGetFileInfoW, ShellExecuteW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON,
};
use winapi::um::wingdi::*;
use winapi::um::winuser::*;

#[derive(Parser)]
#[command(name = "win-taskbar-item-list")]
#[command(about = "Windows Taskbar Item Monitor")]
struct Cli {
    #[command(subcommand)]
    action: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Get icon for a specific HWND as base64 PNG
    GetHwndIcon {
        /// Window handle (HWND) as integer
        #[arg(long)]
        hwnd: i32,
    },
    /// Get icon for a specific executable path as base64 PNG
    GetExecutableIcon {
        /// Path to the executable file
        #[arg(long)]
        path: String,
    },
    /// Get window screenshot as base64 PNG (max 256x256)
    GetWindowScreenshot {
        /// Window handle (HWND) as integer
        #[arg(long)]
        hwnd: i32,
        /// Maximum size as WIDTHxHEIGHT (e.g., 512x512)
        #[arg(long, default_value = "256x256")]
        size: String,
    },
    /// Minimize a window by HWND
    MinimizeWindow {
        /// Window handle (HWND) as integer
        #[arg(long)]
        hwnd: i32,
    },
    /// Maximize a window by HWND
    MaximizeWindow {
        /// Window handle (HWND) as integer
        #[arg(long)]
        hwnd: i32,
    },
    /// Restore a window by HWND (from minimized/maximized state)
    RestoreWindow {
        /// Window handle (HWND) as integer
        #[arg(long)]
        hwnd: i32,
    },
    /// Close a window by HWND
    CloseWindow {
        /// Window handle (HWND) as integer
        #[arg(long)]
        hwnd: i32,
    },
    /// Focus a window by HWND (bring to foreground)
    FocusWindow {
        /// Window handle (HWND) as integer
        #[arg(long)]
        hwnd: i32,
    },
    /// Unfocus a window by HWND (send to background)
    UnfocusWindow {
        /// Window handle (HWND) as integer
        #[arg(long)]
        hwnd: i32,
    },
    /// Toggle focus state of a window by HWND
    ToggleFocusWindow {
        /// Window handle (HWND) as integer
        #[arg(long)]
        hwnd: i32,
    },
    /// Start an executable file
    StartExecutable {
        /// Path to the executable file to start
        #[arg(long)]
        path: String,
    },
    /// Monitor taskbar items (default action)
    Monitor,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct TaskbarItem {
    title: String,
    process_name: String,
    process_id: u32,
    hwnd: i32,
    is_visible: bool,
    is_minimized: bool,
    is_maximized: bool,
    class_name: String,
    has_taskbar_button: bool,
    window_state: String, // "normal", "minimized", "maximized", "hidden"
    is_pinned: bool,
    executable_path: String,
    item_type: String, // "running", "pinned", "both"
    is_tray_icon: bool,
    is_focused: bool, // Pencere şu anda odakta mı
    is_running: bool, // Uygulama şu anda çalışıyor mu (process_id > 0)
    // Yeni filtreleme için özel alanlar
    is_definitely_taskbar: bool, // Kesin olarak taskbar'da görünen
    is_definitely_tray: bool,    // Kesin olarak system tray'de olan
    is_system_window: bool,      // Sistem penceresi (Windows Explorer, etc.)
    display_location: String,    // "taskbar", "tray", "both", "hidden"
}

#[derive(Serialize, Deserialize)]
struct TaskbarUpdate {
    action: String, // "added", "removed", "updated"
    items: Vec<TaskbarItem>,
    timestamp: u64,
}

struct TaskbarMonitor;

impl TaskbarMonitor {
    fn new() -> Self {
        Self
    }

    fn get_window_icon_as_base64(hwnd: i32) -> Option<String> {
        unsafe {
            let hwnd = hwnd as HWND;

            // Büyük icon'u al
            let mut hicon = SendMessageW(hwnd, WM_GETICON, ICON_BIG as usize, 0)
                as winapi::shared::windef::HICON;

            // Eğer büyük icon yoksa küçük icon'u dene
            if hicon.is_null() {
                hicon = SendMessageW(hwnd, WM_GETICON, ICON_SMALL as usize, 0)
                    as winapi::shared::windef::HICON;
            }

            // Hala icon yoksa class icon'unu dene
            if hicon.is_null() {
                hicon = GetClassLongPtrW(hwnd, GCLP_HICON) as winapi::shared::windef::HICON;
            }

            // Son çare olarak küçük class icon'unu dene
            if hicon.is_null() {
                hicon = GetClassLongPtrW(hwnd, GCLP_HICONSM) as winapi::shared::windef::HICON;
            }

            if hicon.is_null() {
                return None;
            }

            // Icon bilgilerini al
            let mut icon_info: ICONINFO = std::mem::zeroed();
            if GetIconInfo(hicon, &mut icon_info) == 0 {
                return None;
            }

            // Bitmap'i device context'e çevir
            let hdc = GetDC(null_mut());
            let hdc_mem = CreateCompatibleDC(hdc);

            // Bitmap boyutlarını al
            let mut bitmap: BITMAP = std::mem::zeroed();
            GetObjectW(
                icon_info.hbmColor as *mut _,
                std::mem::size_of::<BITMAP>() as i32,
                &mut bitmap as *mut _ as *mut _,
            );

            let _width = bitmap.bmWidth;
            let _height = bitmap.bmHeight;

            // 32x32 boyutunda yeni bir bitmap oluştur
            let target_width = 32;
            let target_height = 32;

            let hdc_target = CreateCompatibleDC(hdc);
            let hbitmap_target = CreateCompatibleBitmap(hdc, target_width, target_height);
            let old_bitmap = SelectObject(hdc_target, hbitmap_target as *mut _);

            // Icon'u çiz
            DrawIconEx(
                hdc_target,
                0,
                0,
                hicon,
                target_width,
                target_height,
                0,
                null_mut(),
                0x0003,
            ); // DI_NORMAL | DI_COMPAT

            // Bitmap verisini al
            let mut bmi: BITMAPINFOHEADER = std::mem::zeroed();
            bmi.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bmi.biWidth = target_width;
            bmi.biHeight = -target_height; // Top-down DIB
            bmi.biPlanes = 1;
            bmi.biBitCount = 32;
            bmi.biCompression = BI_RGB;

            let mut buffer: Vec<u8> = vec![0; (target_width * target_height * 4) as usize];

            if GetDIBits(
                hdc_target,
                hbitmap_target,
                0,
                target_height as u32,
                buffer.as_mut_ptr() as *mut _,
                &bmi as *const _ as *mut _,
                DIB_RGB_COLORS,
            ) != 0
            {
                // BGRA'dan RGBA'ya çevir ve PNG formatında encode et
                for i in (0..buffer.len()).step_by(4) {
                    buffer.swap(i, i + 2); // B ve R'yi swap et
                }

                // ImageBuffer oluştur ve PNG'ye çevir
                if let Some(img_buffer) = ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(
                    target_width as u32,
                    target_height as u32,
                    buffer,
                ) {
                    let mut png_data = Vec::new();
                    {
                        let mut cursor = Cursor::new(&mut png_data);
                        if img_buffer
                            .write_to(&mut cursor, image::ImageOutputFormat::Png)
                            .is_ok()
                        {
                            let base64_string = general_purpose::STANDARD.encode(&png_data);

                            // Cleanup
                            SelectObject(hdc_target, old_bitmap);
                            DeleteObject(hbitmap_target as *mut _);
                            DeleteDC(hdc_target);
                            DeleteDC(hdc_mem);
                            ReleaseDC(null_mut(), hdc);
                            DeleteObject(icon_info.hbmColor as *mut _);
                            DeleteObject(icon_info.hbmMask as *mut _);

                            return Some(base64_string);
                        }
                    }
                }

                // Cleanup
                SelectObject(hdc_target, old_bitmap);
                DeleteObject(hbitmap_target as *mut _);
                DeleteDC(hdc_target);
                DeleteDC(hdc_mem);
                ReleaseDC(null_mut(), hdc);
                DeleteObject(icon_info.hbmColor as *mut _);
                DeleteObject(icon_info.hbmMask as *mut _);

                None
            } else {
                // Cleanup on failure
                SelectObject(hdc_target, old_bitmap);
                DeleteObject(hbitmap_target as *mut _);
                DeleteDC(hdc_target);
                DeleteDC(hdc_mem);
                ReleaseDC(null_mut(), hdc);
                DeleteObject(icon_info.hbmColor as *mut _);
                DeleteObject(icon_info.hbmMask as *mut _);

                None
            }
        }
    }

    fn get_window_screenshot_as_base64(hwnd: i32, size_str: &str) -> Option<String> {
        unsafe {
            let hwnd = hwnd as HWND;

            // Size string'ini parse et (örneğin "512x512")
            let (max_width, max_height) = if let Some((w_str, h_str)) = size_str.split_once('x') {
                match (w_str.parse::<i32>(), h_str.parse::<i32>()) {
                    (Ok(w), Ok(h)) if w > 0 && h > 0 => (w, h),
                    _ => (256, 256), // Geçersiz format, varsayılan değer
                }
            } else {
                (256, 256) // Geçersiz format, varsayılan değer
            };

            // Pencere boyutlarını al
            let mut rect = std::mem::zeroed::<winapi::shared::windef::RECT>();
            if GetWindowRect(hwnd, &mut rect) == 0 {
                return None;
            }

            let window_width = rect.right - rect.left;
            let window_height = rect.bottom - rect.top;

            if window_width <= 0 || window_height <= 0 {
                return None;
            }

            // Maksimum boyutları belirle
            let (target_width, target_height) = if window_width > window_height {
                let ratio = max_width as f32 / window_width as f32;
                let new_height = (window_height as f32 * ratio) as i32;
                if new_height > max_height {
                    let ratio = max_height as f32 / window_height as f32;
                    ((window_width as f32 * ratio) as i32, max_height)
                } else {
                    (max_width, new_height)
                }
            } else {
                let ratio = max_height as f32 / window_height as f32;
                let new_width = (window_width as f32 * ratio) as i32;
                if new_width > max_width {
                    let ratio = max_width as f32 / window_width as f32;
                    (max_width, (window_height as f32 * ratio) as i32)
                } else {
                    (new_width, max_height)
                }
            };

            // Device context'ler oluştur
            let hdc_screen = GetDC(null_mut());
            let hdc_window = GetDC(hwnd);
            let hdc_mem = CreateCompatibleDC(hdc_screen);
            let hdc_scaled = CreateCompatibleDC(hdc_screen);

            // Bitmap'ler oluştur
            let hbitmap = CreateCompatibleBitmap(hdc_screen, window_width, window_height);
            let hbitmap_scaled = CreateCompatibleBitmap(hdc_screen, target_width, target_height);

            let old_bitmap = SelectObject(hdc_mem, hbitmap as *mut _);
            let old_bitmap_scaled = SelectObject(hdc_scaled, hbitmap_scaled as *mut _);

            // Pencereyi çiz
            if PrintWindow(hwnd, hdc_mem, 0x00000002) != 0 {
                // PW_RENDERFULLCONTENT
                // Boyutlandır
                SetStretchBltMode(hdc_scaled, 4); // HALFTONE
                StretchBlt(
                    hdc_scaled,
                    0,
                    0,
                    target_width,
                    target_height,
                    hdc_mem,
                    0,
                    0,
                    window_width,
                    window_height,
                    0x00CC0020, // SRCCOPY
                );

                // Bitmap verisini al
                let mut bmi: BITMAPINFOHEADER = std::mem::zeroed();
                bmi.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
                bmi.biWidth = target_width;
                bmi.biHeight = -target_height; // Top-down DIB
                bmi.biPlanes = 1;
                bmi.biBitCount = 32;
                bmi.biCompression = BI_RGB;

                let mut buffer: Vec<u8> = vec![0; (target_width * target_height * 4) as usize];

                if GetDIBits(
                    hdc_scaled,
                    hbitmap_scaled,
                    0,
                    target_height as u32,
                    buffer.as_mut_ptr() as *mut _,
                    &bmi as *const _ as *mut _,
                    DIB_RGB_COLORS,
                ) != 0
                {
                    // BGRA'dan RGBA'ya çevir ve PNG formatında encode et
                    for i in (0..buffer.len()).step_by(4) {
                        buffer.swap(i, i + 2); // B ve R'yi swap et
                        buffer[i + 3] = 255; // Alpha kanalını opaque yap
                    }

                    // ImageBuffer oluştur ve PNG'ye çevir
                    if let Some(img_buffer) = ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(
                        target_width as u32,
                        target_height as u32,
                        buffer,
                    ) {
                        let mut png_data = Vec::new();
                        {
                            let mut cursor = Cursor::new(&mut png_data);
                            if img_buffer
                                .write_to(&mut cursor, image::ImageOutputFormat::Png)
                                .is_ok()
                            {
                                let base64_string = general_purpose::STANDARD.encode(&png_data);

                                // Cleanup
                                SelectObject(hdc_mem, old_bitmap);
                                SelectObject(hdc_scaled, old_bitmap_scaled);
                                DeleteObject(hbitmap as *mut _);
                                DeleteObject(hbitmap_scaled as *mut _);
                                DeleteDC(hdc_mem);
                                DeleteDC(hdc_scaled);
                                ReleaseDC(hwnd, hdc_window);
                                ReleaseDC(null_mut(), hdc_screen);

                                return Some(base64_string);
                            }
                        }
                    }
                }
            }

            // Cleanup on failure
            SelectObject(hdc_mem, old_bitmap);
            SelectObject(hdc_scaled, old_bitmap_scaled);
            DeleteObject(hbitmap as *mut _);
            DeleteObject(hbitmap_scaled as *mut _);
            DeleteDC(hdc_mem);
            DeleteDC(hdc_scaled);
            ReleaseDC(hwnd, hdc_window);
            ReleaseDC(null_mut(), hdc_screen);

            None
        }
    }

    fn get_executable_icon_as_base64(executable_path: &str) -> Option<String> {
        unsafe {
            // Executable dosyasından icon çıkar
            let path_wide: Vec<u16> = executable_path
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            // İlk olarak SHGetFileInfoW ile dene (daha güvenilir)
            let mut shfi: SHFILEINFOW = std::mem::zeroed();
            let result = SHGetFileInfoW(
                path_wide.as_ptr(),
                0,
                &mut shfi,
                std::mem::size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_LARGEICON,
            );

            let hicon = if result != 0 && !shfi.hIcon.is_null() {
                shfi.hIcon
            } else {
                // SHGetFileInfoW başarısız olduysa ExtractIconW dene (UWP uygulamaları için daha iyi)
                let hicon = ExtractIconW(
                    null_mut(),
                    path_wide.as_ptr(),
                    0, // İlk icon'u al
                );

                if hicon.is_null() || hicon as isize == 1 {
                    // ExtractIconW başarısız olduysa LoadImageW dene
                    let hicon = LoadImageW(
                        null_mut(),
                        path_wide.as_ptr(),
                        IMAGE_ICON,
                        32,
                        32,
                        LR_LOADFROMFILE,
                    ) as winapi::shared::windef::HICON;

                    if hicon.is_null() {
                        return None;
                    }
                    hicon
                } else {
                    hicon
                }
            };

            // Icon bilgilerini al
            let mut icon_info: ICONINFO = std::mem::zeroed();
            if GetIconInfo(hicon, &mut icon_info) == 0 {
                // Cleanup
                DestroyIcon(hicon);
                return None;
            }

            // Bitmap'i device context'e çevir
            let hdc = GetDC(null_mut());
            let hdc_mem = CreateCompatibleDC(hdc);

            // Bitmap boyutlarını al
            let mut bitmap: BITMAP = std::mem::zeroed();
            GetObjectW(
                icon_info.hbmColor as *mut _,
                std::mem::size_of::<BITMAP>() as i32,
                &mut bitmap as *mut _ as *mut _,
            );

            let _width = bitmap.bmWidth;
            let _height = bitmap.bmHeight;

            // 32x32 boyutunda yeni bir bitmap oluştur
            let target_width = 32;
            let target_height = 32;

            let hdc_target = CreateCompatibleDC(hdc);
            let hbitmap_target = CreateCompatibleBitmap(hdc, target_width, target_height);
            let old_bitmap = SelectObject(hdc_target, hbitmap_target as *mut _);

            // Icon'u çiz
            DrawIconEx(
                hdc_target,
                0,
                0,
                hicon,
                target_width,
                target_height,
                0,
                null_mut(),
                0x0003,
            ); // DI_NORMAL | DI_COMPAT

            // Bitmap verisini al
            let mut bmi: BITMAPINFOHEADER = std::mem::zeroed();
            bmi.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bmi.biWidth = target_width;
            bmi.biHeight = -target_height; // Top-down DIB
            bmi.biPlanes = 1;
            bmi.biBitCount = 32;
            bmi.biCompression = BI_RGB;

            let mut buffer: Vec<u8> = vec![0; (target_width * target_height * 4) as usize];

            if GetDIBits(
                hdc_target,
                hbitmap_target,
                0,
                target_height as u32,
                buffer.as_mut_ptr() as *mut _,
                &bmi as *const _ as *mut _,
                DIB_RGB_COLORS,
            ) != 0
            {
                // BGRA'dan RGBA'ya çevir ve PNG formatında encode et
                for i in (0..buffer.len()).step_by(4) {
                    buffer.swap(i, i + 2); // B ve R'yi swap et
                }

                // ImageBuffer oluştur ve PNG'ye çevir
                if let Some(img_buffer) = ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(
                    target_width as u32,
                    target_height as u32,
                    buffer,
                ) {
                    let mut png_data = Vec::new();
                    {
                        let mut cursor = Cursor::new(&mut png_data);
                        if img_buffer
                            .write_to(&mut cursor, image::ImageOutputFormat::Png)
                            .is_ok()
                        {
                            let base64_string = general_purpose::STANDARD.encode(&png_data);

                            // Cleanup
                            SelectObject(hdc_target, old_bitmap);
                            DeleteObject(hbitmap_target as *mut _);
                            DeleteDC(hdc_target);
                            DeleteDC(hdc_mem);
                            ReleaseDC(null_mut(), hdc);
                            DeleteObject(icon_info.hbmColor as *mut _);
                            DeleteObject(icon_info.hbmMask as *mut _);
                            DestroyIcon(hicon);

                            return Some(base64_string);
                        }
                    }
                }

                // Cleanup
                SelectObject(hdc_target, old_bitmap);
                DeleteObject(hbitmap_target as *mut _);
                DeleteDC(hdc_target);
                DeleteDC(hdc_mem);
                ReleaseDC(null_mut(), hdc);
                DeleteObject(icon_info.hbmColor as *mut _);
                DeleteObject(icon_info.hbmMask as *mut _);
                DestroyIcon(hicon);

                None
            } else {
                // Cleanup on failure
                SelectObject(hdc_target, old_bitmap);
                DeleteObject(hbitmap_target as *mut _);
                DeleteDC(hdc_target);
                DeleteDC(hdc_mem);
                ReleaseDC(null_mut(), hdc);
                DeleteObject(icon_info.hbmColor as *mut _);
                DeleteObject(icon_info.hbmMask as *mut _);
                DestroyIcon(hicon);

                None
            }
        }
    }

    fn minimize_window(hwnd: i32) -> bool {
        unsafe {
            let hwnd = hwnd as HWND;
            ShowWindow(hwnd, SW_MINIMIZE) != 0
        }
    }

    fn maximize_window(hwnd: i32) -> bool {
        unsafe {
            let hwnd = hwnd as HWND;
            ShowWindow(hwnd, SW_MAXIMIZE) != 0
        }
    }

    fn restore_window(hwnd: i32) -> bool {
        unsafe {
            let hwnd = hwnd as HWND;
            ShowWindow(hwnd, SW_RESTORE) != 0
        }
    }

    fn close_window(hwnd: i32) -> bool {
        unsafe {
            let hwnd = hwnd as HWND;
            // WM_CLOSE mesajı gönder (graceful close)
            PostMessageW(hwnd, WM_CLOSE, 0, 0) != 0
        }
    }

    fn focus_window(hwnd: i32) -> bool {
        unsafe {
            let hwnd = hwnd as HWND;
            // Pencereyi önplana getir
            if SetForegroundWindow(hwnd) != 0 {
                // Eğer minimize edilmişse restore et
                if IsIconic(hwnd) != 0 {
                    ShowWindow(hwnd, SW_RESTORE);
                }
                return true;
            }
            false
        }
    }

    fn unfocus_window(hwnd: i32) -> bool {
        unsafe {
            let hwnd = hwnd as HWND;
            let current_foreground = GetForegroundWindow();

            // Eğer bu pencere şu anda focus'taysa, bir sonraki pencereye geç
            if hwnd == current_foreground {
                // Alt+Tab benzeri davranış için
                keybd_event(0x12, 0, 0, 0); // Alt tuşunu bas
                keybd_event(0x09, 0, 0, 0); // Tab tuşunu bas
                keybd_event(0x09, 0, 0x02, 0); // Tab tuşunu bırak
                keybd_event(0x12, 0, 0x02, 0); // Alt tuşunu bırak
                return true;
            }
            false
        }
    }

    fn toggle_focus_window(hwnd: i32) -> bool {
        unsafe {
            let hwnd = hwnd as HWND;
            let current_foreground = GetForegroundWindow();

            if hwnd == current_foreground {
                // Şu anda focus'ta, unfocus et
                Self::unfocus_window(hwnd as i32)
            } else {
                // Focus'ta değil, focus et
                Self::focus_window(hwnd as i32)
            }
        }
    }

    fn start_executable(executable_path: &str) -> bool {
        unsafe {
            // Executable path'i wide string'e çevir
            let path_wide: Vec<u16> = executable_path
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            // ShellExecuteW kullanarak executable'ı başlat
            let result = ShellExecuteW(
                null_mut(),         // hwnd
                std::ptr::null(),   // lpOperation (default: "open")
                path_wide.as_ptr(), // lpFile
                std::ptr::null(),   // lpParameters
                std::ptr::null(),   // lpDirectory
                1,                  // nShowCmd (SW_SHOWNORMAL)
            );

            // ShellExecuteW 32'den büyük bir değer döndürürse başarılı
            result as isize > 32
        }
    }

    fn get_executable_path(process_id: u32) -> String {
        unsafe {
            let handle = OpenProcess(0x0400 | 0x1000, 0, process_id); // PROCESS_QUERY_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION
            if handle.is_null() {
                return String::new();
            }

            let mut buffer: [u16; 260] = [0; 260];

            if GetModuleFileNameExW(handle, null_mut(), buffer.as_mut_ptr(), buffer.len() as u32)
                > 0
            {
                let slice = &buffer[..buffer.iter().position(|&x| x == 0).unwrap_or(buffer.len())];
                let result = OsString::from_wide(slice).to_string_lossy().into_owned();
                CloseHandle(handle);
                result
            } else {
                CloseHandle(handle);
                String::new()
            }
        }
    }

    fn get_pinned_items() -> Vec<(String, String)> {
        // Taskbar pinned items'ları genellikle şu yolda bulunur:
        // %APPDATA%\Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar
        let mut pinned_items = Vec::new();

        if let Ok(appdata) = std::env::var("APPDATA") {
            let taskbar_path = format!(
                "{}\\Microsoft\\Internet Explorer\\Quick Launch\\User Pinned\\TaskBar",
                appdata
            );

            if let Ok(entries) = std::fs::read_dir(&taskbar_path) {
                for entry in entries.flatten() {
                    if let Some(extension) = entry.path().extension() {
                        if extension == "lnk" {
                            if let Some(name) = entry.path().file_stem() {
                                let display_name = name.to_string_lossy().to_string();

                                // Shortcut dosyasını oku ve target path'ini al
                                let executable_path = match ShellLink::open(&entry.path()) {
                                    Ok(shell_link) => {
                                        if let Some(link_info) = shell_link.link_info() {
                                            if let Some(path) = link_info.local_base_path() {
                                                path.clone()
                                            } else {
                                                String::new()
                                            }
                                        } else {
                                            String::new()
                                        }
                                    }
                                    Err(_) => String::new(),
                                };

                                pinned_items.push((display_name, executable_path));
                            }
                        }
                    }
                }
            }
        }

        pinned_items
    }

    fn is_pinned_app(process_name: &str, executable_path: &str) -> bool {
        let pinned_items = Self::get_pinned_items();

        // Process name ile kontrol et
        if pinned_items.iter().any(|(item_name, _)| {
            item_name
                .to_lowercase()
                .contains(&process_name.to_lowercase().replace(".exe", ""))
        }) {
            return true;
        }

        // Executable path'in filename'i ile kontrol et
        if let Some(filename) = Path::new(executable_path).file_stem() {
            let filename_str = filename.to_string_lossy().to_lowercase();
            if pinned_items
                .iter()
                .any(|(item_name, _)| item_name.to_lowercase().contains(&filename_str))
            {
                return true;
            }
        }

        false
    }

    fn get_window_text(hwnd: HWND) -> String {
        unsafe {
            let length = GetWindowTextLengthW(hwnd);
            if length == 0 {
                return String::new();
            }

            let mut buffer: Vec<u16> = vec![0; (length + 1) as usize];
            let result = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
            if result > 0 {
                buffer.truncate(result as usize);
                OsString::from_wide(&buffer).to_string_lossy().into_owned()
            } else {
                String::new()
            }
        }
    }

    fn get_class_name(hwnd: HWND) -> String {
        unsafe {
            let mut buffer: [u16; 256] = [0; 256];
            let result = GetClassNameW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
            if result > 0 {
                let slice = &buffer[..result as usize];
                OsString::from_wide(slice).to_string_lossy().into_owned()
            } else {
                String::new()
            }
        }
    }

    fn get_process_name(process_id: u32) -> String {
        unsafe {
            let handle = OpenProcess(0x0400 | 0x0010, 0, process_id); // PROCESS_QUERY_INFORMATION | PROCESS_VM_READ
            if handle.is_null() {
                return String::new();
            }

            let mut buffer: [u16; 260] = [0; 260];

            if GetModuleBaseNameW(handle, null_mut(), buffer.as_mut_ptr(), buffer.len() as u32) > 0
            {
                let slice = &buffer[..buffer.iter().position(|&x| x == 0).unwrap_or(buffer.len())];
                let result = OsString::from_wide(slice).to_string_lossy().into_owned();
                CloseHandle(handle);
                result
            } else {
                CloseHandle(handle);
                String::new()
            }
        }
    }

    fn is_taskbar_window(hwnd: HWND) -> bool {
        unsafe {
            // Ana pencere olmalı (parent window olmamalı)
            let parent = GetParent(hwnd);
            if !parent.is_null() {
                return false;
            }

            // WS_CAPTION veya WS_VISIBLE style'ı olmalı (minimize edilmiş pencereler için)
            let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
            let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;

            // Tray icon kontrolü - bunları da dahil edelim
            let is_tray_icon = (ex_style & WS_EX_TOOLWINDOW) != 0;

            // Normal taskbar window kontrolü
            let is_normal_window =
                (style & (WS_CAPTION | WS_VISIBLE)) != 0 && (ex_style & WS_EX_TOOLWINDOW) == 0;

            // Ya normal window ya da tray icon olmalı
            if !is_normal_window && !is_tray_icon {
                return false;
            }

            // Taskbar'da görünmemesi gereken sistem pencereleri
            let class_name = Self::get_class_name(hwnd);
            match class_name.as_str() {
                "Shell_TrayWnd"
                | "Shell_SecondaryTrayWnd"
                | "DV2ControlHost"
                | "MsgrIMEWindowClass"
                | "SysShadow"
                | "Button"
                | "Progman"
                | "WorkerW"
                | "Desktop" => false,
                _ => true,
            }
        }
    }

    fn get_current_taskbar_items(&self) -> Vec<TaskbarItem> {
        let mut items = Vec::new();

        // Çalışan pencerelerden taskbar item'larını al
        unsafe {
            EnumWindows(
                Some(enum_windows_proc),
                &mut items as *mut Vec<TaskbarItem> as isize,
            );
        }

        // Pinned item'ları da ekle (şu anda çalışmayan olanlar)
        let pinned_items = Self::get_pinned_items();
        let running_processes: HashMap<String, bool> = items
            .iter()
            .map(|item: &TaskbarItem| (item.process_name.to_lowercase().replace(".exe", ""), true))
            .collect();

        for (pinned_name, pinned_executable_path) in pinned_items {
            let pinned_name_lower = pinned_name.to_lowercase();

            // Eğer bu pinned item şu anda çalışmıyorsa, sadece pinned olarak ekle
            if !running_processes.contains_key(&pinned_name_lower) {
                let item = TaskbarItem {
                    title: pinned_name.clone(),
                    process_name: format!("{}.exe", pinned_name),
                    process_id: 0,
                    hwnd: 0,
                    is_visible: false,
                    is_minimized: false,
                    is_maximized: false,
                    class_name: String::new(),
                    has_taskbar_button: true,
                    window_state: "pinned_only".to_string(),
                    is_pinned: true,
                    executable_path: pinned_executable_path,
                    item_type: "pinned".to_string(),
                    is_tray_icon: false,
                    is_focused: false, // Pinned item'lar focused olamazlar (çalışmadıkları için)
                    is_running: false, // Pinned-only item'lar çalışmıyor
                    is_definitely_taskbar: true,
                    is_definitely_tray: false,
                    is_system_window: false,
                    display_location: "taskbar".to_string(),
                };
                items.push(item);
            }
        }

        // Itemları tutarlı bir şekilde sırala
        items.sort_by(|a, b| {
            // Önce çalışan/pinned durumuna göre sırala
            match (a.is_running, b.is_running) {
                (true, false) => std::cmp::Ordering::Less, // Çalışan itemlar önce
                (false, true) => std::cmp::Ordering::Greater, // Pinned itemlar sonra
                _ => {
                    // Aynı durumdaysa (ikisi de çalışıyor veya ikisi de pinned)
                    // Process name'e göre alfabetik sırala
                    a.process_name
                        .to_lowercase()
                        .cmp(&b.process_name.to_lowercase())
                }
            }
        });

        items
    }

    async fn monitor_loop(&mut self) {
        loop {
            let current_items = self.get_current_taskbar_items();

            // Tüm mevcut taskbar öğelerini listele
            let update = TaskbarUpdate {
                action: "list".to_string(),
                items: current_items,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            };
            println!("{}", serde_json::to_string(&update).unwrap());

            sleep(Duration::from_millis(500)).await;
        }
    }
}

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: isize) -> i32 {
    let items = &mut *(lparam as *mut Vec<TaskbarItem>);

    if TaskbarMonitor::is_taskbar_window(hwnd) {
        let mut process_id = 0u32;
        GetWindowThreadProcessId(hwnd, &mut process_id);

        let title = TaskbarMonitor::get_window_text(hwnd);
        let process_name = TaskbarMonitor::get_process_name(process_id);
        let class_name = TaskbarMonitor::get_class_name(hwnd);
        let executable_path = TaskbarMonitor::get_executable_path(process_id);

        // En azından process name'i olmalı
        if !process_name.is_empty() {
            let is_minimized = IsIconic(hwnd) != 0;
            let is_visible = IsWindowVisible(hwnd) != 0;
            let is_maximized = IsZoomed(hwnd) != 0;

            // Window placement bilgisi al
            let mut placement = std::mem::zeroed::<WINDOWPLACEMENT>();
            placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
            GetWindowPlacement(hwnd, &mut placement);

            let window_state = if is_minimized {
                "minimized".to_string()
            } else if is_maximized {
                "maximized".to_string()
            } else if is_visible {
                "normal".to_string()
            } else {
                "hidden".to_string()
            };

            // Taskbar button olup olmadığını kontrol et
            let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
            let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
            let has_taskbar_button =
                (ex_style & WS_EX_TOOLWINDOW) == 0 && (style & WS_CAPTION) != 0;

            // Pinned olup olmadığını kontrol et
            let is_pinned = TaskbarMonitor::is_pinned_app(&process_name, &executable_path);

            // Tray icon olup olmadığını kontrol et
            let is_tray_icon = (ex_style & WS_EX_TOOLWINDOW) != 0
                || class_name.contains("NotifyIcon")
                || class_name.contains("TrayNotify")
                || class_name == "tooltips_class32"
                || (!is_visible && !is_minimized && process_id > 0);

            // Kesin filtreleme için yeni alanlar
            let is_definitely_taskbar = has_taskbar_button
                && (is_visible || is_minimized)
                && (ex_style & WS_EX_TOOLWINDOW) == 0
                && !class_name.contains("NotifyIcon");

            let is_definitely_tray = (ex_style & WS_EX_TOOLWINDOW) != 0
                || class_name.contains("NotifyIcon")
                || class_name.contains("TrayNotify")
                || (!has_taskbar_button && !title.is_empty());

            // Sistem penceresi kontrolü
            let is_system_window = match process_name.to_lowercase().as_str() {
                "explorer.exe" | "dwm.exe" | "winlogon.exe" | "csrss.exe" | "wininit.exe"
                | "services.exe" | "lsass.exe" | "svchost.exe" => true,
                _ => false,
            };

            // Display location belirleme
            let display_location = if is_definitely_taskbar && is_definitely_tray {
                "both".to_string()
            } else if is_definitely_taskbar {
                "taskbar".to_string()
            } else if is_definitely_tray {
                "tray".to_string()
            } else {
                "hidden".to_string()
            };

            let item_type = if is_pinned {
                "both".to_string() // Hem running hem pinned
            } else {
                "running".to_string()
            };

            // Pencere focus durumunu kontrol et
            let foreground_window = GetForegroundWindow();
            let is_focused = hwnd == foreground_window;

            let item = TaskbarItem {
                title,
                process_name,
                process_id,
                hwnd: hwnd as i32,
                is_visible,
                is_minimized,
                is_maximized,
                class_name,
                has_taskbar_button,
                window_state,
                is_pinned,
                executable_path,
                item_type,
                is_tray_icon,
                is_focused,
                is_running: process_id > 0, // Process ID varsa çalışıyor
                is_definitely_taskbar,
                is_definitely_tray,
                is_system_window,
                display_location,
            };

            items.push(item);
        }
    }

    1 // TRUE
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.action {
        Some(Commands::GetHwndIcon { hwnd }) => {
            // Icon alma modu
            match TaskbarMonitor::get_window_icon_as_base64(hwnd) {
                Some(base64_icon) => {
                    let response = serde_json::json!({
                        "success": true,
                        "hwnd": hwnd,
                        "icon_base64": base64_icon,
                        "format": "PNG"
                    });
                    println!("{}", response);
                }
                None => {
                    let response = serde_json::json!({
                        "success": false,
                        "hwnd": hwnd,
                        "error": "Could not retrieve icon for the specified HWND"
                    });
                    println!("{}", response);
                }
            }
        }
        Some(Commands::GetExecutableIcon { path }) => {
            // Executable icon alma modu
            match TaskbarMonitor::get_executable_icon_as_base64(&path) {
                Some(base64_icon) => {
                    let response = serde_json::json!({
                        "success": true,
                        "path": path,
                        "icon_base64": base64_icon,
                        "format": "PNG"
                    });
                    println!("{}", response);
                }
                None => {
                    let response = serde_json::json!({
                        "success": false,
                        "path": path,
                        "error": "Could not retrieve icon for the specified executable path"
                    });
                    println!("{}", response);
                }
            }
        }
        Some(Commands::GetWindowScreenshot { hwnd, size }) => {
            // Pencere screenshot alma modu
            match TaskbarMonitor::get_window_screenshot_as_base64(hwnd, &size) {
                Some(base64_screenshot) => {
                    let response = serde_json::json!({
                        "success": true,
                        "hwnd": hwnd,
                        "screenshot_base64": base64_screenshot,
                        "format": "PNG",
                        "max_size": size
                    });
                    println!("{}", response);
                }
                None => {
                    let response = serde_json::json!({
                        "success": false,
                        "hwnd": hwnd,
                        "error": "Could not capture screenshot for the specified HWND"
                    });
                    println!("{}", response);
                }
            }
        }
        Some(Commands::MinimizeWindow { hwnd }) => {
            // Pencereyi minimize et
            if TaskbarMonitor::minimize_window(hwnd) {
                let response = serde_json::json!({
                    "success": true,
                    "hwnd": hwnd,
                    "action": "minimize",
                    "message": "Window minimized successfully"
                });
                println!("{}", response);
            } else {
                let response = serde_json::json!({
                    "success": false,
                    "hwnd": hwnd,
                    "error": "Could not minimize window"
                });
                println!("{}", response);
            }
        }
        Some(Commands::MaximizeWindow { hwnd }) => {
            // Pencereyi maximize et
            if TaskbarMonitor::maximize_window(hwnd) {
                let response = serde_json::json!({
                    "success": true,
                    "hwnd": hwnd,
                    "action": "maximize",
                    "message": "Window maximized successfully"
                });
                println!("{}", response);
            } else {
                let response = serde_json::json!({
                    "success": false,
                    "hwnd": hwnd,
                    "error": "Could not maximize window"
                });
                println!("{}", response);
            }
        }
        Some(Commands::RestoreWindow { hwnd }) => {
            // Pencereyi restore et
            if TaskbarMonitor::restore_window(hwnd) {
                let response = serde_json::json!({
                    "success": true,
                    "hwnd": hwnd,
                    "action": "restore",
                    "message": "Window restored successfully"
                });
                println!("{}", response);
            } else {
                let response = serde_json::json!({
                    "success": false,
                    "hwnd": hwnd,
                    "error": "Could not restore window"
                });
                println!("{}", response);
            }
        }
        Some(Commands::CloseWindow { hwnd }) => {
            // Pencereyi kapat
            if TaskbarMonitor::close_window(hwnd) {
                let response = serde_json::json!({
                    "success": true,
                    "hwnd": hwnd,
                    "action": "close",
                    "message": "Close message sent to window successfully"
                });
                println!("{}", response);
            } else {
                let response = serde_json::json!({
                    "success": false,
                    "hwnd": hwnd,
                    "error": "Could not send close message to window"
                });
                println!("{}", response);
            }
        }
        Some(Commands::FocusWindow { hwnd }) => {
            // Pencereyi focus et
            if TaskbarMonitor::focus_window(hwnd) {
                let response = serde_json::json!({
                    "success": true,
                    "hwnd": hwnd,
                    "action": "focus",
                    "message": "Window focused successfully"
                });
                println!("{}", response);
            } else {
                let response = serde_json::json!({
                    "success": false,
                    "hwnd": hwnd,
                    "error": "Could not focus window"
                });
                println!("{}", response);
            }
        }
        Some(Commands::UnfocusWindow { hwnd }) => {
            // Pencereyi unfocus et
            if TaskbarMonitor::unfocus_window(hwnd) {
                let response = serde_json::json!({
                    "success": true,
                    "hwnd": hwnd,
                    "action": "unfocus",
                    "message": "Window unfocused successfully"
                });
                println!("{}", response);
            } else {
                let response = serde_json::json!({
                    "success": false,
                    "hwnd": hwnd,
                    "error": "Could not unfocus window"
                });
                println!("{}", response);
            }
        }
        Some(Commands::ToggleFocusWindow { hwnd }) => {
            // Pencereyi toggle focus et
            if TaskbarMonitor::toggle_focus_window(hwnd) {
                let response = serde_json::json!({
                    "success": true,
                    "hwnd": hwnd,
                    "action": "toggle_focus",
                    "message": "Window focus toggled successfully"
                });
                println!("{}", response);
            } else {
                let response = serde_json::json!({
                    "success": false,
                    "hwnd": hwnd,
                    "error": "Could not toggle window focus"
                });
                println!("{}", response);
            }
        }
        Some(Commands::StartExecutable { path }) => {
            // Executable'ı başlat
            if TaskbarMonitor::start_executable(&path) {
                let response = serde_json::json!({
                    "success": true,
                    "path": path,
                    "action": "start",
                    "message": "Executable started successfully"
                });
                println!("{}", response);
            } else {
                let response = serde_json::json!({
                    "success": false,
                    "path": path,
                    "error": "Could not start executable"
                });
                println!("{}", response);
            }
        }
        Some(Commands::Monitor) | None => {
            // Varsayılan monitoring modu
            let mut monitor = TaskbarMonitor::new();
            monitor.monitor_loop().await;
        }
    }
}
