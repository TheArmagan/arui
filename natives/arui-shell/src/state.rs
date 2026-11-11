//! Crash-recovery state file.
//!
//! While a [`crate::Shell`] is alive it keeps a small JSON file (by default
//! `%TEMP%\arui-shell-state.json`) describing what it changed on the system: which windows are
//! registered as AppBars and what explorer's taskbar state was before we touched it. On a clean
//! shutdown the file is deleted. If the host dies without running `Drop`, `arui-guard` (or the
//! next `start()`) finds the file and undoes the changes with [`crate::emergency_restore_state`].

use std::io;
use std::path::{Path, PathBuf};

use arui_win::Hwnd;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellState {
    /// Process that owns the changes.
    pub pid: u32,
    /// Windows registered with ABM_NEW that need ABM_REMOVE.
    pub appbars: Vec<Hwnd>,
    /// Whether explorer's taskbars are currently hidden by us.
    pub explorer_hidden: bool,
    /// Explorer's ABM_GETSTATE value before we forced auto-hide, if we changed it.
    pub explorer_prev_state: Option<u32>,
}

/// `%TEMP%\arui-shell-state.json`.
pub fn default_state_file() -> PathBuf {
    std::env::temp_dir().join("arui-shell-state.json")
}

impl ShellState {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("ShellState serializes")
    }

    pub fn from_json(s: &str) -> Option<Self> {
        serde_json::from_str(s).ok()
    }

    /// Reads the file; `None` if it is missing or unreadable.
    pub fn load(path: &Path) -> Option<Self> {
        let s = std::fs::read_to_string(path).ok()?;
        Self::from_json(&s)
    }

    /// Writes atomically (temp file + rename) so a crash mid-write never leaves half a file.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, self.to_json())?;
        std::fs::rename(&tmp, path)
    }

    pub fn delete(path: &Path) {
        let _ = std::fs::remove_file(path);
    }

    /// True when restoring this state would change nothing.
    pub fn is_noop(&self) -> bool {
        self.appbars.is_empty() && !self.explorer_hidden && self.explorer_prev_state.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_roundtrip() {
        let s = ShellState {
            pid: 1234,
            appbars: vec![0x10_2030, 42],
            explorer_hidden: true,
            explorer_prev_state: Some(2),
        };
        let j = s.to_json();
        assert!(j.contains("\"explorerPrevState\": 2"));
        assert_eq!(ShellState::from_json(&j), Some(s));
        assert_eq!(ShellState::from_json("not json"), None);
    }

    #[test]
    fn file_roundtrip() {
        let dir = std::env::temp_dir().join(format!("arui-shell-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        let s = ShellState { pid: 7, appbars: vec![1], explorer_hidden: false, explorer_prev_state: None };
        s.save(&path).unwrap();
        assert_eq!(ShellState::load(&path), Some(s.clone()));
        assert!(!s.is_noop());
        ShellState::delete(&path);
        assert_eq!(ShellState::load(&path), None);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(ShellState { pid: 1, ..Default::default() }.is_noop());
    }
}
