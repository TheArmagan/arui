//! Starts the tray host, prints every tray event as one JSON line on stdout, runs a small
//! self-test (own Shell_NotifyIcon icon, Shell_NotifyIconGetRect, appbar forwarding), then drops
//! the host and checks that our Shell_TrayWnd is gone. Diagnostics go to stderr.
//!
//! `cargo run -p arui-tray --example tray_dump [seconds]` (default 20).

use std::sync::Arc;
use std::time::Duration;

use arui_win::util::{from_hwnd, wide};
use arui_win::{EventSink, IconStore, NativeEvent, Rect, TrayAction, TrayEvent};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

fn pid_of(h: HWND) -> u32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
    pid
}

/// Every top-level Shell_TrayWnd as (hwnd, pid), in z-order.
fn tray_windows() -> Vec<(u64, u32)> {
    let mut out = Vec::new();
    let mut after = None;
    while let Ok(h) = unsafe { FindWindowExW(None, after, w!("Shell_TrayWnd"), PCWSTR::null()) } {
        out.push((from_hwnd(h), pid_of(h)));
        after = Some(h);
    }
    out
}

/// SHAppBarMessage(ABM_GETTASKBARPOS) result, to compare with and without our window.
fn taskbar_pos(label: &str) {
    let mut abd = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        ..Default::default()
    };
    let r = unsafe { SHAppBarMessage(ABM_GETTASKBARPOS, &mut abd) };
    let state = unsafe { SHAppBarMessage(ABM_GETSTATE, &mut abd) };
    eprintln!(
        "# ABM_GETTASKBARPOS ({label}) -> {r}, rect {:?}, edge {}, ABM_GETSTATE {state}",
        abd.rc, abd.uEdge
    );
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--abm") {
        taskbar_pos("child process");
        return;
    }
    let secs: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let own_pid = unsafe { GetCurrentProcessId() };
    eprintln!(
        "# pid {own_pid}, Shell_TrayWnd before start: {:?}",
        tray_windows()
    );
    taskbar_pos("before start");

    let store = IconStore::new();
    let sink: EventSink = Arc::new(|e: NativeEvent| {
        println!("{}", serde_json::to_string(&e).unwrap());
        if let NativeEvent::Tray(TrayEvent::Snapshot(icons)) = &e {
            eprintln!("# snapshot with {} icons", icons.len());
        }
    });
    let host = arui_tray::start(sink, store.clone()).expect("start tray host");
    eprintln!(
        "# our tray hwnd {}, Shell_TrayWnd now: {:?}",
        host.hwnd(),
        tray_windows()
    );
    let first = unsafe { FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) }.unwrap_or_default();
    eprintln!(
        "# FindWindow(Shell_TrayWnd) returns ours: {}",
        from_hwnd(first) == host.hwnd()
    );

    unsafe {
        // Appbar messages must reach explorer through our window.
        taskbar_pos("through our window");
        // And from another process, which is the real-world case.
        let exe = std::env::current_exe().unwrap();
        let _ = std::process::Command::new(exe).arg("--abm").status();

        // Self-test with our own icon, after the initial Snapshot so its diffs are printed.
        std::thread::sleep(Duration::from_millis(3500));
        let owner = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            w!("tray_dump"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            None,
            None,
        )
        .expect("owner window");
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: owner,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_APP + 5,
            hIcon: LoadIconW(None, IDI_APPLICATION).unwrap_or_default(),
            ..Default::default()
        };
        let tip = wide("arui-tray self test");
        nid.szTip[..tip.len()].copy_from_slice(&tip);
        let added = Shell_NotifyIconW(NIM_ADD, &nid).as_bool();
        nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        let ver = Shell_NotifyIconW(NIM_SETVERSION, &nid).as_bool();
        let id = format!("{}:1", from_hwnd(owner));
        let mine = host.icons().into_iter().find(|i| i.id == id);
        eprintln!(
            "# self test: NIM_ADD {added}, NIM_SETVERSION {ver}, seen {:?}, png {} bytes",
            mine.as_ref().map(|i| (&i.tooltip, i.version, &i.exe_path)),
            mine.as_ref()
                .and_then(|i| store.get(&i.icon_key))
                .map_or(0, |p| p.len())
        );
        host.set_icon_rects(vec![(
            id.clone(),
            Rect {
                left: 10,
                top: 20,
                right: 34,
                bottom: 44,
            },
        )]);
        let ident = NOTIFYICONIDENTIFIER {
            cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
            hWnd: owner,
            uID: 1,
            ..Default::default()
        };
        eprintln!(
            "# Shell_NotifyIconGetRect -> {:?}",
            Shell_NotifyIconGetRect(&ident)
        );
        eprintln!(
            "# action(LeftClick) -> {:?}",
            host.action(&id, TrayAction::LeftClick, 5, 5)
        );
        // In-process appbar round trip with input and output (what arui-shell does).
        let mut abd = APPBARDATA {
            cbSize: std::mem::size_of::<APPBARDATA>() as u32,
            hWnd: owner,
            uCallbackMessage: WM_APP + 6,
            ..Default::default()
        };
        let new = SHAppBarMessage(ABM_NEW, &mut abd);
        abd.uEdge = ABE_TOP;
        abd.rc = windows::Win32::Foundation::RECT {
            left: 0,
            top: 0,
            right: 1000,
            bottom: 40,
        };
        let q = SHAppBarMessage(ABM_QUERYPOS, &mut abd);
        let rc = abd.rc;
        let removed = SHAppBarMessage(ABM_REMOVE, &mut abd);
        eprintln!(
            "# appbar in-process: ABM_NEW {new}, ABM_QUERYPOS {q} -> {rc:?}, ABM_REMOVE {removed}"
        );
        let deleted = Shell_NotifyIconW(NIM_DELETE, &nid).as_bool();
        eprintln!("# self test: NIM_DELETE {deleted}");
        let _ = DestroyWindow(owner);
    }

    eprintln!("# listening for {secs} s ...");
    std::thread::sleep(Duration::from_secs(secs));

    let icons = host.icons();
    eprintln!("# {} icons at the end:", icons.len());
    for i in &icons {
        eprintln!(
            "#   {} visible={} v{} tip={:?} exe={:?}",
            i.id,
            i.visible,
            i.version,
            i.tooltip,
            i.exe_path.as_deref().unwrap_or("?")
        );
    }
    drop(host);
    let after = tray_windows();
    let leaked = after.iter().any(|(_, pid)| *pid == own_pid);
    eprintln!("# after drop: Shell_TrayWnd {after:?}, our window left behind: {leaked}");
    if leaked {
        std::process::exit(1);
    }
}
