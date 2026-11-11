//! arui-shell: everything that makes ARUI "the shell".
//!
//! * hides explorer's taskbars (primary + secondary) and keeps them hidden, restores on drop
//! * registers host windows as real AppBars (SHAppBarMessage), so Windows itself reserves the
//!   work area; nothing is written with SPI_SETWORKAREA
//! * tracks monitors (bounds, work area, DPI) and explorer restarts (TaskbarCreated)
//! * indexes start menu apps (shortcuts + packaged apps) with icons, and launches them
//! * optionally captures a lone Win key press as [`ShellEvent::StartKey`]
//!
//! Threads: `arui-shell` (hidden window + WinEvent hook), `arui-shell-apps` (indexer + folder
//! watcher, STA), `arui-shell-keyhook` (only while start key capture is on). AppBar work runs on
//! the host window's own thread through a window subclass. All of it stops on [`Shell`] drop.
//!
//! Crash safety: a state file (see [`state`]) records what must be undone; `arui-guard` uses
//! [`emergency_restore_state`] when the host dies without dropping the [`Shell`].

mod apps;
mod appbar;
mod keyhook;
mod launch;
pub mod monitors;
pub mod state;
mod taskbar;
mod shellwin;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use arui_win::{AppEntry, AppKind, Edge, Error, EventSink, Hwnd, IconStore, MonitorInfo, NativeEvent, Rect, Result, ShellEvent};
use parking_lot::Mutex;

pub use launch::shell_execute;
pub use state::{default_state_file, ShellState};

#[derive(Debug, Clone, Copy, Default)]
pub struct ShellOptions {
    /// Hide explorer's taskbars while the shell runs.
    pub hide_explorer_taskbar: bool,
    /// Install the low-level keyboard hook for lone Win presses at start.
    pub capture_start_key: bool,
}

pub(crate) struct State {
    pub monitors: Vec<MonitorInfo>,
    pub apps: Vec<AppEntry>,
    pub apps_indexed: bool,
    pub appbars: HashMap<Hwnd, appbar::AppBar>,
    /// Explorer's ABM_GETSTATE before we forced auto-hide (Some while we changed it).
    pub explorer_prev_state: Option<u32>,
    /// Explorer's Shell_TrayWnd, to tell real explorer restarts from other TaskbarCreated senders.
    pub explorer_tray: Option<Hwnd>,
}

pub(crate) struct Inner {
    pub sink: EventSink,
    pub icons: IconStore,
    pub state_path: PathBuf,
    pub st: Mutex<State>,
    /// Desired explorer taskbar visibility; read by the WinEvent hook.
    pub hide: AtomicBool,
    /// Set when the Shell is dropped; leftover subclasses turn into pass-throughs.
    pub closed: AtomicBool,
}

impl Inner {
    pub fn emit(&self, e: ShellEvent) {
        (self.sink)(NativeEvent::Shell(e));
    }

    pub fn snapshot_state(&self) -> ShellState {
        let st = self.st.lock();
        let mut appbars: Vec<Hwnd> = st.appbars.keys().copied().collect();
        appbars.sort_unstable();
        ShellState {
            pid: std::process::id(),
            appbars,
            explorer_hidden: self.hide.load(Ordering::SeqCst),
            explorer_prev_state: st.explorer_prev_state,
        }
    }

    pub fn save_state(&self) {
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        if let Err(e) = self.snapshot_state().save(&self.state_path) {
            log::warn!("could not write {}: {e}", self.state_path.display());
        }
    }

    /// Forces explorer's taskbar to auto-hide (so it reserves no work area) and hides all of its
    /// taskbar windows. Idempotent; the original state is captured only once.
    pub fn hide_explorer(&self) {
        self.hide.store(true, Ordering::SeqCst);
        let tray = taskbar::tray_id();
        self.st.lock().explorer_tray = tray;
        if tray.is_none() {
            // Explorer not running; TaskbarCreated will bring us back here.
            self.save_state();
            return;
        }
        let prev = {
            let mut st = self.st.lock();
            *st.explorer_prev_state.get_or_insert_with(taskbar::get_state)
        };
        // Persist the original state before changing it.
        self.save_state();
        let want = taskbar::hidden_state(prev);
        if taskbar::get_state() != want {
            taskbar::set_state(want);
        }
        taskbar::hide_windows();
    }

    pub fn restore_explorer(&self) {
        self.hide.store(false, Ordering::SeqCst);
        let prev = self.st.lock().explorer_prev_state.take();
        if let Some(p) = prev {
            taskbar::set_state(p);
        }
        taskbar::show_windows();
        self.save_state();
    }
}

/// Handle to the running shell service. Dropping it unregisters AppBars, restores explorer's
/// taskbar and stops all threads.
pub struct Shell {
    inner: Arc<Inner>,
    key_hook: Mutex<Option<keyhook::KeyHook>>,
    apps_thread: Option<apps::AppsThread>,
    shell_thread: Option<shellwin::ShellThread>,
}

/// Starts the shell service. Call once per process.
pub fn start(sink: EventSink, icons: IconStore, opts: ShellOptions) -> Result<Shell> {
    start_with_state_file(sink, icons, opts, default_state_file())
}

/// Like [`start`] with a custom crash-recovery state file (pass the same path to arui-guard).
pub fn start_with_state_file(
    sink: EventSink,
    icons: IconStore,
    opts: ShellOptions,
    state_path: PathBuf,
) -> Result<Shell> {
    recover_stale_state(&state_path);

    let inner = Arc::new(Inner {
        sink,
        icons,
        state_path,
        st: Mutex::new(State {
            monitors: Vec::new(),
            apps: Vec::new(),
            apps_indexed: false,
            appbars: HashMap::new(),
            explorer_prev_state: None,
            explorer_tray: taskbar::tray_id(),
        }),
        hide: AtomicBool::new(false),
        closed: AtomicBool::new(false),
    });
    inner.save_state();

    let shell_thread = shellwin::spawn(inner.clone())?;
    let mut shell = Shell {
        inner: inner.clone(),
        key_hook: Mutex::new(None),
        apps_thread: None,
        shell_thread: Some(shell_thread),
    };
    if opts.hide_explorer_taskbar {
        inner.hide_explorer();
    }
    shell.apps_thread = Some(apps::AppsThread::spawn(inner)?);
    if opts.capture_start_key {
        shell.set_start_key_capture(true);
    }
    Ok(shell)
}

/// A state file left by a previous instance that died without cleanup (and without a guard):
/// undo its changes before we start capturing "previous" explorer state again.
fn recover_stale_state(path: &Path) {
    let Some(st) = ShellState::load(path) else { return };
    if st.pid == std::process::id() {
        return;
    }
    if process_alive(st.pid) {
        log::warn!("state file belongs to running pid {}; taking it over", st.pid);
    } else {
        log::info!("restoring state left by crashed pid {}", st.pid);
        emergency_restore_state(&st);
    }
    ShellState::delete(path);
}

fn process_alive(pid: u32) -> bool {
    use windows::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
    unsafe {
        match OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            Ok(h) => {
                let alive = WaitForSingleObject(h, 0) == WAIT_TIMEOUT;
                let _ = CloseHandle(h);
                alive
            }
            Err(_) => false,
        }
    }
}

impl Shell {
    /// Current monitors (fresh enumeration, physical pixels, sorted by id).
    pub fn monitors(&self) -> Vec<MonitorInfo> {
        monitors::enumerate()
    }

    /// Registers `hwnd` (a top-level window owned by the host) as an AppBar on `edge` of the
    /// monitor `monitor_id` ([`MonitorInfo::id`]). With `reserve == true` the work area shrinks
    /// by `thickness_px` (physical pixels); otherwise the bar only gets AppBar notifications and
    /// is placed over the monitor edge. Moves the window and returns its final rect.
    ///
    /// **Must be called on the thread that owns `hwnd`** (the host's UI/main thread, e.g. via
    /// Tauri's `run_on_main_thread`), because it subclasses the window; returns
    /// `Error::Unsupported` otherwise. Calling it again for the same window updates its settings.
    /// The bar is unregistered automatically when the window is destroyed.
    pub fn register_appbar(&self, hwnd: Hwnd, monitor_id: &str, edge: Edge, thickness_px: i32, reserve: bool) -> Result<Rect> {
        appbar::register(&self.inner, hwnd, monitor_id, edge, thickness_px, reserve)
    }

    /// Changes the thickness of a registered bar. Any thread (forwarded to the owner thread with
    /// SendMessageTimeout when needed; do not call while blocking the owner thread).
    pub fn update_appbar(&self, hwnd: Hwnd, thickness_px: i32) -> Result<Rect> {
        appbar::update(&self.inner, hwnd, thickness_px)
    }

    /// ABM_REMOVE + unsubclass. Any thread (same forwarding rules as [`Self::update_appbar`]).
    pub fn unregister_appbar(&self, hwnd: Hwnd) {
        appbar::unregister(&self.inner, hwnd);
    }

    /// Last indexed start menu apps (empty until the first [`ShellEvent::AppsChanged`]).
    pub fn apps(&self) -> Vec<AppEntry> {
        self.inner.st.lock().apps.clone()
    }

    /// Re-indexes the start menu in the background; emits AppsChanged if anything changed.
    pub fn refresh_apps(&self) {
        if let Some(t) = &self.apps_thread {
            t.refresh();
        }
    }

    /// Launches an app from [`Self::apps`] by id. Unknown ids that look like a packaged AUMID are
    /// launched directly. Blocking (ShellExecuteEx); any thread.
    pub fn launch_app(&self, id: &str) -> Result<()> {
        let entry = self.inner.st.lock().apps.iter().find(|a| a.id == id).cloned();
        match entry {
            Some(AppEntry { kind: AppKind::Shortcut, target, .. }) => shell_execute(&target, None, None),
            Some(AppEntry { kind: AppKind::Packaged, target, .. }) => launch_aumid(&target),
            None if apps::is_packaged_aumid(id) => launch_aumid(id),
            None => Err(Error::NotFound(format!("app {id}"))),
        }
    }

    /// Opens shell-level paths/URLs/verbs (`ShellExecuteExW`). Blocking; any thread.
    pub fn shell_execute(&self, path: &str, args: Option<&str>, verb: Option<&str>) -> Result<()> {
        shell_execute(path, args, verb)
    }

    /// Opens the Windows Start menu by injecting a Win key tap that our own hook ignores.
    pub fn open_windows_start(&self) -> Result<()> {
        keyhook::tap_win();
        Ok(())
    }

    /// Installs / removes the low-level keyboard hook for lone Win presses.
    pub fn set_start_key_capture(&self, enabled: bool) {
        let mut hook = self.key_hook.lock();
        if enabled && hook.is_none() {
            match keyhook::KeyHook::spawn(self.inner.sink.clone()) {
                Ok(h) => *hook = Some(h),
                Err(e) => log::error!("start key hook failed: {e}"),
            }
        } else if !enabled {
            hook.take();
        }
    }

    /// Hides explorer's taskbars (again). Done by [`start`] when `hide_explorer_taskbar` is set.
    pub fn hide_explorer_taskbar(&self) {
        self.inner.hide_explorer();
    }

    /// Shows explorer's taskbars and restores its original auto-hide state. Also done on drop.
    pub fn restore_explorer_taskbar(&self) {
        self.inner.restore_explorer();
    }

    /// Path of the crash-recovery state file (pass it to arui-guard with `--state-file`).
    pub fn state_file(&self) -> &Path {
        &self.inner.state_path
    }
}

impl Drop for Shell {
    fn drop(&mut self) {
        self.key_hook.lock().take();
        let bars: Vec<Hwnd> = self.inner.st.lock().appbars.keys().copied().collect();
        for h in bars {
            appbar::unregister(&self.inner, h);
        }
        if self.inner.hide.load(Ordering::SeqCst) || self.inner.st.lock().explorer_prev_state.is_some() {
            self.inner.restore_explorer();
        }
        self.inner.closed.store(true, Ordering::SeqCst);
        self.shell_thread.take();
        self.apps_thread.take();
        ShellState::delete(&self.inner.state_path);
    }
}

fn launch_aumid(aumid: &str) -> Result<()> {
    shell_execute(&format!("shell:AppsFolder\\{aumid}"), None, None)
}

/// Undoes everything described by `state`: removes the AppBars, restores explorer's auto-hide
/// state and shows its taskbars. Safe to call from any process (used by arui-guard).
pub fn emergency_restore_state(state: &ShellState) {
    for &h in &state.appbars {
        taskbar::remove_appbar(h);
    }
    if let Some(p) = state.explorer_prev_state {
        taskbar::set_state(p);
    }
    taskbar::show_windows();
}

/// Stateless restore: shows Shell_TrayWnd + every Shell_SecondaryTrayWnd and removes the given
/// AppBars. If the default state file exists, explorer's original auto-hide state is restored
/// from it too (and the file is deleted).
pub fn emergency_restore(appbar_hwnds: &[Hwnd]) {
    let path = default_state_file();
    let mut st = ShellState::load(&path).unwrap_or_default();
    for h in appbar_hwnds {
        if !st.appbars.contains(h) {
            st.appbars.push(*h);
        }
    }
    emergency_restore_state(&st);
    ShellState::delete(&path);
}
