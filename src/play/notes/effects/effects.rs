use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use crate::animations::AnimationController;
use crate::play::scene::PlayState;

pub use super::effect_pools::spawn_effect_pools;
use crate::play::notes::render::{AnimClipFrames, NoteskinAssets};
use crate::play::notes::strum::HoldCoverCmd;

#[derive(Component)]
pub struct NoteSplashVisual {
    pub is_player: bool,
    pub lane: usize,
    pub active: bool,
    pub rng_state: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldCoverPhase {
    Hidden,
    Start,
    Hold,
    End,
}

#[derive(Component)]
pub struct HoldCoverVisual {
    pub is_player: bool,
    pub lane: usize,
    pub phase: HoldCoverPhase,
}

pub fn sync_note_effects(
    mut state: ResMut<PlayState>,
    skin_assets: Option<Res<NoteskinAssets>>,
    mut splashes: Query<(
        &mut NoteSplashVisual,
        &mut Sprite,
        &mut AnimationController,
        &mut Transform,
        &mut Visibility,
    )>,
    mut covers: Query<
        (
            &mut HoldCoverVisual,
            &mut Sprite,
            &mut AnimationController,
            &mut Transform,
            &mut Visibility,
        ),
        Without<NoteSplashVisual>,
    >,
) {
    let Some(assets) = skin_assets else {
        return;
    };
    let style = &assets.style;

    for (mut vis, _, anim, _, mut visibility) in &mut splashes {
        if vis.active && anim.finished {
            vis.active = false;
            *visibility = Visibility::Hidden;
        }
    }

    let pending_splashes: Vec<_> = state.player_strumline.pending_splashes.drain(..).collect();
    if style.splash_enabled && state.player_strumline.show_notesplash {
        for dir in pending_splashes {
            let variants = &assets.splash_variants[dir.index()];
            if variants.is_empty() {
                continue;
            }
            let mut picked = false;
            for (mut vis, mut sprite, mut anim, _, mut visibility) in &mut splashes {
                if !vis.active {
                    activate_splash(&mut vis, &mut sprite, &mut anim, &mut visibility, dir.index(), variants);
                    picked = true;
                    break;
                }
            }
            if !picked && let Some((mut vis, mut sprite, mut anim, _, mut visibility)) = splashes.iter_mut().next() {
                activate_splash(&mut vis, &mut sprite, &mut anim, &mut visibility, dir.index(), variants);
            }
        }
    }

    let p_strum = &state.player_strumline;
    for (vis, mut sprite, anim, mut transform, mut visibility) in &mut splashes {
        if !vis.active {
            continue;
        }
        if !style.splash_enabled || !p_strum.show_notesplash {
            *visibility = Visibility::Hidden;
            continue;
        }
        sprite.color = Color::srgba(1.0, 1.0, 1.0, style.splash_alpha);
        apply_frame_transform(
            &mut transform,
            &anim,
            p_strum.lane_x(vis.lane),
            p_strum.receptor_y(),
            style.splash_offset,
            style.splash_scale,
            p_strum.strumline_scale,
        );
    }

    for is_player in [false, true] {
        let strum = if is_player { &mut state.player_strumline } else { &mut state.opponent_strumline };
        let allow = style.hold_cover_enabled && strum.show_notesplash;
        let cmds: Vec<HoldCoverCmd> = strum.pending_hold_covers.drain(..).collect();
        for cmd in cmds {
            apply_cover_cmd(is_player, allow, cmd, &mut covers);
        }
    }

    for (mut cov, mut sprite, mut anim, mut transform, mut visibility) in &mut covers {
        let strum = if cov.is_player { &state.player_strumline } else { &state.opponent_strumline };
        if !style.hold_cover_enabled || !strum.show_notesplash {
            cov.phase = HoldCoverPhase::Hidden;
            *visibility = Visibility::Hidden;
            continue;
        }
        if cov.phase == HoldCoverPhase::Start && anim.finished {
            cov.phase = HoldCoverPhase::Hold;
            anim.play("hold", true, false, 0);
            sync_atlas_frame(&mut sprite, &anim);
        } else if cov.phase == HoldCoverPhase::End && anim.finished {
            cov.phase = HoldCoverPhase::Hidden;
            *visibility = Visibility::Hidden;
        }
        if cov.phase == HoldCoverPhase::Hidden {
            continue;
        }
        sprite.color = Color::srgba(1.0, 1.0, 1.0, style.hold_cover_alpha);
        apply_frame_transform(
            &mut transform,
            &anim,
            strum.lane_x(cov.lane),
            strum.receptor_y() + strum.receptors[cov.lane].y_offset,
            style.hold_cover_offset,
            style.hold_cover_scale,
            strum.strumline_scale,
        );
    }
}

fn activate_splash(
    vis: &mut NoteSplashVisual,
    sprite: &mut Sprite,
    anim: &mut AnimationController,
    visibility: &mut Visibility,
    lane: usize,
    variants: &[AnimClipFrames],
) {
    let r1 = xorshift32(&mut vis.rng_state);
    let r2 = xorshift32(&mut vis.rng_state);
    let variant = &variants[(r1 as usize) % variants.len()];
    let fps_delta = (r2 % 5) as f32 - 2.0;
    anim.add_anim_frames(
        "splash",
        variant.indices.clone(),
        variant.offsets.clone(),
        variant.rotated.clone(),
        (24.0 + fps_delta).max(12.0),
        false,
    );
    anim.play("splash", true, false, 0);
    sync_atlas_frame(sprite, anim);
    vis.lane = lane;
    vis.active = true;
    *visibility = Visibility::Visible;
}

fn apply_cover_cmd(
    is_player: bool,
    allow: bool,
    cmd: HoldCoverCmd,
    covers: &mut Query<
        (&mut HoldCoverVisual, &mut Sprite, &mut AnimationController, &mut Transform, &mut Visibility),
        Without<NoteSplashVisual>,
    >,
) {
    let (dir, target_phase) = match cmd {
        HoldCoverCmd::Start(d) => (d, HoldCoverPhase::Start),
        HoldCoverCmd::End(d) => (d, HoldCoverPhase::End),
        HoldCoverCmd::Kill(d) => (d, HoldCoverPhase::Hidden),
    };
    for (mut cov, mut sprite, mut anim, _, mut visibility) in covers.iter_mut() {
        if cov.is_player != is_player || cov.lane != dir.index() {
            continue;
        }
        match target_phase {
            HoldCoverPhase::Start if allow => {
                cov.phase = HoldCoverPhase::Start;
                anim.play("start", true, false, 0);
                sync_atlas_frame(&mut sprite, &anim);
                *visibility = Visibility::Visible;
            }
            HoldCoverPhase::End if allow && cov.phase != HoldCoverPhase::Hidden => {
                cov.phase = HoldCoverPhase::End;
                anim.play("end", true, false, 0);
                sync_atlas_frame(&mut sprite, &anim);
            }
            _ => {
                cov.phase = HoldCoverPhase::Hidden;
                *visibility = Visibility::Hidden;
            }
        }
        break;
    }
}

fn sync_atlas_frame(sprite: &mut Sprite, anim: &AnimationController) {
    if let (Some(atlas), Some(idx)) = (sprite.texture_atlas.as_mut(), anim.current_atlas_index()) {
        atlas.index = idx;
    }
}

fn apply_frame_transform(
    transform: &mut Transform,
    anim: &AnimationController,
    base_x: f32,
    base_y: f32,
    offset: [f32; 2],
    effect_scale: f32,
    strum_scale: f32,
) {
    let scale = effect_scale * strum_scale;
    let f_off = anim.current_frame_offset();
    transform.translation.x = base_x + (offset[0] + f_off[0]) * scale;
    transform.translation.y = base_y + (-offset[1] + f_off[1]) * scale;
    transform.rotation = if anim.current_frame_rotated() {
        Quat::from_rotation_z(-FRAC_PI_2)
    } else {
        Quat::IDENTITY
    };
    transform.scale = Vec3::splat(scale);
}

fn xorshift32(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

