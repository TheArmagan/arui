use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};

use crate::types::Hwnd;

/// Null-terminated UTF-16 buffer for Win32 calls.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// UTF-16 slice (optionally null-terminated) to String.
pub fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

pub fn to_hwnd(h: Hwnd) -> HWND {
    HWND(h as usize as *mut core::ffi::c_void)
}

pub fn from_hwnd(h: HWND) -> Hwnd {
    h.0 as usize as Hwnd
}

/// Initializes an STA COM apartment for the current thread; uninitializes on drop.
pub struct ComGuard {
    ok: bool,
}

impl ComGuard {
    pub fn sta() -> Self {
        let ok = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
        Self { ok }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.ok {
            unsafe { CoUninitialize() };
        }
    }
}
