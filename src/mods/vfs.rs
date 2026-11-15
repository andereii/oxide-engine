use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use bevy::prelude::Resource;
use oxin_app::{resolve_assets_dir, resolve_mods_dir};

use crate::utils::constants::MOD_ASSET_SOURCE_ID;

use super::loader::{
    DiscoveredScript, ScriptScope, default_script_extensions, discover_mod_scripts,
    sort_mods_by_priority_and_deps,
};
use super::manifest::{ModError, ModManifest, load_manifest_for_dir};

#[derive(Debug, Clone)]
pub struct LoadedMod {
    pub manifest: ModManifest,
    pub root: PathBuf,
    pub manifest_path: Option<PathBuf>,
    pub scripts: Vec<DiscoveredScript>,
}

#[derive(Resource, Debug, Clone)]
pub struct ModRegistry {
    pub assets_dir: PathBuf,
    pub mods_dir: PathBuf,
    pub mods: Vec<LoadedMod>,
    pub disabled_mods: Vec<(String, String)>,
    pub load_warnings: Vec<String>,
    pub reload_count: u64,
}

impl Default for ModRegistry {
    fn default() -> Self {
        Self::from_default_dirs()
    }
}

impl ModRegistry {
    pub fn from_default_dirs() -> Self {
        Self::discover(resolve_assets_dir(), resolve_mods_dir())
    }

    pub fn discover(assets_dir: PathBuf, mods_dir: PathBuf) -> Self {
        let mut reg = Self {
            assets_dir,
            mods_dir,
            mods: Vec::new(),
            disabled_mods: Vec::new(),
            load_warnings: Vec::new(),
            reload_count: 0,
        };
        let _ = reg.scan_mods();
        reg
    }

    pub fn reload(&mut self) -> Result<(), ModError> {
        self.reload_count += 1;
        self.scan_mods()
    }

    fn scan_mods(&mut self) -> Result<(), ModError> {
        self.mods.clear();
        self.disabled_mods.clear();
        self.load_warnings.clear();

        if !self.mods_dir.is_dir() {
            return Ok(());
        }

        let entries = fs::read_dir(&self.mods_dir).map_err(|source| ModError::Io {
            path: self.mods_dir.clone(),
            source,
        })?;

        let mut dirs: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.is_dir()
                    && p.file_name()
                        .and_then(|s| s.to_str())
                        .is_some_and(|name| !name.starts_with('.'))
            })
            .collect();
        dirs.sort();

        let exts = default_script_extensions();
        let mut seen_ids = HashSet::new();
        let mut discovered = Vec::new();

        for mod_dir in dirs {
            let folder_name = mod_dir
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("unnamed")
                .to_string();

            let (manifest, manifest_path) = match load_manifest_for_dir(&mod_dir) {
                Ok(pair) => pair,
                Err(err) => {
                    self.load_warnings.push(err.to_string());
                    self.disabled_mods.push((folder_name, err.to_string()));
                    continue;
                }
            };

            if !manifest.enabled {
                self.disabled_mods
                    .push((manifest.id.clone(), "disabled by manifest or marker".to_string()));
                continue;
            }

            if !seen_ids.insert(manifest.id.clone()) {
                self.load_warnings.push(format!(
                    "duplicate mod id '{}' at {} ignored",
                    manifest.id,
                    mod_dir.display()
                ));
                self.disabled_mods
                    .push((manifest.id.clone(), "duplicate mod id".to_string()));
                continue;
            }

            let scripts = discover_mod_scripts(&mod_dir, &manifest.id, manifest.priority, exts);
            discovered.push(LoadedMod {
                manifest,
                root: mod_dir,
                manifest_path,
                scripts,
            });
        }

        self.mods = sort_mods_by_priority_and_deps(discovered, &mut self.load_warnings);
        Ok(())
    }

    pub fn normalize_rel_path(rel: &str) -> Result<PathBuf, ModError> {
        let replaced = rel.trim().replace('\\', "/");
        if replaced.is_empty() || replaced.starts_with('/') {
            return Err(ModError::InvalidPath(rel.to_string()));
        }
        let mut out = PathBuf::new();
        for comp in Path::new(&replaced).components() {
            match comp {
                Component::Normal(part) => out.push(part),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(ModError::InvalidPath(rel.to_string()));
                }
            }
        }
        if out.as_os_str().is_empty() {
            return Err(ModError::InvalidPath(rel.to_string()));
        }
        Ok(out)
    }

    /// Searches mods from highest precedence (end of list) to lowest, falling back to `assets_dir`.
    pub fn resolve_file(&self, rel: &str) -> Option<PathBuf> {
        let normalized = Self::normalize_rel_path(rel).ok()?;
        for loaded_mod in self.mods.iter().rev() {
            let candidate = loaded_mod.root.join(&normalized);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        let base_candidate = self.assets_dir.join(&normalized);
        if base_candidate.is_file() {
            return Some(base_candidate);
        }
        None
    }

    pub fn resolve_dir(&self, rel: &str) -> Option<PathBuf> {
        let normalized = Self::normalize_rel_path(rel).ok()?;
        for loaded_mod in self.mods.iter().rev() {
            let candidate = loaded_mod.root.join(&normalized);
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
        let base_candidate = self.assets_dir.join(&normalized);
        if base_candidate.is_dir() {
            return Some(base_candidate);
        }
        None
    }

    pub fn resolve_all_files(&self, rel: &str) -> Vec<PathBuf> {
        let Ok(normalized) = Self::normalize_rel_path(rel) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let base_candidate = self.assets_dir.join(&normalized);
        if base_candidate.is_file() {
            out.push(base_candidate);
        }
        for loaded_mod in &self.mods {
            let candidate = loaded_mod.root.join(&normalized);
            if candidate.is_file() {
                out.push(candidate);
            }
        }
        out
    }

    pub fn resolve_with_extensions(&self, rel_without_or_with_ext: &str, exts: &[&str]) -> Option<PathBuf> {
        if let Some(exact) = self.resolve_file(rel_without_or_with_ext) {
            return Some(exact);
        }
        let trimmed = rel_without_or_with_ext.trim();
        for ext in exts {
            let candidate = if ext.starts_with('.') {
                format!("{trimmed}{ext}")
            } else {
                format!("{trimmed}.{ext}")
            };
            if let Some(found) = self.resolve_file(&candidate) {
                return Some(found);
            }
        }
        None
    }

    pub fn read_to_string(&self, rel: &str) -> Result<String, ModError> {
        let path = self
            .resolve_file(rel)
            .ok_or_else(|| ModError::NotFound(rel.to_string()))?;
        fs::read_to_string(&path).map_err(|source| ModError::Io { path, source })
    }

    pub fn file_exists(&self, rel: &str) -> bool {
        self.resolve_file(rel).is_some()
    }

    /// Converts an on-disk path under either `assets_dir` or `mods_dir` into a Bevy `AssetServer` path.
    pub fn to_asset_server_path(&self, disk_path: &Path) -> String {
        if let Ok(rel) = disk_path.strip_prefix(&self.mods_dir) {
            return format!(
                "{MOD_ASSET_SOURCE_ID}://{}",
                rel.to_string_lossy().replace('\\', "/")
            );
        }
        if let Ok(rel) = disk_path.strip_prefix(&self.assets_dir) {
            return rel.to_string_lossy().replace('\\', "/");
        }
        if let (Ok(canon_disk), Ok(canon_mods)) = (disk_path.canonicalize(), self.mods_dir.canonicalize())
            && let Ok(rel) = canon_disk.strip_prefix(&canon_mods)
        {
            return format!(
                "{MOD_ASSET_SOURCE_ID}://{}",
                rel.to_string_lossy().replace('\\', "/")
            );
        }
        if let (Ok(canon_disk), Ok(canon_assets)) = (disk_path.canonicalize(), self.assets_dir.canonicalize())
            && let Ok(rel) = canon_disk.strip_prefix(&canon_assets)
        {
            return rel.to_string_lossy().replace('\\', "/");
        }
        disk_path.to_string_lossy().replace('\\', "/")
    }

    pub fn resolve_asset_server_path(&self, rel: &str) -> Option<String> {
        self.resolve_file(rel).map(|p| self.to_asset_server_path(&p))
    }

    pub fn list_dir_entries(&self, rel: &str) -> Vec<String> {
        let Ok(normalized) = Self::normalize_rel_path(rel) else {
            return Vec::new();
        };
        let mut names = BTreeSet::new();
        let collect = |dir: PathBuf, out: &mut BTreeSet<String>| {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.filter_map(Result::ok) {
                    if let Some(name) = entry.file_name().to_str()
                        && !name.starts_with('.')
                    {
                        out.insert(name.to_string());
                    }
                }
            }
        };
        collect(self.assets_dir.join(&normalized), &mut names);
        for loaded_mod in &self.mods {
            collect(loaded_mod.root.join(&normalized), &mut names);
        }
        names.into_iter().collect()
    }

    pub fn list_songs(&self) -> Vec<String> {
        let mut songs = BTreeSet::new();
        let collect_songs = |dir: PathBuf, out: &mut BTreeSet<String>| {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.filter_map(Result::ok) {
                    let p = entry.path();
                    if p.is_dir()
                        && let Some(name) = p.file_name().and_then(|s| s.to_str())
                        && !name.starts_with('.')
                    {
                        out.insert(name.to_string());
                    }
                }
            }
        };
        collect_songs(self.assets_dir.join("songs"), &mut songs);
        for loaded_mod in &self.mods {
            collect_songs(loaded_mod.root.join("songs"), &mut songs);
        }
        songs.into_iter().collect()
    }

    pub fn scripts_for_context(
        &self,
        song_id: &str,
        stage_id: Option<&str>,
        character_ids: &[&str],
        note_kinds: &[&str],
        event_names: &[&str],
    ) -> Vec<DiscoveredScript> {
        let mut selected = Vec::new();
        for loaded_mod in &self.mods {
            for script in &loaded_mod.scripts {
                let matches = match &script.scope {
                    ScriptScope::Global => true,
                    ScriptScope::Song(target) => target.eq_ignore_ascii_case(song_id),
                    ScriptScope::Stage(target) => {
                        stage_id.is_some_and(|s| s.eq_ignore_ascii_case(target))
                    }
                    ScriptScope::Character(target) => {
                        character_ids.iter().any(|c| c.eq_ignore_ascii_case(target))
                    }
                    ScriptScope::NoteKind(target) => {
                        note_kinds.is_empty()
                            || note_kinds.iter().any(|k| k.eq_ignore_ascii_case(target))
                    }
                    ScriptScope::Event(target) => {
                        event_names.is_empty()
                            || event_names.iter().any(|e| e.eq_ignore_ascii_case(target))
                    }
                };
                if matches {
                    selected.push(script.clone());
                }
            }
        }
        selected.sort_by(|a, b| {
            a.scope
                .order_rank()
                .cmp(&b.scope.order_rank())
                .then_with(|| a.priority.cmp(&b.priority))
                .then_with(|| a.mod_id.cmp(&b.mod_id))
                .then_with(|| a.rel_path.cmp(&b.rel_path))
        });
        selected
    }

    pub fn get_mod_config(&self, mod_id: &str, key: &str) -> Option<&serde_json::Value> {
        self.mods
            .iter()
            .find(|m| m.manifest.id == mod_id)
            .and_then(|m| m.manifest.config.get(key))
    }

    pub fn active_mod_ids(&self) -> Vec<&str> {
        self.mods.iter().map(|m| m.manifest.id.as_str()).collect()
    }
}
