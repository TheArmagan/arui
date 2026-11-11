//! The shell thread: a hidden top-level window plus a WinEvent hook.
//!
//! * `TaskbarCreated` is broadcast to top-level windows only, so the window is a hidden
//!   WS_POPUP (not HWND_MESSAGE).
//! * WM_DISPLAYCHANGE / WM_DPICHANGED / WM_SETTINGCHANGE(SPI_SETWORKAREA) refresh the monitor list.
//! * An out-of-context EVENT_OBJECT_SHOW hook on explorer's process re-hides its taskbars
//!   whenever explorer shows them again (auto-hide reveal, new monitor, settings change).

use std::cell::RefCell;
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc, OnceLock};
use std::thread::JoinHandle;

use arui_win::util::{from_hwnd, to_hwnd, wide};
use arui_win::{Error, Hwnd, Result, ShellEvent};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    ChangeWindowMessageFilterEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, PostMessageW, PostQuitMessage, RegisterClassExW, RegisterWindowMessageW,
    ShowWindow, TranslateMessage, EVENT_OBJECT_SHOW, MSG, MSGFLT_ALLOW, OBJID_WINDOW,
    SPI_SETWORKAREA, SW_HIDE, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_CLOSE,
    WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_SETTINGCHANGE, WNDCLASSEXW, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_POPUP,
};

use crate::monitors::{self, DpiScope};
use crate::{appbar, taskbar, Inner};

const CLASS: &str = "ARUI_ShellWindow";

thread_local! {
    static CTX: RefCell<Option<Arc<Inner>>> = const { RefCell::new(None) };
    static HOOK: RefCell<Option<HWINEVENTHOOK>> = const { RefCell::new(None) };
}

fn taskbar_created_msg() -> u32 {
    static M: OnceLock<u32> = OnceLock::new();
    *M.get_or_init(|| {
        let w = wide("TaskbarCreated");
        unsafe { RegisterWindowMessageW(PCWSTR(w.as_ptr())) }
    })
}

pub(crate) struct ShellThread {
    hwnd: Hwnd,
    join: Option<JoinHandle<()>>,
}

impl Drop for ShellThread {
    fn drop(&mut self) {
        unsafe {
            let _ = PostMessageW(Some(to_hwnd(self.hwnd)), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

pub(crate) fn spawn(inner: Arc<Inner>) -> Result<ShellThread> {
    let (tx, rx) = mpsc::channel::<Result<Hwnd>>();
    let join = std::thread::Builder::new()
        .name("arui-shell".into())
        .spawn(move || run(inner, tx))
        .map_err(|e| Error::Other(e.to_string()))?;
    match rx.recv() {
        Ok(Ok(hwnd)) => Ok(ShellThread { hwnd, join: Some(join) }),
        Ok(Err(e)) => {
            let _ = join.join();
            Err(e)
        }
        Err(_) => Err(Error::Other("shell thread died during start".into())),
    }
}

fn run(inner: Arc<Inner>, tx: mpsc::Sender<Result<Hwnd>>) {
    let _dpi = DpiScope::per_monitor();
    let hwnd = match create_window() {
        Ok(h) => h,
        Err(e) => {
            let _ = tx.send(Err(e));
            return;
        }
    };
    unsafe {
        // Allow the broadcast through UIPI when we run elevated.
        let _ = ChangeWindowMessageFilterEx(hwnd, taskbar_created_msg(), MSGFLT_ALLOW, None);
    }
    inner.st.lock().monitors = monitors::enumerate();
    CTX.with(|c| *c.borrow_mut() = Some(inner.clone()));
    install_hook();
    let _ = tx.send(Ok(from_hwnd(hwnd)));
    drop(tx);

    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    uninstall_hook();
    CTX.with(|c| c.borrow_mut().take());
}

fn create_window() -> Result<HWND> {
    unsafe {
        let hinst = HINSTANCE(GetModuleHandleW(None)?.0);
        let class = wide(CLASS);
        static REGISTERED: OnceLock<()> = OnceLock::new();
        REGISTERED.get_or_init(|| {
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(wnd_proc),
                hInstance: hinst,
                lpszClassName: PCWSTR(class.as_ptr()),
                ..Default::default()
            };
            RegisterClassExW(&wc);
        });
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            PCWSTR(class.as_ptr()),
            PCWSTR(class.as_ptr()),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinst),
            None,
        )?;
        Ok(hwnd)
    }
}

fn install_hook() {
    uninstall_hook();
    let Some(pid) = taskbar::explorer_pid() else { return };
    let h = unsafe {
        SetWinEventHook(
            EVENT_OBJECT_SHOW,
            EVENT_OBJECT_SHOW,
            None,
            Some(win_event_proc),
            pid,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    };
    if !h.is_invalid() {
        HOOK.with(|c| *c.borrow_mut() = Some(h));
    }
}

fn uninstall_hook() {
    if let Some(h) = HOOK.with(|c| c.borrow_mut().take()) {
        unsafe {
            let _ = UnhookWinEvent(h);
        }
    }
}

unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if event != EVENT_OBJECT_SHOW || id_object != OBJID_WINDOW.0 || id_child != 0 {
        return;
    }
    let hide = CTX.with(|c| c.borrow().as_ref().is_some_and(|i| i.hide.load(Ordering::SeqCst)));
    if hide && taskbar::is_taskbar_class(&taskbar::class_name(hwnd)) {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

fn with_inner(f: impl FnOnce(&Arc<Inner>)) {
    let inner = CTX.with(|c| c.borrow().clone());
    if let Some(i) = inner {
        f(&i);
    }
}

fn refresh_monitors(inner: &Inner, reposition_bars: bool) {
    let new = monitors::enumerate();
    let changed = {
        let mut st = inner.st.lock();
        let changed = monitors::changed(&st.monitors, &new);
        if changed {
            st.monitors = new.clone();
        }
        changed
    };
    if changed {
        inner.emit(ShellEvent::MonitorsChanged(new));
    }
    if reposition_bars {
        appbar::post_all(inner, appbar::CMD_REPOSITION);
    }
}

fn on_taskbar_created(inner: &Inner) {
    let new_tray = taskbar::tray_id();
    let old_tray = std::mem::replace(&mut inner.st.lock().explorer_tray, new_tray);
    install_hook();
    if inner.hide.load(Ordering::SeqCst) {
        inner.hide_explorer();
    }
    appbar::post_all(inner, appbar::CMD_REREGISTER);
    // arui-tray also broadcasts TaskbarCreated; only a new explorer tray window is a restart.
    if new_tray.is_some() && new_tray != old_tray {
        log::info!("explorer restarted");
        inner.emit(ShellEvent::ExplorerRestarted);
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == taskbar_created_msg() {
        with_inner(|i| on_taskbar_created(i));
        return LRESULT(0);
    }
    match msg {
        WM_DISPLAYCHANGE | WM_DPICHANGED => {
            with_inner(|i| refresh_monitors(i, true));
            LRESULT(0)
        }
        WM_SETTINGCHANGE => {
            if wp.0 as u32 == SPI_SETWORKAREA.0 {
                with_inner(|i| refresh_monitors(i, false));
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
