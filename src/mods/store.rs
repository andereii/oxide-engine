use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use bevy::prelude::Resource;

use super::manifest::{ModError, sanitize_mod_id};

const STORE_DIR_NAME: &str = ".store";
const GLOBAL_NAMESPACE: &str = "global";

#[derive(Resource, Debug, Default, Clone)]
pub struct ModStore {
    root_dir: Option<PathBuf>,
    namespaces: HashMap<String, HashMap<String, serde_json::Value>>,
}

impl ModStore {
    pub fn with_root(mods_dir: impl AsRef<Path>) -> Self {
        Self {
            root_dir: Some(mods_dir.as_ref().join(STORE_DIR_NAME)),
            namespaces: HashMap::new(),
        }
    }

    fn normalize_namespace(ns: &str) -> String {
        let trimmed = ns.trim();
        if trimmed.is_empty() {
            GLOBAL_NAMESPACE.to_string()
        } else {
            sanitize_mod_id(trimmed)
        }
    }

    pub fn set(&mut self, namespace: &str, key: &str, value: serde_json::Value) {
        let ns = Self::normalize_namespace(namespace);
        self.namespaces
            .entry(ns)
            .or_default()
            .insert(key.to_string(), value);
    }

    pub fn get(&self, namespace: &str, key: &str) -> Option<&serde_json::Value> {
        let ns = Self::normalize_namespace(namespace);
        self.namespaces.get(&ns).and_then(|m| m.get(key))
    }

    pub fn has(&self, namespace: &str, key: &str) -> bool {
        self.get(namespace, key).is_some()
    }

    pub fn remove(&mut self, namespace: &str, key: &str) -> Option<serde_json::Value> {
        let ns = Self::normalize_namespace(namespace);
        self.namespaces.get_mut(&ns).and_then(|m| m.remove(key))
    }

    pub fn clear_namespace(&mut self, namespace: &str) {
        let ns = Self::normalize_namespace(namespace);
        self.namespaces.remove(&ns);
    }

    pub fn keys(&self, namespace: &str) -> Vec<String> {
        let ns = Self::normalize_namespace(namespace);
        let mut out: Vec<String> = self
            .namespaces
            .get(&ns)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();
        out.sort();
        out
    }

    pub fn save_namespace(&self, namespace: &str) -> Result<(), ModError> {
        let Some(root) = &self.root_dir else {
            return Ok(());
        };
        let ns = Self::normalize_namespace(namespace);
        fs::create_dir_all(root).map_err(|source| ModError::Io {
            path: root.clone(),
            source,
        })?;
        let path = root.join(format!("{ns}.json"));
        let empty = HashMap::new();
        let data = self.namespaces.get(&ns).unwrap_or(&empty);
        let serialized = serde_json::to_string_pretty(data).map_err(|source| ModError::InvalidJson {
            path: path.clone(),
            source,
        })?;
        fs::write(&path, serialized).map_err(|source| ModError::Io { path, source })
    }

    pub fn load_namespace(&mut self, namespace: &str) -> Result<bool, ModError> {
        let Some(root) = &self.root_dir else {
            return Ok(false);
        };
        let ns = Self::normalize_namespace(namespace);
        let path = root.join(format!("{ns}.json"));
        if !path.is_file() {
            return Ok(false);
        }
        let raw = fs::read_to_string(&path).map_err(|source| ModError::Io {
            path: path.clone(),
            source,
        })?;
        let parsed: HashMap<String, serde_json::Value> =
            serde_json::from_str(&raw).map_err(|source| ModError::InvalidJson { path, source })?;
        self.namespaces.insert(ns, parsed);
        Ok(true)
    }
}
