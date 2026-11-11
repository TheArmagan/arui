//! DWM live thumbnails (taskbar previews).
//!
//! The DWM thumbnail API has no thread affinity: HTHUMBNAIL is a per-process handle, and the
//! only requirement is that the destination is a top-level window owned by the calling
//! process. So these functions run on whatever thread calls them (and on the hook thread when
//! a source moves or is destroyed); a mutex guards the handle map.

use std::sync::Arc;

use arui_win::util::to_hwnd;
use arui_win::{Hwnd, Rect, Result};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Dwm::{
    DwmQueryThumbnailSourceSize, DwmRegisterThumbnail, DwmUnregisterThumbnail,
    DwmUpdateThumbnailProperties, DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY,
    DWM_TNP_RECTDESTINATION, DWM_TNP_SOURCECLIENTAREAONLY, DWM_TNP_VISIBLE,
};

use crate::rules::aspect_fit;
use crate::tracker::Shared;

pub struct Thumb {
    id: isize,
    /// Box requested by the UI; the thumbnail is aspect-fit inside it.
    bounds: Rect,
}

fn update(id: isize, bounds: Rect) -> Result<()> {
    unsafe {
        let size = DwmQueryThumbnailSourceSize(id)?;
        let r = aspect_fit(bounds, size.cx, size.cy);
        let props = DWM_THUMBNAIL_PROPERTIES {
            dwFlags: DWM_TNP_RECTDESTINATION
                | DWM_TNP_VISIBLE
                | DWM_TNP_OPACITY
                | DWM_TNP_SOURCECLIENTAREAONLY,
            rcDestination: RECT {
                left: r.left,
                top: r.top,
                right: r.right,
                bottom: r.bottom,
            },
            opacity: 255,
            fVisible: true.into(),
            fSourceClientAreaOnly: false.into(),
            ..Default::default()
        };
        DwmUpdateThumbnailProperties(id, &props)?;
    }
    Ok(())
}

pub fn show(shared: &Arc<Shared>, dest: Hwnd, source: Hwnd, bounds: Rect) -> Result<()> {
    let mut map = shared.thumbs.lock();
    if let Some(t) = map.get_mut(&(dest, source)) {
        t.bounds = bounds;
        return update(t.id, bounds);
    }
    let id = unsafe { DwmRegisterThumbnail(to_hwnd(dest), to_hwnd(source))? };
    if let Err(e) = update(id, bounds) {
        unsafe {
            let _ = DwmUnregisterThumbnail(id);
        }
        return Err(e);
    }
    map.insert((dest, source), Thumb { id, bounds });
    Ok(())
}

pub fn hide(shared: &Arc<Shared>, dest: Hwnd, source: Option<Hwnd>) {
    shared.thumbs.lock().retain(|&(d, s), t| {
        let drop = d == dest && source.is_none_or(|src| src == s);
        if drop {
            unsafe {
                let _ = DwmUnregisterThumbnail(t.id);
            }
        }
        !drop
    });
}

pub fn has_source(shared: &Arc<Shared>, source: Hwnd) -> bool {
    shared.thumbs.lock().keys().any(|&(_, s)| s == source)
}

/// Source was resized: keep the thumbnail aspect-correct.
pub fn source_moved(shared: &Arc<Shared>, source: Hwnd) {
    for (&(_, s), t) in shared.thumbs.lock().iter() {
        if s == source {
            let _ = update(t.id, t.bounds);
        }
    }
}

/// Source window destroyed (or destination gone): unregister its thumbnails.
pub fn source_gone(shared: &Arc<Shared>, hwnd: Hwnd) {
    shared.thumbs.lock().retain(|&(d, s), t| {
        let drop = s == hwnd || d == hwnd;
        if drop {
            unsafe {
                let _ = DwmUnregisterThumbnail(t.id);
            }
        }
        !drop
    });
}

pub fn clear(shared: &Arc<Shared>) {
    for (_, t) in shared.thumbs.lock().drain() {
        unsafe {
            let _ = DwmUnregisterThumbnail(t.id);
        }
    }
}
