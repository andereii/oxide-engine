mod metadata;
mod scripts;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use bevy::prelude::Resource;
use oxin_app::resolve_assets_dir;

use crate::mods::ModRegistry;
use crate::utils::constants::{DEFAULT_DIFFICULTY, DEFAULT_VARIATION};

use super::notes::SongNoteData;
use super::types::SongChart;
use metadata::{RawChartNote, SongMetaFile, merge_metadata};

pub use scripts::append_song_sidecar_scripts;

#[derive(Resource, Debug, Clone)]
pub struct SongSelection {
    pub song_id: String,
    pub difficulty: String,
    pub variation: String,
}

impl Default for SongSelection {
    fn default() -> Self {
        Self {
            song_id: "blammed".to_string(),
            difficulty: DEFAULT_DIFFICULTY.to_string(),
            variation: DEFAULT_VARIATION.to_string(),
        }
    }
}

pub fn song_root_dir(song_id: &str) -> PathBuf {
    resolve_assets_dir()
        .join("gameplay")
        .join("songs")
        .join(song_id)
}

pub fn song_asset_path(song_id: &str, file: &str) -> String {
    format!("gameplay/songs/{song_id}/song/{file}")
}

pub fn song_asset_path_in(mods: &ModRegistry, song_id: &str, file: &str) -> String {
    for rel in [
        format!("gameplay/songs/{song_id}/song/{file}"),
        format!("songs/{song_id}/song/{file}"),
        format!("songs/{song_id}/{file}"),
    ] {
        if let Some(asset_path) = mods.resolve_asset_server_path(&rel) {
            return asset_path;
        }
    }
    song_asset_path(song_id, file)
}

pub fn list_song_difficulties_in(
    mods: &ModRegistry,
    song_id: &str,
    current_difficulty: &str,
) -> Vec<String> {
    const STANDARD_ORDER: &[&str] = &[
        "easy",
        "normal",
        "hard",
        "erect",
        "nightmare",
        "pico-easy",
        "pico-normal",
        "pico-hard",
    ];
    let meta_path = mods
        .resolve_file(&format!("gameplay/songs/{song_id}/chart/meta.json"))
        .or_else(|| mods.resolve_file(&format!("songs/{song_id}/chart/meta.json")))
        .or_else(|| mods.resolve_file(&format!("songs/{song_id}/meta.json")))
        .unwrap_or_else(|| song_root_dir(song_id).join("chart").join("meta.json"));

    let Ok(meta_raw) = fs::read_to_string(&meta_path) else {
        return vec![current_difficulty.to_string()];
    };
    let Ok(meta) = serde_json::from_str::<SongMetaFile>(&meta_raw) else {
        return vec![current_difficulty.to_string()];
    };

    let want_pico = current_difficulty.to_ascii_lowercase().starts_with("pico-");
    let mut keys: Vec<String> = meta
        .difficulties
        .keys()
        .filter(|k| {
            let lower = k.to_ascii_lowercase();
            if want_pico {
                lower.starts_with("pico-")
            } else {
                !lower.starts_with("pico-")
            }
        })
        .cloned()
        .collect();

    if keys.is_empty() {
        keys = meta.difficulties.keys().cloned().collect();
    }
    if keys.is_empty() {
        keys.push(current_difficulty.to_string());
    }

    keys.sort_by(|a, b| {
        let al = a.to_ascii_lowercase();
        let bl = b.to_ascii_lowercase();
        let ai = STANDARD_ORDER.iter().position(|&d| d == al);
        let bi = STANDARD_ORDER.iter().position(|&d| d == bl);
        match (ai, bi) {
            (Some(ia), Some(ib)) => ia.cmp(&ib),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => al.cmp(&bl),
        }
    });
    keys
}

pub fn load_song_chart_in(
    mods: &ModRegistry,
    song_id: &str,
    difficulty: &str,
    variation: &str,
) -> Result<SongChart, String> {
    let meta_path = mods
        .resolve_file(&format!("gameplay/songs/{song_id}/chart/meta.json"))
        .or_else(|| mods.resolve_file(&format!("songs/{song_id}/chart/meta.json")))
        .or_else(|| mods.resolve_file(&format!("songs/{song_id}/meta.json")))
        .unwrap_or_else(|| song_root_dir(song_id).join("chart").join("meta.json"));
    let notes_path = mods
        .resolve_file(&format!("gameplay/songs/{song_id}/chart/notes.json"))
        .or_else(|| mods.resolve_file(&format!("songs/{song_id}/chart/notes.json")))
        .or_else(|| mods.resolve_file(&format!("songs/{song_id}/notes.json")))
        .unwrap_or_else(|| song_root_dir(song_id).join("chart").join("notes.json"));
    let events_path = mods
        .resolve_file(&format!("gameplay/songs/{song_id}/chart/events.json"))
        .or_else(|| mods.resolve_file(&format!("songs/{song_id}/chart/events.json")))
        .or_else(|| mods.resolve_file(&format!("songs/{song_id}/events.json")))
        .unwrap_or_else(|| song_root_dir(song_id).join("chart").join("events.json"));

    let meta_raw = fs::read_to_string(&meta_path)
        .map_err(|e| format!("failed to read {}: {e}", meta_path.display()))?;
    let meta: SongMetaFile = serde_json::from_str(&meta_raw)
        .map_err(|e| format!("failed to parse {}: {e}", meta_path.display()))?;

    let notes_raw = fs::read_to_string(&notes_path)
        .map_err(|e| format!("failed to read {}: {e}", notes_path.display()))?;
    let all_charts: HashMap<String, Vec<RawChartNote>> = serde_json::from_str(&notes_raw)
        .map_err(|e| format!("failed to parse {}: {e}", notes_path.display()))?;

    let diff_key = difficulty.to_ascii_lowercase();
    let overlay = meta
        .difficulties
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(&diff_key))
        .map(|(_, v)| v);

    let mut chart = merge_metadata(song_id, &diff_key, variation, &meta, overlay);
    chart.events = crate::play::camera::events::load_song_events(&events_path, &diff_key);

    let raw_notes = all_charts
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(&diff_key))
        .map(|(_, v)| v.as_slice())
        .ok_or_else(|| format!("difficulty '{diff_key}' not found in {}", notes_path.display()))?;

    let mut max_end_ms: f64 = 10_000.0;
    for (idx, note) in raw_notes.iter().enumerate() {
        let end_ms = note.t + note.l.max(0.0);
        if end_ms > max_end_ms {
            max_end_ms = end_ms;
        }
        let is_player = note.p.eq_ignore_ascii_case("pl");
        let is_opponent = note.p.eq_ignore_ascii_case("op");
        let parsed = SongNoteData {
            id: idx,
            time_ms: note.t,
            direction: note.d,
            sustain_length_ms: note.l.max(0.0),
            kind: note.k.clone(),
            low_priority: false,
            scoreable: is_player,
        };
        if is_player {
            chart.player_notes.push(parsed);
        } else if is_opponent {
            chart.opponent_notes.push(parsed);
        }
    }

    chart
        .player_notes
        .sort_by(|a, b| a.time_ms.total_cmp(&b.time_ms));
    chart
        .opponent_notes
        .sort_by(|a, b| a.time_ms.total_cmp(&b.time_ms));
    chart.length_ms = max_end_ms + 2000.0;

    Ok(chart)
}
