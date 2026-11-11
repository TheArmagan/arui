//! Pure logic with no Win32 calls: taskbar eligibility, app id normalization, diffing and
//! thumbnail aspect fitting. Everything here is unit tested.

use std::collections::{HashMap, HashSet};

use arui_win::{Hwnd, Rect, WindowEvent, WindowInfo};

// Extended / normal window style bits we look at (values from WinUser.h, kept here so the
// rules stay plain data and are testable without Win32).
pub const WS_CHILD: u32 = 0x4000_0000;
pub const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
pub const WS_EX_APPWINDOW: u32 = 0x0004_0000;
pub const WS_EX_NOACTIVATE: u32 = 0x0800_0000;

/// Window class of the UWP frame window hosted by ApplicationFrameHost.exe.
pub const UWP_FRAME_CLASS: &str = "ApplicationFrameWindow";
/// Window class of the UWP content window (child of the frame while the app is running).
pub const UWP_CORE_CLASS: &str = "Windows.UI.Core.CoreWindow";

/// Top-level windows of these classes never get a taskbar button: they belong to the shell
/// itself, or are top-level duplicates of UWP content windows (the real button is the frame).
pub const SKIPPED_CLASSES: &[&str] = &[
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Progman",
    "WorkerW",
    "Windows.UI.Core.CoreWindow",
    "DV2ControlHost",
    "MsgrIMEWindowClass",
    "SysShadow",
    "Button",
    "tooltips_class32",
    "NotifyIconOverflowWindow",
    "TopLevelWindowForOverflowXamlIsland",
    "XamlExplorerHostIslandWindow",
    "Windows.UI.Input.InputSite.WindowClass",
    "ForegroundStaging",
    "MultitaskingViewFrame",
    "TaskListThumbnailWnd",
    "EdgeUiInputTopWndClass",
    "EdgeUiInputWndClass",
    "Shell_InputSwitchTopLevelWindow",
    "ApplicationManager_ImmersiveShellWindow",
    "ApplicationManager_DesktopShellWindow",
    "Internet Explorer_Hidden",
    "Windows.Internal.Shell.TabProxyWindow",
    "Shell_Dim",
    "Shell_LightDismissOverlay",
];

/// Plain snapshot of the attributes that decide whether a window gets a taskbar button.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowAttrs {
    pub visible: bool,
    pub cloaked: bool,
    pub style: u32,
    pub ex_style: u32,
    pub has_owner: bool,
    pub title: String,
    pub class_name: String,
    pub pid: u32,
}

/// Alt-Tab style eligibility. `ignored_pids` are ARUI's own processes.
pub fn is_eligible(a: &WindowAttrs, ignored_pids: &HashSet<u32>) -> bool {
    let app_window = a.ex_style & WS_EX_APPWINDOW != 0;
    if !a.visible || a.cloaked {
        return false;
    }
    if a.style & WS_CHILD != 0 {
        return false;
    }
    if ignored_pids.contains(&a.pid) {
        return false;
    }
    if SKIPPED_CLASSES.contains(&a.class_name.as_str()) {
        return false;
    }
    if !app_window {
        if a.ex_style & WS_EX_TOOLWINDOW != 0 {
            return false;
        }
        if a.has_owner {
            return false;
        }
        if a.ex_style & WS_EX_NOACTIVATE != 0 {
            return false;
        }
    }
    if a.title.trim().is_empty() && a.class_name != UWP_FRAME_CLASS {
        return false;
    }
    true
}

/// Grouping key. Explicit window AUMID wins, then the package AUMID of the (hosted) process,
/// then the lowercased exe path. `pid` is only used when nothing else is known.
pub fn normalize_app_id(
    explicit: Option<&str>,
    package: Option<&str>,
    exe_path: &str,
    pid: u32,
) -> String {
    if let Some(id) = explicit.map(str::trim).filter(|s| !s.is_empty()) {
        return id.to_string();
    }
    if let Some(id) = package.map(str::trim).filter(|s| !s.is_empty()) {
        return id.to_string();
    }
    let exe = normalize_path(exe_path);
    if exe.is_empty() {
        format!("pid:{pid}")
    } else {
        exe
    }
}

/// Lowercase, backslash separators, `\\?\` prefix stripped. Used for app ids and `exe:` keys.
pub fn normalize_path(path: &str) -> String {
    let p = path.trim();
    let p = p.strip_prefix(r"\\?\").unwrap_or(p);
    p.replace('/', "\\").to_lowercase()
}

/// Largest rect with the source aspect ratio that fits inside `bounds`, centered.
pub fn aspect_fit(bounds: Rect, src_w: i32, src_h: i32) -> Rect {
    let (bw, bh) = (bounds.width(), bounds.height());
    if src_w <= 0 || src_h <= 0 || bw <= 0 || bh <= 0 {
        return bounds;
    }
    // Compare bw/bh with src_w/src_h without floating point rounding surprises.
    let (w, h) = if (bw as i64) * (src_h as i64) > (bh as i64) * (src_w as i64) {
        // Bounds are wider than the source: height limited.
        let w = ((bh as i64 * src_w as i64 + src_h as i64 / 2) / src_h as i64) as i32;
        (w.clamp(1, bw), bh)
    } else {
        let h = ((bw as i64 * src_h as i64 + src_w as i64 / 2) / src_w as i64) as i32;
        (bw, h.clamp(1, bh))
    };
    let left = bounds.left + (bw - w) / 2;
    let top = bounds.top + (bh - h) / 2;
    Rect {
        left,
        top,
        right: left + w,
        bottom: top + h,
    }
}

/// Event for a single window transition. `None` -> nothing changed.
pub fn diff_one(old: Option<&WindowInfo>, new: Option<&WindowInfo>) -> Option<WindowEvent> {
    match (old, new) {
        (None, None) => None,
        (None, Some(n)) => Some(WindowEvent::Added(n.clone())),
        (Some(o), None) => Some(WindowEvent::Removed { hwnd: o.hwnd }),
        (Some(o), Some(n)) if o != n => Some(WindowEvent::Updated(n.clone())),
        _ => None,
    }
}

/// Events that turn `old` into `new` (removals first, then additions/updates in `new` order).
pub fn diff_lists(old: &[WindowInfo], new: &[WindowInfo]) -> Vec<WindowEvent> {
    let old_map: HashMap<Hwnd, &WindowInfo> = old.iter().map(|w| (w.hwnd, w)).collect();
    let new_set: HashSet<Hwnd> = new.iter().map(|w| w.hwnd).collect();
    let mut out: Vec<WindowEvent> = old
        .iter()
        .filter(|w| !new_set.contains(&w.hwnd))
        .map(|w| WindowEvent::Removed { hwnd: w.hwnd })
        .collect();
    out.extend(
        new.iter()
            .filter_map(|n| diff_one(old_map.get(&n.hwnd).copied(), Some(n))),
    );
    out
}

/// Pixel-identical PNG check used to decide whether a window icon is just the exe icon.
pub fn same_image(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a == b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normal() -> WindowAttrs {
        WindowAttrs {
            visible: true,
            title: "Untitled - Notepad".into(),
            class_name: "Notepad".into(),
            pid: 100,
            ..Default::default()
        }
    }

    fn ok(a: &WindowAttrs) -> bool {
        is_eligible(a, &HashSet::new())
    }

    #[test]
    fn normal_window_is_eligible() {
        assert!(ok(&normal()));
    }

    #[test]
    fn hidden_or_cloaked_is_not() {
        assert!(!ok(&WindowAttrs {
            visible: false,
            ..normal()
        }));
        assert!(!ok(&WindowAttrs {
            cloaked: true,
            ..normal()
        }));
    }

    #[test]
    fn tool_window_needs_appwindow() {
        let tool = WindowAttrs {
            ex_style: WS_EX_TOOLWINDOW,
            ..normal()
        };
        assert!(!ok(&tool));
        let both = WindowAttrs {
            ex_style: WS_EX_TOOLWINDOW | WS_EX_APPWINDOW,
            ..normal()
        };
        assert!(ok(&both));
    }

    #[test]
    fn owned_window_needs_appwindow() {
        let owned = WindowAttrs {
            has_owner: true,
            ..normal()
        };
        assert!(!ok(&owned));
        assert!(ok(&WindowAttrs {
            ex_style: WS_EX_APPWINDOW,
            ..owned
        }));
    }

    #[test]
    fn noactivate_needs_appwindow() {
        let na = WindowAttrs {
            ex_style: WS_EX_NOACTIVATE,
            ..normal()
        };
        assert!(!ok(&na));
        assert!(ok(&WindowAttrs {
            ex_style: WS_EX_NOACTIVATE | WS_EX_APPWINDOW,
            ..na
        }));
    }

    #[test]
    fn empty_title_only_for_uwp_frame() {
        assert!(!ok(&WindowAttrs {
            title: "  ".into(),
            ..normal()
        }));
        assert!(ok(&WindowAttrs {
            title: String::new(),
            class_name: UWP_FRAME_CLASS.into(),
            ..normal()
        }));
    }

    #[test]
    fn shell_classes_and_child_windows_skipped() {
        for c in ["Shell_TrayWnd", "Progman", "WorkerW", UWP_CORE_CLASS] {
            assert!(
                !ok(&WindowAttrs {
                    class_name: c.into(),
                    ..normal()
                }),
                "{c}"
            );
        }
        assert!(!ok(&WindowAttrs {
            style: WS_CHILD,
            ..normal()
        }));
    }

    #[test]
    fn ignored_pids_skipped() {
        let ignored: HashSet<u32> = [100].into_iter().collect();
        assert!(!is_eligible(&normal(), &ignored));
        assert!(is_eligible(&WindowAttrs { pid: 7, ..normal() }, &ignored));
    }

    #[test]
    fn app_id_priority_and_normalization() {
        assert_eq!(
            normalize_app_id(Some("Chrome.App.X"), Some("Pkg!App"), r"C:\a.exe", 1),
            "Chrome.App.X"
        );
        assert_eq!(
            normalize_app_id(Some("  "), Some("Pkg!App"), r"C:\a.exe", 1),
            "Pkg!App"
        );
        assert_eq!(
            normalize_app_id(None, None, r"\\?\C:/Program Files/App/App.EXE", 1),
            r"c:\program files\app\app.exe"
        );
        assert_eq!(normalize_app_id(None, Some(""), "", 42), "pid:42");
    }

    #[test]
    fn aspect_fit_wide_and_tall() {
        let b = Rect {
            left: 10,
            top: 20,
            right: 210,
            bottom: 120,
        }; // 200x100
           // 16:9 source in a 2:1 box -> height limited: 178x100 centered.
        let r = aspect_fit(b, 1920, 1080);
        assert_eq!((r.width(), r.height()), (178, 100));
        assert_eq!(r.top, 20);
        assert_eq!(r.left, 10 + (200 - 178) / 2);
        // Tall source -> height limited, narrow.
        let r = aspect_fit(b, 500, 1000);
        assert_eq!((r.width(), r.height()), (50, 100));
        // Very wide source -> width limited.
        let r = aspect_fit(b, 4000, 1000);
        assert_eq!((r.width(), r.height()), (200, 50));
        assert_eq!(r.top, 20 + 25);
        // Degenerate input returns the bounds.
        assert_eq!(aspect_fit(b, 0, 10), b);
    }

    fn info(hwnd: Hwnd, title: &str) -> WindowInfo {
        WindowInfo {
            hwnd,
            pid: 1,
            title: title.into(),
            class_name: "C".into(),
            exe_path: "x".into(),
            app_id: "x".into(),
            icon_key: String::new(),
            monitor: 0,
            is_minimized: false,
            is_maximized: false,
            is_focused: false,
            is_flashing: false,
        }
    }

    #[test]
    fn diff_one_cases() {
        let a = info(1, "a");
        assert_eq!(
            diff_one(None, Some(&a)),
            Some(WindowEvent::Added(a.clone()))
        );
        assert_eq!(
            diff_one(Some(&a), None),
            Some(WindowEvent::Removed { hwnd: 1 })
        );
        assert_eq!(diff_one(Some(&a), Some(&a.clone())), None);
        let b = info(1, "b");
        assert_eq!(diff_one(Some(&a), Some(&b)), Some(WindowEvent::Updated(b)));
    }

    #[test]
    fn diff_lists_mixed() {
        let old = vec![info(1, "a"), info(2, "b"), info(3, "c")];
        let new = vec![info(2, "b"), info(3, "c2"), info(4, "d")];
        let ev = diff_lists(&old, &new);
        assert_eq!(
            ev,
            vec![
                WindowEvent::Removed { hwnd: 1 },
                WindowEvent::Updated(info(3, "c2")),
                WindowEvent::Added(info(4, "d")),
            ]
        );
    }
}
