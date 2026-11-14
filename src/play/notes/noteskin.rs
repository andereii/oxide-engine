use std::fs;
use std::path::{Path, PathBuf};

use bevy::prelude::Color;
use oxin_app::resolve_assets_dir;

use crate::utils::constants::{CONFIRM_HOLD_TIME_S, COUNTDOWN_VOLUME};

use super::direction::NoteDirection;
use super::skin_schema::NoteskinFileRaw;

#[derive(Debug, Clone)]
pub struct StrumAnimNames {
    pub static_anim: String,
    pub press_anim: String,
    pub confirm_anim: String,
}

#[derive(Debug, Clone)]
pub struct SustainAnimNames {
    pub body: String,
    pub end: String,
}

#[derive(Debug, Clone)]
pub struct NoteStyle {
    pub id: String,
    pub is_pixel: bool,
    pub strum_path: String,
    pub strum_scale: f32,
    pub strum_alpha: f32,
    pub strum_offsets: [[f32; 2]; 3],
    pub strum_anims: [StrumAnimNames; 4],
    pub notes_path: String,
    pub notes_scale: f32,
    pub notes_alpha: f32,
    pub notes_offset: [f32; 2],
    pub note_anims: [String; 4],
    pub sustains_path: String,
    pub sustains_scale: f32,
    pub sustains_alpha: f32,
    pub sustains_offset: [f32; 2],
    pub sustains_offset_hit: [f32; 2],
    pub sustains_end_offset: [f32; 2],
    pub sustain_anims: [SustainAnimNames; 4],
    pub miss_sounds: Vec<String>,
    pub miss_volume: f32,
    pub countdown_sounds: [Option<String>; 4],
    pub countdown_volume: f32,
    pub confirm_duration_s: f64,
    pub press_duration_s: f64,
}

impl Default for NoteStyle {
    fn default() -> Self {
        Self::load("funkin").unwrap_or_else(|_| Self::fallback_funkin())
    }
}

impl NoteStyle {
    pub fn fallback_funkin() -> Self {
        Self {
            id: "funkin".to_string(),
            is_pixel: false,
            strum_path: "note-strumline.png".to_string(),
            strum_scale: 0.7,
            strum_alpha: 1.0,
            strum_offsets: [[0.0, 0.0], [30.0, 30.0], [0.0, 0.0]],
            strum_anims: [
                StrumAnimNames {
                    static_anim: "staticLeft".to_string(),
                    press_anim: "pressLeft".to_string(),
                    confirm_anim: "confirmLeft".to_string(),
                },
                StrumAnimNames {
                    static_anim: "staticDown".to_string(),
                    press_anim: "pressDown".to_string(),
                    confirm_anim: "confirmDown".to_string(),
                },
                StrumAnimNames {
                    static_anim: "staticUp".to_string(),
                    press_anim: "pressUp".to_string(),
                    confirm_anim: "confirmUp".to_string(),
                },
                StrumAnimNames {
                    static_anim: "staticRight".to_string(),
                    press_anim: "pressRight".to_string(),
                    confirm_anim: "confirmRight".to_string(),
                },
            ],
            notes_path: "notes.png".to_string(),
            notes_scale: 0.7,
            notes_alpha: 1.0,
            notes_offset: [25.0, 25.0],
            note_anims: [
                "noteLeft".to_string(),
                "noteDown".to_string(),
                "noteUp".to_string(),
                "noteRight".to_string(),
            ],
            sustains_path: "NOTE_hold_assets.png".to_string(),
            sustains_scale: 0.7,
            sustains_alpha: 0.9,
            sustains_offset: [-2.0, 0.0],
            sustains_offset_hit: [0.0, 0.0],
            sustains_end_offset: [0.0, 6.0],
            sustain_anims: [
                SustainAnimNames {
                    body: "purple hold piece".to_string(),
                    end: "purple hold end".to_string(),
                },
                SustainAnimNames {
                    body: "blue hold piece".to_string(),
                    end: "blue hold end".to_string(),
                },
                SustainAnimNames {
                    body: "green hold piece".to_string(),
                    end: "green hold end".to_string(),
                },
                SustainAnimNames {
                    body: "red hold piece".to_string(),
                    end: "red hold end".to_string(),
                },
            ],
            miss_sounds: vec![
                "miss-note-1.ogg".to_string(),
                "miss-note-2.ogg".to_string(),
                "miss-note-3.ogg".to_string(),
            ],
            miss_volume: 0.8,
            countdown_sounds: [
                Some("countdown/sound/intro-three.ogg".to_string()),
                Some("countdown/sound/intro-two.ogg".to_string()),
                Some("countdown/sound/intro-one.ogg".to_string()),
                Some("countdown/sound/intro-go.ogg".to_string()),
            ],
            countdown_volume: COUNTDOWN_VOLUME,
            confirm_duration_s: CONFIRM_HOLD_TIME_S,
            press_duration_s: 0.10,
        }
    }

    pub fn load(skin_id: &str) -> Result<Self, String> {
        let id = if skin_id.trim().is_empty() {
            "funkin"
        } else {
            skin_id.trim()
        };
        let json_path = resolve_assets_dir()
            .join("gameplay")
            .join("noteskins")
            .join(id)
            .join(format!("{id}.json"));
        let raw = fs::read_to_string(&json_path)
            .map_err(|e| format!("failed to read {}: {e}", json_path.display()))?;
        let parsed: NoteskinFileRaw = serde_json::from_str(&raw)
            .map_err(|e| format!("failed to parse {}: {e}", json_path.display()))?;

        let mut style = Self::fallback_funkin();
        style.id = id.to_string();
        style.is_pixel = parsed.is_pixel;

        let gp = parsed.gameplay;
        style.strum_path = gp.strumline.path;
        style.strum_scale = gp.strumline.scale;
        style.strum_alpha = gp.strumline.alpha;
        if let Some(off) = gp.strumline.offsets {
            style.strum_offsets = [off.static_offset, off.press, off.confirm];
        }

        let dirs = ["left", "down", "up", "right"];
        for (i, dir) in dirs.iter().enumerate() {
            if let Some(a) = gp.strumline.animations.get(*dir) {
                style.strum_anims[i] = StrumAnimNames {
                    static_anim: a.static_anim.clone(),
                    press_anim: a.press.clone(),
                    confirm_anim: a.confirm.clone(),
                };
            }
            if let Some(n) = gp.notes.animations.get(*dir) {
                style.note_anims[i] = n.clone();
            }
            if let Some(s) = gp.sustains.animations.get(*dir) {
                style.sustain_anims[i] = SustainAnimNames {
                    body: s.body.clone(),
                    end: s.end.clone(),
                };
            }
        }

        style.notes_path = gp.notes.path;
        style.notes_scale = gp.notes.scale;
        style.notes_alpha = gp.notes.alpha;
        style.notes_offset = gp.notes.offset;

        style.sustains_path = gp.sustains.path;
        style.sustains_scale = gp.sustains.scale;
        style.sustains_alpha = gp.sustains.alpha;
        style.sustains_offset = gp.sustains.offset;
        style.sustains_offset_hit = gp.sustains.offset_hit;
        style.sustains_end_offset = gp.sustains.end_offset;

        if let Some(misses) = parsed.misses {
            if !misses.sounds.path.is_empty() {
                style.miss_sounds = misses.sounds.path;
            }
            style.miss_volume = misses.sounds.volume;
        }

        if let Some(cd) = parsed.ui.and_then(|u| u.countdown) {
            let steps = [cd.three, cd.two, cd.one, cd.go];
            for (idx, step) in steps.into_iter().enumerate() {
                if let Some(audio) = step.and_then(|s| s.audio) {
                    style.countdown_sounds[idx] = audio.path.or(audio.asset_path);
                    style.countdown_volume = audio.volume;
                }
            }
        }

        Ok(style)
    }

    pub fn asset_path(&self, rel: &str) -> String {
        format!("gameplay/noteskins/{}/{rel}", self.id)
    }

    pub fn disk_xml(&self, rel: &str) -> PathBuf {
        let xml_rel = Path::new(rel).with_extension("xml");
        resolve_assets_dir()
            .join("gameplay")
            .join("noteskins")
            .join(&self.id)
            .join(xml_rel)
    }

    pub fn miss_sound_asset(rel: &str) -> String {
        format!("gameplay/general/sounds/{rel}")
    }

    pub fn lane_color(&self, dir: NoteDirection) -> Color {
        match dir {
            NoteDirection::Left => Color::srgb(0.76, 0.29, 0.60),
            NoteDirection::Down => Color::srgb(0.00, 0.85, 0.95),
            NoteDirection::Up => Color::srgb(0.07, 0.90, 0.05),
            NoteDirection::Right => Color::srgb(0.98, 0.22, 0.25),
        }
    }
}
