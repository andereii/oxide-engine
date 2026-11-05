//! Per-layer name indexes and mod roots.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::{Source, zip};

/// A filesystem-grounded invalidation path.
///
/// Cache keys are canonical, while callers and platform watchers may report
/// symlinked or differently-prefixed paths. `new` grounds the event path before
/// any cache key is derived from it.
pub(crate) enum GroundedPath {
    /// Existing canonical file: its parent listing changed.
    File(PathBuf),
    /// Existing canonical directory: its own listing and descendants changed.
    Dir(PathBuf),
    /// Missing path whose canonical parent exists: something under it changed.
    Parent(PathBuf),
    /// Missing path whose parent is missing too: the nearest existing ancestor.
    Ancestor(PathBuf),
}

impl GroundedPath {
    pub(crate) fn new(path: &Path) -> Self {
        if let Ok(canonical) = std::fs::canonicalize(path) {
            return match std::fs::metadata(&canonical) {
                Ok(metadata) if metadata.is_dir() => Self::Dir(canonical),
                _ => Self::File(canonical),
            };
        }

        if let Some(parent) = path.parent()
            && let Ok(canonical) = std::fs::canonicalize(parent)
        {
            return Self::Parent(canonical);
        }

        let mut ancestor = path.parent();
        while let Some(candidate) = ancestor {
            if let Ok(canonical) = std::fs::canonicalize(candidate) {
                return Self::Ancestor(canonical);
            }
            ancestor = candidate.parent();
        }
        Self::Ancestor(path.to_path_buf())
    }
}

/// A directory name index built from `read_dir`, used to enforce case-exact
/// lookup independently of the host filesystem.
#[derive(Default)]
pub(crate) struct DirIndex {
    listings: RwLock<HashMap<PathBuf, Arc<Vec<String>>>>,
}

impl DirIndex {
    /// Resolves `rel` under `root`, matching every component exactly.
    pub(crate) fn resolve(&self, root: &Path, rel: &str) -> Option<PathBuf> {
        if rel.is_empty() {
            return None;
        }

        let components: Vec<&str> = rel.split('/').collect();
        let mut current = root.to_path_buf();
        for (index, component) in components.iter().enumerate() {
            if component.is_empty() || *component == "." || *component == ".." {
                return None;
            }

            let names = self.listing(&current)?;
            if !names.iter().any(|name| name == component) {
                return None;
            }
            current.push(component);

            if index + 1 == components.len() {
                return current.is_file().then_some(current);
            }
        }
        None
    }

    /// Purges the cached state affected by a grounded invalidation path.
    ///
    /// File events drop the parent listing; directory events drop the parent,
    /// the directory itself, and its cached descendants. Missing paths drop
    /// every cached listing at or below their grounded parent, since the
    /// missing child's casing is unknown. Listings are only ever rebuilt from
    /// `read_dir`; names from the event path are never inserted into the index.
    pub(crate) fn invalidate(&self, grounded: &GroundedPath) {
        let Ok(mut listings) = self.listings.write() else {
            return;
        };

        match grounded {
            GroundedPath::File(file) => {
                if let Some(parent) = file.parent() {
                    listings.remove(parent);
                }
            }
            GroundedPath::Dir(dir) => {
                listings.retain(|cached, _| !cached.starts_with(dir));
                if let Some(parent) = dir.parent() {
                    listings.remove(parent);
                } else {
                    listings.remove(dir);
                }
            }
            GroundedPath::Parent(parent) => {
                listings.retain(|cached, _| !cached.starts_with(parent));
            }
            GroundedPath::Ancestor(ancestor) => {
                listings.retain(|cached, _| !cached.starts_with(ancestor));
            }
        }
    }

    fn listing(&self, dir: &Path) -> Option<Arc<Vec<String>>> {
        if let Ok(listings) = self.listings.read()
            && let Some(names) = listings.get(dir)
        {
            return Some(Arc::clone(names));
        }

        // Read `read_dir` while holding the write lock so an invalidation can
        // never race with the insertion of a listing that is already stale.
        let mut listings = self.listings.write().ok()?;
        if let Some(names) = listings.get(dir) {
            return Some(Arc::clone(names));
        }
        let names = Arc::new(read_names(dir)?);
        listings.insert(dir.to_path_buf(), Arc::clone(&names));
        Some(names)
    }
}

fn read_names(dir: &Path) -> Option<Vec<String>> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut names = Vec::new();
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            names.push(name.to_owned());
        }
    }
    names.sort();
    Some(names)
}

/// One mod layer: either a grounded directory or a zip archive.
pub(crate) enum Layer {
    Dir {
        path: PathBuf,
        index: DirIndex,
    },
    Zip {
        path: PathBuf,
        entries: RwLock<HashSet<String>>,
    },
}

impl Layer {
    pub(crate) fn new_dir(path: PathBuf) -> Self {
        Self::Dir {
            path,
            index: DirIndex::default(),
        }
    }

    pub(crate) fn new_zip(path: PathBuf, entries: HashSet<String>) -> Self {
        Self::Zip {
            path,
            entries: RwLock::new(entries),
        }
    }

    pub(crate) fn resolve(&self, rel: &str) -> Option<Source> {
        match self {
            Self::Dir { path, index } => index.resolve(path, rel).map(Source::Dir),
            Self::Zip { path, entries } => {
                let entries = entries.read().ok()?;
                entries.contains(rel).then(|| Source::Zip {
                    path: path.clone(),
                    entry: rel.to_owned(),
                })
            }
        }
    }

    pub(crate) fn invalidate(&self, raw_path: &Path, grounded: &GroundedPath) {
        match self {
            Self::Dir { index, .. } => index.invalidate(grounded),
            Self::Zip {
                path: zip_path,
                entries,
            } => {
                let candidate =
                    std::fs::canonicalize(raw_path).unwrap_or_else(|_| raw_path.to_path_buf());
                if candidate == *zip_path
                    && let Ok(reloaded) = zip::load_entries(zip_path)
                    && let Ok(mut entries) = entries.write()
                {
                    *entries = reloaded;
                }
            }
        }
    }

    /// The path to watch and whether the watch is recursive.
    pub(crate) fn watch_target(&self) -> (&Path, bool) {
        match self {
            Self::Dir { path, .. } => (path, true),
            Self::Zip { path, .. } => (path, false),
        }
    }
}
