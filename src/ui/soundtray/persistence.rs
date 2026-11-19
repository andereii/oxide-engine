use std::fs;
use std::path::PathBuf;

use bevy::prelude::warn;
use oxin_app::resolve_assets_dir;
use serde::{Deserialize, Serialize};

const PREFS_REL_PATH: &str = "userdata/sound.json";

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(super) struct SoundPreferences {
    pub(super) volume: f32,
    pub(super) muted: bool,
}

impl Default for SoundPreferences {
    fn default() -> Self {
        Self {
            volume: 1.0,
            muted: false,
        }
    }
}

pub(super) fn snap_volume(volume: f32) -> f32 {
    if !volume.is_finite() {
        return 1.0;
    }
    ((volume.clamp(0.0, 1.0) * 10.0).round() / 10.0).clamp(0.0, 1.0)
}

fn preferences_path() -> PathBuf {
    let assets_dir = resolve_assets_dir();
    let root = assets_dir
        .parent()
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    root.join(PREFS_REL_PATH)
}

pub(super) fn load_sound_preferences() -> SoundPreferences {
    let path = preferences_path();
    let Ok(raw) = fs::read_to_string(&path) else {
        return SoundPreferences::default();
    };
    match serde_json::from_str::<SoundPreferences>(&raw) {
        Ok(mut prefs) => {
            prefs.volume = snap_volume(prefs.volume);
            prefs
        }
        Err(_) => SoundPreferences::default(),
    }
}

pub(super) fn save_sound_preferences(volume: f32, muted: bool) {
    let path = preferences_path();
    if let Some(parent) = path.parent()
        && let Err(err) = fs::create_dir_all(parent)
    {
        warn!("Failed to create sound preferences dir: {err}");
        return;
    }
    let prefs = SoundPreferences {
        volume: snap_volume(volume),
        muted,
    };
    match serde_json::to_string_pretty(&prefs) {
        Ok(json) => {
            if let Err(err) = fs::write(&path, json) {
                warn!("Failed to write sound preferences: {err}");
            }
        }
        Err(err) => {
            warn!("Failed to serialize sound preferences: {err}");
        }
    }
}
