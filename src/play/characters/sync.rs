use std::f32::consts::FRAC_PI_2;

use bevy::image::ImageSampler;
use bevy::prelude::*;

use crate::animations::atlas_composite::{CompositePiece, upright_size};
use crate::play::notes::CharNoteSignal;
use crate::play::scene::PlayState;
use crate::play::stage::schema::stage_layer_z;
use crate::play::types::PlayPhase;
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};

use super::behavior::{CharacterActor, ClipVisual};
use super::spawn::CharacterPartVisual;

#[derive(Resource)]
pub struct CharacterSyncState {
    pub last_step: Option<i32>,
    pub last_song_pos_ms: Option<f64>,
}

impl Default for CharacterSyncState {
    fn default() -> Self {
        Self {
            last_step: None,
            last_song_pos_ms: None,
        }
    }
}

pub fn sync_characters(
    time: Res<Time>,
    mut state: ResMut<PlayState>,
    mut sync_state: ResMut<CharacterSyncState>,
    mut images: ResMut<Assets<Image>>,
    mut actors: Query<&mut CharacterActor>,
    mut parts: Query<(&mut Sprite, &mut Transform, &mut Visibility), With<CharacterPartVisual>>,
) {
    let player_signals: Vec<CharNoteSignal> = state.player_strumline.pending_char_events.drain(..).collect();
    let opponent_signals: Vec<CharNoteSignal> = state.opponent_strumline.pending_char_events.drain(..).collect();

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
    let step_changed = if did_reset {
        sync_state.last_step = Some(current_step);
        false
    } else if !paused && sync_state.last_step != Some(current_step) {
        sync_state.last_step = Some(current_step);
        true
    } else {
        false
    };

    let elapsed_s = if paused { 0.0 } else { f64::from(time.delta_secs()) };
    let song_pos_ms = (state.phase == PlayPhase::Playing).then_some(current_pos_ms);
    let step_len_ms = state.conductor.step_length_ms().max(1.0);
    let player_holding = !state.is_bot_play_mode && state.player_strumline.has_any_held_key();

    let cam_scroll_x = state.camera.follow_point[0] as f32 - SCREEN_WIDTH * 0.5;
    let cam_scroll_y = state.camera.follow_point[1] as f32 - SCREEN_HEIGHT * 0.5;
    let zoom = (state.camera.current_zoom * state.camera.bop_multiplier).max(0.05) as f32;

    for mut actor in &mut actors {
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

        if did_reset {
            actor.reset_character();
        } else if !paused {
            actor.on_update(elapsed_s, song_pos_ms, step_len_ms, player_holding);
            for sig in &player_signals {
                actor.handle_player_signal(sig, &state.note_kinds);
            }
            for sig in &opponent_signals {
                actor.handle_opponent_signal(sig, &state.note_kinds);
            }
            if step_changed {
                actor.on_step_hit(current_step);
            }
        }

        sync_actor_visuals(&actor, cam_scroll_x, cam_scroll_y, zoom, &mut parts);
    }
}

fn sync_actor_visuals(
    actor: &CharacterActor,
    cam_scroll_x: f32,
    cam_scroll_y: f32,
    zoom: f32,
    parts: &mut Query<(&mut Sprite, &mut Transform, &mut Visibility), With<CharacterPartVisual>>,
) {
    let scale = actor.total_scale();
    let eff_flip = actor.effective_flip();
    let anim_flip = actor.controller.current_flip();
    let flip = [eff_flip[0] ^ anim_flip[0], eff_flip[1] ^ anim_flip[1]];
    let anim_off = actor.controller.current_offset();
    let tint = Color::srgba(1.0, 1.0, 1.0, actor.slot.opacity.clamp(0.0, 1.0));

    let cur_clip = actor.controller.current_name().and_then(|n| actor.clips.get(n));
    match cur_clip {
        Some(ClipVisual::Sheet { source_idx }) => {
            if let Ok((mut sprite, mut tf, mut vis)) = parts.get_mut(actor.sheet_entity) {
                sync_sheet_part(
                    actor,
                    *source_idx,
                    scale,
                    flip,
                    anim_off,
                    tint,
                    cam_scroll_x,
                    cam_scroll_y,
                    zoom,
                    &mut sprite,
                    &mut tf,
                    &mut vis,
                );
            }
            hide_entities(&actor.piece_entities, parts);
        }
        Some(ClipVisual::Composite { source_idx, clip, feet_anchor }) => {
            hide_entities(&[actor.sheet_entity], parts);
            let pieces = clip.frames.get(actor.controller.frame_cursor).map(Vec::as_slice).unwrap_or(&[]);
            sync_composite_parts(
                actor,
                *source_idx,
                pieces,
                *feet_anchor,
                scale,
                flip,
                anim_off,
                tint,
                cam_scroll_x,
                cam_scroll_y,
                zoom,
                parts,
            );
        }
        None => {
            hide_entities(&[actor.sheet_entity], parts);
            hide_entities(&actor.piece_entities, parts);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_sheet_part(
    actor: &CharacterActor,
    source_idx: usize,
    scale: [f32; 2],
    flip: [bool; 2],
    anim_off: [f32; 2],
    tint: Color,
    cam_scroll_x: f32,
    cam_scroll_y: f32,
    zoom: f32,
    sprite: &mut Sprite,
    tf: &mut Transform,
    vis: &mut Visibility,
) {
    let Some(src) = actor.sources.get(source_idx) else {
        *vis = Visibility::Hidden;
        return;
    };
    let frame_idx = actor.controller.current_atlas_index().unwrap_or(0);
    sprite.image = src.texture.clone();
    sprite.texture_atlas = Some(TextureAtlas {
        layout: src.layout.clone(),
        index: frame_idx,
    });
    sprite.color = tint;
    sprite.flip_x = flip[0];
    sprite.flip_y = flip[1];

    let frame_sz = src.frame_sizes.get(&frame_idx).copied().unwrap_or(actor.idle_size);
    let mut world_cx = actor.slot.position[0] + (frame_sz[0] - actor.idle_size[0]) * 0.5 * scale[0] + actor.data.offsets[0];
    let mut world_cy = actor.slot.position[1] - (actor.idle_size[1] - frame_sz[1] * 0.5) * scale[1] + actor.data.offsets[1];
    world_cx -= anim_off[0] * scale[0];
    world_cy -= anim_off[1] * scale[1];

    let screen_cx = world_cx - cam_scroll_x * actor.slot.scroll_factor[0];
    let screen_cy = world_cy - cam_scroll_y * actor.slot.scroll_factor[1];
    let frame_off = actor.controller.current_frame_offset();
    let foff_x = if flip[0] { -frame_off[0] } else { frame_off[0] };
    let foff_y = if flip[1] { -frame_off[1] } else { frame_off[1] };

    let bx = (screen_cx - SCREEN_WIDTH * 0.5) * zoom + foff_x * scale[0] * zoom;
    let by = (SCREEN_HEIGHT * 0.5 - screen_cy) * zoom + foff_y * scale[1] * zoom;
    tf.translation = Vec3::new(bx, by, stage_layer_z(actor.slot.layer, 0));
    tf.scale = Vec3::new(scale[0] * zoom, scale[1] * zoom, 1.0);
    let rot = if actor.controller.current_frame_rotated() { -FRAC_PI_2 } else { 0.0 };
    tf.rotation = Quat::from_rotation_z(rot);
    *vis = if actor.slot.visible { Visibility::Visible } else { Visibility::Hidden };
}

#[allow(clippy::too_many_arguments)]
fn sync_composite_parts(
    actor: &CharacterActor,
    source_idx: usize,
    pieces: &[CompositePiece],
    feet_anchor: [f32; 2],
    scale: [f32; 2],
    flip: [bool; 2],
    anim_off: [f32; 2],
    tint: Color,
    cam_scroll_x: f32,
    cam_scroll_y: f32,
    zoom: f32,
    parts: &mut Query<(&mut Sprite, &mut Transform, &mut Visibility), With<CharacterPartVisual>>,
) {
    let Some(src) = actor.sources.get(source_idx) else {
        hide_entities(&actor.piece_entities, parts);
        return;
    };
    let screen_fx = actor.slot.position[0] + actor.data.offsets[0] - anim_off[0] * scale[0]
        - cam_scroll_x * actor.slot.scroll_factor[0];
    let screen_fy = actor.slot.position[1] + actor.data.offsets[1] - anim_off[1] * scale[1]
        - cam_scroll_y * actor.slot.scroll_factor[1];
    let bevy_fx = (screen_fx - SCREEN_WIDTH * 0.5) * zoom;
    let bevy_fy = (SCREEN_HEIGHT * 0.5 - screen_fy) * zoom;
    let flip_sx = if flip[0] { -1.0 } else { 1.0 };
    let flip_sy = if flip[1] { -1.0 } else { 1.0 };

    for (i, &ent) in actor.piece_entities.iter().enumerate() {
        let Ok((mut sprite, mut tf, mut vis)) = parts.get_mut(ent) else {
            continue;
        };
        let Some(piece) = pieces.get(i) else {
            *vis = Visibility::Hidden;
            continue;
        };
        let (w, h) = upright_size(piece);
        let m = piece.matrix;
        let fcx = m[0] * (w * 0.5) + m[2] * (h * 0.5) + m[4];
        let fcy = m[1] * (w * 0.5) + m[3] * (h * 0.5) + m[5];
        let lx = (fcx - feet_anchor[0]) * flip_sx * scale[0] * zoom;
        let ly = -(fcy - feet_anchor[1]) * flip_sy * scale[1] * zoom;

        let [mut m00, mut m01, mut m10, mut m11] = if piece.rotated {
            [-m[2], -m[0], m[3], m[1]]
        } else {
            [m[0], -m[2], -m[1], m[3]]
        };
        m00 *= flip_sx;
        m01 *= flip_sx;
        m10 *= flip_sy;
        m11 *= flip_sy;

        let det = m00 * m11 - m01 * m10;
        let (p_flip_x, c0x, c0y) = if det < 0.0 { (true, -m00, -m10) } else { (false, m00, m10) };
        let sx = c0x.hypot(c0y);
        let sy = m01.hypot(m11);
        let rot = c0y.atan2(c0x);

        sprite.image = src.texture.clone();
        sprite.texture_atlas = Some(TextureAtlas {
            layout: src.layout.clone(),
            index: piece.sprite_index,
        });
        sprite.color = tint;
        sprite.flip_x = p_flip_x;
        sprite.flip_y = false;
        tf.translation = Vec3::new(bevy_fx + lx, bevy_fy + ly, stage_layer_z(actor.slot.layer, i));
        tf.rotation = Quat::from_rotation_z(rot);
        tf.scale = Vec3::new(sx * scale[0] * zoom, sy * scale[1] * zoom, 1.0);
        *vis = if actor.slot.visible { Visibility::Visible } else { Visibility::Hidden };
    }
}

fn hide_entities(
    entities: &[Entity],
    parts: &mut Query<(&mut Sprite, &mut Transform, &mut Visibility), With<CharacterPartVisual>>,
) {
    for &ent in entities {
        if let Ok((_, _, mut vis)) = parts.get_mut(ent) {
            *vis = Visibility::Hidden;
        }
    }
}
