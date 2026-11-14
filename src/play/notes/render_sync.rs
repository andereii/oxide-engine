use bevy::prelude::*;

use crate::animations::AnimationController;
use crate::play::scene::PlayState;

use super::noteskin::NoteStyle;
use super::strumline::{ReceptorAnimState, Strumline};
use super::sustain_trail::compute_sustain_geometry;
use super::visuals::{
    NoteVisual, NoteskinAssets, ReceptorVisual, SustainBodyVisual, SustainEndVisual,
};

fn strumline(state: &PlayState, is_player: bool) -> &Strumline {
    if is_player {
        &state.player_strumline
    } else {
        &state.opponent_strumline
    }
}

pub fn sync_arrow_visuals(
    state: Res<PlayState>,
    skin_assets: Option<Res<NoteskinAssets>>,
    mut receptors: Query<(&ReceptorVisual, &mut Transform, &mut AnimationController)>,
    mut notes: Query<(&NoteVisual, &mut Transform, &mut Visibility), Without<ReceptorVisual>>,
    mut sustain_bodies: Query<
        (&SustainBodyVisual, &mut Sprite, &mut Transform, &mut Visibility),
        (Without<ReceptorVisual>, Without<NoteVisual>),
    >,
    mut sustain_ends: Query<
        (&SustainEndVisual, &mut Sprite, &mut Transform, &mut Visibility),
        (
            Without<ReceptorVisual>,
            Without<NoteVisual>,
            Without<SustainBodyVisual>,
        ),
    >,
) {
    let style = skin_assets
        .as_ref()
        .map(|s| &s.style)
        .unwrap_or(&state.player_strumline.note_style);

    sync_receptors(&state, &mut receptors);
    let pos_ms = state.conductor.song_position_ms;
    sync_note_heads(&state, style, pos_ms, &mut notes);
    sync_sustain_bodies(&state, style, pos_ms, &mut sustain_bodies);
    sync_sustain_ends(&state, style, pos_ms, &mut sustain_ends);
}

fn sync_receptors(
    state: &PlayState,
    receptors: &mut Query<(&ReceptorVisual, &mut Transform, &mut AnimationController)>,
) {
    for (rec, mut transform, mut anim) in receptors.iter_mut() {
        let strum = strumline(state, rec.is_player);
        transform.translation.x = strum.lane_x(rec.lane);
        transform.translation.y = strum.receptor_y();

        match strum.anim_state[rec.lane] {
            ReceptorAnimState::Confirm => anim.play("confirm", false, false, 0),
            ReceptorAnimState::Press => anim.play("press", false, false, 0),
            ReceptorAnimState::Static => anim.play("static", false, false, 0),
        }
    }
}

fn sync_note_heads(
    state: &PlayState,
    style: &NoteStyle,
    pos_ms: f64,
    notes: &mut Query<(&NoteVisual, &mut Transform, &mut Visibility), Without<ReceptorVisual>>,
) {
    for (vis, mut transform, mut visibility) in notes.iter_mut() {
        let strum = strumline(state, vis.is_player);
        let Some(note) = strum.notes.iter().find(|n| n.data.id == vis.note_id) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        if note.has_been_hit {
            *visibility = Visibility::Hidden;
            continue;
        }
        let y = strum.note_y_at(note.data.time_ms, pos_ms) + style.notes_offset[1];
        if y < -500.0 || y > 500.0 {
            *visibility = Visibility::Hidden;
            continue;
        }
        *visibility = Visibility::Visible;
        transform.translation.x = strum.lane_x(vis.lane) + style.notes_offset[0];
        transform.translation.y = y;
    }
}

fn sync_sustain_bodies(
    state: &PlayState,
    style: &NoteStyle,
    pos_ms: f64,
    bodies: &mut Query<
        (&SustainBodyVisual, &mut Sprite, &mut Transform, &mut Visibility),
        (Without<ReceptorVisual>, Without<NoteVisual>),
    >,
) {
    for (vis, mut sprite, mut transform, mut visibility) in bodies.iter_mut() {
        let strum = strumline(state, vis.is_player);
        let Some(note) = strum.notes.iter().find(|n| n.data.id == vis.note_id) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let geom = compute_sustain_geometry(note, strum, style, pos_ms, vis.end_raw_h);
        if !geom.visible {
            *visibility = Visibility::Hidden;
            continue;
        }
        let x_offset = if note.hold_hit {
            style.sustains_offset_hit[0]
        } else {
            style.sustains_offset[0]
        };
        *visibility = Visibility::Visible;
        transform.translation.x = strum.lane_x(vis.lane) + x_offset;
        transform.translation.y = geom.body_y;
        sprite.flip_y = geom.flip_y;
        sprite.color = Color::srgba(1.0, 1.0, 1.0, geom.alpha);
        sprite.custom_size = Some(Vec2::new(
            vis.raw_width_px,
            (geom.body_height_px / style.sustains_scale.max(0.01)).max(1.0),
        ));
    }
}

fn sync_sustain_ends(
    state: &PlayState,
    style: &NoteStyle,
    pos_ms: f64,
    ends: &mut Query<
        (&SustainEndVisual, &mut Sprite, &mut Transform, &mut Visibility),
        (
            Without<ReceptorVisual>,
            Without<NoteVisual>,
            Without<SustainBodyVisual>,
        ),
    >,
) {
    for (vis, mut sprite, mut transform, mut visibility) in ends.iter_mut() {
        let strum = strumline(state, vis.is_player);
        let Some(note) = strum.notes.iter().find(|n| n.data.id == vis.note_id) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let geom = compute_sustain_geometry(note, strum, style, pos_ms, vis.end_raw_h);
        if !geom.visible {
            *visibility = Visibility::Hidden;
            continue;
        }
        let x_offset = if note.hold_hit {
            style.sustains_offset_hit[0]
        } else {
            style.sustains_offset[0]
        };
        *visibility = Visibility::Visible;
        transform.translation.x = strum.lane_x(vis.lane) + x_offset + geom.end_x_offset;
        transform.translation.y = geom.end_y;
        sprite.flip_y = geom.flip_y;
        sprite.color = Color::srgba(1.0, 1.0, 1.0, geom.alpha);
    }
}
