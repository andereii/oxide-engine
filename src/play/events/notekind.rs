use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::mods::{DiscoveredScript, ModRegistry, ScriptScope};
use crate::play::notes::data::{NoteKind, NoteKindManager};
use crate::play::scene::PlayState;
use crate::utils::constants::{SCRIPT_DIR_NOTEKINDS, SCRIPT_EXT_DISABLED};

const NOTEKIND_DIRS: &[&str] = &["gameplay/notekinds", SCRIPT_DIR_NOTEKINDS, "notekinds"];

/// Normalizes and registers a scripted [`NoteKind`] in `manager`.
/// Returns `false` if the `id` is empty after trimming.
pub fn register_scripted_notekind(manager: &mut NoteKindManager, mut kind: NoteKind) -> bool {
    let trimmed_id = kind.id.trim().to_string();
    if trimmed_id.is_empty() {
        return false;
    }
    kind.id = trimmed_id;
    kind.note_style_id = kind
        .note_style_id
        .take()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    manager.register(kind);
    true
}

/// Registers a scripted [`NoteKind`] on `state` and propagates its `scoreable` and
/// `low_priority` attributes onto matching chart and strumline notes.
pub fn apply_notekind_to_play_state(state: &mut PlayState, kind: NoteKind) -> bool {
    let lookup_id = kind.id.trim().to_string();
    if !register_scripted_notekind(&mut state.note_kinds, kind) {
        return false;
    }
    let Some(registered) = state.note_kinds.get(Some(&lookup_id)).cloned() else {
        return false;
    };

    let matches_kind =
        |opt: Option<&str>| opt.is_some_and(|k| k.trim().eq_ignore_ascii_case(&registered.id));

    for note in &mut state.chart.player_notes {
        if matches_kind(note.kind.as_deref()) {
            note.scoreable = registered.scoreable;
            note.low_priority = registered.low_priority;
        }
    }
    for note in &mut state.chart.opponent_notes {
        if matches_kind(note.kind.as_deref()) {
            note.low_priority = registered.low_priority;
        }
    }
    for note in &mut state.player_strumline.notes {
        if matches_kind(note.data.kind.as_deref()) {
            note.data.scoreable = registered.scoreable;
            note.data.low_priority = registered.low_priority;
        }
    }
    for note in &mut state.opponent_strumline.notes {
        if matches_kind(note.data.kind.as_deref()) {
            note.data.low_priority = registered.low_priority;
        }
    }

    state.tallies.total_notes = state
        .player_strumline
        .notes
        .iter()
        .filter(|n| n.data.scoreable)
        .count() as u32;
    true
}

struct NotekindDiscovery<'a> {
    discovered: &'a mut Vec<DiscoveredScript>,
    seen_canon: HashSet<PathBuf>,
    seen_rel_paths: HashSet<String>,
}

/// Discovers softcoded note-kind scripts across `gameplay/notekinds/`, `scripts/notekinds/`,
/// and `notekinds/` for any active chart note kinds, deduplicating by canonical path.
pub fn append_notekind_sidecar_scripts(
    mods: &ModRegistry,
    note_kinds: &[&str],
    supported_exts: &[String],
    discovered: &mut Vec<DiscoveredScript>,
) {
    let active_kinds: Vec<&str> = note_kinds
        .iter()
        .map(|k| k.trim())
        .filter(|k| !k.is_empty())
        .collect();
    if active_kinds.is_empty() || supported_exts.is_empty() {
        return;
    }

    let seen_canon: HashSet<PathBuf> = discovered
        .iter()
        .map(|d| fs::canonicalize(&d.path).unwrap_or_else(|_| d.path.clone()))
        .collect();
    let seen_rel_paths: HashSet<String> = discovered
        .iter()
        .map(|d| d.rel_path.to_ascii_lowercase())
        .collect();
    let mut out = NotekindDiscovery {
        discovered,
        seen_canon,
        seen_rel_paths,
    };

    for &kind in &active_kinds {
        for ext in supported_exts {
            for rel_dir in NOTEKIND_DIRS {
                for rel in [
                    format!("{rel_dir}/{kind}.{ext}"),
                    format!("{rel_dir}/{kind}/{kind}.{ext}"),
                ] {
                    if let Some(candidate) = mods.resolve_file(&rel) {
                        push_notekind_script(mods, candidate, kind, ext, &mut out);
                    }
                }
            }
        }
    }

    let mut search_roots: Vec<&Path> = Vec::with_capacity(mods.mods.len() + 1);
    for loaded_mod in mods.mods.iter().rev() {
        if !loaded_mod.root.as_os_str().is_empty() {
            search_roots.push(&loaded_mod.root);
        }
    }
    if !mods.assets_dir.as_os_str().is_empty() {
        search_roots.push(&mods.assets_dir);
    }

    for root in search_roots {
        for rel_dir in NOTEKIND_DIRS {
            scan_notekind_dir(
                mods,
                &root.join(rel_dir),
                &active_kinds,
                supported_exts,
                &mut out,
            );
        }
    }
}

fn scan_notekind_dir(
    mods: &ModRegistry,
    dir: &Path,
    active_kinds: &[&str],
    supported_exts: &[String],
    out: &mut NotekindDiscovery<'_>,
) {
    if !dir.is_dir() {
        return;
    }
    for entry_path in read_sorted_entries(dir) {
        let Some(name_os) = entry_path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if is_ignored_script_entry(name_os) {
            continue;
        }
        if entry_path.is_dir() {
            if let Some(&matched_kind) = active_kinds
                .iter()
                .find(|k| k.eq_ignore_ascii_case(name_os))
            {
                collect_dir_scripts_recursive(
                    mods,
                    &entry_path,
                    matched_kind,
                    supported_exts,
                    out,
                );
            }
            continue;
        }
        let Some(ext) = supported_ext_of(&entry_path, supported_exts) else {
            continue;
        };
        let Some(stem) = entry_path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if let Some(&matched_kind) = active_kinds.iter().find(|k| k.eq_ignore_ascii_case(stem)) {
            push_notekind_script(mods, entry_path, matched_kind, &ext, out);
        }
    }
}

fn collect_dir_scripts_recursive(
    mods: &ModRegistry,
    dir: &Path,
    kind: &str,
    supported_exts: &[String],
    out: &mut NotekindDiscovery<'_>,
) {
    for entry_path in read_sorted_entries(dir) {
        let Some(name_os) = entry_path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if is_ignored_script_entry(name_os) {
            continue;
        }
        if entry_path.is_dir() {
            collect_dir_scripts_recursive(mods, &entry_path, kind, supported_exts, out);
        } else if let Some(ext) = supported_ext_of(&entry_path, supported_exts) {
            push_notekind_script(mods, entry_path, kind, &ext, out);
        }
    }
}

fn push_notekind_script(
    mods: &ModRegistry,
    candidate: PathBuf,
    kind: &str,
    ext: &str,
    out: &mut NotekindDiscovery<'_>,
) {
    if !candidate.is_file() {
        return;
    }
    let canon = fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
    if !out.seen_canon.insert(canon) {
        return;
    }
    let (mod_id, rel_path) = resolve_script_origin(mods, &candidate);
    if !out.seen_rel_paths.insert(rel_path.to_ascii_lowercase()) {
        return;
    }
    let stem = candidate
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(kind)
        .to_string();
    out.discovered.push(DiscoveredScript {
        id: format!("notekind:{kind}:{rel_path}"),
        mod_id,
        name: stem,
        extension: ext.to_ascii_lowercase(),
        rel_path,
        path: candidate,
        scope: ScriptScope::NoteKind(kind.to_string()),
        priority: 0,
    });
}

fn resolve_script_origin(mods: &ModRegistry, candidate: &Path) -> (String, String) {
    for m in &mods.mods {
        if let Ok(rel) = candidate.strip_prefix(&m.root) {
            return (
                m.manifest.id.clone(),
                rel.to_string_lossy().replace('\\', "/"),
            );
        }
    }
    let rel = candidate
        .strip_prefix(&mods.assets_dir)
        .unwrap_or(candidate)
        .to_string_lossy()
        .replace('\\', "/");
    ("assets".to_string(), rel)
}

fn supported_ext_of(path: &Path, supported_exts: &[String]) -> Option<String> {
    let ext = path.extension().and_then(|e| e.to_str())?;
    supported_exts
        .iter()
        .find(|s| s.eq_ignore_ascii_case(ext))
        .map(|s| s.to_ascii_lowercase())
}

fn is_ignored_script_entry(file_name: &str) -> bool {
    file_name.starts_with('.')
        || file_name
            .rsplit('.')
            .next()
            .is_some_and(|ext| ext.eq_ignore_ascii_case(SCRIPT_EXT_DISABLED))
}

fn read_sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    paths
}
