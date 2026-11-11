//! Surface windows. Each surface (taskbar, start menu, flyouts, mod surfaces) is a webview window
//! loading `index.html?surface=<id>&monitor=<monitorId>`. Specs come from the UI's host window
//! (`surfaces_define`), so mods decide what surfaces exist and how they are placed.

use std::collections::HashMap;

use arui_win::{Edge, MonitorInfo, Rect};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SurfaceKind {
    Appbar,
    #[default]
    Popup,
    Panel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Anchor {
    #[default]
    Center,
    Cursor,
    Rect,
    BottomLeft,
    BottomCenter,
    BottomRight,
    TopLeft,
    TopCenter,
    TopRight,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowOptions {
    pub kind: SurfaceKind,
    pub edge: Option<Edge>,
    pub thickness: Option<f64>,
    pub reserve: Option<bool>,
    pub per_monitor: Option<bool>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub anchor: Option<Anchor>,
    pub offset: Option<f64>,
    pub close_on_blur: Option<bool>,
    pub focusable: Option<bool>,
    pub auto_open: Option<bool>,
}

impl WindowOptions {
    pub fn is_appbar(&self) -> bool {
        self.kind == SurfaceKind::Appbar
    }
    pub fn edge(&self) -> Edge {
        self.edge.unwrap_or(Edge::Bottom)
    }
    pub fn thickness(&self) -> f64 {
        self.thickness.unwrap_or(64.0)
    }
    pub fn reserve(&self) -> bool {
        self.reserve.unwrap_or(true)
    }
    pub fn per_monitor(&self) -> bool {
        self.per_monitor.unwrap_or(self.is_appbar())
    }
    pub fn focusable(&self) -> bool {
        self.focusable.unwrap_or(!self.is_appbar())
    }
    pub fn close_on_blur(&self) -> bool {
        self.close_on_blur.unwrap_or(self.kind == SurfaceKind::Popup)
    }
    pub fn auto_open(&self) -> bool {
        self.auto_open.unwrap_or(self.is_appbar())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceSpec {
    pub id: String,
    pub mod_id: String,
    pub window: WindowOptions,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OpenArgs {
    pub id: String,
    pub monitor: Option<String>,
    pub rect: Option<Rect>,
    #[serde(default)]
    pub params: Option<serde_json::Map<String, Value>>,
}

/// A live surface window.
#[derive(Debug, Clone)]
pub struct SurfaceWindow {
    pub surface: String,
    pub monitor: Option<String>,
    pub params: Value,
    pub spec: SurfaceSpec,
}

#[derive(Default)]
pub struct Surfaces {
    pub specs: HashMap<String, SurfaceSpec>,
    /// label -> window
    pub windows: HashMap<String, SurfaceWindow>,
    /// label -> when a popup was last hidden by losing focus. Clicking the button that toggles a
    /// popup first blurs it, so a toggle right after a blur-hide must not reopen it.
    pub blurred: HashMap<String, std::time::Instant>,
}

/// Window label for a surface: Tauri labels allow `a-zA-Z0-9-/:_`.
pub fn label(surface: &str, monitor: Option<&str>) -> String {
    let clean = |s: &str| {
        s.chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect::<String>()
    };
    match monitor {
        Some(m) => format!("s:{}:{}", clean(surface), clean(m.trim_start_matches(r"\\.\"))),
        None => format!("s:{}", clean(surface)),
    }
}

pub fn url(surface: &str, monitor: Option<&str>) -> String {
    let enc = |s: &str| percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string();
    match monitor {
        Some(m) => format!("index.html?surface={}&monitor={}", enc(surface), enc(m)),
        None => format!("index.html?surface={}", enc(surface)),
    }
}

/// Placement of a popup/panel in physical px, clamped into the monitor's work area.
pub fn place(
    opts: &WindowOptions,
    monitor: &MonitorInfo,
    cursor: (i32, i32),
    rect: Option<Rect>,
) -> Rect {
    let s = monitor.scale;
    let w = (opts.width.unwrap_or(480.0) * s).round() as i32;
    let h = (opts.height.unwrap_or(560.0) * s).round() as i32;
    let off = (opts.offset.unwrap_or(8.0) * s).round() as i32;
    let wa = monitor.work_area;
    let cx = wa.left + (wa.width() - w) / 2;
    let (x, y) = match opts.anchor.unwrap_or_default() {
        Anchor::Center => (cx, wa.top + (wa.height() - h) / 2),
        Anchor::Cursor => (cursor.0 - w / 2, cursor.1 - h - off),
        Anchor::Rect => match rect {
            // Above the element, centered on it; flips below if it doesn't fit.
            Some(r) => {
                let x = r.left + (r.width() - w) / 2;
                let above = r.top - h - off;
                (x, if above >= wa.top { above } else { r.bottom + off })
            }
            None => (cursor.0 - w / 2, cursor.1 - h - off),
        },
        Anchor::BottomLeft => (wa.left + off, wa.bottom - h - off),
        Anchor::BottomCenter => (cx, wa.bottom - h - off),
        Anchor::BottomRight => (wa.right - w - off, wa.bottom - h - off),
        Anchor::TopLeft => (wa.left + off, wa.top + off),
        Anchor::TopCenter => (cx, wa.top + off),
        Anchor::TopRight => (wa.right - w - off, wa.top + off),
    };
    // Clamp to the full monitor bounds (popups may overlap appbars, e.g. previews).
    let b = monitor.bounds;
    let x = x.clamp(b.left, (b.right - w).max(b.left));
    let y = y.clamp(b.top, (b.bottom - h).max(b.top));
    Rect { left: x, top: y, right: x + w, bottom: y + h }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon() -> MonitorInfo {
        MonitorInfo {
            id: r"\\.\DISPLAY1".into(),
            handle: 1,
            bounds: Rect { left: 0, top: 0, right: 1920, bottom: 1080 },
            work_area: Rect { left: 0, top: 0, right: 1920, bottom: 1000 },
            scale: 1.0,
            primary: true,
        }
    }

    #[test]
    fn labels() {
        assert_eq!(label("taskbar", Some(r"\\.\DISPLAY1")), "s:taskbar:DISPLAY1");
        assert_eq!(label("start-menu", None), "s:start-menu");
    }

    #[test]
    fn bottom_center_sits_on_work_area() {
        let o = WindowOptions { width: Some(600.0), height: Some(400.0), anchor: Some(Anchor::BottomCenter), offset: Some(10.0), ..Default::default() };
        let r = place(&o, &mon(), (0, 0), None);
        assert_eq!(r, Rect { left: 660, top: 590, right: 1260, bottom: 990 });
    }

    #[test]
    fn rect_anchor_clamps() {
        let o = WindowOptions { width: Some(300.0), height: Some(200.0), anchor: Some(Anchor::Rect), offset: Some(0.0), ..Default::default() };
        let r = place(&o, &mon(), (0, 0), Some(Rect { left: 0, top: 1010, right: 40, bottom: 1070 }));
        assert_eq!(r.left, 0);
        assert_eq!(r.bottom, 1010);
    }

    #[test]
    fn options_from_json() {
        let o: WindowOptions = serde_json::from_str(r#"{"kind":"appbar","edge":"left","thickness":56}"#).unwrap();
        assert!(o.is_appbar() && o.per_monitor() && !o.focusable() && o.auto_open());
        assert_eq!(o.edge(), Edge::Left);
    }
}
