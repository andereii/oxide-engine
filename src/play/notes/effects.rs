use bevy::prelude::*;

use crate::animations::AnimationController;
use crate::play::scene::PlayState;
use crate::utils::constants::{KEY_COUNT, NOTE_SPLASH_CAP};

use super::strumline::HoldCoverCmd;
use super::visuals::NoteskinAssets;

#[derive(Component)]
pub struct NoteSplashVisual {
    pub is_player: bool,
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

pub fn spawn_effect_pools(commands: &mut Commands, assets: &NoteskinAssets) {
    let style = &assets.style;
    if let (Some(tex), Some(layout)) = (&assets.splash_texture, &assets.splash_layout) {
        let default_idx = assets.splash_variants[0]
            .first()
            .and_then(|v| v.first().copied())
            .unwrap_or(0);
        for slot in 0..NOTE_SPLASH_CAP {
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
                AnimationController::default(),
                NoteSplashVisual {
                    is_player: true,
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
            let init_idx = hold_f.first().or_else(|| start_f.first()).copied().unwrap_or(0);
            let mut anim = AnimationController::default();
            if !start_f.is_empty() {
                anim.add_anim("start", start_f.clone(), 24.0, false);
            }
            if !hold_f.is_empty() {
                anim.add_anim("hold", hold_f.clone(), 24.0, true);
            }
            if !end_f.is_empty() {
                anim.add_anim("end", end_f.clone(), 24.0, false);
            }
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

pub fn sync_note_effects(
    mut state: ResMut<PlayState>,
    skin_assets: Option<Res<NoteskinAssets>>,
    mut splashes: Query<(
        &mut NoteSplashVisual,
        &mut AnimationController,
        &mut Transform,
        &mut Visibility,
    )>,
    mut covers: Query<
        (
            &mut HoldCoverVisual,
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

    for (mut vis, anim, _, mut visibility) in &mut splashes {
        if vis.active && anim.finished {
            vis.active = false;
            *visibility = Visibility::Hidden;
        }
    }

    let pending_splashes: Vec<_> = state.player_strumline.pending_splashes.drain(..).collect();
    for dir in pending_splashes {
        let variants = &assets.splash_variants[dir.index()];
        if variants.is_empty() {
            continue;
        }
        let target_x = state.player_strumline.lane_x(dir.index()) + style.splash_offset[0] * 0.2;
        let target_y = state.player_strumline.receptor_y();

        let mut picked = false;
        for (mut vis, mut anim, mut transform, mut visibility) in &mut splashes {
            if vis.active {
                continue;
            }
            activate_splash(&mut vis, &mut anim, &mut transform, &mut visibility, variants, target_x, target_y);
            picked = true;
            break;
        }
        if !picked && let Some((mut vis, mut anim, mut transform, mut visibility)) = splashes.iter_mut().next() {
            activate_splash(&mut vis, &mut anim, &mut transform, &mut visibility, variants, target_x, target_y);
        }
    }

    for (mut cov, mut anim, mut transform, mut visibility) in &mut covers {
        let strum = if cov.is_player {
            &state.player_strumline
        } else {
            &state.opponent_strumline
        };
        transform.translation.x = strum.lane_x(cov.lane) + style.hold_cover_offset[0] - 12.0;
        transform.translation.y = strum.receptor_y() + strum.receptors[cov.lane].y_offset;
        if cov.phase == HoldCoverPhase::Start && anim.finished {
            cov.phase = HoldCoverPhase::Hold;
            anim.play("hold", true, false, 0);
        } else if cov.phase == HoldCoverPhase::End && anim.finished {
            cov.phase = HoldCoverPhase::Hidden;
            *visibility = Visibility::Hidden;
        }
    }

    for is_player in [false, true] {
        let cmds: Vec<HoldCoverCmd> = if is_player {
            state.player_strumline.pending_hold_covers.drain(..).collect()
        } else {
            state.opponent_strumline.pending_hold_covers.drain(..).collect()
        };
        for cmd in cmds {
            apply_cover_cmd(is_player, cmd, &mut covers);
        }
    }
}

fn activate_splash(
    vis: &mut NoteSplashVisual,
    anim: &mut AnimationController,
    transform: &mut Transform,
    visibility: &mut Visibility,
    variants: &[Vec<usize>],
    x: f32,
    y: f32,
) {
    let r1 = xorshift32(&mut vis.rng_state);
    let r2 = xorshift32(&mut vis.rng_state);
    let variant = &variants[(r1 as usize) % variants.len()];
    let fps_delta = (r2 % 5) as f32 - 2.0;
    anim.add_anim("splash", variant.clone(), (24.0 + fps_delta).max(12.0), false);
    anim.play("splash", true, false, 0);
    transform.translation.x = x;
    transform.translation.y = y;
    vis.active = true;
    *visibility = Visibility::Visible;
}

fn apply_cover_cmd(
    is_player: bool,
    cmd: HoldCoverCmd,
    covers: &mut Query<
        (
            &mut HoldCoverVisual,
            &mut AnimationController,
            &mut Transform,
            &mut Visibility,
        ),
        Without<NoteSplashVisual>,
    >,
) {
    let (dir, target_phase) = match cmd {
        HoldCoverCmd::Start(d) => (d, HoldCoverPhase::Start),
        HoldCoverCmd::End(d) => (d, HoldCoverPhase::End),
        HoldCoverCmd::Kill(d) => (d, HoldCoverPhase::Hidden),
    };
    for (mut cov, mut anim, _, mut visibility) in covers.iter_mut() {
        if cov.is_player != is_player || cov.lane != dir.index() {
            continue;
        }
        match target_phase {
            HoldCoverPhase::Start => {
                cov.phase = HoldCoverPhase::Start;
                anim.play("start", true, false, 0);
                *visibility = Visibility::Visible;
            }
            HoldCoverPhase::End => {
                if cov.phase != HoldCoverPhase::Hidden {
                    cov.phase = HoldCoverPhase::End;
                    anim.play("end", true, false, 0);
                }
            }
            HoldCoverPhase::Hidden => {
                cov.phase = HoldCoverPhase::Hidden;
                *visibility = Visibility::Hidden;
            }
            HoldCoverPhase::Hold => {}
        }
        break;
    }
}

fn xorshift32(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}
