use crate::play::shaders::defs::FunkinBlendMode;
use crate::utils::constants::{CONFIRM_HOLD_TIME_S, COUNTDOWN_VOLUME};

use super::noteskin::NoteStyle;
use super::skin_schema::{
    HoldCoverDirStyle, PopupSkinConfig, StrumAnimNames, SustainAnimNames, UiSpriteStyle,
};

pub fn fallback_funkin_style() -> NoteStyle {
    let strum_anims = ["Left", "Down", "Up", "Right"].map(|d| StrumAnimNames {
        static_anim: format!("static{d}"),
        press_anim: format!("press{d}"),
        confirm_anim: format!("confirm{d}"),
        confirm_hold_anim: format!("confirmHold{d}"),
    });
    let note_anims = ["noteLeft", "noteDown", "noteUp", "noteRight"].map(str::to_string);
    let sustain_anims = ["purple", "blue", "green", "red"].map(|c| SustainAnimNames {
        body: format!("{c} hold piece"),
        end: format!("{c} hold end"),
    });
    let splash_anims = ["purple", "blue", "green", "red"].map(|c| {
        vec![format!("note impact 1 {c}"), format!("note impact 2 {c}")]
    });
    let hold_cover_dirs = [
        ("left", "Purple"),
        ("down", "Blue"),
        ("up", "Green"),
        ("right", "Red"),
    ]
    .map(|(dir, color)| HoldCoverDirStyle {
        path: format!("hold-cover-{dir}.png"),
        start_anim: format!("holdCoverStart{color}"),
        hold_anim: format!("holdCover{color}"),
        end_anim: format!("holdCoverEnd{color}"),
    });

    NoteStyle {
        id: "funkin".to_string(),
        is_pixel: false,
        strum_path: "note-strumline.png".to_string(),
        strum_scale: 0.7,
        strum_alpha: 1.0,
        strum_offsets: [[0.0, 0.0], [30.0, 30.0], [0.0, 0.0]],
        strum_anims,
        notes_path: "notes.png".to_string(),
        notes_scale: 0.7,
        notes_alpha: 1.0,
        notes_offset: [25.0, 25.0],
        note_anims,
        sustains_path: "NOTE_hold_assets.png".to_string(),
        sustains_scale: 0.7,
        sustains_alpha: 0.9,
        sustains_offset: [-2.0, 0.0],
        sustains_offset_hit: [0.0, 0.0],
        sustains_end_offset: [0.0, 6.0],
        sustain_anims,
        splash_enabled: true,
        splash_path: "note-splashes.png".to_string(),
        splash_scale: 0.9,
        splash_alpha: 1.0,
        splash_fps: 24.0,
        splash_fps_variance: 2.0,
        splash_blend_mode: FunkinBlendMode::Normal,
        splash_offset: [0.0, -2.0],
        splash_anims,
        hold_cover_enabled: true,
        hold_cover_scale: 1.0,
        hold_cover_alpha: 1.0,
        hold_cover_fps: 24.0,
        hold_cover_offset: [-11.0, 48.0],
        hold_cover_dirs,
        popup: PopupSkinConfig::fallback_funkin(),
        health_bar: UiSpriteStyle {
            path: Some("health-bar.png".to_string()),
            ..Default::default()
        },
        miss_sounds: vec![
            "misses/miss-note-1.ogg".to_string(),
            "misses/miss-note-2.ogg".to_string(),
            "misses/miss-note-3.ogg".to_string(),
        ],
        has_custom_miss_sounds: false,
        miss_volume: 0.8,
        countdown_sounds: [
            Some("countdown/sound/intro-three.ogg".to_string()),
            Some("countdown/sound/intro-two.ogg".to_string()),
            Some("countdown/sound/intro-one.ogg".to_string()),
            Some("countdown/sound/intro-go.ogg".to_string()),
        ],
        countdown_volumes: [COUNTDOWN_VOLUME; 4],
        countdown_images: [
            UiSpriteStyle::default(),
            UiSpriteStyle { path: Some("countdown/graphics/ready.png".to_string()), ..Default::default() },
            UiSpriteStyle { path: Some("countdown/graphics/set.png".to_string()), ..Default::default() },
            UiSpriteStyle { path: Some("countdown/graphics/go.png".to_string()), ..Default::default() },
        ],
        countdown_volume: COUNTDOWN_VOLUME,
        confirm_duration_s: CONFIRM_HOLD_TIME_S,
        press_duration_s: 0.10,
    }
}

