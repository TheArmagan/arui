//! Shared state and the public [`WindowTracker`] handle.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use arui_win::util::to_hwnd;
use arui_win::{
    Error, EventSink, Hwnd, IconStore, NativeEvent, Rect, Result, WindowEvent, WindowInfo,
};
use parking_lot::Mutex;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::icons::IconJob;
use crate::probe::Identity;
use crate::thumbs::Thumb;
use crate::{actions, hook, icons, thumbs};

pub struct Tracked {
    pub info: WindowInfo,
    pub identity: Identity,
}

#[derive(Default)]
pub struct State {
    /// Tracked windows in the order they were first seen.
    pub order: Vec<Hwnd>,
    pub windows: HashMap<Hwnd, Tracked>,
    /// Tracked window that is (or owns) the foreground window.
    pub focused: Option<Hwnd>,
    pub ignored: HashSet<u32>,
    /// False while the initial snapshot is being built (no diff events yet).
    pub live: bool,
}

impl State {
    pub fn list(&self) -> Vec<WindowInfo> {
        self.order
            .iter()
            .filter_map(|h| self.windows.get(h).map(|t| t.info.clone()))
            .collect()
    }
}

pub struct Shared {
    pub state: Mutex<State>,
    pub icons: IconStore,
    /// Windows with an icon job in the queue (dedupes REDRAW storms).
    pub icon_pending: Mutex<HashSet<Hwnd>>,
    pub icon_tx: Sender<IconJob>,
    pub thumbs: Mutex<HashMap<(Hwnd, Hwnd), Thumb>>,
    events_tx: Sender<Option<NativeEvent>>,
}

impl Shared {
    /// Queues an event for the dispatcher thread. Call it while holding `state` so events
    /// leave in the same order the state changed.
    pub fn emit(&self, ev: WindowEvent) {
        let _ = self.events_tx.send(Some(NativeEvent::Window(ev)));
    }
}

/// Running window tracker. Dropping it unhooks everything and joins its threads.
///
/// Threads: a hook thread (WinEvent hooks + shell hook window + message loop), an icon worker
/// and an event dispatcher that calls the sink, so the sink may call back into the tracker.
/// All methods can be called from any thread.
pub struct WindowTracker {
    shared: Arc<Shared>,
    hook_hwnd: usize,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

/// Starts the tracker. Returns after the initial snapshot was taken; the sink receives
/// `WindowEvent::Snapshot` first, then diffs.
pub fn start(sink: EventSink, icons: IconStore) -> Result<WindowTracker> {
    let (events_tx, events_rx) = channel::<Option<NativeEvent>>();
    let (icon_tx, icon_rx) = channel::<IconJob>();
    let mut ignored = HashSet::new();
    ignored.insert(std::process::id());
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            ignored,
            ..Default::default()
        }),
        icons,
        icon_pending: Mutex::new(HashSet::new()),
        icon_tx,
        thumbs: Mutex::new(HashMap::new()),
        events_tx,
    });

    let dispatcher = thread::Builder::new()
        .name("arui-windows-events".into())
        .spawn(move || {
            while let Ok(Some(ev)) = events_rx.recv() {
                sink(ev);
            }
        })
        .map_err(|e| Error::Other(e.to_string()))?;

    let s = shared.clone();
    let worker = thread::Builder::new()
        .name("arui-windows-icons".into())
        .spawn(move || icons::run(s, icon_rx))
        .map_err(|e| Error::Other(e.to_string()))?;

    let (ready_tx, ready_rx) = channel::<Result<usize>>();
    let s = shared.clone();
    let hook = thread::Builder::new()
        .name("arui-windows-hooks".into())
        .spawn(move || hook::run(s, ready_tx))
        .map_err(|e| Error::Other(e.to_string()))?;

    let mut tracker = WindowTracker {
        shared,
        hook_hwnd: 0,
        threads: Mutex::new(vec![hook, worker, dispatcher]),
    };
    match ready_rx.recv() {
        Ok(Ok(hwnd)) => {
            tracker.hook_hwnd = hwnd;
            Ok(tracker)
        }
        Ok(Err(e)) => Err(e), // Drop of `tracker` stops the worker and dispatcher.
        Err(_) => Err(Error::Other(
            "window tracker thread died during start".into(),
        )),
    }
}

impl WindowTracker {
    /// Current taskbar windows, in first-seen order.
    pub fn list(&self) -> Vec<WindowInfo> {
        self.shared.state.lock().list()
    }

    /// Windows of these processes never appear (ARUI's own process is always ignored).
    /// Triggers a rescan that emits Added/Removed for windows that changed eligibility.
    pub fn set_ignored_pids(&self, pids: Vec<u32>) {
        {
            let mut st = self.shared.state.lock();
            st.ignored = pids.into_iter().collect();
            st.ignored.insert(std::process::id());
        }
        unsafe {
            let _ = PostMessageW(
                Some(to_hwnd(self.hook_hwnd as Hwnd)),
                hook::WM_APP_RESCAN,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }

    pub fn focus(&self, hwnd: Hwnd) -> Result<()> {
        actions::focus(hwnd)
    }

    /// Minimizes the window if it (or one of its owned popups) is the foreground window,
    /// otherwise restores and focuses it.
    pub fn toggle(&self, hwnd: Hwnd) -> Result<()> {
        actions::toggle(hwnd)
    }

    pub fn minimize(&self, hwnd: Hwnd) -> Result<()> {
        actions::minimize(hwnd)
    }

    pub fn maximize(&self, hwnd: Hwnd) -> Result<()> {
        actions::maximize(hwnd)
    }

    pub fn restore(&self, hwnd: Hwnd) -> Result<()> {
        actions::restore(hwnd)
    }

    /// Posts WM_CLOSE (the app may still ask to save).
    pub fn close(&self, hwnd: Hwnd) -> Result<()> {
        actions::close(hwnd)
    }

    /// Live DWM thumbnail of `source` drawn into top-level window `dest` (owned by this
    /// process) at `rect` (physical px, dest client coords), aspect-fit and centered.
    /// Calling again with the same pair just moves/resizes it.
    pub fn show_preview(&self, dest: Hwnd, source: Hwnd, rect: Rect) -> Result<()> {
        thumbs::show(&self.shared, dest, source, rect)
    }

    /// Removes the thumbnail of `source` on `dest`, or every thumbnail on `dest` for `None`.
    pub fn hide_preview(&self, dest: Hwnd, source: Option<Hwnd>) {
        thumbs::hide(&self.shared, dest, source)
    }
}

impl Drop for WindowTracker {
    fn drop(&mut self) {
        if self.hook_hwnd != 0 {
            unsafe {
                let _ = PostMessageW(
                    Some(to_hwnd(self.hook_hwnd as Hwnd)),
                    hook::WM_APP_QUIT,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }
        let _ = self.shared.icon_tx.send(IconJob::Stop);
        let _ = self.shared.events_tx.send(None);
        let me = thread::current().id();
        for t in self.threads.lock().drain(..) {
            // Dropping the tracker from inside the sink must not join the dispatcher itself.
            if t.thread().id() != me {
                let _ = t.join();
            }
        }
        thumbs::clear(&self.shared);
    }
}
