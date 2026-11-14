use bevy::prelude::*;

use crate::animations::AnimationController;
use crate::play::scene::PlayState;
use crate::utils::constants::{HIT_WINDOW_MS, KEY_COUNT};

use super::strumline::Strumline;
use super::visuals::{NoteVisual, NoteskinAssets, ReceptorVisual, SustainBodyVisual, SustainEndVisual};

pub fn warm_pool_counts(strum: &Strumline) -> (usize, usize) {
    let window = strum.render_distance_ms() + HIT_WINDOW_MS;
    let mut note_start = 0usize;
    let mut live_notes = 0usize;
    let mut peak_notes = 0usize;
    let mut hold_ends: Vec<f64> = Vec::new();
    let mut peak_holds = 0usize;

    for (i, note) in strum.notes.iter().enumerate() {
        while note_start < i {
            if strum.notes[note_start].data.time_ms + window >= note.data.time_ms {
                break;
            }
            live_notes = live_notes.saturating_sub(1);
            note_start += 1;
        }
        live_notes += 1;
        peak_notes = peak_notes.max(live_notes);
        if note.is_hold_note() {
            hold_ends.retain(|&end_ms| end_ms >= note.data.time_ms);
            hold_ends.push(note.data.time_ms + note.data.sustain_length_ms + window);
            peak_holds = peak_holds.max(hold_ends.len());
        }
    }

    let note_cap = ((peak_notes as f64 * 1.25).ceil() as usize + 4).clamp(16, 128);
    let hold_cap = if peak_holds > 0 {
        ((peak_holds as f64 * 1.25).ceil() as usize + 2).clamp(8, 64)
    } else {
        0
    };
    (note_cap, hold_cap)
}

pub fn spawn_strumline_and_pools(
    commands: &mut Commands,
    state: &PlayState,
    assets: &NoteskinAssets,
) {
    let style = &assets.style;
    for (is_player, strum) in [
        (false, &state.opponent_strumline),
        (true, &state.player_strumline),
    ] {
        spawn_receptors(commands, strum, assets, is_player);
        let (note_cap, hold_cap) = warm_pool_counts(strum);
        for _ in 0..note_cap {
            let mut sprite = Sprite::from_atlas_image(
                assets.notes_texture.clone(),
                TextureAtlas {
                    layout: assets.notes_layout.clone(),
                    index: assets.note_indices[0],
                },
            );
            sprite.color = Color::srgba(1.0, 1.0, 1.0, style.notes_alpha);
            commands.spawn((
                sprite,
                Transform::from_xyz(0.0, -900.0, 2.0).with_scale(Vec3::splat(style.notes_scale)),
                Visibility::Hidden,
                NoteVisual { is_player, lane: 0, assigned_idx: None, assigned_id: 0 },
            ));
        }
        if let (Some(s_tex), Some(s_layout)) = (&assets.sustain_texture, &assets.sustain_layout) {
            let (body_idx, end_idx, body_raw_w, end_raw_h) = assets.sustain_indices[0];
            for _ in 0..hold_cap {
                let mut body_sprite = Sprite::from_atlas_image(
                    s_tex.clone(),
                    TextureAtlas { layout: s_layout.clone(), index: body_idx },
                );
                body_sprite.color = Color::srgba(1.0, 1.0, 1.0, style.sustains_alpha);
                commands.spawn((
                    body_sprite,
                    Transform::from_xyz(0.0, -900.0, 1.0).with_scale(Vec3::splat(style.sustains_scale)),
                    Visibility::Hidden,
                    SustainBodyVisual {
                        is_player, lane: 0, assigned_idx: None, assigned_id: 0,
                        raw_width_px: body_raw_w, end_raw_h,
                    },
                ));

                let mut end_sprite = Sprite::from_atlas_image(
                    s_tex.clone(),
                    TextureAtlas { layout: s_layout.clone(), index: end_idx },
                );
                end_sprite.color = Color::srgba(1.0, 1.0, 1.0, style.sustains_alpha);
                commands.spawn((
                    end_sprite,
                    Transform::from_xyz(0.0, -900.0, 1.0).with_scale(Vec3::splat(style.sustains_scale)),
                    Visibility::Hidden,
                    SustainEndVisual {
                        is_player, lane: 0, assigned_idx: None, assigned_id: 0,
                        end_raw_w: body_raw_w, end_raw_h,
                    },
                ));
            }
        }
    }
}

fn spawn_receptors(
    commands: &mut Commands,
    strum: &Strumline,
    assets: &NoteskinAssets,
    is_player: bool,
) {
    let style = &assets.style;
    let rec_y = strum.receptor_y();
    for lane in 0..KEY_COUNT {
        let (st, pr, cf, ch) = &assets.strum_frames[lane];
        let static_idx = st.first().copied().unwrap_or(0);
        let mut anim = AnimationController::default();
        anim.add_anim("static", st.clone(), 24.0, true);
        anim.add_anim("press", pr.clone(), 24.0, false);
        anim.add_anim("confirm", cf.clone(), 24.0, false);
        anim.add_anim("confirm-hold", ch.clone(), 24.0, true);
        anim.play("static", true, false, 0);

        let mut sprite = Sprite::from_atlas_image(
            assets.strum_texture.clone(),
            TextureAtlas { layout: assets.strum_layout.clone(), index: static_idx },
        );
        sprite.color = Color::srgba(1.0, 1.0, 1.0, style.strum_alpha);
        commands.spawn((
            sprite,
            Transform::from_xyz(strum.lane_x(lane), rec_y, 0.0)
                .with_scale(Vec3::splat(style.strum_scale * strum.strumline_scale)),
            anim,
            ReceptorVisual { lane, is_player },
        ));
    }
}

