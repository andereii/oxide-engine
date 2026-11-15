use std::fs;
use std::path::{Path, PathBuf};

use bevy::prelude::Color;
use oxin_app::resolve_assets_dir;

use crate::play::notes::data::NoteDirection;
use super::defaults::fallback_funkin_style;
pub use super::skin_schema::{
    HoldCoverDirStyle, NoteskinFileRaw, PopupSkinConfig, StrumAnimNames, SustainAnimNames,
    UiSpriteStyle,
};

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
    pub splash_enabled: bool,
    pub splash_path: String,
    pub splash_scale: f32,
    pub splash_alpha: f32,
    pub splash_offset: [f32; 2],
    pub splash_anims: [Vec<String>; 4],
    pub hold_cover_enabled: bool,
    pub hold_cover_scale: f32,
    pub hold_cover_alpha: f32,
    pub hold_cover_fps: f32,
    pub hold_cover_offset: [f32; 2],
    pub hold_cover_dirs: [HoldCoverDirStyle; 4],
    pub popup: PopupSkinConfig,
    pub health_bar: UiSpriteStyle,
    pub miss_sounds: Vec<String>,
    pub has_custom_miss_sounds: bool,
    pub miss_volume: f32,
    pub countdown_sounds: [Option<String>; 4],
    pub countdown_volumes: [f32; 4],
    pub countdown_images: [UiSpriteStyle; 4],
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
        fallback_funkin_style()
    }

    pub fn load(skin_id: &str) -> Result<Self, String> {
        let id = if skin_id.trim().is_empty() { "funkin" } else { skin_id.trim() };
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
                let confirm_hold = a
                    .confirm_hold
                    .clone()
                    .unwrap_or_else(|| a.confirm.replacen("confirm", "confirmHold", 1));
                style.strum_anims[i] = StrumAnimNames {
                    static_anim: a.static_anim.clone(),
                    press_anim: a.press.clone(),
                    confirm_anim: a.confirm.clone(),
                    confirm_hold_anim: confirm_hold,
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

        if let Some(sp) = gp.note_splashes {
            style.splash_enabled = true;
            style.splash_path = sp.path;
            style.splash_scale = sp.scale;
            style.splash_alpha = sp.alpha;
            style.splash_offset = sp.offset;
            for (i, dir) in dirs.iter().enumerate() {
                if let Some(anims) = sp.animations.get(*dir) {
                    style.splash_anims[i] = anims.clone();
                }
            }
        } else {
            style.splash_enabled = false;
        }

        if let Some(hc) = gp.hold_covers {
            style.hold_cover_enabled = true;
            style.hold_cover_scale = hc.scale;
            style.hold_cover_alpha = hc.alpha;
            style.hold_cover_fps = hc.frames.max(1.0);
            style.hold_cover_offset = hc.offset;
            for (i, dir) in dirs.iter().enumerate() {
                if let Some(d) = hc.directions.get(*dir) {
                    style.hold_cover_dirs[i] = HoldCoverDirStyle {
                        path: d.path.clone(),
                        start_anim: d.animations.start.clone(),
                        hold_anim: d.animations.hold.clone(),
                        end_anim: d.animations.end.clone(),
                    };
                }
            }
        } else {
            style.hold_cover_enabled = false;
        }

        if let Some(misses) = parsed.misses {
            if !misses.sounds.path.is_empty() {
                style.miss_sounds = misses.sounds.path;
                style.has_custom_miss_sounds = true;
            }
            style.miss_volume = misses.sounds.volume;
        }
        if let Some(ui) = parsed.ui {
            style.popup.apply_ui(&ui);
            if let Some(cd) = ui.countdown {
                for (idx, step) in [cd.three, cd.two, cd.one, cd.go].into_iter().enumerate() {
                    let Some(step) = step else { continue };
                    if let Some(audio) = step.audio {
                        style.countdown_sounds[idx] = audio.path.or(audio.asset_path);
                        style.countdown_volumes[idx] = audio.volume;
                        style.countdown_volume = audio.volume;
                    }
                    if let Some(img) = step.image {
                        style.countdown_images[idx] = UiSpriteStyle {
                            path: img.path,
                            scale: img.scale,
                            alpha: img.alpha,
                            offset: img.offset,
                        };
                    }
                }
            }
            style.health_bar = ui.bars.and_then(|b| b.health).map_or_else(
                UiSpriteStyle::default,
                |h| UiSpriteStyle {
                    path: h.path.filter(|p| !p.trim().is_empty()),
                    scale: h.scale,
                    alpha: h.alpha,
                    offset: h.offset,
                },
            );
        } else {
            style.health_bar.path = None;
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

    pub fn miss_sound_assets(&self) -> Vec<String> {
        if self.has_custom_miss_sounds {
            self.miss_sounds.iter().map(|rel| self.asset_path(rel)).collect()
        } else {
            self.miss_sounds
                .iter()
                .map(|rel| format!("gameplay/noteskins/funkin/{rel}"))
                .collect()
        }
    }

    pub fn health_bar_asset(&self) -> String {
        if let Some(rel) = &self.health_bar.path {
            let candidate = self.asset_path(rel);
            if resolve_assets_dir().join(&candidate).exists() {
                return candidate;
            }
        }
        "gameplay/noteskins/funkin/health-bar.png".to_string()
    }

    pub fn lane_color(&self, dir: NoteDirection) -> Color {
        dir.color()
    }
}
