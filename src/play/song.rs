use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use bevy::prelude::Resource;
use oxin_app::resolve_assets_dir;
use serde::Deserialize;

use crate::utils::constants::{DEFAULT_DIFFICULTY, DEFAULT_VARIATION};

use super::notes::SongNoteData;
use super::types::{AudioTrackSpec, CharactersMeta, ResolvedAudioConfig, SkinsMeta, SongChart};

#[derive(Resource, Debug, Clone)]
pub struct SongSelection {
    pub song_id: String,
    pub difficulty: String,
    pub variation: String,
}

impl Default for SongSelection {
    fn default() -> Self {
        Self {
            song_id: "bopeebo".to_string(),
            difficulty: DEFAULT_DIFFICULTY.to_string(),
            variation: DEFAULT_VARIATION.to_string(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SongMetaFile {
    song_name: Option<String>,
    base: SongMetaLayer,
    #[serde(default)]
    difficulties: HashMap<String, SongMetaLayer>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SongMetaLayer {
    stage: Option<String>,
    scroll_speed: Option<f64>,
    lanes: Option<usize>,
    characters: Option<CharactersMetaRaw>,
    credits: Option<Vec<String>>,
    skins: Option<SkinsMetaRaw>,
    audio: Option<AudioMetaLayer>,
}

#[derive(Debug, Default, Deserialize)]
struct CharactersMetaRaw {
    players: Option<Vec<String>>,
    opponents: Option<Vec<String>>,
    spectator: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
struct SkinsMetaRaw {
    hud: Option<String>,
    ui: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AudioMetaLayer {
    bpm: Option<f64>,
    needs_voices: Option<bool>,
    multi_vocal: Option<bool>,
    instrumental: Option<HashMap<String, TrackSpecRaw>>,
    vocals: Option<VocalsMetaRaw>,
}

#[derive(Debug, Default, Deserialize)]
struct VocalsMetaRaw {
    player: Option<TrackSpecRaw>,
    opponent: Option<TrackSpecRaw>,
    spectator: Option<TrackSpecRaw>,
}

#[derive(Debug, Clone, Deserialize)]
struct TrackSpecRaw {
    file: String,
    #[serde(default)]
    offset: f64,
    #[serde(default = "default_volume")]
    vol: f32,
}

fn default_volume() -> f32 {
    1.0
}

impl From<TrackSpecRaw> for AudioTrackSpec {
    fn from(raw: TrackSpecRaw) -> Self {
        Self {
            file: raw.file,
            offset_ms: raw.offset,
            volume: raw.vol,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawChartNote {
    t: f64,
    d: usize,
    #[serde(default)]
    l: f64,
    p: String,
    #[serde(default)]
    k: Option<String>,
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

pub fn load_song_chart(song_id: &str, difficulty: &str, variation: &str) -> Result<SongChart, String> {
    let chart_dir = song_root_dir(song_id).join("chart");
    let meta_path = chart_dir.join("meta.json");
    let notes_path = chart_dir.join("notes.json");

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

    chart.player_notes.sort_by(|a, b| a.time_ms.total_cmp(&b.time_ms));
    chart.opponent_notes.sort_by(|a, b| a.time_ms.total_cmp(&b.time_ms));
    chart.length_ms = max_end_ms + 2000.0;

    Ok(chart)
}

fn merge_metadata(
    song_id: &str,
    difficulty: &str,
    variation: &str,
    meta: &SongMetaFile,
    overlay: Option<&SongMetaLayer>,
) -> SongChart {
    let base = &meta.base;
    let mut chart = SongChart {
        song_id: song_id.to_string(),
        song_name: meta.song_name.clone().unwrap_or_else(|| song_id.to_string()),
        difficulty: difficulty.to_string(),
        variation: variation.to_string(),
        stage_id: base.stage.clone().unwrap_or_else(|| "mainStage".to_string()),
        scroll_speed: base.scroll_speed.unwrap_or(1.0),
        lanes: base.lanes.unwrap_or(4),
        credits: base.credits.clone().unwrap_or_default(),
        characters: apply_characters(CharactersMeta::default(), base.characters.as_ref()),
        skins: apply_skins(SkinsMeta::default(), base.skins.as_ref()),
        audio: apply_audio(ResolvedAudioConfig::default(), base.audio.as_ref()),
        ..Default::default()
    };

    if let Some(ov) = overlay {
        if let Some(stage) = &ov.stage {
            chart.stage_id = stage.clone();
        }
        if let Some(scroll_speed) = ov.scroll_speed {
            chart.scroll_speed = scroll_speed;
        }
        if let Some(lanes) = ov.lanes {
            chart.lanes = lanes;
        }
        if let Some(credits) = &ov.credits {
            chart.credits = credits.clone();
        }
        chart.characters = apply_characters(chart.characters, ov.characters.as_ref());
        chart.skins = apply_skins(chart.skins, ov.skins.as_ref());
        chart.audio = apply_audio(chart.audio, ov.audio.as_ref());
    }

    chart.bpm = chart.audio.bpm;
    chart
}

fn apply_characters(mut current: CharactersMeta, layer: Option<&CharactersMetaRaw>) -> CharactersMeta {
    if let Some(c) = layer {
        if let Some(players) = &c.players {
            current.players = players.clone();
        }
        if let Some(opponents) = &c.opponents {
            current.opponents = opponents.clone();
        }
        if let Some(spectator) = &c.spectator {
            current.spectator = spectator.clone();
        }
    }
    current
}

fn apply_skins(mut current: SkinsMeta, layer: Option<&SkinsMetaRaw>) -> SkinsMeta {
    if let Some(s) = layer {
        if let Some(hud) = &s.hud {
            current.hud = hud.clone();
        }
        if let Some(ui) = &s.ui {
            current.ui = ui.clone();
        }
    }
    current
}

fn apply_audio(mut current: ResolvedAudioConfig, layer: Option<&AudioMetaLayer>) -> ResolvedAudioConfig {
    let Some(a) = layer else {
        return current;
    };
    if let Some(bpm) = a.bpm {
        current.bpm = bpm;
    }
    if let Some(needs_voices) = a.needs_voices {
        current.needs_voices = needs_voices;
    }
    if let Some(multi_vocal) = a.multi_vocal {
        current.multi_vocal = multi_vocal;
    }
    if let Some(inst) = &a.instrumental {
        current.instrumental = inst
            .iter()
            .map(|(k, v)| (k.clone(), AudioTrackSpec::from(v.clone())))
            .collect();
    }
    if let Some(vocals) = &a.vocals {
        if let Some(player) = &vocals.player {
            current.player_vocal = Some(AudioTrackSpec::from(player.clone()));
        }
        if let Some(opponent) = &vocals.opponent {
            current.opponent_vocal = Some(AudioTrackSpec::from(opponent.clone()));
        }
        if let Some(spectator) = &vocals.spectator {
            current.spectator_vocal = Some(AudioTrackSpec::from(spectator.clone()));
        }
    }
    current
}

