mod render;
mod trail;
mod underlay;

use bevy::prelude::*;

use crate::play::notes::CharNoteSignal;
use crate::play::scene::PlayState;
use crate::play::types::PlayPhase;
use crate::utils::conductor::advance_step_range;
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};

use super::behavior::CharacterActor;
use super::spawn::{CharacterPartVisual, CharacterTrailVisual, CompositeAffine2d};
use render::{ensure_pixel_sampler, sync_actor_visuals};
use underlay::{
    ParentMirrorState, capture_mirror_state, role_index, sync_underlay_actors,
};

pub(super) type CharPartsQuery<'w, 's> = Query<
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

pub(super) type CharTrailQuery<'w, 's> = Query<
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

#[allow(clippy::too_many_arguments)]
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

    let elapsed_s = if paused {
        0.0
    } else {
        f64::from(time.delta_secs())
    };
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
            mirror_states[idx] = Some(capture_mirror_state(&actor));
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

    if has_underlays {
        sync_underlay_actors(
            &mirror_states,
            cam_scroll_x,
            cam_scroll_y,
            zoom,
            &mut images,
            &mut actors,
            &mut parts,
            &mut trails,
        );
    }
}
