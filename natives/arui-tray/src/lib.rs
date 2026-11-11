//! arui-tray: hosts the Windows notification area (system tray).
//!
//! `Shell_NotifyIconW` finds the first top-level window of class `Shell_TrayWnd` and sends it
//! WM_COPYDATA. [`start`] creates our own invisible, topmost `Shell_TrayWnd` (with a
//! `TrayNotifyWnd` child) so apps talk to ARUI instead of explorer, then broadcasts
//! `TaskbarCreated` so running apps re-add their icons.
//!
//! * dwData 1 (NIM_* icon messages): parsed by [`parse`], applied by [`state`], reported as
//!   [`arui_win::TrayEvent`]s, and also forwarded to explorer's tray so its state stays consistent.
//! * dwData 3 (`Shell_NotifyIconGetRect`): answered from [`TrayHost::set_icon_rects`].
//! * dwData 0 (`SHAppBarMessage`) and every other value: forwarded to explorer's real
//!   `Shell_TrayWnd`, and explorer's result is returned, so appbars keep working.
//!
//! Dropping the [`TrayHost`] destroys our windows and broadcasts `TaskbarCreated` again, so apps
//! re-register their icons with explorer.

pub mod parse;
pub mod state;

mod host;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;

use arui_win::util::to_hwnd;
use arui_win::{Error, EventSink, IconStore, Rect, Result, TrayAction, TrayIcon};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, SendNotifyMessageW};

use host::{Inner, Shared};
use state::{action_notifications, callback_params, TrayState};

/// Only one process-wide tray host can exist (window classes are per process).
static RUNNING: AtomicBool = AtomicBool::new(false);

/// Running tray host. Methods are callable from any thread.
pub struct TrayHost {
    shared: Arc<Shared>,
    tray_hwnd: u64,
    thread: Option<JoinHandle<()>>,
}

/// Starts the tray host on its own message-loop thread.
///
/// Events: one `TrayEvent::Snapshot` once the initial re-registration burst settles (at most
/// ~3 s after start), then `Added` / `Updated` / `Removed` / `Balloon` diffs. Icons are stored
/// in `icons` under `tray:<id>:<n>`; the key changes whenever the image changes.
pub fn start(sink: EventSink, icons: IconStore) -> Result<TrayHost> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err(Error::Other("tray host already running".into()));
    }
    let shared = Arc::new(Shared {
        inner: Mutex::new(Inner {
            state: TrayState::new(icons),
            live: false,
        }),
        rects: Mutex::new(HashMap::new()),
        sink,
    });
    let (tx, rx) = mpsc::channel();
    let s = shared.clone();
    let thread = std::thread::Builder::new()
        .name("arui-tray".into())
        .spawn(move || host::run(s, tx))
        .map_err(|e| {
            RUNNING.store(false, Ordering::SeqCst);
            Error::Other(format!("spawn tray thread: {e}"))
        })?;
    match rx.recv() {
        Ok(Ok(tray_hwnd)) => Ok(TrayHost {
            shared,
            tray_hwnd,
            thread: Some(thread),
        }),
        Ok(Err(e)) => {
            let _ = thread.join();
            RUNNING.store(false, Ordering::SeqCst);
            Err(e)
        }
        Err(_) => {
            let _ = thread.join();
            RUNNING.store(false, Ordering::SeqCst);
            Err(Error::Other("tray thread exited during setup".into()))
        }
    }
}

/// Explorer's real `Shell_TrayWnd` (the first one not owned by this process). While a
/// [`TrayHost`] runs, `FindWindow("Shell_TrayWnd")` returns *our* window, so code that wants to
/// hide / show explorer's taskbar must use this instead.
pub fn explorer_tray_hwnd() -> Option<arui_win::Hwnd> {
    let own = unsafe { windows::Win32::System::Threading::GetCurrentProcessId() };
    host::find_foreign_tray(own).map(arui_win::util::from_hwnd)
}

impl TrayHost {
    /// Current icons (including hidden ones, see `TrayIcon::visible`), in the order apps added
    /// them. Icons whose owner window died are dropped first.
    pub fn icons(&self) -> Vec<TrayIcon> {
        self.shared.purge_dead();
        self.shared.lock().state.icons()
    }

    /// Our `Shell_TrayWnd` window handle.
    pub fn hwnd(&self) -> u64 {
        self.tray_hwnd
    }

    /// Forwards a user interaction to the owning app via its callback message. `x`/`y` are the
    /// screen position (physical px) the app should anchor its menu / flyout at.
    ///
    /// For the app's menu to take focus, ARUI should be the foreground process when this is
    /// called (e.g. right after a click in an ARUI window); we grant the app foreground rights
    /// with AllowSetForegroundWindow, which only works if we have them.
    pub fn action(&self, id: &str, action: TrayAction, x: i32, y: i32) -> Result<()> {
        let rec = self
            .shared
            .lock()
            .state
            .get(id)
            .cloned()
            .ok_or_else(|| Error::NotFound(format!("tray icon {id}")))?;
        let owner = rec.icon.owner_hwnd;
        if !host::is_window(owner) {
            let (changes, live) = {
                let mut g = self.shared.lock();
                (g.state.remove_id(id), g.live)
            };
            if live {
                self.shared.emit(changes);
            }
            return Err(Error::NotFound(format!(
                "owner window of tray icon {id} is gone"
            )));
        }
        let cb = rec.icon.callback_message;
        if cb == 0 {
            return Ok(()); // the app never asked for notifications (no NIF_MESSAGE)
        }
        let hwnd = to_hwnd(owner);
        let pid = host::window_pid(hwnd);
        unsafe {
            // Fails harmlessly when we are not foreground ourselves.
            let _ = AllowSetForegroundWindow(if pid != 0 { pid } else { rec.pid });
            for n in action_notifications(rec.icon.version, rec.show_tip, action) {
                let (w, l) = callback_params(rec.icon.version, rec.icon.uid, n, x, y);
                SendNotifyMessageW(hwnd, cb, WPARAM(w), LPARAM(l))?;
            }
        }
        Ok(())
    }

    /// Where ARUI draws each icon (screen px). Replaces the previous set. Used to answer
    /// `Shell_NotifyIconGetRect`.
    pub fn set_icon_rects(&self, rects: Vec<(String, Rect)>) {
        *self.shared.rects.lock().unwrap_or_else(|e| e.into_inner()) = rects.into_iter().collect();
    }
}

impl Drop for TrayHost {
    fn drop(&mut self) {
        host::request_quit(self.tray_hwnd);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        RUNNING.store(false, Ordering::SeqCst);
    }
}
