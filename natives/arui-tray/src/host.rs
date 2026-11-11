//! Win32 side of the tray host: the fake `Shell_TrayWnd` window, its window procedure, WinEvent
//! hooks and message forwarding to explorer's real tray window.
//!
//! Everything in here runs on the tray thread, except [`Shared`] which is also used by
//! [`crate::TrayHost`] methods from other threads.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use arui_win::icons::hicon_to_png;
use arui_win::util::{from_hwnd, to_hwnd};
use arui_win::{EventSink, NativeEvent, Rect, TrayEvent};
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Shell::{SHAllocShared, SHFreeShared, SHLockShared, SHUnlockShared};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::parse::{self, make_long};
use crate::state::{Change, Env, TrayState};

/// Forward NIM_* messages to explorer's tray as well (after handling them ourselves).
///
/// ManagedShell does not do this because it usually *replaces* explorer. ARUI runs next to a
/// live (hidden) explorer taskbar, so forwarding keeps explorer's icon list in sync: icons from
/// apps that ignore TaskbarCreated survive ARUI exiting, and explorer keeps turning NIF_INFO
/// balloons into native Windows toasts. Explorer's answer is ignored; ours is returned.
const FORWARD_NIM_TO_EXPLORER: bool = true;
/// Timeout for messages forwarded to explorer (SMTO_ABORTIFHUNG also bails out early on a hung
/// explorer).
const FORWARD_TIMEOUT_MS: u32 = 2000;

/// Private message posted to our tray window to shut the thread down.
const WM_ARUI_QUIT: u32 = WM_APP + 0x51;
/// Explorer handles this one asynchronously; ManagedShell posts it instead of sending.
const WM_TRAY_POSTED: u32 = WM_USER + 372;

const SNAPSHOT_TIMER: usize = 1;
/// The first Snapshot goes out once no icon changed for this long after start...
const SNAPSHOT_QUIET: Duration = Duration::from_millis(500);
/// ...but no later than this.
const SNAPSHOT_MAX: Duration = Duration::from_millis(3000);

const OBJID_WINDOW: i32 = 0;
const WINEVENT_OUTOFCONTEXT: u32 = 0x0000;
const WINEVENT_SKIPOWNPROCESS: u32 = 0x0002;

pub(crate) struct Inner {
    pub state: TrayState,
    /// False until the first Snapshot was emitted; diffs are not emitted before that.
    pub live: bool,
}

pub(crate) struct Shared {
    pub inner: Mutex<Inner>,
    pub rects: Mutex<HashMap<String, Rect>>,
    pub sink: EventSink,
}

impl Shared {
    pub fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Emits changes (call without holding the lock).
    pub fn emit(&self, changes: Vec<Change>) {
        for c in changes {
            let ev = match c {
                Change::Added(i) => TrayEvent::Added(i),
                Change::Updated(i) => TrayEvent::Updated(i),
                Change::Removed(id) => {
                    self.rects
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .remove(&id);
                    TrayEvent::Removed { id }
                }
                Change::Balloon { id, title, text } => TrayEvent::Balloon { id, title, text },
            };
            (self.sink)(NativeEvent::Tray(ev));
        }
    }

    /// Removes icons whose owner window no longer exists.
    pub fn purge_dead(&self) {
        let (changes, live) = {
            let mut g = self.lock();
            let ch = g.state.remove_where(|r| !is_window(r.icon.owner_hwnd));
            (ch, g.live)
        };
        if live {
            self.emit(changes);
        }
    }
}

pub(crate) fn is_window(h: u64) -> bool {
    unsafe { IsWindow(Some(to_hwnd(h))).as_bool() }
}

/// Real Win32 implementation of the state's side effects.
pub(crate) struct WinEnv;

impl Env for WinEnv {
    fn icon_png(&mut self, hicon: u64) -> Option<Vec<u8>> {
        unsafe {
            // The handle belongs to the calling app and may be destroyed right after its
            // Shell_NotifyIcon call returns, so copy and convert it now.
            let copy = CopyIcon(HICON(hicon as usize as *mut _)).ok()?;
            let png = hicon_to_png(copy);
            let _ = DestroyIcon(copy);
            match png {
                Ok(p) => Some(p),
                Err(e) => {
                    log::debug!("tray: icon {hicon:#x} not convertible: {e}");
                    None
                }
            }
        }
    }

    fn owner_process(&mut self, hwnd: u64) -> (u32, Option<String>) {
        let pid = window_pid(to_hwnd(hwnd));
        (pid, if pid == 0 { None } else { exe_path(pid) })
    }
}

pub(crate) fn window_pid(h: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
    pid
}

fn exe_path(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let r =
            QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        r.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

// ---------------------------------------------------------------- tray thread context

struct Ctx {
    shared: Arc<Shared>,
    tray: Cell<HWND>,
    taskbar_created: u32,
    /// Set while we broadcast TaskbarCreated ourselves, so our own window ignores it.
    broadcasting: Cell<bool>,
    /// Cached explorer Shell_TrayWnd.
    explorer: Cell<HWND>,
    started: Instant,
    own_pid: u32,
}

thread_local! {
    static CTX: RefCell<Option<Rc<Ctx>>> = const { RefCell::new(None) };
}

fn ctx() -> Option<Rc<Ctx>> {
    CTX.with(|c| c.borrow().clone())
}

impl Ctx {
    /// Explorer's real Shell_TrayWnd: the first window of that class not owned by our process.
    fn explorer_tray(&self) -> Option<HWND> {
        let cached = self.explorer.get();
        if !cached.is_invalid() && unsafe { IsWindow(Some(cached)).as_bool() } {
            return Some(cached);
        }
        let found = find_foreign_tray(self.own_pid);
        if let Some(h) = found {
            self.explorer.set(h);
        }
        found
    }

    /// Makes sure FindWindow("Shell_TrayWnd") returns our window.
    fn ensure_topmost(&self) {
        let tray = self.tray.get();
        if tray.is_invalid() {
            return;
        }
        let first = unsafe { FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) }.unwrap_or_default();
        if first != tray {
            log::debug!(
                "tray: another Shell_TrayWnd ({:?}) is above ours, re-asserting",
                first.0
            );
            unsafe {
                let _ = SetWindowPos(
                    tray,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                );
            }
        }
    }

    fn broadcast_taskbar_created(&self) {
        self.broadcasting.set(true);
        unsafe {
            let _ = SendNotifyMessageW(HWND_BROADCAST, self.taskbar_created, WPARAM(0), LPARAM(0));
        }
        self.broadcasting.set(false);
    }

    fn forward(&self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let Some(target) = self.explorer_tray() else {
            return LRESULT(0);
        };
        unsafe {
            if msg == WM_TRAY_POSTED {
                let _ = PostMessageW(Some(target), msg, wparam, lparam);
                return LRESULT(0);
            }
            let mut result = 0usize;
            let ok = SendMessageTimeoutW(
                target,
                msg,
                wparam,
                lparam,
                SMTO_ABORTIFHUNG | SMTO_NORMAL,
                FORWARD_TIMEOUT_MS,
                Some(&mut result),
            );
            if ok.0 == 0 {
                log::debug!("tray: forwarding message {msg:#x} to explorer failed or timed out");
            }
            LRESULT(result as isize)
        }
    }

    /// Restarts the snapshot debounce while the initial burst of NIM_ADDs is still coming.
    fn bump_snapshot_timer(&self) {
        let elapsed = self.started.elapsed();
        if elapsed >= SNAPSHOT_MAX {
            return; // the pending timer (or the next WM_TIMER) finishes it
        }
        let wait = SNAPSHOT_QUIET.min(SNAPSHOT_MAX - elapsed);
        unsafe {
            SetTimer(
                Some(self.tray.get()),
                SNAPSHOT_TIMER,
                wait.as_millis() as u32,
                None,
            )
        };
    }

    fn go_live(&self) {
        self.shared.purge_dead();
        let icons = {
            let mut g = self.shared.lock();
            if g.live {
                return;
            }
            g.live = true;
            g.state.icons()
        };
        (self.shared.sink)(NativeEvent::Tray(TrayEvent::Snapshot(icons)));
    }

    fn on_copydata(&self, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let cds = lparam.0 as *const COPYDATASTRUCT;
        if cds.is_null() {
            return LRESULT(0);
        }
        // SAFETY: the system copied the sender's COPYDATASTRUCT into our address space; it stays
        // valid until we return from WM_COPYDATA.
        let cds = unsafe { &*cds };
        let data: &[u8] = if cds.lpData.is_null() || cds.cbData == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(cds.lpData as *const u8, cds.cbData as usize) }
        };
        match cds.dwData {
            1 => match parse::parse_tray_data(data) {
                Ok(m) => {
                    if m.signature != parse::SHELLTRAY_SIGNATURE {
                        log::debug!(
                            "tray: unexpected SHELLTRAYDATA signature {:#x}",
                            m.signature
                        );
                    }
                    let result = self.handle_nim(&m);
                    if FORWARD_NIM_TO_EXPLORER {
                        self.forward(WM_COPYDATA, wparam, lparam);
                    }
                    LRESULT(result as isize)
                }
                Err(e) => {
                    log::debug!("tray: bad notify icon data: {e}");
                    self.forward(WM_COPYDATA, wparam, lparam)
                }
            },
            3 => match parse::parse_icon_identifier(data) {
                Ok(req) => match self.icon_rect_reply(&req) {
                    Some(v) => v,
                    // Not an icon we know: maybe explorer does.
                    None => self.forward(WM_COPYDATA, wparam, lparam),
                },
                Err(_) => self.forward(WM_COPYDATA, wparam, lparam),
            },
            // SHAppBarMessage: explorer must answer, otherwise appbars (ARUI's own too) break.
            0 => self.forward_appbar(wparam, cds, data),
            // 2 = LoadInProc and anything else: explorer's business too.
            _ => self.forward(WM_COPYDATA, wparam, lparam),
        }
    }

    /// Forwards an SHAppBarMessage (WM_COPYDATA dwData 0) to explorer.
    ///
    /// Payload: APPBARMSGDATAV3 = APPBARDATAV2 abd (40 bytes, 32-bit layout), u32 dwMessage,
    /// u32 pad, u64 hSharedMemory, u32 dwSourceProcessId, u32 pad. For messages with output
    /// (ABM_QUERYPOS, ABM_SETPOS, ABM_GETTASKBARPOS, ...) shell32 allocates the output block (an
    /// APPBARDATAV2) with SHAllocShared *for the process that owns the Shell_TrayWnd it found*,
    /// i.e. for us, and puts our pid in dwSourceProcessId. Explorer would then try to map it from
    /// our process. That happens to work for callers in other processes, but when the caller is
    /// in our own process (arui-shell's appbars!) SHAllocShared returns an in-process pointer
    /// that explorer cannot map, and the caller gets an all-zero answer.
    ///
    /// So whenever the block is addressed to us: map it locally, copy it into a block allocated
    /// for explorer, forward a patched message that points at that block, and copy explorer's
    /// output back. (Windows 11 explorer may re-enter us with its own SHAppBarMessage while
    /// handling the forwarded one; that nested call goes through this same path.)
    fn forward_appbar(&self, wparam: WPARAM, cds: &COPYDATASTRUCT, data: &[u8]) -> LRESULT {
        const SHARED_OFF: usize = 48;
        const PID_OFF: usize = 56;
        let lparam = LPARAM(cds as *const _ as isize);
        if data.len() < 64 {
            return self.forward(WM_COPYDATA, wparam, lparam);
        }
        let shared = u64::from_le_bytes(data[SHARED_OFF..SHARED_OFF + 8].try_into().unwrap());
        let pid = u32::from_le_bytes(data[PID_OFF..PID_OFF + 4].try_into().unwrap());
        if shared == 0 || pid != self.own_pid {
            return self.forward(WM_COPYDATA, wparam, lparam);
        }
        let Some(explorer) = self.explorer_tray() else {
            return LRESULT(0);
        };
        let explorer_pid = window_pid(explorer);
        // Output block size = APPBARDATAV2 size (cbSize of the embedded abd), sanity-clamped.
        let size = u32::from_le_bytes(data[0..4].try_into().unwrap()).clamp(40, 64);
        unsafe {
            let local = SHLockShared(HANDLE(shared as usize as *mut _), self.own_pid);
            if local.is_null() {
                return self.forward(WM_COPYDATA, wparam, lparam);
            }
            let h = SHAllocShared(Some(local as *const _), size, explorer_pid);
            if h.is_invalid() {
                let _ = SHUnlockShared(local);
                return self.forward(WM_COPYDATA, wparam, lparam);
            }
            let mut patched = data.to_vec();
            patched[SHARED_OFF..SHARED_OFF + 8]
                .copy_from_slice(&(h.0 as usize as u64).to_le_bytes());
            patched[PID_OFF..PID_OFF + 4].copy_from_slice(&explorer_pid.to_le_bytes());
            let copy = COPYDATASTRUCT {
                dwData: cds.dwData,
                cbData: patched.len() as u32,
                lpData: patched.as_mut_ptr() as *mut _,
            };
            let result = self.forward(WM_COPYDATA, wparam, LPARAM(&copy as *const _ as isize));
            let out = SHLockShared(h, explorer_pid);
            if !out.is_null() {
                std::ptr::copy_nonoverlapping(out as *const u8, local as *mut u8, size as usize);
                let _ = SHUnlockShared(out);
            }
            let _ = SHUnlockShared(local);
            let _ = SHFreeShared(h, explorer_pid);
            result
        }
    }

    fn handle_nim(&self, m: &parse::TrayMessage) -> bool {
        let (out, live) = {
            let mut g = self.shared.lock();
            let out = g.state.apply(m.message, &m.nid, &mut WinEnv);
            (out, g.live)
        };
        if live {
            self.shared.emit(out.changes);
        } else if !out.changes.is_empty() {
            self.bump_snapshot_timer();
        }
        out.result
    }

    /// Answer for Shell_NotifyIconGetRect, which asks twice: message 1 wants
    /// MAKELPARAM(left, top), message 2 MAKELPARAM(width, height). (ManagedShell's code reads
    /// like right/bottom, but shell32 on Windows 11 computes right = left + LOWORD, verified with
    /// examples/tray_dump.rs.) `None` if we don't know the icon.
    fn icon_rect_reply(&self, req: &parse::IconIdentifier) -> Option<LRESULT> {
        let id = {
            let g = self.shared.lock();
            let i = g.state.find(req.guid.as_deref(), req.hwnd, req.uid)?;
            g.state.icons()[i].id.clone()
        };
        let rect = self
            .shared
            .rects
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
            .copied();
        let Some(r) = rect else {
            return Some(LRESULT(0));
        };
        let v = match req.message {
            1 => make_long(r.left, r.top),
            2 => make_long(r.width(), r.height()),
            _ => 0,
        };
        Some(LRESULT(v as isize))
    }

    fn owner_destroyed(&self, hwnd: u64) {
        let (changes, live) = {
            let mut g = self.shared.lock();
            if !g.state.owners().any(|o| o == hwnd) {
                return;
            }
            (g.state.remove_owner(hwnd), g.live)
        };
        if live {
            self.shared.emit(changes);
        }
    }
}

/// First top-level `Shell_TrayWnd` that belongs to another process (explorer).
pub(crate) fn find_foreign_tray(own_pid: u32) -> Option<HWND> {
    let mut after: Option<HWND> = None;
    loop {
        let h = unsafe { FindWindowExW(None, after, w!("Shell_TrayWnd"), PCWSTR::null()) }.ok()?;
        if h.is_invalid() {
            return None;
        }
        if window_pid(h) != own_pid {
            return Some(h);
        }
        after = Some(h);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let Some(c) = ctx() else {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    };
    let is_tray = hwnd == c.tray.get();
    match msg {
        WM_COPYDATA => return c.on_copydata(wparam, lparam),
        WM_TIMER if wparam.0 == SNAPSHOT_TIMER => {
            let _ = KillTimer(Some(hwnd), SNAPSHOT_TIMER);
            c.go_live();
            return LRESULT(0);
        }
        WM_WINDOWPOSCHANGED if is_tray => {
            // Like ManagedShell: if anything shows our window, hide it again.
            let pos = &*(lparam.0 as *const WINDOWPOS);
            if pos.flags.contains(SWP_SHOWWINDOW) {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
        WM_ARUI_QUIT if is_tray => {
            let _ = DestroyWindow(hwnd);
            return LRESULT(0);
        }
        WM_DESTROY if is_tray => {
            PostQuitMessage(0);
            return LRESULT(0);
        }
        _ => {}
    }
    if msg == c.taskbar_created {
        if !c.broadcasting.get() {
            // explorer restarted and broadcast TaskbarCreated: its new tray window may now be
            // above ours. Take the top spot back and ask apps to re-add their icons to us.
            log::info!("tray: explorer restarted, re-registering tray icons");
            c.explorer.set(HWND::default());
            c.ensure_topmost();
            c.broadcast_taskbar_created();
        }
        return LRESULT(0);
    }
    // Everything apps send to "the taskbar" that we don't implement goes to explorer, like
    // ManagedShell does: WM_COMMAND (e.g. minimize all) and private WM_USER range messages.
    // Registered (0xC000+) messages are skipped since broadcasts reach explorer on their own.
    if is_tray && (msg == WM_COMMAND || (WM_USER..0xC000).contains(&msg)) && msg != WM_ARUI_QUIT {
        return c.forward(msg, wparam, lparam);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe extern "system" fn win_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if id_object != OBJID_WINDOW || id_child != 0 || hwnd.is_invalid() {
        return;
    }
    let Some(c) = ctx() else { return };
    match event {
        EVENT_OBJECT_DESTROY => c.owner_destroyed(from_hwnd(hwnd)),
        EVENT_SYSTEM_FOREGROUND => c.ensure_topmost(),
        EVENT_OBJECT_SHOW if hwnd == c.explorer.get() => c.ensure_topmost(),
        _ => {}
    }
}

/// Body of the tray thread. Sends the setup result (our tray HWND) through `ready`, then pumps
/// messages until [`request_quit`] is called.
pub(crate) fn run(shared: Arc<Shared>, ready: Sender<arui_win::Result<u64>>) {
    let setup = unsafe { setup(shared.clone()) };
    let (tray, hooks) = match setup {
        Ok(v) => v,
        Err(e) => {
            unsafe { teardown(&[]) };
            let _ = ready.send(Err(e));
            return;
        }
    };
    let _ = ready.send(Ok(from_hwnd(tray)));

    let c = ctx().expect("context set in setup");
    c.ensure_topmost();
    // Ask every running app to (re-)add its icons; they will now find our window.
    c.broadcast_taskbar_created();

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        teardown(&hooks);
    }
    drop(c);
    shared.lock().state.clear();
    // Our window is gone now, so apps re-register with explorer's tray.
    let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    unsafe {
        let _ = SendNotifyMessageW(HWND_BROADCAST, taskbar_created, WPARAM(0), LPARAM(0));
    }
}

unsafe fn setup(shared: Arc<Shared>) -> arui_win::Result<(HWND, Vec<HWINEVENTHOOK>)> {
    let hinst = GetModuleHandleW(None)?;
    let taskbar_created = RegisterWindowMessageW(w!("TaskbarCreated"));
    for class in [w!("Shell_TrayWnd"), w!("TrayNotifyWnd")] {
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_DBLCLKS,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: class,
            ..Default::default()
        };
        if RegisterClassExW(&wc) == 0 {
            return Err(windows::core::Error::from_thread().into());
        }
    }
    let c = Rc::new(Ctx {
        shared,
        tray: Cell::new(HWND::default()),
        taskbar_created,
        broadcasting: Cell::new(false),
        explorer: Cell::new(HWND::default()),
        started: Instant::now(),
        own_pid: GetCurrentProcessId(),
    });
    CTX.with(|x| *x.borrow_mut() = Some(c.clone()));

    // Invisible, zero size, never activated, not in Alt-Tab. WS_EX_TOPMOST so FindWindow
    // returns it before explorer's (also topmost) taskbar.
    let tray = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        w!("Shell_TrayWnd"),
        w!(""),
        WS_POPUP | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
        0,
        0,
        0,
        0,
        None,
        None,
        Some(hinst.into()),
        None,
    )?;
    c.tray.set(tray);
    // Some apps look for Shell_TrayWnd\TrayNotifyWnd to decide that a tray exists.
    CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("TrayNotifyWnd"),
        w!(""),
        WS_CHILD | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
        0,
        0,
        0,
        0,
        Some(tray),
        None,
        Some(hinst.into()),
        None,
    )?;
    // If ARUI runs elevated, still accept messages from normal apps (UIPI).
    for m in [WM_COPYDATA, taskbar_created] {
        let _ = ChangeWindowMessageFilterEx(tray, m, MSGFLT_ALLOW, None);
    }

    let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
    let mut hooks = Vec::new();
    for (lo, hi) in [
        (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
        (EVENT_OBJECT_DESTROY, EVENT_OBJECT_SHOW),
    ] {
        let h = SetWinEventHook(lo, hi, None, Some(win_event), 0, 0, flags);
        if h.is_invalid() {
            log::warn!("tray: SetWinEventHook({lo:#x}..{hi:#x}) failed");
        } else {
            hooks.push(h);
        }
    }
    SetTimer(
        Some(tray),
        SNAPSHOT_TIMER,
        SNAPSHOT_MAX.as_millis() as u32,
        None,
    );
    c.explorer_tray();
    Ok((tray, hooks))
}

unsafe fn teardown(hooks: &[HWINEVENTHOOK]) {
    for h in hooks {
        let _ = UnhookWinEvent(*h);
    }
    if let Some(c) = ctx() {
        let tray = c.tray.get();
        if !tray.is_invalid() && IsWindow(Some(tray)).as_bool() {
            let _ = DestroyWindow(tray);
        }
    }
    CTX.with(|x| *x.borrow_mut() = None);
    if let Ok(hinst) = GetModuleHandleW(None) {
        let _ = UnregisterClassW(w!("TrayNotifyWnd"), Some(hinst.into()));
        let _ = UnregisterClassW(w!("Shell_TrayWnd"), Some(hinst.into()));
    }
}

/// Asks the tray thread to destroy its window and exit its message loop.
pub(crate) fn request_quit(tray: u64) {
    unsafe {
        let _ = PostMessageW(Some(to_hwnd(tray)), WM_ARUI_QUIT, WPARAM(0), LPARAM(0));
    }
}
