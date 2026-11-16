use std::collections::HashSet;
use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use crate::animations::AnimationController;
use super::blend_alpha::convert_screen_image_to_alpha;
use crate::play::scene::PlayState;
use crate::play::shaders::defs::FunkinBlendMode;
use crate::utils::ecs::{set_atlas_index, set_sprite_color, set_transform_full, set_visibility};

use crate::play::notes::data::NoteDirection;
use crate::play::notes::render::{AnimClipFrames, NoteskinAssets};
use crate::play::notes::strum::HoldCoverCmd;

#[derive(Default)]
pub struct EffectsScratch {
    splashes: Vec<NoteDirection>,
    covers: Vec<HoldCoverCmd>,
    screen_converted_images: HashSet<AssetId<Image>>,
}

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
    mut images: Option<ResMut<Assets<Image>>>,
    mut scratch: Local<EffectsScratch>,
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
    if matches!(
        style.splash_blend_mode,
        FunkinBlendMode::Screen | FunkinBlendMode::Add
    ) && let Some(splash_tex) = &assets.splash_texture
        && !scratch.screen_converted_images.contains(&splash_tex.id())
        && let Some(imgs) = images.as_deref_mut()
        && let Some(mut img) = imgs.get_mut(splash_tex)
    {
        convert_screen_image_to_alpha(&mut img);
        scratch.screen_converted_images.insert(splash_tex.id());
    }

    for (mut vis, _, anim, _, mut visibility) in &mut splashes {
        if vis.active && anim.finished {
            vis.active = false;
            set_visibility(&mut visibility, Visibility::Hidden);
        }
    }

    scratch.splashes.clear();
    scratch.splashes.extend(state.player_strumline.pending_splashes.drain(..));
    if style.splash_enabled && state.player_strumline.show_notesplash {
        for dir in scratch.splashes.iter().copied() {
            let variants = &assets.splash_variants[dir.index()];
            if variants.is_empty() {
                continue;
            }
            let mut picked = false;
            for (mut vis, mut sprite, mut anim, _, mut visibility) in &mut splashes {
                if !vis.active {
                    activate_splash(&mut vis, &mut sprite, &mut anim, &mut visibility, dir.index(), variants, style.splash_fps, style.splash_fps_variance);
                    picked = true;
                    break;
                }
            }
            if !picked && let Some((mut vis, mut sprite, mut anim, _, mut visibility)) = splashes.iter_mut().next() {
                activate_splash(&mut vis, &mut sprite, &mut anim, &mut visibility, dir.index(), variants, style.splash_fps, style.splash_fps_variance);
            }
        }
    }

    let p_strum = &state.player_strumline;
    for (vis, mut sprite, anim, mut transform, mut visibility) in &mut splashes {
        if !vis.active {
            continue;
        }
        if !style.splash_enabled || !p_strum.show_notesplash {
            set_visibility(&mut visibility, Visibility::Hidden);
            continue;
        }
        let mod_alpha = p_strum.receptors.get(vis.lane).map_or(1.0, |r| r.mod_alpha);
        set_sprite_color(&mut sprite, Color::srgba(1.0, 1.0, 1.0, style.splash_alpha * mod_alpha));
        apply_frame_transform(
            &mut transform,
            &anim,
            p_strum.lane_x(vis.lane),
            p_strum.lane_y(vis.lane),
            style.splash_offset,
            style.splash_scale,
            p_strum.strumline_scale,
        );
    }

    for is_player in [false, true] {
        let strum = if is_player { &mut state.player_strumline } else { &mut state.opponent_strumline };
        let allow = style.hold_cover_enabled && strum.show_notesplash;
        scratch.covers.clear();
        scratch.covers.extend(strum.pending_hold_covers.drain(..));
        for cmd in scratch.covers.iter().copied() {
            apply_cover_cmd(is_player, allow, cmd, &mut covers);
        }
    }

    for (mut cov, mut sprite, mut anim, mut transform, mut visibility) in &mut covers {
        let strum = if cov.is_player { &state.player_strumline } else { &state.opponent_strumline };
        if !style.hold_cover_enabled || !strum.show_notesplash || (strum.middlescroll && !strum.is_player) {
            cov.phase = HoldCoverPhase::Hidden;
            set_visibility(&mut visibility, Visibility::Hidden);
            continue;
        }
        if cov.phase == HoldCoverPhase::Start && anim.finished {
            cov.phase = HoldCoverPhase::Hold;
            anim.play("hold", true, false, 0);
            sync_atlas_frame(&mut sprite, &anim);
        } else if cov.phase == HoldCoverPhase::End && anim.finished {
            cov.phase = HoldCoverPhase::Hidden;
            set_visibility(&mut visibility, Visibility::Hidden);
        }
        if cov.phase == HoldCoverPhase::Hidden {
            continue;
        }
        let mod_alpha = strum.receptors.get(cov.lane).map_or(1.0, |r| r.mod_alpha);
        set_sprite_color(&mut sprite, Color::srgba(1.0, 1.0, 1.0, style.hold_cover_alpha * mod_alpha));
        apply_frame_transform(
            &mut transform,
            &anim,
            strum.lane_x(cov.lane),
            strum.lane_y(cov.lane) + strum.receptors[cov.lane].y_offset,
            style.hold_cover_offset,
            style.hold_cover_scale,
            strum.strumline_scale,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn activate_splash(
    vis: &mut NoteSplashVisual,
    sprite: &mut Mut<'_, Sprite>,
    anim: &mut AnimationController,
    visibility: &mut Mut<'_, Visibility>,
    lane: usize,
    variants: &[AnimClipFrames],
    splash_fps: f32,
    splash_fps_variance: f32,
) {
    let r1 = xorshift32(&mut vis.rng_state);
    let r2 = xorshift32(&mut vis.rng_state);
    let variant_idx = (r1 as usize) % variants.len();
    let base_fps = splash_fps.max(1.0);
    let var = splash_fps_variance.round().max(0.0) as u32;
    let fps_delta = if var > 0 {
        (r2 % (var * 2 + 1)) as f32 - var as f32
    } else {
        0.0
    };
    let key = super::effect_pools::splash_clip_name(lane, variant_idx);
    anim.time_scale = (base_fps + fps_delta).max(12.0) / base_fps;
    anim.play(key.as_ref(), true, false, 0);
    sync_atlas_frame(sprite, anim);
    vis.lane = lane;
    vis.active = true;
    set_visibility(visibility, Visibility::Visible);
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
                set_visibility(&mut visibility, Visibility::Visible);
            }
            HoldCoverPhase::End if allow && cov.phase != HoldCoverPhase::Hidden => {
                cov.phase = HoldCoverPhase::End;
                anim.play("end", true, false, 0);
                sync_atlas_frame(&mut sprite, &anim);
            }
            _ => {
                cov.phase = HoldCoverPhase::Hidden;
                set_visibility(&mut visibility, Visibility::Hidden);
            }
        }
        break;
    }
}

fn sync_atlas_frame(sprite: &mut Mut<'_, Sprite>, anim: &AnimationController) {
    if let Some(idx) = anim.current_atlas_index() {
        set_atlas_index(sprite, idx);
    }
}

fn apply_frame_transform(
    transform: &mut Mut<'_, Transform>,
    anim: &AnimationController,
    base_x: f32,
    base_y: f32,
    offset: [f32; 2],
    effect_scale: f32,
    strum_scale: f32,
) {
    let scale = effect_scale * strum_scale;
    let f_off = anim.current_frame_offset();
    let z = { let t: &Mut<'_, Transform> = &*transform; t.translation.z };
    let flip = anim.current_flip();
    let rot = if anim.current_frame_rotated() {
        Quat::from_rotation_z(if flip[0] ^ flip[1] { -FRAC_PI_2 } else { FRAC_PI_2 })
    } else {
        Quat::IDENTITY
    };
    set_transform_full(
        transform,
        Vec3::new(base_x + (offset[0] + f_off[0]) * scale, base_y + (-offset[1] + f_off[1]) * scale, z),
        Vec3::splat(scale),
        rot,
    );
}

/// Converts opaque black-keyed RGB spritesheets (`blendMode: "screen"` / `"add"`)
fn xorshift32(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::animations::SparrowAtlas;
    use crate::play::notes::skin::NoteStyle;

    #[test]
    fn pixel_noteskin_splashes_and_hold_covers_are_aligned() {
        let style = NoteStyle::load("pixel").expect("pixel noteskin should load");
        assert_eq!(style.splash_blend_mode, FunkinBlendMode::Screen);
        assert!((style.splash_fps - 33.0).abs() <= 0.01);
        assert!((style.splash_offset[0] - 0.0).abs() <= 0.01);
        assert!((style.splash_offset[1] - (-3.5)).abs() <= 0.01);

        for dir_cfg in &style.hold_cover_dirs {
            assert_eq!(dir_cfg.start_anim, "loop0000");
            assert_eq!(dir_cfg.hold_anim, "loop");
            assert_eq!(dir_cfg.end_anim, "explode");
        }

        let hc_xml = SparrowAtlas::from_file(style.disk_xml(&style.hold_cover_dirs[0].path))
            .expect("pixel-note-hold-cover.xml should parse");
        let loop0 = hc_xml
            .frames
            .iter()
            .find(|f| f.name == "loop0000")
            .expect("loop0000 frame must exist");
        let f_off = loop0.trim_center_offset();
        // Inside the 36x32 trimmed tile, wave pixels sit at (-1.5, -3.0) from tile center;
        // combining style.hold_cover_offset + f_off should keep the wave centered on lane_x
        // and directly over the sustain clip edge at lane_y.
        let wave_dx = (style.hold_cover_offset[0] + f_off[0] - 1.5) * style.hold_cover_scale;
        let wave_dy = (-style.hold_cover_offset[1] + f_off[1] - 3.0) * style.hold_cover_scale;
        assert!(wave_dx.abs() <= 2.0, "expected pixel wave centered horizontally, got dx={wave_dx}");
        assert!(
            (0.0..=6.0).contains(&wave_dy),
            "expected pixel wave over sustain cut edge, got dy={wave_dy}"
        );

        let mut dark_fringe = [8u8, 8, 8, 255];
        screen_pixel_to_alpha(&mut dark_fringe, true);
        assert!(
            dark_fringe[3] <= 2,
            "near-black screen pixel should become almost transparent, got alpha={}",
            dark_fringe[3]
        );

        let mut bright = [246u8, 255, 230, 255];
        screen_pixel_to_alpha(&mut bright, true);
        assert_eq!(bright, [246, 255, 230, 255]);
    }
}


