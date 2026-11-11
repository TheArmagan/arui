//! arui-windows: event-driven taskbar window tracker.
//!
//! * [`start`] spawns the tracker; the sink gets `NativeEvent::Window(Snapshot)` once, then
//!   `Added` / `Updated` / `Removed` / `Focused` diffs. `Updated` is only sent when a field
//!   other than `is_focused` changed; focus changes come as `Focused`.
//! * Sources: WinEvent hooks (create/destroy/show/hide/name/location/parent/cloak, foreground,
//!   minimize) and a shell hook window (created/destroyed/flash/redraw/activated). No polling.
//! * Eligibility follows Alt-Tab rules, see [`rules::is_eligible`].
//! * Grouping: [`WindowInfo::app_id`] is the explicit window AUMID, else the package AUMID of
//!   the (hosted, for ApplicationFrameHost) app process, else the lowercased exe path.
//! * Icons are put into the shared [`IconStore`] by a worker thread (`app:`, `hwnd:`, `exe:`
//!   keys), followed by an `Updated` carrying the new `icon_key`.

mod actions;
mod hook;
mod icons;
mod probe;
pub mod rules;
mod thumbs;
mod tracker;

pub use arui_win::{Hwnd, IconStore, Rect, WindowEvent, WindowInfo};
pub use tracker::{start, WindowTracker};
