//! Manual test: hides explorer's taskbar, registers a plain Win32 window as a 60px bottom AppBar
//! on the primary monitor for ~10 s, then restores everything.
//!
//! As a safety net it spawns `arui-guard` (if built: `cargo build -p arui-guard`) on its own pid,
//! so killing this demo mid-way still restores the taskbar.
//!
//!     cargo build -p arui-guard && cargo run -p arui-shell --example appbar_demo

use std::sync::Arc;
use std::time::{Duration, Instant};

use arui_win::util::{from_hwnd, wide};
use arui_win::{Edge, IconStore, MonitorInfo};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::CreateSolidBrush;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

unsafe extern "system" fn wnd_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    DefWindowProcW(h, m, w, l)
}

/// Runs the message loop for `d` (the AppBar subclass needs this thread to pump messages).
fn pump(d: Duration) {
    let end = Instant::now() + d;
    unsafe {
        while let Some(left) = end.checked_duration_since(Instant::now()) {
            MsgWaitForMultipleObjects(None, false, left.as_millis() as u32, QS_ALLINPUT);
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

fn print_monitors(label: &str, list: &[MonitorInfo]) {
    println!("--- {label}");
    for m in list {
        println!("  {} primary={} bounds={:?} work={:?} scale={}", m.id, m.primary, m.bounds, m.work_area, m.scale);
    }
}

fn main() {
    let guard_exe = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().join("arui-guard.exe");
    let guard = if guard_exe.exists() {
        Some(
            std::process::Command::new(&guard_exe)
                .args(["--pid", &std::process::id().to_string()])
                .spawn()
                .expect("spawn guard"),
        )
    } else {
        println!("(arui-guard not built, running without a watchdog)");
        None
    };

    unsafe {
        let hinst = HINSTANCE(GetModuleHandleW(None).unwrap().0);
        let class = wide("ARUI_AppBarDemo");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinst,
            lpszClassName: PCWSTR(class.as_ptr()),
            hbrBackground: CreateSolidBrush(COLORREF(0x00804020)),
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            PCWSTR(class.as_ptr()),
            PCWSTR(class.as_ptr()),
            WS_POPUP,
            0,
            0,
            100,
            100,
            None,
            None,
            Some(hinst),
            None,
        )
        .unwrap();

        let shell = arui_shell::start(
            Arc::new(|e| println!("event: {}", serde_json::to_string(&e).unwrap_or_default())),
            IconStore::new(),
            arui_shell::ShellOptions { hide_explorer_taskbar: false, capture_start_key: false },
        )
        .unwrap();
        let before = shell.monitors();
        print_monitors("before", &before);
        let primary = before.iter().find(|m| m.primary).unwrap().id.clone();

        shell.hide_explorer_taskbar();
        let rect = shell.register_appbar(from_hwnd(hwnd), &primary, Edge::Bottom, 60, true).unwrap();
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        println!("appbar rect: {rect:?}");

        pump(Duration::from_secs(2));
        print_monitors("while registered (expect primary work bottom = bounds - 60, others full)", &shell.monitors());
        let rect = shell.update_appbar(from_hwnd(hwnd), 80).unwrap();
        println!("resized appbar rect: {rect:?}");
        pump(Duration::from_secs(8));

        drop(shell);
        let _ = DestroyWindow(hwnd);
        pump(Duration::from_secs(2));
        let after = arui_shell::monitors::enumerate();
        print_monitors("after restore", &after);
        println!("work areas restored: {}", before.iter().zip(&after).all(|(a, b)| a.work_area == b.work_area));
        println!("state file left behind: {}", arui_shell::default_state_file().exists());
    }
    // Do not wait for the guard: it waits for us. Once we exit it finds no state file and quits.
    drop(guard);
}
