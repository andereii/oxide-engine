use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub(super) const MAX_SPRITEMAP_WATCH: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FileStamp {
    pub(super) exists: bool,
    pub(super) len: u64,
    pub(super) modified: Option<SystemTime>,
}

impl FileStamp {
    pub(super) fn from_path(path: &Path) -> Self {
        match fs::metadata(path) {
            Ok(meta) if meta.is_file() => Self {
                exists: true,
                len: meta.len(),
                modified: meta.modified().ok(),
            },
            _ => Self {
                exists: false,
                len: 0,
                modified: None,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DirStamp {
    pub(super) exists: bool,
    pub(super) entries: Vec<String>,
}

impl DirStamp {
    pub(super) fn from_path(path: &Path) -> Self {
        let Ok(rd) = fs::read_dir(path) else {
            return Self {
                exists: false,
                entries: Vec::new(),
            };
        };
        let mut entries: Vec<String> = rd
            .filter_map(Result::ok)
            .filter_map(|e| e.file_name().to_str().map(str::to_string))
            .filter(|name| !name.starts_with('.'))
            .collect();
        entries.sort();
        Self {
            exists: true,
            entries,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct DomainWatch {
    pub(super) json_files: BTreeMap<PathBuf, FileStamp>,
    pub(super) meta_files: BTreeMap<PathBuf, FileStamp>,
    pub(super) asset_files: BTreeMap<PathBuf, (FileStamp, Option<String>)>,
    pub(super) dirs: BTreeMap<PathBuf, DirStamp>,
}

#[derive(Debug, Clone)]
pub(super) enum DomainDiff {
    Unchanged,
    InPlaceAssets(Vec<String>),
    Full(Vec<String>),
}

impl DomainDiff {
    pub(super) fn is_changed(&self) -> bool {
        !matches!(self, Self::Unchanged)
    }
}

#[derive(Default)]
pub(super) struct ScanCache {
    files: HashMap<PathBuf, FileStamp>,
    dirs: HashMap<PathBuf, DirStamp>,
}

impl ScanCache {
    pub(super) fn dir_stamp(&mut self, dir: &Path) -> &DirStamp {
        if !self.dirs.contains_key(dir) {
            self.dirs.insert(dir.to_path_buf(), DirStamp::from_path(dir));
        }
        &self.dirs[dir]
    }

    pub(super) fn file_stamp(
        &mut self,
        path: &Path,
        old: &FileStamp,
        baseline_dirs: &BTreeMap<PathBuf, DirStamp>,
    ) -> FileStamp {
        if let Some(cached) = self.files.get(path) {
            return cached.clone();
        }
        // Skip syscalls for missing fallback candidates when their parent directory did not change.
        if !old.exists
            && let Some(parent) = path.parent()
            && let Some(old_dir) = baseline_dirs.get(parent)
            && !path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with('.'))
        {
            let cur_dir = self.dir_stamp(parent);
            if cur_dir == old_dir {
                self.files.insert(path.to_path_buf(), old.clone());
                return old.clone();
            }
        }
        let stamp = FileStamp::from_path(path);
        self.files.insert(path.to_path_buf(), stamp.clone());
        stamp
    }
}

impl DomainWatch {
    pub(super) fn track_json(&mut self, path: PathBuf) {
        self.json_files
            .entry(path.clone())
            .or_insert_with(|| FileStamp::from_path(&path));
    }

    pub(super) fn track_meta(&mut self, path: PathBuf) {
        self.meta_files
            .entry(path.clone())
            .or_insert_with(|| FileStamp::from_path(&path));
    }

    pub(super) fn track_asset(&mut self, path: PathBuf, server_path: Option<String>) {
        self.asset_files
            .entry(path.clone())
            .or_insert_with(|| (FileStamp::from_path(&path), server_path));
    }

    pub(super) fn track_dir(&mut self, dir: PathBuf) {
        self.dirs
            .entry(dir.clone())
            .or_insert_with(|| DirStamp::from_path(&dir));
    }

    pub(super) fn diff_with_cache(
        &self,
        cache: &mut ScanCache,
        baseline_dirs: &BTreeMap<PathBuf, DirStamp>,
    ) -> DomainDiff {
        let mut structural = false;
        for (path, old) in &self.json_files {
            if &cache.file_stamp(path, old, baseline_dirs) != old {
                structural = true;
                break;
            }
        }
        if !structural {
            for (path, old) in &self.meta_files {
                if &cache.file_stamp(path, old, baseline_dirs) != old {
                    structural = true;
                    break;
                }
            }
        }
        if !structural {
            for (dir, old) in &self.dirs {
                if cache.dir_stamp(dir) != old {
                    structural = true;
                    break;
                }
            }
        }

        let mut modified_server_paths = Vec::new();
        for (path, (old, server_opt)) in &self.asset_files {
            let cur = cache.file_stamp(path, old, baseline_dirs);
            if &cur != old {
                if !cur.exists || !old.exists {
                    structural = true;
                }
                if cur.exists
                    && let Some(sp) = server_opt
                {
                    modified_server_paths.push(sp.clone());
                }
            }
        }

        if structural {
            DomainDiff::Full(modified_server_paths)
        } else if !modified_server_paths.is_empty() {
            DomainDiff::InPlaceAssets(modified_server_paths)
        } else {
            DomainDiff::Unchanged
        }
    }

    /// Collects Bevy asset server paths that are new or changed relative to `old`,
    /// ensuring newly re-targeted paths in edited JSON files are reloaded from disk.
    pub(super) fn collect_changed_or_new_server_paths(&self, old: &DomainWatch) -> Vec<String> {
        let mut out = Vec::new();
        for (path, (new_stamp, server_opt)) in &self.asset_files {
            if !new_stamp.exists {
                continue;
            }
            let Some(sp) = server_opt else {
                continue;
            };
            let needs_reload = match old.asset_files.get(path) {
                Some((old_stamp, _)) => !old_stamp.exists || old_stamp != new_stamp,
                None => true,
            };
            if needs_reload {
                out.push(sp.clone());
            }
        }
        out
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct WatchPlan {
    pub(super) mods_dir: Option<(PathBuf, DirStamp)>,
    pub(super) mod_manifests: BTreeMap<PathBuf, FileStamp>,
    pub(super) chart: DomainWatch,
    pub(super) stage: DomainWatch,
    pub(super) chars: BTreeMap<String, DomainWatch>,
    pub(super) skin: DomainWatch,
    pub(super) scripts: DomainWatch,
    pub(super) audio: DomainWatch,
    /// Baseline snapshots for parent directories of tracked files so missing candidates avoid redundant stat calls.
    pub(super) parent_dirs: BTreeMap<PathBuf, DirStamp>,
}

impl WatchPlan {
    pub(super) fn populate_parent_dirs(&mut self) {
        self.parent_dirs.clear();
        let all_domains = [
            &self.chart,
            &self.stage,
            &self.skin,
            &self.scripts,
            &self.audio,
        ]
        .into_iter()
        .chain(self.chars.values());

        for domain in all_domains {
            for (dir, stamp) in &domain.dirs {
                self.parent_dirs
                    .entry(dir.clone())
                    .or_insert_with(|| stamp.clone());
            }
            let file_paths = domain
                .json_files
                .iter()
                .filter(|(_, s)| !s.exists)
                .map(|(p, _)| p)
                .chain(domain.meta_files.iter().filter(|(_, s)| !s.exists).map(|(p, _)| p))
                .chain(
                    domain
                        .asset_files
                        .iter()
                        .filter(|(_, (s, _))| !s.exists)
                        .map(|(p, _)| p),
                );
            for path in file_paths {
                if let Some(parent) = path.parent()
                    && !self.parent_dirs.contains_key(parent)
                {
                    self.parent_dirs
                        .insert(parent.to_path_buf(), DirStamp::from_path(parent));
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct HotReloadBatchEvent {
    pub(super) generation: u64,
    pub(super) mods_changed: bool,
    pub(super) chart: DomainDiff,
    pub(super) stage: DomainDiff,
    pub(super) chars: BTreeMap<String, DomainDiff>,
    pub(super) skin: DomainDiff,
    pub(super) scripts_changed: bool,
    pub(super) audio: DomainDiff,
}

impl HotReloadBatchEvent {
    pub(super) fn any_change(&self) -> bool {
        self.mods_changed
            || self.chart.is_changed()
            || self.stage.is_changed()
            || self.chars.values().any(DomainDiff::is_changed)
            || self.skin.is_changed()
            || self.scripts_changed
            || self.audio.is_changed()
    }
}
