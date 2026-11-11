//! Locations of ARUI's user data: %APPDATA%\ARUI\{settings.json, mods\}.

use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("ARUI")
}

pub fn settings_file() -> PathBuf {
    data_dir().join("settings.json")
}

pub fn mods_dir() -> PathBuf {
    data_dir().join("mods")
}
