use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::utils::constants::{
    SCRIPT_DIR_CHARACTERS, SCRIPT_DIR_EVENTS, SCRIPT_DIR_NOTEKINDS, SCRIPT_DIR_SONGS,
    SCRIPT_DIR_STAGES, SCRIPT_DIRS_GLOBAL, SCRIPT_EXT_DISABLED, SCRIPT_EXT_LUA, SCRIPT_EXT_RHAI,
    SCRIPT_EXT_WASM,
};

use super::manifest::sanitize_mod_id;
use super::vfs::LoadedMod;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ScriptScope {
    Global,
    Stage(String),
    Character(String),
    NoteKind(String),
    Event(String),
    Song(String),
}

impl ScriptScope {
    pub fn order_rank(&self) -> u8 {
        match self {
            Self::Global => 0,
            Self::Stage(_) => 1,
            Self::Character(_) => 2,
            Self::NoteKind(_) => 3,
            Self::Event(_) => 4,
            Self::Song(_) => 5,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiscoveredScript {
    pub id: String,
    pub mod_id: String,
    pub name: String,
    pub extension: String,
    pub rel_path: String,
    pub path: PathBuf,
    pub scope: ScriptScope,
    pub priority: i32,
}

pub fn default_script_extensions() -> &'static [&'static str] {
    &[SCRIPT_EXT_RHAI, SCRIPT_EXT_LUA, SCRIPT_EXT_WASM]
}

fn is_disabled_script_name(file_name: &str) -> bool {
    file_name.starts_with('.')
        || file_name
            .rsplit('.')
            .next()
            .is_some_and(|ext| ext.eq_ignore_ascii_case(SCRIPT_EXT_DISABLED))
}

fn read_sorted_dir(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    paths
}

fn collect_recursive(
    base_dir: &Path,
    mod_root: &Path,
    mod_id: &str,
    priority: i32,
    allowed_exts: &[&str],
    scope_for_file: &dyn Fn(&Path) -> ScriptScope,
    out: &mut Vec<DiscoveredScript>,
    seen_paths: &mut HashSet<PathBuf>,
) {
    if !base_dir.is_dir() {
        return;
    }
    for entry_path in read_sorted_dir(base_dir) {
        let Some(name_os) = entry_path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if is_disabled_script_name(name_os) {
            continue;
        }
        if entry_path.is_dir() {
            collect_recursive(
                &entry_path,
                mod_root,
                mod_id,
                priority,
                allowed_exts,
                scope_for_file,
                out,
                seen_paths,
            );
            continue;
        }
        if !entry_path.is_file() {
            continue;
        }
        let Some(ext) = entry_path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let ext_lower = ext.to_ascii_lowercase();
        if !allowed_exts.iter().any(|e| e.eq_ignore_ascii_case(&ext_lower)) {
            continue;
        }
        if !seen_paths.insert(entry_path.clone()) {
            continue;
        }
        let stem = entry_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("script")
            .to_string();
        let rel = entry_path
            .strip_prefix(mod_root)
            .unwrap_or(&entry_path)
            .to_string_lossy()
            .replace('\\', "/");
        let scope = scope_for_file(&entry_path);
        out.push(DiscoveredScript {
            id: format!("{mod_id}:{rel}"),
            mod_id: mod_id.to_string(),
            name: stem,
            extension: ext_lower,
            rel_path: rel,
            path: entry_path,
            scope,
            priority,
        });
    }
}

fn scope_target_name(root_dir: &Path, file_path: &Path) -> String {
    let rel = file_path.strip_prefix(root_dir).unwrap_or(file_path);
    let mut comps = rel.components();
    match (comps.next(), comps.next()) {
        (Some(first), Some(_)) => first.as_os_str().to_string_lossy().into_owned(),
        (Some(_), None) => file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    }
}

pub fn discover_mod_scripts(
    mod_root: &Path,
    mod_id: &str,
    priority: i32,
    allowed_exts: &[&str],
) -> Vec<DiscoveredScript> {
    let mut out = Vec::new();
    let mut seen_paths = HashSet::new();

    let scoped_dirs: &[(&str, fn(String) -> ScriptScope)] = &[
        (SCRIPT_DIR_SONGS, ScriptScope::Song),
        (SCRIPT_DIR_STAGES, ScriptScope::Stage),
        (SCRIPT_DIR_CHARACTERS, ScriptScope::Character),
        (SCRIPT_DIR_NOTEKINDS, ScriptScope::NoteKind),
        (SCRIPT_DIR_EVENTS, ScriptScope::Event),
    ];

    // Collect scoped directories before generic `scripts/` so song/stage/character
    // scripts are never misclassified as global scripts.
    for (rel_dir, scope_ctor) in scoped_dirs {
        let dir = mod_root.join(rel_dir);
        collect_recursive(
            &dir,
            mod_root,
            mod_id,
            priority,
            allowed_exts,
            &|path| scope_ctor(scope_target_name(&dir, path)),
            &mut out,
            &mut seen_paths,
        );
    }

    let songs_root = mod_root.join("songs");
    if songs_root.is_dir() {
        for song_dir in read_sorted_dir(&songs_root) {
            if !song_dir.is_dir() {
                continue;
            }
            let Some(song_id) = song_dir.file_name().and_then(|s| s.to_str()).map(str::to_string) else {
                continue;
            };
            collect_recursive(
                &song_dir,
                mod_root,
                mod_id,
                priority,
                allowed_exts,
                &|_| ScriptScope::Song(song_id.clone()),
                &mut out,
                &mut seen_paths,
            );
        }
    }

    let colocated_gameplay: &[(&str, fn(String) -> ScriptScope)] = &[
        ("gameplay/stages", ScriptScope::Stage),
        ("gameplay/characters", ScriptScope::Character),
        ("gameplay/notekinds", ScriptScope::NoteKind),
        ("gameplay/events", ScriptScope::Event),
    ];
    for (rel_dir, scope_ctor) in colocated_gameplay {
        let dir = mod_root.join(rel_dir);
        collect_recursive(
            &dir,
            mod_root,
            mod_id,
            priority,
            allowed_exts,
            &|path| scope_ctor(scope_target_name(&dir, path)),
            &mut out,
            &mut seen_paths,
        );
    }

    for global_dir in SCRIPT_DIRS_GLOBAL {
        let dir = mod_root.join(global_dir);
        collect_recursive(
            &dir,
            mod_root,
            mod_id,
            priority,
            allowed_exts,
            &|_| ScriptScope::Global,
            &mut out,
            &mut seen_paths,
        );
    }

    out.sort_by(|a, b| {
        a.scope
            .order_rank()
            .cmp(&b.scope.order_rank())
            .then_with(|| a.rel_path.cmp(&b.rel_path))
    });
    out
}

pub fn sort_mods_by_priority_and_deps(mods: Vec<LoadedMod>, warnings: &mut Vec<String>) -> Vec<LoadedMod> {
    if mods.len() <= 1 {
        return mods;
    }

    let mut ordered = mods;
    ordered.sort_by(|a, b| {
        a.manifest
            .priority
            .cmp(&b.manifest.priority)
            .then_with(|| a.manifest.id.cmp(&b.manifest.id))
    });

    let id_to_idx: HashMap<String, usize> = ordered
        .iter()
        .enumerate()
        .map(|(idx, m)| (m.manifest.id.clone(), idx))
        .collect();

    let n = ordered.len();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut indegree = vec![0usize; n];

    for (idx, m) in ordered.iter().enumerate() {
        for dep in &m.manifest.dependencies {
            let dep_id = sanitize_mod_id(dep.mod_id());
            if let Some(&dep_idx) = id_to_idx.get(&dep_id) {
                if dep_idx != idx && !adj[dep_idx].contains(&idx) {
                    adj[dep_idx].push(idx);
                    indegree[idx] += 1;
                }
            } else if !dep.is_optional() {
                warnings.push(format!(
                    "mod '{}' lists missing dependency '{}'",
                    m.manifest.id, dep_id
                ));
            }
        }
    }

    let mut ready: Vec<usize> = (0..n).filter(|&i| indegree[i] == 0).collect();
    let mut result_indices = Vec::with_capacity(n);
    while let Some(pos) = ready
        .iter()
        .enumerate()
        .min_by_key(|(_, idx)| (ordered[**idx].manifest.priority, &ordered[**idx].manifest.id))
        .map(|(p, _)| p)
    {
        let u = ready.remove(pos);
        result_indices.push(u);
        for &v in &adj[u] {
            indegree[v] -= 1;
            if indegree[v] == 0 {
                ready.push(v);
            }
        }
    }

    if result_indices.len() < n {
        warnings.push("cyclic mod dependency detected; falling back to priority order".to_string());
        for i in 0..n {
            if !result_indices.contains(&i) {
                result_indices.push(i);
            }
        }
    }

    let mut slots: Vec<Option<LoadedMod>> = ordered.into_iter().map(Some).collect();
    result_indices
        .into_iter()
        .filter_map(|idx| slots[idx].take())
        .collect()
}
