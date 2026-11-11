//! Data contract between natives and the UI.
//!
//! Every type serializes as camelCase JSON. Enums are adjacently tagged:
//! `{ "type": "added", "data": { ... } }`. Keep this file and
//! `renderers/sdk/src/types.ts` in sync.

use serde::{Deserialize, Serialize};

/// Window handle as a plain integer (safe in JS: HWNDs fit in 53 bits).
pub type Hwnd = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Edge {
    Left,
    Top,
    Right,
    Bottom,
}

// ---------------------------------------------------------------- monitors

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    /// Stable id: the GDI device name, e.g. `\\.\DISPLAY1`.
    pub id: String,
    /// HMONITOR as integer (not stable across display changes).
    pub handle: u64,
    /// Physical pixels.
    pub bounds: Rect,
    /// Physical pixels, as currently reserved by appbars.
    pub work_area: Rect,
    /// DPI scale, 1.0 = 96 DPI.
    pub scale: f64,
    pub primary: bool,
}

// ---------------------------------------------------------------- windows

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    pub hwnd: Hwnd,
    pub pid: u32,
    pub title: String,
    pub class_name: String,
    /// Full path of the owning executable (for UWP: the real app exe, not ApplicationFrameHost).
    pub exe_path: String,
    /// Grouping key: explicit AppUserModelID if present, otherwise the lowercased exe path.
    pub app_id: String,
    /// Key into the icon store (`arui-icon://localhost/<key>`), e.g. `hwnd:1234` or `exe:c:\...`.
    pub icon_key: String,
    /// HMONITOR the window is mostly on.
    pub monitor: u64,
    pub is_minimized: bool,
    pub is_maximized: bool,
    pub is_focused: bool,
    /// Window requested attention (HSHELL_FLASH).
    pub is_flashing: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum WindowEvent {
    /// Full list, sent once on start and after resyncs.
    Snapshot(Vec<WindowInfo>),
    Added(WindowInfo),
    Updated(WindowInfo),
    Removed { hwnd: Hwnd },
    /// Foreground changed; `None` = desktop / a non-taskbar window.
    Focused { hwnd: Option<Hwnd> },
}

// ---------------------------------------------------------------- start menu apps

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppKind {
    /// `.lnk` from a Start Menu folder.
    Shortcut,
    /// Packaged (UWP / MSIX) app from shell:AppsFolder.
    Packaged,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEntry {
    /// Stable id: AUMID for packaged apps, lowercased .lnk path for shortcuts.
    pub id: String,
    pub name: String,
    pub kind: AppKind,
    /// Shortcut path or AUMID; what `launch` uses.
    pub target: String,
    /// Resolved executable for shortcuts (used to match running windows), if known.
    pub exe_path: Option<String>,
    /// AppUserModelID if known (matches `WindowInfo::app_id`).
    pub app_id: Option<String>,
    pub icon_key: String,
    /// Start menu sub folder, e.g. "Accessories".
    pub folder: Option<String>,
}

// ---------------------------------------------------------------- tray

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayIcon {
    /// `guid:<GUID>` when the icon has one, else `<ownerHwnd>:<uid>`.
    pub id: String,
    pub owner_hwnd: Hwnd,
    pub uid: u32,
    pub guid: Option<String>,
    pub callback_message: u32,
    /// NOTIFYICON_VERSION (0, 3 or 4).
    pub version: u32,
    pub tooltip: String,
    pub icon_key: String,
    /// False when NIS_HIDDEN is set.
    pub visible: bool,
    pub exe_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum TrayEvent {
    Snapshot(Vec<TrayIcon>),
    Added(TrayIcon),
    Updated(TrayIcon),
    Removed { id: String },
    Balloon { id: String, title: String, text: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrayAction {
    LeftClick,
    RightClick,
    MiddleClick,
    DoubleClick,
    HoverEnter,
    HoverLeave,
}

// ---------------------------------------------------------------- media

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlaybackStatus {
    Closed,
    Opened,
    Changing,
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaSession {
    /// SourceAppUserModelId (unique per session in practice).
    pub id: String,
    pub app_id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub status: PlaybackStatus,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_next: bool,
    pub can_previous: bool,
    pub position_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    /// When the position was sampled (unix ms), so the UI can extrapolate while playing.
    pub position_at_ms: Option<u64>,
    /// Icon store key for the artwork (`art:<id>:<n>`), changes when artwork changes.
    pub artwork_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum MediaEvent {
    Snapshot { sessions: Vec<MediaSession>, current: Option<String> },
    Updated(MediaSession),
    Removed { id: String },
    Current { id: Option<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MediaAction {
    Play,
    Pause,
    TogglePlayPause,
    Next,
    Previous,
    Stop,
}

// ---------------------------------------------------------------- system

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeState {
    /// 0.0 - 1.0
    pub level: f32,
    pub muted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NetworkKind {
    None,
    Ethernet,
    Wifi,
    Cellular,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkState {
    pub connected: bool,
    pub internet: bool,
    pub kind: NetworkKind,
    /// SSID / profile name if known.
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum SystemEvent {
    Volume(VolumeState),
    Network(NetworkState),
}

// ---------------------------------------------------------------- shell

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum ShellEvent {
    /// Monitor set, resolution, DPI or work area changed.
    MonitorsChanged(Vec<MonitorInfo>),
    /// explorer.exe restarted (TaskbarCreated). Shell already re-hid the taskbar.
    ExplorerRestarted,
    /// Lone Win key press (start menu request).
    StartKey,
    /// Start menu app index changed.
    AppsChanged(Vec<AppEntry>),
    /// Some app went fullscreen on a monitor (ABN_FULLSCREENAPP); bars may hide.
    Fullscreen { monitor: u64, active: bool },
}

// ---------------------------------------------------------------- envelope

/// What services push through the [`crate::EventSink`]. The host forwards each variant to the
/// UI as Tauri event `native://<variant>` with the inner event as payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", content = "event", rename_all = "camelCase")]
pub enum NativeEvent {
    Window(WindowEvent),
    Tray(TrayEvent),
    Media(MediaEvent),
    System(SystemEvent),
    Shell(ShellEvent),
}

impl NativeEvent {
    /// Tauri event name for this event.
    pub fn channel(&self) -> &'static str {
        match self {
            NativeEvent::Window(_) => "native://window",
            NativeEvent::Tray(_) => "native://tray",
            NativeEvent::Media(_) => "native://media",
            NativeEvent::System(_) => "native://system",
            NativeEvent::Shell(_) => "native://shell",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_json_shape() {
        let e = WindowEvent::Removed { hwnd: 42 };
        assert_eq!(
            serde_json::to_string(&e).unwrap(),
            r#"{"type":"removed","data":{"hwnd":42}}"#
        );
        let e = WindowEvent::Focused { hwnd: None };
        assert_eq!(
            serde_json::to_string(&e).unwrap(),
            r#"{"type":"focused","data":{"hwnd":null}}"#
        );
    }
}
