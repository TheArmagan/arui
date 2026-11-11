//! Shared foundation for every ARUI native crate.
//!
//! * [`types`]: the serde data contract shared with the UI (mirrored in `renderers/sdk/src/types.ts`).
//! * [`EventSink`]: how services push [`NativeEvent`]s to the host (the Tauri app).
//! * [`icons`]: in-memory PNG store served to the UI as `arui-icon://localhost/<key>`.
//! * [`util`]: wide strings, COM init guard, HWND conversions.
//!
//! Service crate convention: `pub fn start(sink: EventSink, ...) -> Result<Service>`; the returned
//! `Service` exposes typed actions and stops its threads on `Drop`. Services must not poll; use
//! Win32 hooks / WinRT events and run their own message-loop thread when needed.

pub mod error;
pub mod icons;
pub mod types;
pub mod util;

pub use error::{Error, Result};
pub use icons::IconStore;
pub use types::*;

use std::sync::Arc;

/// Callback the host passes to each service. Must be cheap and thread-safe; it is called from
/// hook / message-loop threads.
pub type EventSink = Arc<dyn Fn(NativeEvent) + Send + Sync + 'static>;
