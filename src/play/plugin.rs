use bevy::audio::AudioSink;
use bevy::prelude::*;

use crate::animations::AnimationController;
use crate::audio::SongAudioPlayer;
use crate::input::{Action, Controls, CursorState, PreciseInputManager};

use super::notes::NoteStyle;
use super::notes::visuals::{
    NoteVisual, ReceptorVisual, SustainBodyVisual, SustainEndVisual, load_noteskin_assets,
    spawn_strumline_and_notes,
};
use super::playlist::PlayStatePlaylist;
use super::scene::PlayState;
use super::song::{SongSelection, load_song_chart};
use super::types::{PauseMode, PlayPhase, PlayStateParams, SongChart};

pub struct PlayPlugin;

#[derive(Component)]
struct HudTextVisual;

#[derive(Resource, Default)]
struct AudioSyncState {
    was_countdown: bool,
    was_paused: bool,
}

impl Plugin for PlayPlugin {
    fn build(&self, app: &mut App) {
        let selection = SongSelection::default();
        let (chart, note_style, load_error) = match load_song_chart(
            &selection.song_id,
            &selection.difficulty,
            &selection.variation,
        ) {
            Ok(chart) => {
                let style = NoteStyle::load(&chart.skins.hud).unwrap_or_default();
                (chart, style, None)
            }
            Err(err) => (SongChart::default(), NoteStyle::default(), Some(err)),
        };

        let mut state = PlayState::new(PlayStateParams {
            chart,
            note_style,
            ..default()
        });
        if let Some(err) = load_error {
            state.critical_failure = true;
            state.status_message = err;
        }

        let controls = Controls::default();
        let mut precise_input = PreciseInputManager::default();
        precise_input.initialize_keys(&controls);

        app.insert_resource(selection)
            .insert_resource(controls)
            .insert_resource(precise_input)
            .insert_resource(CursorState::default())
            .insert_resource(PlayStatePlaylist::default())
            .insert_resource(SongAudioPlayer::default())
            .insert_resource(AudioSyncState {
                was_countdown: state.is_in_countdown,
                was_paused: false,
            })
            .insert_resource(state)
            .add_systems(Startup, setup_play_scene)
            .add_systems(
                Update,
                (
                    handle_play_input,
                    tick_play_state,
                    sync_arrow_visuals,
                    sync_hud_text,
                )
                    .chain(),
            );
    }
}

fn setup_play_scene(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    state: Res<PlayState>,
    mut audio: ResMut<SongAudioPlayer>,
) {
    if !state.critical_failure {
        audio.load_for_chart(&mut commands, &asset_server, &state.chart, state.playback_rate);
        if let Ok(skin_assets) =
            load_noteskin_assets(&state.player_strumline.note_style, &asset_server, &mut layouts)
        {
            spawn_strumline_and_notes(&mut commands, &state, &skin_assets);
            commands.insert_resource(skin_assets);
        }
    }

    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(20.0),
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(24.0),
            bottom: Val::Px(20.0),
            ..default()
        },
        HudTextVisual,
    ));
}

fn handle_play_input(
    keys: Res<ButtonInput<KeyCode>>,
    controls: Res<Controls>,
    mut precise: ResMut<PreciseInputManager>,
    mut state: ResMut<PlayState>,
) {
    if controls.just_pressed(&keys, Action::Pause) {
        if matches!(state.phase, PlayPhase::Paused(_)) {
            state.resume();
        } else {
            state.pause(PauseMode::Standard);
        }
        return;
    }

    if controls.just_pressed(&keys, Action::Reset) {
        state.needs_reset = true;
        return;
    }

    let pos_ms = state.conductor.song_position_ms;
    precise.poll_keyboard(&keys, pos_ms);

    for ev in precise.drain_presses() {
        let latency = precise.latency_ms(&ev);
        state.on_key_press_precise(ev.direction, ev.song_position_ms, latency);
    }
    for ev in precise.drain_releases() {
        let latency = precise.latency_ms(&ev);
        state.on_key_release_precise(ev.direction, ev.song_position_ms, latency);
    }
}

fn tick_play_state(
    time: Res<Time>,
    mut state: ResMut<PlayState>,
    mut playlist: ResMut<PlayStatePlaylist>,
    mut audio: ResMut<SongAudioPlayer>,
    mut sync: ResMut<AudioSyncState>,
    mut sinks: Query<&mut AudioSink>,
) {
    let is_paused = matches!(
        state.phase,
        PlayPhase::Paused(_) | PlayPhase::GameOver | PlayPhase::Ended
    );
    if is_paused && !sync.was_paused {
        audio.pause(&mut sinks);
        sync.was_paused = true;
    } else if !is_paused && sync.was_paused {
        audio.resume(&mut sinks);
        sync.was_paused = false;
    }

    if state.needs_reset {
        audio.pause(&mut sinks);
        sync.was_countdown = true;
    }

    let master_ms = audio.master_time_ms(&sinks);
    state.update(f64::from(time.delta_secs()), master_ms, &mut playlist);

    if sync.was_countdown && !state.is_in_countdown && state.phase == PlayPhase::Playing {
        audio.play_from(state.start_timestamp_ms, &mut sinks);
        sync.was_countdown = false;
    }

    if state.phase == PlayPhase::Playing {
        audio.sync_volumes(
            state.instrumental_volume as f32,
            state.player_vocals_volume as f32,
            state.opponent_vocals_volume as f32,
            &mut sinks,
        );
        audio.tick(state.conductor.song_position_ms, &mut sinks);
    }
}

fn sync_arrow_visuals(
    state: Res<PlayState>,
    mut receptors: Query<(&ReceptorVisual, &mut AnimationController)>,
    mut notes: Query<(&NoteVisual, &mut Transform, &mut Visibility)>,
    mut sustain_bodies: Query<
        (&SustainBodyVisual, &mut Transform, &mut Visibility),
        Without<NoteVisual>,
    >,
    mut sustain_ends: Query<
        (&SustainEndVisual, &mut Transform, &mut Visibility),
        (Without<NoteVisual>, Without<SustainBodyVisual>),
    >,
) {
    for (rec, mut anim) in &mut receptors {
        let strum = if rec.is_player {
            &state.player_strumline
        } else {
            &state.opponent_strumline
        };
        if strum.lane_confirm_timer[rec.lane] > 0.0 {
            anim.play("confirm", false, false, 0);
        } else if strum.held_lanes[rec.lane] || strum.lane_press_timer[rec.lane] > 0.0 {
            anim.play("press", false, false, 0);
        } else {
            anim.play("static", false, false, 0);
        }
    }

    let pos_ms = state.conductor.song_position_ms;

    for (vis, mut transform, mut visibility) in &mut notes {
        let strum = if vis.is_player {
            &state.player_strumline
        } else {
            &state.opponent_strumline
        };
        let Some(note) = strum.notes.iter().find(|n| n.data.id == vis.note_id) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        if note.has_been_hit || (note.data.time_ms - pos_ms).abs() > 2000.0 {
            *visibility = Visibility::Hidden;
            continue;
        }
        *visibility = Visibility::Visible;
        transform.translation.y =
            240.0 - ((note.data.time_ms - pos_ms) * 0.45 * strum.scroll_speed) as f32;
    }

    for (vis, mut transform, mut visibility) in &mut sustain_bodies {
        let strum = if vis.is_player {
            &state.player_strumline
        } else {
            &state.opponent_strumline
        };
        let Some(note) = strum.notes.iter().find(|n| n.data.id == vis.note_id) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        if note.has_been_hit && !strum.active_holds.iter().flatten().any(|h| h.note_id == vis.note_id) {
            *visibility = Visibility::Hidden;
            continue;
        }
        let head_y = 240.0 - ((note.data.time_ms - pos_ms) * 0.45 * strum.scroll_speed) as f32;
        if head_y < -600.0 || head_y - vis.height_px > 420.0 {
            *visibility = Visibility::Hidden;
            continue;
        }
        *visibility = Visibility::Visible;
        transform.translation.y = head_y - vis.height_px * 0.5;
    }

    for (vis, mut transform, mut visibility) in &mut sustain_ends {
        let strum = if vis.is_player {
            &state.player_strumline
        } else {
            &state.opponent_strumline
        };
        let Some(note) = strum.notes.iter().find(|n| n.data.id == vis.note_id) else {
            *visibility = Visibility::Hidden;
            continue;
        };
        if note.has_been_hit && !strum.active_holds.iter().flatten().any(|h| h.note_id == vis.note_id) {
            *visibility = Visibility::Hidden;
            continue;
        }
        let head_y = 240.0 - ((note.data.time_ms - pos_ms) * 0.45 * strum.scroll_speed) as f32;
        let end_y = head_y - vis.body_height_px - vis.half_end_px;
        if end_y < -600.0 || end_y > 420.0 {
            *visibility = Visibility::Hidden;
            continue;
        }
        *visibility = Visibility::Visible;
        transform.translation.y = end_y;
    }
}

fn sync_hud_text(state: Res<PlayState>, mut hud_text: Query<&mut Text, With<HudTextVisual>>) {
    for mut text in &mut hud_text {
        **text = format!("{}  |  {}", state.score_text, state.status_message);
    }
}
