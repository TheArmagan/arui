//! Explorer taskbar helpers: finding, hiding, showing and the auto-hide trick.
//!
//! Hiding `Shell_TrayWnd` with SW_HIDE alone is not enough: explorer's taskbar is itself an
//! AppBar and keeps its work-area reservation while invisible. Like RetroBar / ManagedShell we
//! switch explorer's taskbar to auto-hide with ABM_SETSTATE (auto-hide bars reserve nothing) and
//! put the previous state back when we restore. ABM_SETSTATE is persisted by explorer, which is
//! why the previous value goes into the crash-recovery state file.

use arui_win::util::{from_hwnd, to_hwnd, wide};
use arui_win::Hwnd;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::Shell::{
    SHAppBarMessage, ABM_GETSTATE, ABM_REMOVE, ABM_SETSTATE, ABS_ALWAYSONTOP, ABS_AUTOHIDE,
    APPBARDATA,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible, ShowWindow, SW_HIDE,
    SW_SHOWNOACTIVATE,
};

pub(crate) const PRIMARY_CLASS: &str = "Shell_TrayWnd";
pub(crate) const SECONDARY_CLASS: &str = "Shell_SecondaryTrayWnd";

fn pid_of(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

/// All top-level windows of `class` that are not owned by this process (arui-tray creates its
/// own `Shell_TrayWnd`, which must never be mistaken for explorer's).
fn find_all(class: &str) -> Vec<HWND> {
    let class_w = wide(class);
    let me = std::process::id();
    let mut out = Vec::new();
    let mut after: Option<HWND> = None;
    loop {
        let found = unsafe { FindWindowExW(None, after, PCWSTR(class_w.as_ptr()), PCWSTR::null()) };
        match found {
            Ok(h) if !h.is_invalid() => {
                if pid_of(h) != me {
                    out.push(h);
                }
                after = Some(h);
            }
            _ => break,
        }
    }
    out
}

/// Explorer's primary taskbar window, if explorer is running.
pub(crate) fn explorer_tray() -> Option<HWND> {
    find_all(PRIMARY_CLASS).into_iter().next()
}

/// Explorer's primary and all secondary taskbars.
pub(crate) fn explorer_taskbars() -> Vec<HWND> {
    let mut v = find_all(PRIMARY_CLASS);
    v.truncate(1);
    v.extend(find_all(SECONDARY_CLASS));
    v
}

pub(crate) fn explorer_pid() -> Option<u32> {
    explorer_tray().map(pid_of)
}

pub(crate) fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

pub(crate) fn is_taskbar_class(name: &str) -> bool {
    name == PRIMARY_CLASS || name == SECONDARY_CLASS
}

fn abd(hwnd: Option<HWND>) -> APPBARDATA {
    APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: hwnd.unwrap_or_default(),
        ..Default::default()
    }
}

/// Explorer taskbar ABS_* flags.
pub(crate) fn get_state() -> u32 {
    let mut d = abd(explorer_tray());
    unsafe { SHAppBarMessage(ABM_GETSTATE, &mut d) as u32 }
}

pub(crate) fn set_state(state: u32) {
    let mut d = abd(explorer_tray());
    d.lParam = LPARAM(state as isize);
    unsafe { SHAppBarMessage(ABM_SETSTATE, &mut d) };
}

/// The state we force while hidden: auto-hide, keeping the always-on-top bit as it was.
pub(crate) fn hidden_state(prev: u32) -> u32 {
    ABS_AUTOHIDE | (prev & ABS_ALWAYSONTOP)
}

pub(crate) fn hide_windows() {
    for h in explorer_taskbars() {
        unsafe {
            if IsWindowVisible(h).as_bool() {
                let _ = ShowWindow(h, SW_HIDE);
            }
        }
    }
}

pub(crate) fn show_windows() {
    for h in explorer_taskbars() {
        unsafe {
            let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
        }
    }
}

/// ABM_REMOVE for a window handle; works for windows of dead processes too, since explorer
/// matches its AppBar list by handle value.
pub(crate) fn remove_appbar(hwnd: Hwnd) {
    let mut d = abd(Some(to_hwnd(hwnd)));
    unsafe { SHAppBarMessage(ABM_REMOVE, &mut d) };
}

pub(crate) fn tray_id() -> Option<Hwnd> {
    explorer_tray().map(from_hwnd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_state_keeps_on_top() {
        assert_eq!(hidden_state(0), ABS_AUTOHIDE);
        assert_eq!(hidden_state(ABS_ALWAYSONTOP), ABS_AUTOHIDE | ABS_ALWAYSONTOP);
        assert_eq!(hidden_state(ABS_AUTOHIDE), ABS_AUTOHIDE);
        assert!(is_taskbar_class("Shell_SecondaryTrayWnd"));
        assert!(!is_taskbar_class("Progman"));
    }
}
