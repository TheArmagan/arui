//! External mod discovery (%APPDATA%\ARUI\mods\<id>\mod.json) and hot-reload watching.
//! Built-in mods ship inside the UI bundle; the UI merges them with this list.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use serde::Serialize;
use serde_json::Value;

use crate::settings::Settings;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledMod {
    pub manifest: Value,
    pub base_url: String,
    pub builtin: bool,
    pub enabled: bool,
}

/// `core.enabledMods` = { "<id>": bool }. External mods default to enabled.
pub fn is_enabled(settings: &Settings, id: &str) -> bool {
    settings
        .get("core", "enabledMods")
        .and_then(|v| v.get(id).and_then(Value::as_bool))
        .unwrap_or(true)
}

pub fn set_enabled(settings: &Settings, id: &str, enabled: bool) -> std::io::Result<()> {
    let mut map = settings
        .get("core", "enabledMods")
        .filter(Value::is_object)
        .unwrap_or_else(|| Value::Object(Default::default()));
    map.as_object_mut().unwrap().insert(id.to_string(), Value::Bool(enabled));
    settings.set("core", "enabledMods", map)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !id.starts_with('.')
}

pub fn scan(dir: &Path, settings: &Settings) -> Vec<InstalledMod> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(text) = std::fs::read_to_string(path.join("mod.json")) else { continue };
        let Ok(manifest) = serde_json::from_str::<Value>(&text) else {
            log::warn!("invalid mod.json in {}", path.display());
            continue;
        };
        let Some(id) = manifest.get("id").and_then(Value::as_str).map(str::to_string) else { continue };
        // The folder name must match the id so mod URLs map to folders directly.
        if !valid_id(&id) || entry.file_name().to_string_lossy() != id {
            log::warn!("mod folder {} does not match id {id:?}", path.display());
            continue;
        }
        out.push(InstalledMod {
            enabled: is_enabled(settings, &id),
            base_url: format!("http://arui-mod.localhost/{id}/"),
            builtin: false,
            manifest,
        });
    }
    out.sort_by(|a, b| a.base_url.cmp(&b.base_url));
    out
}

/// Resolves `<id>/<path>` inside the mods dir, rejecting traversal.
pub fn resolve_file(dir: &Path, url_path: &str) -> Option<PathBuf> {
    let decoded = percent_encoding::percent_decode_str(url_path.trim_start_matches('/'))
        .decode_utf8()
        .ok()?;
    let mut parts = decoded.split('/').filter(|p| !p.is_empty());
    let id = parts.next()?;
    if !valid_id(id) {
        return None;
    }
    let mut path = dir.join(id);
    for part in parts {
        if part == ".." || part == "." || part.contains(['\\', ':']) {
            return None;
        }
        path.push(part);
    }
    path.is_file().then_some(path)
}

pub fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("js" | "mjs") => "text/javascript",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("html") => "text/html",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        _ => "application/octet-stream",
    }
}

pub enum ModChange {
    /// A file inside a mod changed.
    Files(String),
    /// A mod.json appeared, disappeared or changed.
    List,
}

/// Watches the mods dir and calls `on_change` once a burst of file events has been quiet for 250 ms.
pub fn watch(
    dir: PathBuf,
    on_change: impl Fn(ModChange) + Send + 'static,
) -> notify::Result<notify::RecommendedWatcher> {
    std::fs::create_dir_all(&dir).ok();
    // (mod id, manifest changed)
    let (tx, rx) = std::sync::mpsc::channel::<(String, bool)>();

    std::thread::Builder::new()
        .name("arui-mods-debounce".into())
        .spawn(move || {
            // Block until the first event, then collect until 250 ms of silence.
            while let Ok(first) = rx.recv() {
                let mut batch: HashMap<String, bool> = HashMap::new();
                batch.insert(first.0, first.1);
                loop {
                    match rx.recv_timeout(Duration::from_millis(250)) {
                        Ok((id, manifest)) => *batch.entry(id).or_default() |= manifest,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
                        Err(_) => return,
                    }
                }
                let list_changed = batch.values().any(|m| *m);
                for id in batch.into_keys() {
                    on_change(ModChange::Files(id));
                }
                if list_changed {
                    on_change(ModChange::List);
                }
            }
        })
        .ok();

    let root = dir.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };
        for path in event.paths {
            let Ok(rel) = path.strip_prefix(&root) else { continue };
            let mut comps = rel.components();
            let Some(id) = comps.next().map(|c| c.as_os_str().to_string_lossy().to_string()) else { continue };
            let rest: PathBuf = comps.collect();
            // Source and dependency churn should not reload the mod; only built output and the manifest matter.
            let rest_str = rest.to_string_lossy().replace('\\', "/");
            if rest_str.starts_with("node_modules") || rest_str.starts_with("src/") {
                continue;
            }
            let manifest = rest_str.is_empty() || rest_str == "mod.json";
            let _ = tx.send((id, manifest));
        }
    })?;
    watcher.watch(&dir, RecursiveMode::Recursive)?;
    Ok(watcher)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal() {
        let dir = std::env::temp_dir();
        assert!(resolve_file(&dir, "/../x").is_none());
        assert!(resolve_file(&dir, "/a/../../x").is_none());
        assert!(resolve_file(&dir, "/a/%2e%2e/x").is_none());
        assert!(resolve_file(&dir, "/a/c:/x").is_none());
    }

    #[test]
    fn ids() {
        assert!(valid_id("focus-timer"));
        assert!(!valid_id("../x"));
        assert!(!valid_id(".hidden"));
    }
}
