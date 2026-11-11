//! settings.json: `{ "<namespace>": { "<key>": value } }`. Core uses "core", mods their id.
//! Written atomically (temp file + rename) on every change.

use std::path::PathBuf;

use parking_lot::Mutex;
use serde_json::{Map, Value};

pub struct Settings {
    path: PathBuf,
    data: Mutex<Map<String, Value>>,
}

impl Settings {
    pub fn load(path: PathBuf) -> Self {
        let data = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .and_then(|v| match v {
                Value::Object(m) => Some(m),
                _ => None,
            })
            .unwrap_or_default();
        Self { path, data: Mutex::new(data) }
    }

    pub fn all(&self) -> Value {
        Value::Object(self.data.lock().clone())
    }

    pub fn get(&self, namespace: &str, key: &str) -> Option<Value> {
        self.data.lock().get(namespace)?.get(key).cloned()
    }

    pub fn set(&self, namespace: &str, key: &str, value: Value) -> std::io::Result<()> {
        let snapshot = {
            let mut data = self.data.lock();
            let ns = data
                .entry(namespace.to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            if !ns.is_object() {
                *ns = Value::Object(Map::new());
            }
            ns.as_object_mut().unwrap().insert(key.to_string(), value);
            serde_json::to_string_pretty(&*data).unwrap_or_default()
        };
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, snapshot)?;
        std::fs::rename(&tmp, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_persist() {
        let path = std::env::temp_dir().join(format!("arui-settings-test-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let s = Settings::load(path.clone());
        s.set("core", "theme", Value::String("x".into())).unwrap();
        let s2 = Settings::load(path.clone());
        assert_eq!(s2.get("core", "theme"), Some(Value::String("x".into())));
        let _ = std::fs::remove_file(&path);
    }
}
