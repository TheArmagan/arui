//! Window actions. All of them are asynchronous towards the target app (ShowWindowAsync /
//! PostMessage), so a hung app can never block the caller.
//!
//! Focus strategy (ARUI's taskbar window is non-activating, so ARUI is usually not the
//! foreground process when the user clicks a button):
//! 1. restore the window first if it is minimized (ShowWindowAsync SW_RESTORE);
//! 2. plain SetForegroundWindow. This already works most of the time, because the click on
//!    ARUI's window made ARUI the process that "received the last input event";
//! 3. otherwise send an empty mouse input with SendInput (no movement, no buttons). That makes
//!    us the last-input process without side effects like an injected ALT, which can open
//!    menu bars in the target app. This is the trick PowerToys uses;
//! 4. last resort: AttachThreadInput to the current foreground thread, BringWindowToTop and
//!    SetForegroundWindow, then detach.

use arui_win::util::{from_hwnd, to_hwnd};
use arui_win::{Error, Hwnd, Result};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_MOUSE};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetAncestor, GetForegroundWindow, GetWindowThreadProcessId, IsIconic,
    IsWindow, PostMessageW, SetForegroundWindow, ShowWindowAsync, GA_ROOTOWNER, SHOW_WINDOW_CMD,
    SW_MINIMIZE, SW_RESTORE, SW_SHOWMAXIMIZED, WM_CLOSE,
};

fn checked(hwnd: Hwnd) -> Result<HWND> {
    let h = to_hwnd(hwnd);
    if hwnd == 0 || !unsafe { IsWindow(Some(h)).as_bool() } {
        return Err(Error::NotFound(format!("window {hwnd}")));
    }
    Ok(h)
}

fn show(hwnd: Hwnd, cmd: SHOW_WINDOW_CMD) -> Result<()> {
    let h = checked(hwnd)?;
    // The return value is the previous visibility, not success; nothing to check.
    unsafe {
        let _ = ShowWindowAsync(h, cmd);
    }
    Ok(())
}

pub fn minimize(hwnd: Hwnd) -> Result<()> {
    show(hwnd, SW_MINIMIZE)
}

pub fn maximize(hwnd: Hwnd) -> Result<()> {
    show(hwnd, SW_SHOWMAXIMIZED)
}

pub fn restore(hwnd: Hwnd) -> Result<()> {
    show(hwnd, SW_RESTORE)
}

pub fn close(hwnd: Hwnd) -> Result<()> {
    let h = checked(hwnd)?;
    unsafe { PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0))? };
    Ok(())
}

/// True if `h` or a window it owns is in the foreground.
fn is_foreground(h: HWND) -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        !fg.is_invalid() && (fg == h || GetAncestor(fg, GA_ROOTOWNER) == h)
    }
}

pub fn focus(hwnd: Hwnd) -> Result<()> {
    let h = checked(hwnd)?;
    unsafe {
        if IsIconic(h).as_bool() {
            let _ = ShowWindowAsync(h, SW_RESTORE);
        }
        if is_foreground(h) {
            return Ok(());
        }
        if SetForegroundWindow(h).as_bool() && is_foreground(h) {
            return Ok(());
        }

        let input = INPUT {
            r#type: INPUT_MOUSE,
            ..Default::default()
        };
        SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        if SetForegroundWindow(h).as_bool() && is_foreground(h) {
            return Ok(());
        }

        let fg = GetForegroundWindow();
        let fg_thread = if fg.is_invalid() {
            0
        } else {
            GetWindowThreadProcessId(fg, None)
        };
        let me = GetCurrentThreadId();
        let attached =
            fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, true).as_bool();
        let _ = BringWindowToTop(h);
        let _ = SetForegroundWindow(h);
        if attached {
            let _ = AttachThreadInput(me, fg_thread, false);
        }
    }
    // A restore from minimized is asynchronous, so the window may become foreground a moment
    // later; only log here instead of failing.
    if !is_foreground(h) {
        log::debug!(
            "window {} not in the foreground yet after focus",
            from_hwnd(h)
        );
    }
    Ok(())
}

pub fn toggle(hwnd: Hwnd) -> Result<()> {
    let h = checked(hwnd)?;
    let minimized = unsafe { IsIconic(h).as_bool() };
    if !minimized && is_foreground(h) {
        minimize(hwnd)
    } else {
        focus(hwnd)
    }
}
