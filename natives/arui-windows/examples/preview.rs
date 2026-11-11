//! Manual check for DWM thumbnails and window actions.
//!
//!   cargo run -p arui-windows --example preview -- <title substring> [--close]
//!
//! Opens a 640x400 window at (100,100) on its own thread, then (from the main thread) shows a
//! live thumbnail of the first tracked window whose title contains the argument for 5 seconds.
//! With --close, toggles the source twice and then posts WM_CLOSE to it.

use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::Duration;

use arui_win::util::{from_hwnd, wide};
use arui_win::{IconStore, Rect};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

unsafe extern "system" fn wndproc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if m == WM_DESTROY {
        PostQuitMessage(0);
        return LRESULT(0);
    }
    DefWindowProcW(h, m, w, l)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let needle = args.first().cloned().unwrap_or_default().to_lowercase();
    let tracker = arui_windows::start(Arc::new(|_| {}), IconStore::new()).expect("start");
    let Some(src) = tracker
        .list()
        .into_iter()
        .find(|w| w.title.to_lowercase().contains(&needle))
    else {
        eprintln!("no window matches {needle:?}");
        return;
    };
    eprintln!("source: {} {:?}", src.hwnd, src.title);

    let (tx, rx) = channel();
    let ui = std::thread::spawn(move || unsafe {
        let inst = GetModuleHandleW(None).unwrap();
        let class = wide("ARUI.PreviewTest");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: inst.into(),
            lpszClassName: PCWSTR(class.as_ptr()),
            hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH(6 as _), // COLOR_WINDOW + 1
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let h = CreateWindowExW(
            WS_EX_TOPMOST,
            PCWSTR(class.as_ptr()),
            PCWSTR(wide("preview test").as_ptr()),
            WS_POPUP | WS_VISIBLE,
            100,
            100,
            640,
            400,
            None,
            None,
            Some(inst.into()),
            None,
        )
        .unwrap();
        tx.send(from_hwnd(h)).unwrap();
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
    let dest = rx.recv().unwrap();
    let rect = Rect {
        left: 10,
        top: 10,
        right: 630,
        bottom: 390,
    };
    eprintln!(
        "show_preview from a non-owner thread: {:?}",
        tracker.show_preview(dest, src.hwnd, rect)
    );
    std::thread::sleep(Duration::from_secs(5));
    tracker.hide_preview(dest, None);
    eprintln!("hidden");

    if args.iter().any(|a| a == "--close") {
        eprintln!("toggle: {:?}", tracker.toggle(src.hwnd));
        std::thread::sleep(Duration::from_millis(1500));
        eprintln!("toggle: {:?}", tracker.toggle(src.hwnd));
        std::thread::sleep(Duration::from_millis(1500));
        eprintln!("close: {:?}", tracker.close(src.hwnd));
        std::thread::sleep(Duration::from_millis(1000));
        let still = tracker.list().iter().any(|w| w.hwnd == src.hwnd);
        eprintln!("still tracked after close: {still}");
    }
    unsafe {
        let _ = PostMessageW(
            Some(arui_win::util::to_hwnd(dest)),
            WM_CLOSE,
            WPARAM(0),
            LPARAM(0),
        );
    }
    let _ = ui.join();
}
