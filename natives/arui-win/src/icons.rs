//! Shared PNG store. Services put images under a key; the host serves them at
//! `arui-icon://localhost/<url-encoded key>`.
//!
//! Key conventions: `exe:<lowercased path>`, `hwnd:<hwnd>`, `app:<app id>`, `tray:<tray id>`,
//! `art:<session id>:<n>`. When the image for a key changes, put it under a new key (or let the
//! UI add a cache-busting query) so webviews don't show a stale cached image.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{GetIconInfo, HICON, ICONINFO};

use crate::{Error, Result};

#[derive(Clone, Default)]
pub struct IconStore {
    inner: Arc<RwLock<HashMap<String, Arc<Vec<u8>>>>>,
}

impl IconStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn put(&self, key: impl Into<String>, png: Vec<u8>) {
        self.inner.write().insert(key.into(), Arc::new(png));
    }

    pub fn get(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        self.inner.read().get(key).cloned()
    }

    pub fn contains(&self, key: &str) -> bool {
        self.inner.read().contains_key(key)
    }

    pub fn remove(&self, key: &str) {
        self.inner.write().remove(key);
    }

    /// Removes every key with this prefix (e.g. all art of a session).
    pub fn remove_prefix(&self, prefix: &str) {
        self.inner.write().retain(|k, _| !k.starts_with(prefix));
    }
}

/// Encodes straight (non-premultiplied) RGBA8 pixels as PNG.
pub fn rgba_to_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().map_err(|e| Error::Other(e.to_string()))?;
        w.write_image_data(rgba).map_err(|e| Error::Other(e.to_string()))?;
    }
    Ok(out)
}

/// Reads a 32-bit top-down BGRA copy of `hbm`. Returns (width, height, bgra).
unsafe fn read_bitmap(hbm: HBITMAP) -> Result<(u32, u32, Vec<u8>)> {
    let mut bm = BITMAP::default();
    let got = GetObjectW(
        HGDIOBJ(hbm.0),
        std::mem::size_of::<BITMAP>() as i32,
        Some(&mut bm as *mut _ as *mut _),
    );
    if got == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 {
        return Err(Error::Other("GetObjectW failed".into()));
    }
    let (w, h) = (bm.bmWidth as u32, bm.bmHeight as u32);
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32), // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let dc = CreateCompatibleDC(None);
    let lines = GetDIBits(
        dc,
        hbm,
        0,
        h,
        Some(buf.as_mut_ptr() as *mut _),
        &mut info,
        DIB_RGB_COLORS,
    );
    let _ = DeleteDC(dc);
    if lines == 0 {
        return Err(Error::Other("GetDIBits failed".into()));
    }
    Ok((w, h, buf))
}

/// Converts an HICON to PNG bytes. Handles icons with and without an alpha channel (falls back
/// to the AND mask). Does not destroy `icon`.
pub fn hicon_to_png(icon: HICON) -> Result<Vec<u8>> {
    unsafe {
        let mut ii = ICONINFO::default();
        GetIconInfo(icon, &mut ii)?;
        let color = ii.hbmColor;
        let mask = ii.hbmMask;
        let result = (|| {
            if color.is_invalid() {
                return Err(Error::Unsupported("monochrome icon".into()));
            }
            let (w, h, mut px) = read_bitmap(color)?;
            let has_alpha = px.chunks_exact(4).any(|p| p[3] != 0);
            if !has_alpha {
                // Use the AND mask: 0 = opaque, non-zero = transparent.
                let mask_px = if mask.is_invalid() { None } else { read_bitmap(mask).ok() };
                for (i, p) in px.chunks_exact_mut(4).enumerate() {
                    let transparent = match &mask_px {
                        Some((mw, mh, m)) if *mw == w && *mh >= h => m[i * 4] != 0,
                        _ => false,
                    };
                    p[3] = if transparent { 0 } else { 255 };
                }
            }
            // BGRA -> RGBA
            for p in px.chunks_exact_mut(4) {
                p.swap(0, 2);
            }
            rgba_to_png(w, h, &px)
        })();
        if !color.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(color.0));
        }
        if !mask.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(mask.0));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_roundtrip() {
        let s = IconStore::new();
        s.put("art:a:1", vec![1]);
        s.put("art:a:2", vec![2]);
        s.put("exe:x", vec![3]);
        s.remove_prefix("art:a:");
        assert!(s.get("art:a:1").is_none());
        assert_eq!(s.get("exe:x").unwrap().as_slice(), &[3]);
    }

    #[test]
    fn png_encodes() {
        let png = rgba_to_png(1, 1, &[255, 0, 0, 255]).unwrap();
        assert_eq!(&png[1..4], b"PNG");
    }
}
