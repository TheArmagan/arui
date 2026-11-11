//! Hook thread: WinEvent hooks, a hidden shell-hook window and the message loop.
//!
//! WinEvent callbacks (WINEVENT_OUTOFCONTEXT) and shell hook messages are delivered on this
//! thread while it pumps messages. Every notification goes through a small queue so that a
//! callback re-entered while we are busy (for example during a COM call that pumps) is only
//! queued, never processed recursively.

use std::cell::{Cell, RefCell};
use std::collections::{HashSet, VecDeque};
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::sync::Arc;

use arui_win::util::{from_hwnd, wide, ComGuard};
use arui_win::{Error, Hwnd, Result, WindowEvent, WindowInfo};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::icons::{app_key, exe_key, IconJob};
use crate::probe::{self, Identity};
use crate::rules::{diff_one, is_eligible};
use crate::thumbs;
use crate::tracker::{Shared, Tracked};

pub const WM_APP_RESCAN: u32 = WM_APP + 1;
pub const WM_APP_QUIT: u32 = WM_APP + 2;

const LOCATION_TIMER: usize = 1;
/// Location changes are coalesced: at most one refresh per window per this many ms.
const LOCATION_COALESCE_MS: u32 = 100;

// Shell hook codes (WinUser.h).
const HSHELL_WINDOWCREATED: u32 = 1;
const HSHELL_WINDOWDESTROYED: u32 = 2;
const HSHELL_WINDOWACTIVATED: u32 = 4;
const HSHELL_REDRAW: u32 = 6;
const HSHELL_MONITORCHANGED: u32 = 16;
const HSHELL_HIGHBIT: u32 = 0x8000;
const HSHELL_FLASH: u32 = HSHELL_REDRAW | HSHELL_HIGHBIT;
const HSHELL_RUDEAPPACTIVATED: u32 = HSHELL_WINDOWACTIVATED | HSHELL_HIGHBIT;

const OBJID_WINDOW: i32 = 0;
const CHILDID_SELF: i32 = 0;

enum Note {
    WinEvent { event: u32, hwnd: Hwnd },
    Shell { code: u32, hwnd: Hwnd },
    LocationFlush,
    Rescan,
    Snapshot,
}

struct Ctx {
    shared: Arc<Shared>,
    hwnd: HWND,
    shell_msg: u32,
    hooks: RefCell<Vec<HWINEVENTHOOK>>,
    pending_location: RefCell<HashSet<Hwnd>>,
    timer_armed: Cell<bool>,
}

thread_local! {
    static CTX: RefCell<Option<Rc<Ctx>>> = const { RefCell::new(None) };
    static QUEUE: RefCell<VecDeque<Note>> = const { RefCell::new(VecDeque::new()) };
    static BUSY: Cell<bool> = const { Cell::new(false) };
}

fn ctx() -> Option<Rc<Ctx>> {
    CTX.with(|c| c.borrow().clone())
}

/// Queues a notification and drains the queue unless we are already draining it.
fn submit(note: Note) {
    QUEUE.with(|q| q.borrow_mut().push_back(note));
    if BUSY.with(|b| b.replace(true)) {
        return;
    }
    if let Some(ctx) = ctx() {
        while let Some(n) = QUEUE.with(|q| q.borrow_mut().pop_front()) {
            ctx.handle(n);
        }
    } else {
        QUEUE.with(|q| q.borrow_mut().clear());
    }
    BUSY.with(|b| b.set(false));
}

pub fn run(shared: Arc<Shared>, ready: Sender<Result<usize>>) {
    let _com = ComGuard::sta();
    let ctx = match setup(shared) {
        Ok(c) => c,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    CTX.with(|c| *c.borrow_mut() = Some(ctx.clone()));
    ctx.install_hooks();
    // Hooks are in place before enumerating, so nothing created meanwhile is missed; those
    // notifications wait in the queue and are applied as diffs after the snapshot.
    submit(Note::Snapshot);
    let _ = ready.send(Ok(ctx.hwnd.0 as usize));

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    CTX.with(|c| *c.borrow_mut() = None);
}

fn setup(shared: Arc<Shared>) -> Result<Rc<Ctx>> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = wide("ARUI.WindowTracker");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: PCWSTR(class.as_ptr()),
            ..Default::default()
        };
        // Fails harmlessly with "class already exists" when a tracker was started before.
        RegisterClassExW(&wc);
        // A real (hidden) top-level window: message-only windows don't get shell hooks.
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            PCWSTR(class.as_ptr()),
            PCWSTR(wide("ARUI window tracker").as_ptr()),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        let shell_msg = RegisterWindowMessageW(PCWSTR(wide("SHELLHOOK").as_ptr()));
        if !RegisterShellHookWindow(hwnd).as_bool() {
            let _ = DestroyWindow(hwnd);
            return Err(Error::Other("RegisterShellHookWindow failed".into()));
        }
        Ok(Rc::new(Ctx {
            shared,
            hwnd,
            shell_msg,
            hooks: RefCell::new(Vec::new()),
            pending_location: RefCell::new(HashSet::new()),
            timer_armed: Cell::new(false),
        }))
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
    if id_object != OBJID_WINDOW || id_child != CHILDID_SELF {
        return;
    }
    if hwnd.is_invalid() && event != EVENT_SYSTEM_FOREGROUND {
        return;
    }
    submit(Note::WinEvent {
        event,
        hwnd: from_hwnd(hwnd),
    });
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(ctx) = ctx() else {
        return DefWindowProcW(hwnd, msg, wp, lp);
    };
    if msg == ctx.shell_msg && msg != 0 {
        submit(Note::Shell {
            code: wp.0 as u32,
            hwnd: lp.0 as usize as Hwnd,
        });
        return LRESULT(0);
    }
    match msg {
        WM_TIMER if wp.0 == LOCATION_TIMER => {
            submit(Note::LocationFlush);
            LRESULT(0)
        }
        WM_APP_RESCAN => {
            submit(Note::Rescan);
            LRESULT(0)
        }
        WM_APP_QUIT => {
            ctx.shutdown();
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

impl Ctx {
    fn install_hooks(&self) {
        let skip = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        let ranges = [
            (EVENT_OBJECT_CREATE, EVENT_OBJECT_HIDE, skip), // CREATE, DESTROY, SHOW, HIDE
            (EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_NAMECHANGE, skip),
            (EVENT_OBJECT_PARENTCHANGE, EVENT_OBJECT_PARENTCHANGE, skip),
            (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED, skip),
            (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND, skip),
            // Not skipping our own process here, so the UI learns when ARUI itself (start
            // menu, popups) takes the foreground: that reports `Focused { hwnd: None }`.
            (
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                WINEVENT_OUTOFCONTEXT,
            ),
        ];
        let mut hooks = self.hooks.borrow_mut();
        for (min, max, flags) in ranges {
            let h = unsafe { SetWinEventHook(min, max, None, Some(win_event_proc), 0, 0, flags) };
            if h.is_invalid() {
                log::warn!("SetWinEventHook({min:#x}..{max:#x}) failed");
            } else {
                hooks.push(h);
            }
        }
    }

    fn shutdown(&self) {
        unsafe {
            for h in self.hooks.borrow_mut().drain(..) {
                let _ = UnhookWinEvent(h);
            }
            let _ = KillTimer(Some(self.hwnd), LOCATION_TIMER);
            let _ = DeregisterShellHookWindow(self.hwnd);
            let _ = DestroyWindow(self.hwnd);
        }
    }

    fn handle(&self, note: Note) {
        match note {
            Note::WinEvent { event, hwnd } => self.on_win_event(event, hwnd),
            Note::Shell { code, hwnd } => self.on_shell(code, hwnd),
            Note::LocationFlush => self.flush_location(),
            Note::Rescan => self.rescan(),
            Note::Snapshot => self.snapshot(),
        }
    }

    fn is_tracked(&self, hwnd: Hwnd) -> bool {
        self.shared.state.lock().windows.contains_key(&hwnd)
    }

    fn on_win_event(&self, event: u32, hwnd: Hwnd) {
        match event {
            EVENT_SYSTEM_FOREGROUND => self.on_foreground(hwnd),
            EVENT_OBJECT_DESTROY => self.destroyed(hwnd),
            EVENT_OBJECT_HIDE => {
                if self.is_tracked(hwnd) {
                    self.refresh(hwnd, false);
                }
            }
            EVENT_OBJECT_LOCATIONCHANGE => {
                if self.is_tracked(hwnd) || thumbs::has_source(&self.shared, hwnd) {
                    self.schedule_location(hwnd);
                }
            }
            EVENT_OBJECT_CREATE
            | EVENT_OBJECT_SHOW
            | EVENT_OBJECT_NAMECHANGE
            | EVENT_OBJECT_PARENTCHANGE
            | EVENT_OBJECT_CLOAKED
            | EVENT_OBJECT_UNCLOAKED
            | EVENT_SYSTEM_MINIMIZESTART
            | EVENT_SYSTEM_MINIMIZEEND => {
                if probe::is_top_level(hwnd) {
                    self.refresh(hwnd, false);
                } else if matches!(
                    event,
                    EVENT_OBJECT_CREATE | EVENT_OBJECT_SHOW | EVENT_OBJECT_PARENTCHANGE
                ) {
                    // A UWP CoreWindow attaching to its ApplicationFrameWindow: resolve the
                    // frame's real app identity now.
                    let root = probe::root(hwnd);
                    let unresolved = self
                        .shared
                        .state
                        .lock()
                        .windows
                        .get(&root)
                        .is_some_and(|t| !t.identity.resolved);
                    if unresolved {
                        self.refresh(root, false);
                    }
                }
            }
            _ => {}
        }
    }

    fn on_shell(&self, code: u32, hwnd: Hwnd) {
        match code {
            HSHELL_WINDOWCREATED | HSHELL_MONITORCHANGED => self.refresh(hwnd, false),
            HSHELL_WINDOWDESTROYED => self.destroyed(hwnd),
            HSHELL_WINDOWACTIVATED | HSHELL_RUDEAPPACTIVATED => self.on_foreground(hwnd),
            HSHELL_REDRAW => self.refresh(hwnd, true),
            HSHELL_FLASH => self.on_flash(hwnd),
            _ => {}
        }
    }

    fn schedule_location(&self, hwnd: Hwnd) {
        self.pending_location.borrow_mut().insert(hwnd);
        if !self.timer_armed.replace(true) {
            unsafe { SetTimer(Some(self.hwnd), LOCATION_TIMER, LOCATION_COALESCE_MS, None) };
        }
    }

    fn flush_location(&self) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), LOCATION_TIMER);
        }
        self.timer_armed.set(false);
        let pending: Vec<Hwnd> = self.pending_location.borrow_mut().drain().collect();
        for h in pending {
            if self.is_tracked(h) {
                self.refresh(h, false);
            }
            thumbs::source_moved(&self.shared, h);
        }
    }

    /// Re-reads a window and applies Added / Updated / Removed.
    fn refresh(&self, hwnd: Hwnd, recheck_icon: bool) {
        if hwnd == 0 {
            return;
        }
        let attrs = probe::attrs(hwnd);
        let eligible = attrs
            .as_ref()
            .is_some_and(|a| is_eligible(a, &self.shared.state.lock().ignored));
        let Some(a) = attrs.filter(|_| eligible) else {
            self.remove(hwnd);
            return;
        };

        let cached = self
            .shared
            .state
            .lock()
            .windows
            .get(&hwnd)
            .map(|t| t.identity.clone());
        let identity = match cached {
            Some(id) if id.resolved => id,
            _ => probe::identity(hwnd, a.pid, &a.class_name),
        };
        let pl = probe::placement(hwnd);

        let mut st = self.shared.state.lock();
        let focused = st.focused == Some(hwnd);
        let prev = st.windows.get(&hwnd);
        let identity_changed = prev.is_some_and(|t| t.identity != identity);
        let info = WindowInfo {
            hwnd,
            pid: a.pid,
            title: a.title,
            class_name: a.class_name,
            exe_path: identity.exe_path.clone(),
            app_id: identity.app_id.clone(),
            icon_key: match prev {
                Some(t) if !identity_changed => t.info.icon_key.clone(),
                _ => self.initial_icon_key(&identity),
            },
            monitor: pl.monitor,
            is_minimized: pl.minimized,
            is_maximized: pl.maximized,
            is_focused: focused,
            is_flashing: prev.is_some_and(|t| t.info.is_flashing) && !focused,
        };
        let is_new = prev.is_none();
        let event = diff_one(prev.map(|t| &t.info), Some(&info));
        let job = (is_new || recheck_icon || identity_changed).then(|| IconJob::Window {
            hwnd,
            exe_path: identity.exe_path.clone(),
            package_aumid: identity.package_aumid.clone(),
        });
        if is_new {
            st.order.push(hwnd);
        }
        st.windows.insert(hwnd, Tracked { info, identity });
        if st.live {
            if let Some(ev) = event {
                self.shared.emit(ev);
            }
        }
        drop(st);
        if let Some(job) = job {
            if self.shared.icon_pending.lock().insert(hwnd) {
                let _ = self.shared.icon_tx.send(job);
            }
        }
    }

    /// Best icon key known right now; the worker refines it and emits Updated.
    fn initial_icon_key(&self, id: &Identity) -> String {
        if let Some(a) = &id.package_aumid {
            let k = app_key(a);
            if self.shared.icons.contains(&k) {
                return k;
            }
        }
        let k = exe_key(&id.exe_path);
        if !id.exe_path.is_empty() && self.shared.icons.contains(&k) {
            return k;
        }
        String::new()
    }

    fn remove(&self, hwnd: Hwnd) {
        let removed = {
            let mut st = self.shared.state.lock();
            let removed = st.windows.remove(&hwnd).is_some();
            if removed {
                st.order.retain(|h| *h != hwnd);
                if st.focused == Some(hwnd) {
                    st.focused = None;
                }
                if st.live {
                    self.shared.emit(WindowEvent::Removed { hwnd });
                }
            }
            removed
        };
        if removed {
            self.shared.icon_pending.lock().remove(&hwnd);
            let _ = self.shared.icon_tx.send(IconJob::Forget(hwnd));
        }
    }

    fn destroyed(&self, hwnd: Hwnd) {
        self.remove(hwnd);
        thumbs::source_gone(&self.shared, hwnd);
    }

    fn on_foreground(&self, hwnd: Hwnd) {
        if hwnd != 0 && probe::is_top_level(hwnd) && !self.is_tracked(hwnd) {
            self.refresh(hwnd, false);
        }
        let target = if hwnd == 0 {
            None
        } else if self.is_tracked(hwnd) {
            Some(hwnd)
        } else {
            // Owned dialogs highlight their owner's button.
            let owner = probe::root_owner(hwnd);
            (owner != 0 && self.is_tracked(owner)).then_some(owner)
        };
        self.set_focused(target);
    }

    fn set_focused(&self, target: Option<Hwnd>) {
        let mut st = self.shared.state.lock();
        if st.focused == target {
            return;
        }
        if let Some(old) = st.focused.and_then(|h| st.windows.get_mut(&h)) {
            old.info.is_focused = false;
        }
        st.focused = target;
        let mut cleared_flash = None;
        if let Some(t) = target.and_then(|h| st.windows.get_mut(&h)) {
            t.info.is_focused = true;
            if t.info.is_flashing {
                t.info.is_flashing = false;
                cleared_flash = Some(t.info.clone());
            }
        }
        if st.live {
            // is_focused changes are announced by Focused only; Updated is for other fields.
            self.shared.emit(WindowEvent::Focused { hwnd: target });
            if let Some(info) = cleared_flash {
                self.shared.emit(WindowEvent::Updated(info));
            }
        }
    }

    fn on_flash(&self, hwnd: Hwnd) {
        if !self.is_tracked(hwnd) {
            self.refresh(hwnd, false);
        }
        let mut st = self.shared.state.lock();
        let focused = st.focused == Some(hwnd);
        let live = st.live;
        if let Some(t) = st.windows.get_mut(&hwnd) {
            if !focused && !t.info.is_flashing {
                t.info.is_flashing = true;
                let info = t.info.clone();
                if live {
                    self.shared.emit(WindowEvent::Updated(info));
                }
            }
        }
    }

    fn enum_top_level() -> Vec<Hwnd> {
        unsafe extern "system" fn cb(h: HWND, lp: LPARAM) -> windows::core::BOOL {
            let v = &mut *(lp.0 as *mut Vec<Hwnd>);
            v.push(from_hwnd(h));
            true.into()
        }
        let mut v: Vec<Hwnd> = Vec::new();
        unsafe {
            let _ = EnumWindows(Some(cb), LPARAM(&mut v as *mut _ as isize));
        }
        v
    }

    /// Re-evaluates every top-level window (after ignored pids changed).
    fn rescan(&self) {
        let all = Self::enum_top_level();
        let alive: HashSet<Hwnd> = all.iter().copied().collect();
        for h in all {
            self.refresh(h, false);
        }
        let gone: Vec<Hwnd> = {
            let st = self.shared.state.lock();
            st.order
                .iter()
                .copied()
                .filter(|h| !alive.contains(h))
                .collect()
        };
        for h in gone {
            self.remove(h);
        }
    }

    /// Initial enumeration; emits one Snapshot and switches to diff mode.
    fn snapshot(&self) {
        // EnumWindows is top of z-order first; reverse it so the most recently used windows
        // end up last, which is closer to "order of appearance" than z-order.
        let mut all = Self::enum_top_level();
        all.reverse();
        for h in all {
            self.refresh(h, false);
        }
        let fg = unsafe { from_hwnd(GetForegroundWindow()) };
        let target = if self.is_tracked(fg) {
            Some(fg)
        } else {
            let owner = if fg == 0 { 0 } else { probe::root_owner(fg) };
            (owner != 0 && self.is_tracked(owner)).then_some(owner)
        };
        self.set_focused(target);
        let mut st = self.shared.state.lock();
        st.live = true;
        let list = st.list();
        self.shared.emit(WindowEvent::Snapshot(list));
    }
}
