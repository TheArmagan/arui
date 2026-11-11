//! Reading window / process attributes. None of these send messages to other apps' windows
//! (GetWindowTextW reads the cached caption for windows of other processes), so they are safe
//! to call on the hook thread.

use arui_win::util::{from_hwnd, from_wide, to_hwnd, wide};
use arui_win::Hwnd;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONEAREST};
use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
use windows::Win32::Storage::Packaging::Appx::GetApplicationUserModelId;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, GetAncestor, GetClassNameW, GetDesktopWindow, GetWindow, GetWindowLongPtrW,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow,
    IsWindowVisible, IsZoomed, GA_PARENT, GA_ROOTOWNER, GWL_EXSTYLE, GWL_STYLE, GW_OWNER,
};

use crate::rules::{normalize_app_id, WindowAttrs, UWP_CORE_CLASS, UWP_FRAME_CLASS};

/// True when `h` is a top-level window (its parent is the desktop).
pub fn is_top_level(h: Hwnd) -> bool {
    unsafe {
        let parent = GetAncestor(to_hwnd(h), GA_PARENT);
        !parent.is_invalid() && parent == GetDesktopWindow()
    }
}

pub fn root_owner(h: Hwnd) -> Hwnd {
    unsafe { from_hwnd(GetAncestor(to_hwnd(h), GA_ROOTOWNER)) }
}

pub fn root(h: Hwnd) -> Hwnd {
    unsafe {
        from_hwnd(GetAncestor(
            to_hwnd(h),
            windows::Win32::UI::WindowsAndMessaging::GA_ROOT,
        ))
    }
}

pub fn pid_of(h: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
    pid
}

pub fn class_name(h: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(h, &mut buf) };
    from_wide(&buf[..n.max(0) as usize])
}

pub fn title(h: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(h);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(h, &mut buf);
        from_wide(&buf[..n.max(0) as usize])
    }
}

pub fn is_cloaked(h: HWND) -> bool {
    let mut cloaked = 0u32;
    unsafe {
        DwmGetWindowAttribute(
            h,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut _,
            std::mem::size_of::<u32>() as u32,
        )
        .is_ok()
            && cloaked != 0
    }
}

/// All eligibility inputs for `h`, or `None` if the window no longer exists.
pub fn attrs(hwnd: Hwnd) -> Option<WindowAttrs> {
    let h = to_hwnd(hwnd);
    unsafe {
        if !IsWindow(Some(h)).as_bool() {
            return None;
        }
        let owner = GetWindow(h, GW_OWNER).unwrap_or_default();
        let pid = pid_of(h);
        // GetWindowTextW on a window of our own process sends WM_GETTEXT synchronously to the
        // owning thread. If that thread is waiting on the tracker (e.g. the host's UI thread),
        // that deadlocks. Our own windows are never eligible, so skip the title.
        let title = if pid == std::process::id() { String::new() } else { title(h) };
        Some(WindowAttrs {
            visible: IsWindowVisible(h).as_bool(),
            cloaked: is_cloaked(h),
            style: GetWindowLongPtrW(h, GWL_STYLE) as u32,
            ex_style: GetWindowLongPtrW(h, GWL_EXSTYLE) as u32,
            has_owner: !owner.is_invalid(),
            title,
            class_name: class_name(h),
            pid,
        })
    }
}

/// Volatile per-window state.
pub struct Placement {
    pub minimized: bool,
    pub maximized: bool,
    pub monitor: u64,
}

pub fn placement(hwnd: Hwnd) -> Placement {
    let h = to_hwnd(hwnd);
    unsafe {
        Placement {
            minimized: IsIconic(h).as_bool(),
            maximized: IsZoomed(h).as_bool(),
            monitor: MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST).0 as usize as u64,
        }
    }
}

struct ProcessHandle(HANDLE);
impl Drop for ProcessHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn open_process(pid: u32) -> Option<ProcessHandle> {
    unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
        .ok()
        .map(ProcessHandle)
}

pub fn process_path(pid: u32) -> String {
    let Some(p) = open_process(pid) else {
        return String::new();
    };
    let mut buf = vec![0u16; 1024];
    let mut len = buf.len() as u32;
    unsafe {
        if QueryFullProcessImageNameW(p.0, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len)
            .is_ok()
        {
            return from_wide(&buf[..len as usize]);
        }
    }
    String::new()
}

/// AUMID of a packaged process (UWP / MSIX), `None` for unpackaged processes.
pub fn package_aumid(pid: u32) -> Option<String> {
    let p = open_process(pid)?;
    let mut buf = vec![0u16; 512];
    let mut len = buf.len() as u32;
    let err = unsafe { GetApplicationUserModelId(p.0, &mut len, Some(PWSTR(buf.as_mut_ptr()))) };
    if err.0 != 0 {
        return None;
    }
    let s = from_wide(&buf[..(len as usize).min(buf.len())]);
    (!s.is_empty()).then_some(s)
}

/// Explicit per-window AUMID (PKEY_AppUserModel_ID), set by e.g. browser PWAs or apps that
/// group their windows themselves.
pub fn window_aumid(hwnd: Hwnd) -> Option<String> {
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(to_hwnd(hwnd)).ok()?;
        let mut pv = store.GetValue(&PKEY_AppUserModel_ID).ok()?;
        let s = PropVariantToStringAlloc(&pv).ok();
        let _ = PropVariantClear(&mut pv);
        let s = s?;
        let out = s.to_string().ok();
        CoTaskMemFree(Some(s.0 as *const _));
        out.filter(|v| !v.trim().is_empty())
    }
}

/// For an ApplicationFrameWindow: pid of the hosted CoreWindow child (the real app process).
pub fn uwp_child_pid(frame: Hwnd, frame_pid: u32) -> Option<u32> {
    let class = wide(UWP_CORE_CLASS);
    let mut after: Option<HWND> = None;
    unsafe {
        loop {
            let child = FindWindowExW(
                Some(to_hwnd(frame)),
                after,
                PCWSTR(class.as_ptr()),
                PCWSTR::null(),
            )
            .ok()?;
            if child.is_invalid() {
                return None;
            }
            let pid = pid_of(child);
            if pid != 0 && pid != frame_pid {
                return Some(pid);
            }
            after = Some(child);
        }
    }
}

/// Who a window belongs to, for grouping and icons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub exe_path: String,
    pub app_id: String,
    /// Package AUMID of the app process, if packaged (used for the `app:` icon).
    pub package_aumid: Option<String>,
    /// False for a UWP frame whose CoreWindow child was not found yet; retried on later events.
    pub resolved: bool,
}

pub fn identity(hwnd: Hwnd, pid: u32, class: &str) -> Identity {
    let explicit = window_aumid(hwnd);
    let mut app_pid = pid;
    let mut resolved = true;
    if class == UWP_FRAME_CLASS {
        match uwp_child_pid(hwnd, pid) {
            Some(child) => app_pid = child,
            None => resolved = false,
        }
    }
    let exe_path = process_path(app_pid);
    let package_aumid = package_aumid(app_pid);
    let app_id = normalize_app_id(
        explicit.as_deref(),
        package_aumid.as_deref(),
        &exe_path,
        app_pid,
    );
    Identity {
        exe_path,
        app_id,
        package_aumid,
        resolved,
    }
}
