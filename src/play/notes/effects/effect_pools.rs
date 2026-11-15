use std::borrow::Cow;

use bevy::prelude::*;

use crate::animations::AnimationController;
use crate::utils::constants::{KEY_COUNT, NOTE_SPLASH_CAP};

use super::effects::{HoldCoverPhase, HoldCoverVisual, NoteSplashVisual};
use crate::play::notes::render::{AnimClipFrames, NoteskinAssets};

const SPLASH_KEYS: [[&str; 4]; KEY_COUNT] = [
    ["s0_0", "s0_1", "s0_2", "s0_3"],
    ["s1_0", "s1_1", "s1_2", "s1_3"],
    ["s2_0", "s2_1", "s2_2", "s2_3"],
    ["s3_0", "s3_1", "s3_2", "s3_3"],
];

pub(super) fn splash_clip_name(lane: usize, variant: usize) -> Cow<'static, str> {
    if let Some(key) = SPLASH_KEYS.get(lane).and_then(|row| row.get(variant)) {
        Cow::Borrowed(key)
    } else {
        Cow::Owned(format!("s{lane}_{variant}"))
    }
}

fn add_clip(
    anim: &mut AnimationController,
    name: &str,
    clip: &AnimClipFrames,
    fps: f32,
    looped: bool,
) {
    if !clip.indices.is_empty() {
        anim.add_anim_frames(
            name,
            clip.indices.clone(),
            clip.offsets.clone(),
            clip.rotated.clone(),
            fps,
            looped,
        );
    }
}

pub fn spawn_effect_pools(commands: &mut Commands, assets: &NoteskinAssets) {
    let style = &assets.style;
    if let (Some(tex), Some(layout)) = (&assets.splash_texture, &assets.splash_layout) {
        let default_idx = assets.splash_variants[0]
            .first()
            .and_then(|v| v.indices.first().copied())
            .unwrap_or(0);
        for slot in 0..NOTE_SPLASH_CAP {
            let mut anim = AnimationController::default();
            for (lane, variants) in assets.splash_variants.iter().enumerate() {
                for (v_idx, clip) in variants.iter().enumerate() {
                    let key = splash_clip_name(lane, v_idx);
                    add_clip(&mut anim, key.as_ref(), clip, 24.0, false);
                }
            }
            let mut sprite = Sprite::from_atlas_image(
                tex.clone(),
                TextureAtlas {
                    layout: layout.clone(),
                    index: default_idx,
                },
            );
            sprite.color = Color::srgba(1.0, 1.0, 1.0, style.splash_alpha);
            commands.spawn((
                sprite,
                Transform::from_xyz(0.0, -900.0, 3.0).with_scale(Vec3::splat(style.splash_scale)),
                Visibility::Hidden,
                anim,
                NoteSplashVisual {
                    is_player: true,
                    lane: 0,
                    active: false,
                    rng_state: 0x9E37_79B9_u32.wrapping_add(slot as u32 * 7919),
                },
            ));
        }
    }

    for is_player in [false, true] {
        for lane in 0..KEY_COUNT {
            let (Some(tex), Some(layout)) = (
                &assets.hold_cover_textures[lane],
                &assets.hold_cover_layouts[lane],
            ) else {
                continue;
            };
            let (start_f, hold_f, end_f) = &assets.hold_cover_frames[lane];
            let init_idx = hold_f
                .indices
                .first()
                .or_else(|| start_f.indices.first())
                .copied()
                .unwrap_or(0);
            let fps = style.hold_cover_fps.max(1.0);
            let mut anim = AnimationController::default();
            add_clip(&mut anim, "start", start_f, fps, false);
            add_clip(&mut anim, "hold", hold_f, fps, true);
            add_clip(&mut anim, "end", end_f, fps, false);
            let mut sprite = Sprite::from_atlas_image(
                tex.clone(),
                TextureAtlas {
                    layout: layout.clone(),
                    index: init_idx,
                },
            );
            sprite.color = Color::srgba(1.0, 1.0, 1.0, style.hold_cover_alpha);
            commands.spawn((
                sprite,
                Transform::from_xyz(0.0, -900.0, 3.0)
                    .with_scale(Vec3::splat(style.hold_cover_scale)),
                Visibility::Hidden,
                anim,
                HoldCoverVisual {
                    is_player,
                    lane,
                    phase: HoldCoverPhase::Hidden,
                },
            ));
        }
    }
}

