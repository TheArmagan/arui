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
    BringWindowToTop, GetAncestor, GetForegroundWindow, GetWindow, GetWindowLongW,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, PostMessageW,
    SetForegroundWindow, SetWindowPos, ShowWindowAsync, GA_ROOTOWNER, GWL_EXSTYLE, GW_HWNDPREV,
    HWND_NOTOPMOST, HWND_TOPMOST, SET_WINDOW_POS_FLAGS, SHOW_WINDOW_CMD, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SW_MINIMIZE, SW_RESTORE, SW_SHOWMAXIMIZED, SW_SHOWMINNOACTIVE,
    SW_SHOWNOACTIVATE, WM_CLOSE, WS_EX_TOPMOST,
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

// ---------------------------------------------------------------- peek

/// A window temporarily brought to the front by [`peek`], with what is needed to put it back.
struct Peek {
    hwnd: Hwnd,
    was_minimized: bool,
    /// Was already always-on-top itself (then it keeps that).
    was_topmost: bool,
    /// The visible, non-topmost window that was directly above it (None: it was already on top).
    above: Option<Hwnd>,
}

static PEEK: std::sync::Mutex<Option<Peek>> = std::sync::Mutex::new(None);

const PEEK_FLAGS: SET_WINDOW_POS_FLAGS =
    SET_WINDOW_POS_FLAGS(SWP_NOMOVE.0 | SWP_NOSIZE.0 | SWP_NOACTIVATE.0 | SWP_ASYNCWINDOWPOS.0);

fn is_topmost(h: HWND) -> bool {
    unsafe { (GetWindowLongW(h, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0) != 0 }
}

/// The nearest visible, non-topmost window above `h` in z-order. Topmost windows are skipped:
/// inserting a normal window after a topmost one would make it topmost too.
fn window_above(h: HWND) -> Option<Hwnd> {
    let mut cur = h;
    loop {
        cur = unsafe { GetWindow(cur, GW_HWNDPREV) }.ok()?;
        if cur.0.is_null() {
            return None;
        }
        if unsafe { IsWindowVisible(cur).as_bool() } && !is_topmost(cur) {
            return Some(from_hwnd(cur));
        }
    }
}

/// Shows `hwnd` in front of the other windows without activating it (like Aero Peek). Minimized
/// windows are restored. A previous peek is undone first. Undo with [`unpeek`].
///
/// Windows does not let a background process raise another app's window above the foreground
/// window with HWND_TOP, so the window is made topmost for the duration of the peek. With
/// `under` (a topmost window, e.g. the preview popup) it is inserted right below it so the
/// popup stays visible.
pub fn peek(hwnd: Hwnd, under: Option<Hwnd>) -> Result<()> {
    let h = checked(hwnd)?;
    let mut slot = PEEK.lock().unwrap_or_else(|e| e.into_inner());
    if slot.as_ref().is_some_and(|p| p.hwnd == hwnd) {
        return Ok(());
    }
    if let Some(prev) = slot.take() {
        undo(&prev);
    }
    let was_minimized = unsafe { IsIconic(h).as_bool() };
    let was_topmost = is_topmost(h);
    let above = if was_minimized { None } else { window_above(h) };
    let insert = under
        .map(to_hwnd)
        .filter(|u| unsafe { IsWindow(Some(*u)).as_bool() } && is_topmost(*u))
        .unwrap_or(HWND_TOPMOST);
    unsafe {
        if was_minimized {
            let _ = ShowWindowAsync(h, SW_SHOWNOACTIVATE);
        }
        let _ = SetWindowPos(h, Some(insert), 0, 0, 0, 0, PEEK_FLAGS);
    }
    *slot = Some(Peek { hwnd, was_minimized, was_topmost, above });
    Ok(())
}

/// Ends the current peek. With `restore` the window goes back to where it was (minimized again,
/// or behind the window that was above it); without, it stays where it is (e.g. it was clicked).
pub fn unpeek(restore: bool) {
    let prev = PEEK.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let (Some(p), true) = (prev, restore) {
        undo(&p);
    }
}

fn undo(p: &Peek) {
    let Ok(h) = checked(p.hwnd) else { return };
    unsafe {
        if p.was_topmost {
            // It was always-on-top already: only undo the restore.
        } else if let Some(above) = p.above.filter(|a| IsWindow(Some(to_hwnd(*a))).as_bool()) {
            // Inserting after a non-topmost window also clears the topmost flag.
            let _ = SetWindowPos(h, Some(to_hwnd(above)), 0, 0, 0, 0, PEEK_FLAGS);
        } else {
            let _ = SetWindowPos(h, Some(HWND_NOTOPMOST), 0, 0, 0, 0, PEEK_FLAGS);
        }
        if p.was_minimized {
            let _ = ShowWindowAsync(h, SW_SHOWMINNOACTIVE);
        }
    }
}
