use std::f32::consts::FRAC_PI_2;

use bevy::image::ImageSampler;
use bevy::prelude::*;

use crate::animations::atlas_composite::{
    CompositePiece, FlxAnimateTransformParams, compose_flx_piece_bevy_affine, modulate_color,
};
use crate::play::notes::CharNoteSignal;
use crate::play::scene::PlayState;
use crate::play::stage::schema::{composite_piece_z, stage_layer_z};
use crate::play::types::PlayPhase;
use crate::utils::conductor::advance_step_range;
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::utils::ecs::{
    set_sprite_atlas, set_sprite_color, set_sprite_flip, set_transform_full, set_visibility,
};

use super::behavior::{CharacterActor, CharacterTrailSample, ClipVisual};
use super::schema::CharacterRole;
use super::spawn::{CharacterPartVisual, CharacterTrailVisual, CompositeAffine2d};

type CharPartsQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Sprite,
        &'static mut Transform,
        &'static mut Visibility,
        Option<&'static mut CompositeAffine2d>,
    ),
    With<CharacterPartVisual>,
>;

type CharTrailQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Sprite,
        &'static mut Transform,
        &'static mut Visibility,
    ),
    (With<CharacterTrailVisual>, Without<CharacterPartVisual>),
>;

#[derive(Resource, Default)]
pub struct CharacterSyncState {
    pub last_step: Option<i32>,
    pub last_song_pos_ms: Option<f64>,
}

#[derive(Clone)]
struct ParentMirrorState {
    anim_name: Option<String>,
    frame_cursor: usize,
    reversed: bool,
    position: [f32; 2],
    scale: [f32; 2],
    flip: [bool; 2],
    scroll_factor: [f32; 2],
    visible: bool,
    alpha: f32,
    data_offsets: [f32; 2],
    idle_size: [f32; 2],
    facing_flip_x: bool,
}

#[inline]
pub(crate) fn compute_underlay_alpha(parent_alpha: f32) -> f32 {
    if parent_alpha < 0.999 { 1.0 } else { 0.0 }
}

fn ensure_pixel_sampler(actor: &mut CharacterActor, images: &mut Assets<Image>) {
    if actor.data.is_pixel && !actor.sampler_applied {
        let mut all_ready = true;
        for src in &actor.sources {
            if let Some(mut img) = images.get_mut(&src.texture) {
                img.sampler = ImageSampler::nearest();
            } else {
                all_ready = false;
            }
        }
        actor.sampler_applied = all_ready;
    }
}

fn role_index(role: CharacterRole) -> usize {
    match role {
        CharacterRole::Player => 0,
        CharacterRole::Opponent => 1,
        CharacterRole::Spectator => 2,
    }
}

pub fn sync_characters(
    time: Res<Time>,
    mut state: ResMut<PlayState>,
    mut sync_state: ResMut<CharacterSyncState>,
    mut images: ResMut<Assets<Image>>,
    mut signals: Local<(Vec<CharNoteSignal>, Vec<CharNoteSignal>)>,
    mut actors: Query<&mut CharacterActor>,
    mut parts: CharPartsQuery,
    mut trails: CharTrailQuery,
) {
    let (player_signals, opponent_signals) = &mut *signals;
    player_signals.clear();
    player_signals.extend(state.player_strumline.pending_char_events.drain(..));
    opponent_signals.clear();
    opponent_signals.extend(state.opponent_strumline.pending_char_events.drain(..));

    let paused = matches!(
        state.phase,
        PlayPhase::Paused(_) | PlayPhase::GameOver | PlayPhase::Ended
    );
    let current_pos_ms = state.conductor.song_position_ms;
    let did_reset = state.needs_reset
        || sync_state
            .last_song_pos_ms
            .is_some_and(|prev| current_pos_ms < prev - 50.0);
    sync_state.last_song_pos_ms = Some(current_pos_ms);

    let current_step = state.conductor.current_step;
    let step_range = advance_step_range(&mut sync_state.last_step, current_step, paused, did_reset);

    let elapsed_s = if paused { 0.0 } else { f64::from(time.delta_secs()) };
    let song_pos_ms = (state.phase == PlayPhase::Playing).then_some(current_pos_ms);
    let step_len_ms = state.conductor.step_length_ms().max(1.0);
    let player_holding = !state.is_bot_play_mode && state.player_strumline.has_any_held_key();

    let cam_scroll_x = state.camera.follow_point[0] as f32 - SCREEN_WIDTH * 0.5;
    let cam_scroll_y = state.camera.follow_point[1] as f32 - SCREEN_HEIGHT * 0.5;
    let zoom = (state.camera.current_zoom * state.camera.bop_multiplier).max(0.05) as f32;

    let dt_trail = if paused { 0.0 } else { elapsed_s };
    let mut mirror_states: [Option<ParentMirrorState>; 3] = [None, None, None];
    let mut has_underlays = false;

    for mut actor in &mut actors {
        if actor.mirror_of.is_some() {
            has_underlays = true;
            continue;
        }
        ensure_pixel_sampler(&mut actor, &mut images);

        if did_reset {
            actor.reset_character();
        } else if !paused {
            actor.on_update(elapsed_s, song_pos_ms, step_len_ms, player_holding);
            let skip_default_sing = actor.script_anim_override;
            actor.script_anim_override = false;
            if !skip_default_sing {
                for sig in player_signals.iter() {
                    actor.handle_player_signal(sig, &state.note_kinds);
                }
                for sig in opponent_signals.iter() {
                    actor.handle_opponent_signal(sig, &state.note_kinds);
                }
                if let Some(steps) = step_range.clone() {
                    for step in steps {
                        actor.on_step_hit(step);
                    }
                }
            }
        }

        let idx = role_index(actor.role);
        state.camera.focus_points[idx] = Some(actor.camera_focus_point);
        if mirror_states[idx].is_none() {
            mirror_states[idx] = Some(ParentMirrorState {
                anim_name: actor.controller.current_name().map(str::to_string),
                frame_cursor: actor.controller.frame_cursor,
                reversed: actor.controller.reversed,
                position: actor.slot.position,
                scale: actor.slot.scale,
                flip: actor.slot.flip,
                scroll_factor: actor.slot.scroll_factor,
                visible: actor.slot.visible && actor.mod_visible,
                alpha: (actor.slot.opacity * actor.mod_tint[3]).clamp(0.0, 1.0),
                data_offsets: actor.data.offsets,
                idle_size: actor.idle_size,
                facing_flip_x: actor.facing_flip_x,
            });
        }

        sync_actor_visuals(
            &mut actor,
            dt_trail,
            cam_scroll_x,
            cam_scroll_y,
            zoom,
            &mut parts,
            &mut trails,
        );
    }

    if !has_underlays {
        return;
    }

    for mut actor in &mut actors {
        let Some(cfg) = actor.mirror_of else {
            continue;
        };
        let Some(ref parent) = mirror_states[role_index(cfg.parent_role)] else {
            actor.mod_visible = false;
            sync_actor_visuals(
                &mut actor,
                0.0,
                cam_scroll_x,
                cam_scroll_y,
                zoom,
                &mut parts,
                &mut trails,
            );
            continue;
        };
        ensure_pixel_sampler(&mut actor, &mut images);

        actor.slot.position = [
            parent.position[0] + cfg.offset[0],
            parent.position[1] + cfg.offset[1],
        ];
        actor.slot.scale = parent.scale;
        actor.slot.flip = parent.flip;
        actor.slot.scroll_factor = parent.scroll_factor;
        actor.slot.visible = parent.visible;
        actor.slot.opacity = 1.0;
        actor.data.offsets = parent.data_offsets;
        actor.idle_size = parent.idle_size;
        actor.facing_flip_x = parent.facing_flip_x;

        let underlay_alpha = compute_underlay_alpha(parent.alpha);
        actor.mod_tint[3] = underlay_alpha;
        actor.mod_visible = parent.visible && underlay_alpha > 0.0005;

        if let Some(ref target_anim) = parent.anim_name {
            let resolved = actor.controller.resolve_name(target_anim).or_else(|| {
                if target_anim == "idle" {
                    actor.controller.resolve_name("danceRight").or_else(|| actor.controller.resolve_name("danceLeft"))
                } else if target_anim.starts_with("dance") {
                    actor.controller.resolve_name("idle")
                } else {
                    None
                }
            });
            if let Some(anim_key) = resolved {
                let needs_switch = actor.controller.current_name() != Some(anim_key.as_str())
                    || actor.controller.reversed != parent.reversed;
                if needs_switch {
                    actor.controller.play(&anim_key, true, parent.reversed, parent.frame_cursor);
                } else if let Some(anim_def) = actor.controller.animations.get(&anim_key) {
                    let max_f = anim_def.frames.len().saturating_sub(1);
                    actor.controller.frame_cursor = parent.frame_cursor.min(max_f);
                }
            }
        }

        sync_actor_visuals(
            &mut actor,
            0.0,
            cam_scroll_x,
            cam_scroll_y,
            zoom,
            &mut parts,
            &mut trails,
        );
    }
}

fn compute_sheet_sample(
    actor: &CharacterActor,
    source_idx: usize,
    scale: [f32; 2],
    flip: [bool; 2],
    anim_off: [f32; 2],
) -> Option<CharacterTrailSample> {
    let src = actor.sources.get(source_idx)?;
    let frame_idx = actor.controller.current_atlas_index().unwrap_or(0);
    let frame_sz = src.frame_sizes.get(&frame_idx).copied().unwrap_or(actor.idle_size);
    let mut world_cx = actor.slot.position[0]
        + (frame_sz[0] - actor.idle_size[0]) * 0.5 * scale[0]
        + actor.data.offsets[0] * (1.0 + scale[0]);
    let mut world_cy = actor.slot.position[1]
        - (actor.idle_size[1] - frame_sz[1] * 0.5) * scale[1]
        + actor.data.offsets[1] * (1.0 + scale[1]);
    world_cx -= anim_off[0] * scale[0];
    world_cy -= anim_off[1] * scale[1];

    let frame_off = actor.controller.current_frame_offset();
    let foff = [
        if flip[0] { -frame_off[0] } else { frame_off[0] },
        if flip[1] { -frame_off[1] } else { frame_off[1] },
    ];
    let rot = if actor.controller.current_frame_rotated() {
        if flip[0] ^ flip[1] { -FRAC_PI_2 } else { FRAC_PI_2 }
    } else {
        0.0
    };
    Some(CharacterTrailSample {
        source_idx,
        frame_idx,
        world_cx,
        world_cy,
        foff,
        scale,
        flip,
        rot,
    })
}

#[allow(clippy::too_many_arguments)]
fn apply_sheet_sample_to_sprite(
    actor: &CharacterActor,
    sample: &CharacterTrailSample,
    tint: Color,
    z: f32,
    visible: bool,
    cam_scroll_x: f32,
    cam_scroll_y: f32,
    zoom: f32,
    sprite: &mut Mut<'_, Sprite>,
    tf: &mut Mut<'_, Transform>,
    vis: &mut Mut<'_, Visibility>,
) {
    let Some(src) = actor.sources.get(sample.source_idx) else {
        set_visibility(vis, Visibility::Hidden);
        return;
    };
    set_sprite_atlas(sprite, &src.texture, &src.layout, sample.frame_idx);
    set_sprite_color(sprite, tint);
    set_sprite_flip(sprite, sample.flip[0], sample.flip[1]);

    let screen_cx = sample.world_cx - cam_scroll_x * actor.slot.scroll_factor[0];
    let screen_cy = sample.world_cy - cam_scroll_y * actor.slot.scroll_factor[1];
    let bx = (screen_cx - SCREEN_WIDTH * 0.5) * zoom + sample.foff[0] * sample.scale[0] * zoom;
    let by = (SCREEN_HEIGHT * 0.5 - screen_cy) * zoom + sample.foff[1] * sample.scale[1] * zoom;
    set_transform_full(
        tf,
        Vec3::new(bx, by, z),
        Vec3::new(sample.scale[0] * zoom, sample.scale[1] * zoom, 1.0),
        Quat::from_rotation_z(sample.rot),
    );
    set_visibility(vis, if visible { Visibility::Visible } else { Visibility::Hidden });
}

fn sync_actor_visuals(
    actor: &mut CharacterActor,
    dt_trail_s: f64,
    cam_scroll_x: f32,
    cam_scroll_y: f32,
    zoom: f32,
    parts: &mut CharPartsQuery,
    trails: &mut CharTrailQuery,
) {
    let scale = actor.total_scale();
    let eff_flip = actor.effective_flip();
    let anim_flip = actor.controller.current_flip();
    let flip = [eff_flip[0] ^ anim_flip[0], eff_flip[1] ^ anim_flip[1]];
    let anim_off = actor.controller.current_offset();
    let alpha = (actor.slot.opacity * actor.mod_tint[3]).clamp(0.0, 1.0);
    let tint = Color::srgba(actor.mod_tint[0], actor.mod_tint[1], actor.mod_tint[2], alpha);
    let actor_visible = actor.slot.visible && actor.mod_visible;

    let mut current_sample = None;
    let cur_clip = actor.controller.current_name().and_then(|n| actor.clips.get(n)).cloned();
    match cur_clip {
        Some(ClipVisual::Sheet { source_idx }) => {
            current_sample = compute_sheet_sample(actor, source_idx, scale, flip, anim_off);
            if let Ok((mut sprite, mut tf, mut vis, aff_opt)) = parts.get_mut(actor.sheet_entity) {
                if let Some(mut aff) = aff_opt
                    && aff.active
                {
                    aff.active = false;
                }
                if let Some(ref sample) = current_sample {
                    let z = actor.mod_z.unwrap_or_else(|| stage_layer_z(actor.slot.layer, 0));
                    apply_sheet_sample_to_sprite(
                        actor,
                        sample,
                        tint,
                        z,
                        actor_visible,
                        cam_scroll_x,
                        cam_scroll_y,
                        zoom,
                        &mut sprite,
                        &mut tf,
                        &mut vis,
                    );
                } else {
                    set_visibility(&mut vis, Visibility::Hidden);
                }
            }
            hide_entities(&actor.piece_entities, parts);
        }
        Some(ClipVisual::Composite { source_idx, clip, stage_matrix }) => {
            hide_entities(&[actor.sheet_entity], parts);
            let pieces = clip.frames.get(actor.controller.frame_cursor).map(Vec::as_slice).unwrap_or(&[]);
            sync_composite_parts(
                actor, source_idx, pieces, clip.bounds_min, clip.bounds_size, stage_matrix,
                scale, flip, anim_off, tint, cam_scroll_x, cam_scroll_y, zoom, parts,
            );
        }
        None => {
            hide_entities(&[actor.sheet_entity], parts);
            hide_entities(&actor.piece_entities, parts);
        }
    }

    if let (Some(sample), Some(trail)) = (current_sample, actor.trail.as_mut())
        && actor_visible
    {
        trail.step_and_capture(dt_trail_s, sample);
    }

    if let Some(ref trail) = actor.trail {
        for (idx, &trail_ent) in actor.trail_entities.iter().enumerate() {
            let Ok((mut sprite, mut tf, mut vis)) = trails.get_mut(trail_ent) else {
                continue;
            };
            let Some(sample) = trail.samples.get(idx) else {
                set_visibility(&mut vis, Visibility::Hidden);
                continue;
            };
            let ghost_alpha = trail.ghost_alpha(idx, alpha);
            if ghost_alpha <= 0.001 || !actor_visible {
                set_visibility(&mut vis, Visibility::Hidden);
                continue;
            }
            let ghost_tint = Color::srgba(
                actor.mod_tint[0],
                actor.mod_tint[1],
                actor.mod_tint[2],
                ghost_alpha,
            );
            let z = trail.config.z_layer - (idx as f32) * 0.0001;
            apply_sheet_sample_to_sprite(
                actor,
                sample,
                ghost_tint,
                z,
                true,
                cam_scroll_x,
                cam_scroll_y,
                zoom,
                &mut sprite,
                &mut tf,
                &mut vis,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_composite_parts(
    actor: &CharacterActor,
    source_idx: usize,
    pieces: &[CompositePiece],
    bounds_min: [f32; 2],
    bounds_size: [f32; 2],
    stage_matrix: Option<[f32; 6]>,
    scale: [f32; 2],
    flip: [bool; 2],
    anim_off: [f32; 2],
    tint: Color,
    cam_scroll_x: f32,
    cam_scroll_y: f32,
    zoom: f32,
    parts: &mut CharPartsQuery,
) {
    let Some(src) = actor.sources.get(source_idx) else {
        hide_entities(&actor.piece_entities, parts);
        return;
    };
    let screen_fx = actor.slot.position[0]
        + actor.data.offsets[0] * (1.0 + scale[0])
        - anim_off[0] * scale[0]
        - cam_scroll_x * actor.slot.scroll_factor[0];
    let screen_fy = actor.slot.position[1]
        + actor.data.offsets[1] * (1.0 + scale[1])
        - anim_off[1] * scale[1]
        - cam_scroll_y * actor.slot.scroll_factor[1];
    let top_left = [
        screen_fx - actor.idle_size[0] * scale[0] * 0.5,
        screen_fy - actor.idle_size[1] * scale[1],
    ];
    let atlas_settings = actor.data.effective_atlas_settings();
    let params = FlxAnimateTransformParams {
        bounds_min,
        bounds_size,
        top_left,
        scale,
        angle_deg: 0.0,
        flip,
        stage_matrix,
        apply_stage_matrix: atlas_settings.apply_stage_matrix,
        post_stage_matrix_apply: atlas_settings.post_stage_matrix_apply,
        screen_size: [SCREEN_WIDTH, SCREEN_HEIGHT],
        zoom,
    };
    let visible = if actor.slot.visible && actor.mod_visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };

    for (i, &ent) in actor.piece_entities.iter().enumerate() {
        let Ok((mut sprite, _, mut vis, aff_opt)) = parts.get_mut(ent) else {
            continue;
        };
        let Some(mut aff) = aff_opt else {
            continue;
        };
        let Some(piece) = pieces.get(i) else {
            if aff.active {
                aff.active = false;
            }
            set_visibility(&mut vis, Visibility::Hidden);
            continue;
        };
        let ([bx, by], cols) = compose_flx_piece_bevy_affine(piece, &params);
        let base_z = actor.mod_z.unwrap_or_else(|| stage_layer_z(actor.slot.layer, 0));
        let pos = Vec3::new(bx, by, composite_piece_z(base_z, i));
        let (tex, layout) = src.atlas_pair(piece.spritemap_idx);
        set_sprite_atlas(&mut sprite, tex, layout, piece.sprite_index);
        set_sprite_color(&mut sprite, modulate_color(tint, piece.color));
        set_sprite_flip(&mut sprite, false, false);
        aff.active = true;
        aff.cols = cols;
        aff.translation = pos;
        set_visibility(&mut vis, visible);
    }
}

fn hide_entities(entities: &[Entity], parts: &mut CharPartsQuery) {
    for &ent in entities {
        if let Ok((_, _, mut vis, aff_opt)) = parts.get_mut(ent) {
            if let Some(mut aff) = aff_opt
                && aff.active
            {
                aff.active = false;
            }
            set_visibility(&mut vis, Visibility::Hidden);
        }
    }
}
