use std::collections::HashMap;

use serde::Deserialize;

use super::super::types::{
    AudioTrackSpec, CharactersMeta, ResolvedAudioConfig, SkinsMeta, SongChart,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SongMetaFile {
    pub(super) song_name: Option<String>,
    #[serde(default)]
    pub(super) script: Option<bool>,
    #[serde(default)]
    pub(super) scripts: Option<bool>,
    #[serde(default, alias = "script_path")]
    pub(super) script_path: Option<String>,
    #[serde(default)]
    pub(super) base: SongMetaLayer,
    #[serde(default)]
    pub(super) difficulties: HashMap<String, SongMetaLayer>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SongMetaLayer {
    pub(super) stage: Option<String>,
    pub(super) scroll_speed: Option<f64>,
    pub(super) lanes: Option<usize>,
    pub(super) characters: Option<CharactersMetaRaw>,
    pub(super) credits: Option<Vec<String>>,
    pub(super) skins: Option<SkinsMetaRaw>,
    pub(super) audio: Option<AudioMetaLayer>,
    #[serde(default)]
    pub(super) script: Option<bool>,
    #[serde(default)]
    pub(super) scripts: Option<bool>,
    #[serde(default, alias = "script_path")]
    pub(super) script_path: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct CharactersMetaRaw {
    pub(super) players: Option<Vec<String>>,
    pub(super) opponents: Option<Vec<String>>,
    pub(super) spectator: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct SkinsMetaRaw {
    pub(super) hud: Option<String>,
    pub(super) ui: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AudioMetaLayer {
    pub(super) bpm: Option<f64>,
    pub(super) needs_voices: Option<bool>,
    pub(super) multi_vocal: Option<bool>,
    pub(super) instrumental: Option<HashMap<String, TrackSpecRaw>>,
    pub(super) vocals: Option<VocalsMetaRaw>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct VocalsMetaRaw {
    pub(super) player: Option<TrackSpecRaw>,
    pub(super) opponent: Option<TrackSpecRaw>,
    pub(super) spectator: Option<TrackSpecRaw>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct TrackSpecRaw {
    pub(super) file: String,
    #[serde(default)]
    pub(super) offset: f64,
    #[serde(default = "default_volume")]
    pub(super) vol: f32,
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
pub(super) struct RawChartNote {
    pub(super) t: f64,
    pub(super) d: usize,
    #[serde(default)]
    pub(super) l: f64,
    #[serde(default)]
    pub(super) p: String,
    #[serde(default)]
    pub(super) k: Option<String>,
}

pub(super) fn merge_metadata(
    song_id: &str,
    difficulty: &str,
    variation: &str,
    meta: &SongMetaFile,
    overlay: Option<&SongMetaLayer>,
) -> SongChart {
    let base = &meta.base;
    let mut chart = SongChart {
        song_id: song_id.to_string(),
        song_name: meta
            .song_name
            .clone()
            .unwrap_or_else(|| song_id.to_string()),
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

    chart.scripts = overlay
        .and_then(|o| o.scripts.or(o.script))
        .or(meta.scripts)
        .or(meta.script)
        .or(base.scripts)
        .or(base.script)
        .unwrap_or(false);
    chart.script_path = overlay
        .and_then(|o| o.script_path.as_deref())
        .or(meta.script_path.as_deref())
        .or(base.script_path.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);

    chart.bpm = chart.audio.bpm;
    chart
}

fn apply_characters(
    mut current: CharactersMeta,
    layer: Option<&CharactersMetaRaw>,
) -> CharactersMeta {
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

fn apply_audio(
    mut current: ResolvedAudioConfig,
    layer: Option<&AudioMetaLayer>,
) -> ResolvedAudioConfig {
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
