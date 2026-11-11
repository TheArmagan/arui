//! Icon worker thread. Icon extraction talks to other processes (WM_GETICON) and to the shell
//! (package logos), so it runs here instead of on the hook thread.
//!
//! Key choice per window:
//! * packaged app (UWP / MSIX): `app:<aumid>` from the package logo, exe icon as fallback;
//! * window icon (WM_GETICON ICON_BIG, then GCLP_HICON) that differs from the exe icon at the
//!   same size: `hwnd:<hwnd>` (`hwnd:<hwnd>:<n>` after it changes);
//! * otherwise `exe:<lowercased path>` (48 px, extracted once and cached).

use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use arui_win::icons::{hicon_to_png, rgba_to_png};
use arui_win::util::{to_hwnd, wide, ComGuard};
use arui_win::{Hwnd, WindowEvent};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{LPARAM, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON,
    SHGFI_LARGEICON, SIIGBF_ICONONLY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, GetClassLongPtrW, PrivateExtractIconsW, SendMessageTimeoutW, GCLP_HICON, HICON,
    ICON_BIG, SMTO_ABORTIFHUNG, SMTO_ERRORONEXIT, WM_GETICON,
};

use crate::rules::{normalize_path, same_image};
use crate::tracker::Shared;

/// Size of exe and package icons we store.
const ICON_SIZE: i32 = 48;
/// Timeout for WM_GETICON on other apps' windows.
const GETICON_TIMEOUT_MS: u32 = 50;

pub enum IconJob {
    /// Resolve (or re-check) the icon of a tracked window.
    Window {
        hwnd: Hwnd,
        exe_path: String,
        package_aumid: Option<String>,
    },
    /// Window is gone; drop its per-window cache entry and images.
    Forget(Hwnd),
    Stop,
}

pub fn exe_key(exe_path: &str) -> String {
    format!("exe:{}", normalize_path(exe_path))
}

pub fn app_key(aumid: &str) -> String {
    format!("app:{aumid}")
}

struct HwndIcon {
    png: Vec<u8>,
    generation: u32,
}

struct Worker {
    shared: Arc<Shared>,
    /// (normalized exe path, size) -> PNG, `None` when extraction failed.
    exe_cache: HashMap<(String, i32), Option<Vec<u8>>>,
    /// Packages whose logo could not be loaded.
    failed_apps: std::collections::HashSet<String>,
    hwnd_icons: HashMap<Hwnd, HwndIcon>,
}

pub fn run(shared: Arc<Shared>, rx: Receiver<IconJob>) {
    let _com = ComGuard::sta();
    let mut w = Worker {
        shared,
        exe_cache: HashMap::new(),
        failed_apps: Default::default(),
        hwnd_icons: HashMap::new(),
    };
    while let Ok(job) = rx.recv() {
        match job {
            IconJob::Stop => break,
            IconJob::Forget(hwnd) => {
                w.hwnd_icons.remove(&hwnd);
                w.shared.icons.remove(&format!("hwnd:{hwnd}"));
                w.shared.icons.remove_prefix(&format!("hwnd:{hwnd}:"));
            }
            IconJob::Window {
                hwnd,
                exe_path,
                package_aumid,
            } => {
                w.shared.icon_pending.lock().remove(&hwnd);
                let key = w.resolve(hwnd, &exe_path, package_aumid.as_deref());
                w.apply(hwnd, key);
            }
        }
    }
}

impl Worker {
    fn resolve(&mut self, hwnd: Hwnd, exe_path: &str, aumid: Option<&str>) -> String {
        if let Some(aumid) = aumid {
            let key = app_key(aumid);
            if self.shared.icons.contains(&key) {
                return key;
            }
            if !self.failed_apps.contains(aumid) {
                match package_logo(aumid) {
                    Some(png) => {
                        self.shared.icons.put(key.clone(), png);
                        return key;
                    }
                    None => {
                        self.failed_apps.insert(aumid.to_string());
                    }
                }
            }
        }

        let exe_png = self.exe_icon(exe_path, ICON_SIZE).is_some();
        if aumid.is_none() {
            if let Some((png, size)) = window_icon(hwnd) {
                let same_as_exe = self
                    .exe_icon(exe_path, size)
                    .is_some_and(|exe| same_image(&exe, &png));
                if !same_as_exe || !exe_png {
                    return self.hwnd_key(hwnd, png);
                }
            }
        }
        if exe_png {
            let key = exe_key(exe_path);
            if !self.shared.icons.contains(&key) {
                if let Some(png) = self.exe_icon(exe_path, ICON_SIZE) {
                    self.shared.icons.put(key.clone(), png);
                }
            }
            return key;
        }
        String::new()
    }

    /// Stores a per-window icon, bumping the key generation when the image changed.
    fn hwnd_key(&mut self, hwnd: Hwnd, png: Vec<u8>) -> String {
        let key_for = |g: u32| {
            if g == 0 {
                format!("hwnd:{hwnd}")
            } else {
                format!("hwnd:{hwnd}:{g}")
            }
        };
        if let Some(cur) = self.hwnd_icons.get_mut(&hwnd) {
            if same_image(&cur.png, &png) {
                return key_for(cur.generation);
            }
            self.shared.icons.remove(&key_for(cur.generation));
            cur.generation += 1;
            cur.png = png.clone();
            let key = key_for(cur.generation);
            self.shared.icons.put(key.clone(), png);
            return key;
        }
        let key = key_for(0);
        self.shared.icons.put(key.clone(), png.clone());
        self.hwnd_icons
            .insert(hwnd, HwndIcon { png, generation: 0 });
        key
    }

    fn exe_icon(&mut self, exe_path: &str, size: i32) -> Option<Vec<u8>> {
        if exe_path.is_empty() {
            return None;
        }
        let k = (normalize_path(exe_path), size);
        if let Some(v) = self.exe_cache.get(&k) {
            return v.clone();
        }
        let v = extract_exe_icon(exe_path, size);
        self.exe_cache.insert(k, v.clone());
        v
    }

    fn apply(&self, hwnd: Hwnd, key: String) {
        if key.is_empty() {
            return;
        }
        let mut st = self.shared.state.lock();
        if let Some(t) = st.windows.get_mut(&hwnd) {
            if t.info.icon_key != key {
                t.info.icon_key = key;
                let info = t.info.clone();
                self.shared.emit(WindowEvent::Updated(info));
            }
        }
    }
}

/// WM_GETICON (with timeout) then the class icon. Returns (png, width).
fn window_icon(hwnd: Hwnd) -> Option<(Vec<u8>, i32)> {
    let h = to_hwnd(hwnd);
    let mut result: usize = 0;
    unsafe {
        let ok = SendMessageTimeoutW(
            h,
            WM_GETICON,
            WPARAM(ICON_BIG as usize),
            LPARAM(0),
            SMTO_ABORTIFHUNG | SMTO_ERRORONEXIT,
            GETICON_TIMEOUT_MS,
            Some(&mut result),
        );
        let mut icon = if ok.0 != 0 { result } else { 0 };
        if icon == 0 {
            icon = GetClassLongPtrW(h, GCLP_HICON);
        }
        if icon == 0 {
            return None;
        }
        // The icon belongs to the other process; never destroy it.
        let png = hicon_to_png(HICON(icon as *mut _)).ok()?;
        let width = png_width(&png)? as i32;
        Some((png, width))
    }
}

/// Width from the PNG IHDR chunk.
fn png_width(png: &[u8]) -> Option<u32> {
    let b = png.get(16..20)?;
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn extract_exe_icon(exe_path: &str, size: i32) -> Option<Vec<u8>> {
    let w = wide(exe_path);
    unsafe {
        if w.len() <= 260 {
            let mut buf = [0u16; 260];
            buf[..w.len()].copy_from_slice(&w);
            let mut icons = [HICON::default()];
            let n = PrivateExtractIconsW(&buf, 0, size, size, Some(&mut icons), None, 0);
            if n >= 1 && n != u32::MAX && !icons[0].is_invalid() {
                let png = hicon_to_png(icons[0]).ok();
                let _ = DestroyIcon(icons[0]);
                if png.is_some() {
                    return png;
                }
            }
        }
        // Fallback: the shell's large icon (32 px; handles paths PrivateExtractIcons can't).
        let mut info = SHFILEINFOW::default();
        let r = SHGetFileInfoW(
            PCWSTR(w.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );
        if r != 0 && !info.hIcon.is_invalid() {
            let png = hicon_to_png(info.hIcon).ok();
            let _ = DestroyIcon(info.hIcon);
            return png;
        }
    }
    None
}

/// Package logo through the shell: `shell:AppsFolder\<AUMID>`.
fn package_logo(aumid: &str) -> Option<Vec<u8>> {
    let path = wide(&format!("shell:AppsFolder\\{aumid}"));
    unsafe {
        let factory: IShellItemImageFactory =
            SHCreateItemFromParsingName(PCWSTR(path.as_ptr()), None).ok()?;
        let hbm = factory
            .GetImage(
                SIZE {
                    cx: ICON_SIZE,
                    cy: ICON_SIZE,
                },
                SIIGBF_ICONONLY,
            )
            .ok()?;
        let r = hbitmap_to_png(hbm);
        let _ = DeleteObject(HGDIOBJ(hbm.0));
        r
    }
}

/// 32-bit HBITMAP (as returned by IShellItemImageFactory) to PNG. The shell returns
/// premultiplied alpha, so it is un-premultiplied here.
unsafe fn hbitmap_to_png(hbm: HBITMAP) -> Option<Vec<u8>> {
    let mut bm = BITMAP::default();
    let got = GetObjectW(
        HGDIOBJ(hbm.0),
        std::mem::size_of::<BITMAP>() as i32,
        Some(&mut bm as *mut _ as *mut _),
    );
    if got == 0 || bm.bmWidth <= 0 || bm.bmHeight == 0 {
        return None;
    }
    let (w, h) = (bm.bmWidth as u32, bm.bmHeight.unsigned_abs());
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut px = vec![0u8; (w * h * 4) as usize];
    let dc = CreateCompatibleDC(None);
    let lines = GetDIBits(
        dc,
        hbm,
        0,
        h,
        Some(px.as_mut_ptr() as *mut _),
        &mut info,
        DIB_RGB_COLORS,
    );
    let _ = DeleteDC(dc);
    if lines == 0 {
        return None;
    }
    let has_alpha = px.chunks_exact(4).any(|p| p[3] != 0);
    let premultiplied = has_alpha
        && px
            .chunks_exact(4)
            .all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]);
    for p in px.chunks_exact_mut(4) {
        if !has_alpha {
            p[3] = 255;
        } else if premultiplied && p[3] != 0 && p[3] != 255 {
            let a = p[3] as u32;
            for c in &mut p[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
        p.swap(0, 2); // BGRA -> RGBA
    }
    rgba_to_png(w, h, &px).ok()
}
