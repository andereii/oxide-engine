use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::mods::{DiscoveredScript, ModRegistry, ScriptScope};
use crate::utils::constants::DEFAULT_VARIATION;

use super::super::types::SongChart;

/// If a song's metadata enables `"scripts": true` (or `"script": true`), discovers sidecar
/// script files for that song (and variation or explicit `script_path`) across mods and base
/// assets, preserving mod overrides and deduplicating by canonical path.
pub fn append_song_sidecar_scripts(
    mods: &ModRegistry,
    chart: &SongChart,
    supported_exts: &[String],
    discovered: &mut Vec<DiscoveredScript>,
) {
    if !chart.scripts || chart.song_id.trim().is_empty() || supported_exts.is_empty() {
        return;
    }

    let song_id = chart.song_id.trim();
    let mut seen_canon: HashSet<PathBuf> = discovered
        .iter()
        .map(|d| fs::canonicalize(&d.path).unwrap_or_else(|_| d.path.clone()))
        .collect();

    if let Some(explicit) = chart
        .script_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        && resolve_explicit_song_script(
            mods,
            song_id,
            explicit,
            supported_exts,
            discovered,
            &mut seen_canon,
        )
    {
        return;
    }

    resolve_song_stem_sidecar(
        mods,
        song_id,
        song_id,
        supported_exts,
        discovered,
        &mut seen_canon,
    );

    let variation = chart.variation.trim();
    if !variation.is_empty() && !variation.eq_ignore_ascii_case(DEFAULT_VARIATION) {
        let var_stem = format!("{song_id}-{variation}");
        resolve_song_stem_sidecar(
            mods,
            song_id,
            &var_stem,
            supported_exts,
            discovered,
            &mut seen_canon,
        );
    }
}

fn resolve_explicit_song_script(
    mods: &ModRegistry,
    song_id: &str,
    explicit: &str,
    supported_exts: &[String],
    discovered: &mut Vec<DiscoveredScript>,
    seen_canon: &mut HashSet<PathBuf>,
) -> bool {
    let explicit_ext = Path::new(explicit).extension().and_then(|e| e.to_str());
    if let Some(ext) = explicit_ext {
        let ext_lower = ext.to_ascii_lowercase();
        if !supported_exts
            .iter()
            .any(|s| s.eq_ignore_ascii_case(&ext_lower))
        {
            return false;
        }
        for rel in [
            explicit.to_string(),
            format!("gameplay/songs/{song_id}/{explicit}"),
            format!("songs/{song_id}/{explicit}"),
        ] {
            if let Some(candidate) = mods.resolve_file(&rel) {
                return push_song_sidecar_script(
                    mods, song_id, candidate, &ext_lower, discovered, seen_canon,
                );
            }
        }
        return false;
    }

    for ext in supported_exts {
        for rel in [
            format!("{explicit}.{ext}"),
            format!("gameplay/songs/{song_id}/{explicit}.{ext}"),
            format!("songs/{song_id}/{explicit}.{ext}"),
        ] {
            if let Some(candidate) = mods.resolve_file(&rel) {
                return push_song_sidecar_script(
                    mods, song_id, candidate, ext, discovered, seen_canon,
                );
            }
        }
    }
    false
}

fn resolve_song_stem_sidecar(
    mods: &ModRegistry,
    song_id: &str,
    stem: &str,
    supported_exts: &[String],
    discovered: &mut Vec<DiscoveredScript>,
    seen_canon: &mut HashSet<PathBuf>,
) -> bool {
    let search_roots = mods
        .mods
        .iter()
        .rev()
        .map(|m| m.root.as_path())
        .chain((!mods.assets_dir.as_os_str().is_empty()).then_some(mods.assets_dir.as_path()));

    for root in search_roots {
        if root.as_os_str().is_empty() {
            continue;
        }
        for ext in supported_exts {
            for rel in [
                format!("gameplay/songs/{song_id}/{stem}.{ext}"),
                format!("songs/{song_id}/{stem}.{ext}"),
            ] {
                if let Ok(norm) = ModRegistry::normalize_rel_path(&rel)
                    && push_song_sidecar_script(
                        mods,
                        song_id,
                        root.join(norm),
                        ext,
                        discovered,
                        seen_canon,
                    )
                {
                    return true;
                }
            }
        }
    }

    false
}

fn push_song_sidecar_script(
    mods: &ModRegistry,
    song_id: &str,
    candidate: PathBuf,
    ext: &str,
    discovered: &mut Vec<DiscoveredScript>,
    seen_canon: &mut HashSet<PathBuf>,
) -> bool {
    if !candidate.is_file() {
        return false;
    }
    let canon = fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
    if !seen_canon.insert(canon) {
        return true;
    }
    let mut mod_id = "assets".to_string();
    let mut rel_path = candidate
        .strip_prefix(&mods.assets_dir)
        .unwrap_or(&candidate)
        .to_string_lossy()
        .replace('\\', "/");
    for m in &mods.mods {
        if let Ok(rel) = candidate.strip_prefix(&m.root) {
            mod_id = m.manifest.id.clone();
            rel_path = rel.to_string_lossy().replace('\\', "/");
            break;
        }
    }
    let stem = candidate
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(song_id)
        .to_string();
    discovered.push(DiscoveredScript {
        id: format!("song:{song_id}:{stem}.{ext}"),
        mod_id,
        name: stem,
        extension: ext.to_ascii_lowercase(),
        rel_path,
        path: candidate,
        scope: ScriptScope::Song(song_id.to_string()),
        priority: 0,
    });
    true
}
