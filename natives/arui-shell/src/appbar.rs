//! AppBar registration for host-owned windows.
//!
//! Every AppBar is a host window (e.g. a Tauri/tao window). We subclass it with
//! `SetWindowSubclass` so the AppBar callback message (ABN_POSCHANGED, ABN_FULLSCREENAPP) and our
//! own command message are handled on the window's own thread. Subclassing only works from the
//! thread that owns the window, which is why [`register`] must run there. [`update`] and
//! [`unregister`] forward themselves to the owning thread through the subclass when called from
//! elsewhere.
//!
//! Re-entrancy: ABM_SETPOS and SetWindowPos synchronously send messages that land back in the
//! subclass procedure (ours or another AppBar's on the same thread), so the state lock is never
//! held across a Win32 call.

use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};

use arui_win::util::{from_hwnd, to_hwnd, wide};
use arui_win::{Edge, Error, Hwnd, Rect, Result, ShellEvent};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONEAREST};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Shell::{
    DefSubclassProc, RemoveWindowSubclass, SHAppBarMessage, SetWindowSubclass, ABE_BOTTOM,
    ABE_LEFT, ABE_RIGHT, ABE_TOP, ABM_ACTIVATE, ABM_NEW, ABM_QUERYPOS, ABM_SETPOS,
    ABM_WINDOWPOSCHANGED, ABN_FULLSCREENAPP, ABN_POSCHANGED, APPBARDATA,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId, PostMessageW,
    RegisterWindowMessageW, SendMessageTimeoutW, SetWindowPos, SMTO_ABORTIFHUNG, SMTO_NORMAL,
    SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER, WM_ACTIVATE, WM_NCDESTROY,
    WM_WINDOWPOSCHANGED,
};

use crate::monitors::{self, rect_from, rect_to, DpiScope};
use crate::Inner;

const SUBCLASS_ID: usize = 0x4152_5549; // "ARUI"

pub(crate) const CMD_REPOSITION: usize = 1;
pub(crate) const CMD_REREGISTER: usize = 2;
pub(crate) const CMD_UPDATE: usize = 3;
pub(crate) const CMD_UNREGISTER: usize = 4;

#[derive(Debug, Clone)]
pub(crate) struct AppBar {
    pub monitor_id: String,
    pub edge: Edge,
    pub thickness: i32,
    pub reserve: bool,
    pub rect: Rect,
    pub fullscreen: bool,
    /// `Arc<Inner>` raw pointer owned by the subclass; 0 until subclassed.
    pub refdata: usize,
}

fn registered(name: &str, cell: &'static OnceLock<u32>) -> u32 {
    *cell.get_or_init(|| {
        let w = wide(name);
        unsafe { RegisterWindowMessageW(PCWSTR(w.as_ptr())) }
    })
}

/// AppBar notification message (APPBARDATA.uCallbackMessage).
pub(crate) fn callback_msg() -> u32 {
    static M: OnceLock<u32> = OnceLock::new();
    registered("ARUI_AppBarCallback", &M)
}

/// Our command message: wParam = CMD_*, lParam = argument.
pub(crate) fn cmd_msg() -> u32 {
    static M: OnceLock<u32> = OnceLock::new();
    registered("ARUI_AppBarCommand", &M)
}

fn edge_code(e: Edge) -> u32 {
    match e {
        Edge::Left => ABE_LEFT,
        Edge::Top => ABE_TOP,
        Edge::Right => ABE_RIGHT,
        Edge::Bottom => ABE_BOTTOM,
    }
}

/// Bar rect hugging `edge` of `bounds` with the given thickness.
pub(crate) fn initial_rect(b: Rect, edge: Edge, t: i32) -> Rect {
    match edge {
        Edge::Bottom => Rect { top: b.bottom - t, ..b },
        Edge::Top => Rect { bottom: b.top + t, ..b },
        Edge::Left => Rect { right: b.left + t, ..b },
        Edge::Right => Rect { left: b.right - t, ..b },
    }
}

/// After ABM_QUERYPOS moved the edge side of `r` (to make room for other bars), restore the
/// thickness by moving the opposite side, as the AppBar documentation prescribes.
pub(crate) fn fit_to_edge(r: Rect, edge: Edge, t: i32) -> Rect {
    match edge {
        Edge::Bottom => Rect { top: r.bottom - t, ..r },
        Edge::Top => Rect { bottom: r.top + t, ..r },
        Edge::Left => Rect { right: r.left + t, ..r },
        Edge::Right => Rect { left: r.right - t, ..r },
    }
}

fn abd(hwnd: HWND) -> APPBARDATA {
    APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        ..Default::default()
    }
}

fn abm(msg: u32, hwnd: HWND) {
    let mut d = abd(hwnd);
    unsafe { SHAppBarMessage(msg, &mut d) };
}

fn abm_new(hwnd: HWND) -> bool {
    let mut d = abd(hwnd);
    d.uCallbackMessage = callback_msg();
    unsafe { SHAppBarMessage(ABM_NEW, &mut d) != 0 }
}

pub(crate) fn on_owner_thread(hwnd: HWND) -> bool {
    unsafe { GetWindowThreadProcessId(hwnd, None) == GetCurrentThreadId() }
}

/// See [`crate::Shell::register_appbar`]. Must run on the window's thread.
pub(crate) fn register(
    inner: &Arc<Inner>,
    hwnd: Hwnd,
    monitor_id: &str,
    edge: Edge,
    thickness: i32,
    reserve: bool,
) -> Result<Rect> {
    let h = to_hwnd(hwnd);
    if !on_owner_thread(h) {
        return Err(Error::Unsupported(
            "register_appbar must be called on the thread that owns the window".into(),
        ));
    }
    if thickness <= 0 {
        return Err(Error::Other("thickness must be positive".into()));
    }
    let is_new = {
        let mut st = inner.st.lock();
        match st.appbars.get_mut(&hwnd) {
            Some(e) => {
                e.monitor_id = monitor_id.to_string();
                e.edge = edge;
                e.thickness = thickness;
                e.reserve = reserve;
                false
            }
            None => {
                st.appbars.insert(
                    hwnd,
                    AppBar {
                        monitor_id: monitor_id.to_string(),
                        edge,
                        thickness,
                        reserve,
                        rect: Rect::default(),
                        fullscreen: false,
                        refdata: 0,
                    },
                );
                true
            }
        }
    };
    if is_new {
        let raw = Arc::into_raw(inner.clone()) as usize;
        let ok = unsafe { SetWindowSubclass(h, Some(subclass_proc), SUBCLASS_ID, raw) }.as_bool();
        if !ok {
            unsafe { drop(Arc::from_raw(raw as *const Inner)) };
            inner.st.lock().appbars.remove(&hwnd);
            return Err(Error::Other("SetWindowSubclass failed".into()));
        }
        if let Some(e) = inner.st.lock().appbars.get_mut(&hwnd) {
            e.refdata = raw;
        }
        abm_new(h);
        inner.save_state();
    }
    reposition(inner, hwnd, true)
}

/// See [`crate::Shell::update_appbar`]. Any thread.
pub(crate) fn update(inner: &Arc<Inner>, hwnd: Hwnd, thickness: i32) -> Result<Rect> {
    if thickness <= 0 {
        return Err(Error::Other("thickness must be positive".into()));
    }
    if !inner.st.lock().appbars.contains_key(&hwnd) {
        return Err(Error::NotFound(format!("appbar {hwnd}")));
    }
    let h = to_hwnd(hwnd);
    if on_owner_thread(h) {
        if let Some(e) = inner.st.lock().appbars.get_mut(&hwnd) {
            e.thickness = thickness;
        }
        return reposition(inner, hwnd, true);
    }
    send_cmd(h, CMD_UPDATE, thickness as isize)?;
    inner
        .st
        .lock()
        .appbars
        .get(&hwnd)
        .map(|e| e.rect)
        .ok_or_else(|| Error::NotFound(format!("appbar {hwnd}")))
}

/// See [`crate::Shell::unregister_appbar`]. Any thread.
pub(crate) fn unregister(inner: &Inner, hwnd: Hwnd) {
    let h = to_hwnd(hwnd);
    if on_owner_thread(h) {
        unregister_local(inner, hwnd);
    } else if send_cmd(h, CMD_UNREGISTER, 0).is_err() {
        // Owner thread is hung or gone: at least release the work area. The subclass stays in
        // place (holding its Arc) and turns into a pass-through once `closed` is set.
        log::warn!("appbar {hwnd:#x}: owner thread did not answer, removing without unsubclass");
        if inner.st.lock().appbars.remove(&hwnd).is_some() {
            crate::taskbar::remove_appbar(hwnd);
            inner.save_state();
        }
    }
}

fn send_cmd(h: HWND, cmd: usize, arg: isize) -> Result<()> {
    let r = unsafe {
        SendMessageTimeoutW(
            h,
            cmd_msg(),
            WPARAM(cmd),
            LPARAM(arg),
            SMTO_NORMAL | SMTO_ABORTIFHUNG,
            5000,
            None,
        )
    };
    if r.0 == 0 {
        Err(Error::Other("appbar owner thread did not respond".into()))
    } else {
        Ok(())
    }
}

/// Posts a command to every registered AppBar (used by the shell thread on display changes and
/// explorer restarts).
pub(crate) fn post_all(inner: &Inner, cmd: usize) {
    let hwnds: Vec<Hwnd> = inner.st.lock().appbars.keys().copied().collect();
    for h in hwnds {
        unsafe {
            let _ = PostMessageW(Some(to_hwnd(h)), cmd_msg(), WPARAM(cmd), LPARAM(0));
        }
    }
}

/// Owner thread only. Must not touch `inner` after the Arc is released.
fn unregister_local(inner: &Inner, hwnd: Hwnd) {
    let Some(e) = inner.st.lock().appbars.remove(&hwnd) else { return };
    let h = to_hwnd(hwnd);
    crate::taskbar::remove_appbar(hwnd);
    inner.save_state();
    if e.refdata != 0 {
        let removed = unsafe { RemoveWindowSubclass(h, Some(subclass_proc), SUBCLASS_ID) }.as_bool();
        if removed {
            unsafe { drop(Arc::from_raw(e.refdata as *const Inner)) };
        }
    }
}

/// Runs ABM_QUERYPOS/ABM_SETPOS and moves the window. With `force == false` the reservation is
/// only re-sent when the rect actually changed (avoids ping-pong between bars on
/// ABN_POSCHANGED).
fn reposition(inner: &Inner, hwnd: Hwnd, force: bool) -> Result<Rect> {
    let Some(bar) = inner.st.lock().appbars.get(&hwnd).cloned() else {
        return Err(Error::NotFound(format!("appbar {hwnd}")));
    };
    let _dpi = DpiScope::per_monitor();
    let mon = monitors::enumerate()
        .into_iter()
        .find(|m| m.id == bar.monitor_id)
        .ok_or_else(|| Error::NotFound(format!("monitor {}", bar.monitor_id)))?;
    let h = to_hwnd(hwnd);
    let rect = if bar.reserve {
        let mut d = abd(h);
        d.uEdge = edge_code(bar.edge);
        d.rc = rect_to(initial_rect(mon.bounds, bar.edge, bar.thickness));
        unsafe { SHAppBarMessage(ABM_QUERYPOS, &mut d) };
        d.rc = rect_to(fit_to_edge(rect_from(d.rc), bar.edge, bar.thickness));
        if force || rect_from(d.rc) != bar.rect {
            unsafe { SHAppBarMessage(ABM_SETPOS, &mut d) };
        }
        rect_from(d.rc)
    } else {
        initial_rect(mon.bounds, bar.edge, bar.thickness)
    };
    let mut cur = RECT::default();
    let _ = unsafe { GetWindowRect(h, &mut cur) };
    if force || rect_from(cur) != rect {
        unsafe {
            SetWindowPos(
                h,
                None,
                rect.left,
                rect.top,
                rect.width(),
                rect.height(),
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
            )?;
        }
    }
    if let Some(e) = inner.st.lock().appbars.get_mut(&hwnd) {
        e.rect = rect;
    }
    Ok(rect)
}

fn on_fullscreen(inner: &Inner, hwnd: HWND, active: bool) {
    let mon = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    if active {
        // ABN_FULLSCREENAPP is broadcast to every bar; only react on the monitor that hosts the
        // fullscreen (foreground) window.
        let fg = unsafe { GetForegroundWindow() };
        if !fg.is_invalid() && unsafe { MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST) } != mon {
            return;
        }
    }
    let changed = {
        let mut st = inner.st.lock();
        match st.appbars.get_mut(&from_hwnd(hwnd)) {
            Some(e) if e.fullscreen != active => {
                e.fullscreen = active;
                true
            }
            _ => false,
        }
    };
    if changed {
        inner.emit(ShellEvent::Fullscreen { monitor: mon.0 as usize as u64, active });
    }
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    refdata: usize,
) -> LRESULT {
    let inner = &*(refdata as *const Inner);
    if inner.closed.load(Ordering::SeqCst) && msg != WM_NCDESTROY {
        return DefSubclassProc(hwnd, msg, wp, lp);
    }
    let id = from_hwnd(hwnd);
    if msg == callback_msg() {
        match wp.0 as u32 {
            ABN_POSCHANGED => {
                if let Err(e) = reposition(inner, id, false) {
                    log::warn!("appbar {id:#x}: reposition failed: {e}");
                }
            }
            ABN_FULLSCREENAPP => on_fullscreen(inner, hwnd, lp.0 != 0),
            _ => {}
        }
        return LRESULT(0);
    }
    if msg == cmd_msg() {
        match wp.0 {
            CMD_REPOSITION => {
                let _ = reposition(inner, id, false);
            }
            CMD_REREGISTER => {
                // After an explorer restart the AppBar list is empty. ABM_NEW returns FALSE when
                // we are still registered (spurious TaskbarCreated), which is fine.
                abm_new(hwnd);
                let _ = reposition(inner, id, true);
            }
            CMD_UPDATE => {
                if let Some(e) = inner.st.lock().appbars.get_mut(&id) {
                    e.thickness = lp.0 as i32;
                }
                let _ = reposition(inner, id, true);
            }
            CMD_UNREGISTER => {
                unregister_local(inner, id);
                // `inner` may be dangling from here on.
            }
            _ => {}
        }
        return LRESULT(1);
    }
    match msg {
        WM_ACTIVATE => abm(ABM_ACTIVATE, hwnd),
        WM_WINDOWPOSCHANGED => abm(ABM_WINDOWPOSCHANGED, hwnd),
        WM_NCDESTROY => {
            unregister_local(inner, id);
        }
        _ => {}
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: Rect = Rect { left: 1920, top: 0, right: 3840, bottom: 1080 };

    #[test]
    fn initial_rects() {
        assert_eq!(initial_rect(B, Edge::Bottom, 48), Rect { top: 1032, ..B });
        assert_eq!(initial_rect(B, Edge::Top, 48), Rect { bottom: 48, ..B });
        assert_eq!(initial_rect(B, Edge::Left, 60), Rect { right: 1980, ..B });
        assert_eq!(initial_rect(B, Edge::Right, 60), Rect { left: 3780, ..B });
    }

    #[test]
    fn fit_after_querypos() {
        // Another bar already took 40px at the bottom: QUERYPOS moved our bottom up.
        let q = Rect { top: 1032, bottom: 1040, ..B };
        assert_eq!(fit_to_edge(q, Edge::Bottom, 48), Rect { top: 992, bottom: 1040, ..B });
        let q = Rect { top: 30, bottom: 48, ..B };
        assert_eq!(fit_to_edge(q, Edge::Top, 48), Rect { top: 30, bottom: 78, ..B });
        let q = Rect { left: 2000, right: 1980, ..B };
        assert_eq!(fit_to_edge(q, Edge::Left, 60).width(), 60);
        let q = Rect { left: 3780, right: 3800, ..B };
        assert_eq!(fit_to_edge(q, Edge::Right, 60), Rect { left: 3740, right: 3800, ..B });
    }
}
