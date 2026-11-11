//! Monitor enumeration in physical pixels.

use arui_win::util::from_wide;
use arui_win::{MonitorInfo, Rect};
use windows::core::BOOL;
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::UI::HiDpi::{
    GetDpiForMonitor, SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, MDT_EFFECTIVE_DPI,
};

const MONITORINFOF_PRIMARY: u32 = 1;

/// Switches the calling thread to per-monitor-v2 DPI awareness for its lifetime, so every
/// coordinate we read or write is in physical pixels regardless of the host's settings.
pub(crate) struct DpiScope(DPI_AWARENESS_CONTEXT);

impl DpiScope {
    pub fn per_monitor() -> Self {
        Self(unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) })
    }
}

impl Drop for DpiScope {
    fn drop(&mut self) {
        if !self.0 .0.is_null() {
            unsafe { SetThreadDpiAwarenessContext(self.0) };
        }
    }
}

pub(crate) fn rect_from(r: RECT) -> Rect {
    Rect { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
}

pub(crate) fn rect_to(r: Rect) -> RECT {
    RECT { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
}

unsafe extern "system" fn enum_proc(hmon: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    let out = &mut *(data.0 as *mut Vec<MonitorInfo>);
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if GetMonitorInfoW(hmon, &mut info.monitorInfo as *mut MONITORINFO).as_bool() {
        let (mut dx, mut dy) = (96u32, 96u32);
        if GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy).is_err() {
            dx = 96;
        }
        out.push(MonitorInfo {
            id: from_wide(&info.szDevice),
            handle: hmon.0 as usize as u64,
            bounds: rect_from(info.monitorInfo.rcMonitor),
            work_area: rect_from(info.monitorInfo.rcWork),
            scale: dx as f64 / 96.0,
            primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
        });
    }
    BOOL(1)
}

/// All monitors, sorted by id.
pub fn enumerate() -> Vec<MonitorInfo> {
    let _dpi = DpiScope::per_monitor();
    let mut out: Vec<MonitorInfo> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(enum_proc), LPARAM(&mut out as *mut _ as isize));
    }
    normalize(&mut out);
    out
}

pub(crate) fn normalize(list: &mut [MonitorInfo]) {
    list.sort_by(|a, b| a.id.cmp(&b.id));
}

/// True when `new` differs from `old` in anything the UI cares about. Both lists must be
/// normalized (see [`normalize`]).
pub(crate) fn changed(old: &[MonitorInfo], new: &[MonitorInfo]) -> bool {
    old != new
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(id: &str, left: i32, work_bottom: i32, scale: f64) -> MonitorInfo {
        MonitorInfo {
            id: id.into(),
            handle: 1,
            bounds: Rect { left, top: 0, right: left + 1920, bottom: 1080 },
            work_area: Rect { left, top: 0, right: left + 1920, bottom: work_bottom },
            scale,
            primary: left == 0,
        }
    }

    #[test]
    fn diffing() {
        let mut a = vec![mon("\\\\.\\DISPLAY2", 1920, 1080, 1.0), mon("\\\\.\\DISPLAY1", 0, 1032, 1.25)];
        let mut b = vec![mon("\\\\.\\DISPLAY1", 0, 1032, 1.25), mon("\\\\.\\DISPLAY2", 1920, 1080, 1.0)];
        normalize(&mut a);
        normalize(&mut b);
        assert!(!changed(&a, &b), "order must not matter");
        b[0].work_area.bottom = 1080;
        assert!(changed(&a, &b), "work area change");
        b[0].work_area.bottom = 1032;
        b[1].scale = 1.5;
        assert!(changed(&a, &b), "dpi change");
        b.pop();
        assert!(changed(&a, &b), "monitor removed");
    }

    #[test]
    fn enumerates_real_monitors() {
        let list = enumerate();
        assert!(!list.is_empty());
        assert_eq!(list.iter().filter(|m| m.primary).count(), 1);
        assert!(list.iter().all(|m| m.bounds.width() > 0 && m.scale >= 1.0));
    }
}
