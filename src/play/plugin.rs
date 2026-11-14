use bevy::audio::{AudioPlayer, AudioSink, PlaybackMode, PlaybackSettings, Volume};
use bevy::prelude::*;

use crate::audio::SongAudioPlayer;
use crate::input::{Action, Controls, CursorState, PreciseInputManager};

use super::notes::NoteStyle;
use super::notes::effects::{spawn_effect_pools, sync_note_effects};
use super::notes::render_sync::sync_arrow_visuals;
use super::notes::visuals::{load_noteskin_assets, spawn_strumline_and_pools};
use super::playlist::PlayStatePlaylist;
use super::popups::{load_popup_assets, sync_popups};
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

#[derive(Resource, Default)]
struct PlaySfxAssets {
    miss_sounds: Vec<Handle<AudioSource>>,
    miss_volume: f32,
    countdown_sounds: [Option<Handle<AudioSource>>; 4],
    countdown_volume: f32,
    miss_cursor: usize,
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
            .insert_resource(PlaySfxAssets::default())
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
                    sync_note_effects,
                    sync_popups,
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
    mut state: ResMut<PlayState>,
    mut audio: ResMut<SongAudioPlayer>,
    mut sfx: ResMut<PlaySfxAssets>,
) {
    let style = &state.player_strumline.note_style;
    sfx.miss_sounds = style
        .miss_sounds
        .iter()
        .map(|rel| asset_server.load(NoteStyle::miss_sound_asset(rel)))
        .collect();
    sfx.miss_volume = style.miss_volume;
    sfx.countdown_volume = style.countdown_volume;
    for (idx, path_opt) in style.countdown_sounds.iter().enumerate() {
        sfx.countdown_sounds[idx] = path_opt
            .as_ref()
            .map(|p| asset_server.load(style.asset_path(p)));
    }

    if !state.critical_failure {
        commands.insert_resource(load_popup_assets(&state.player_strumline.note_style, &asset_server));
        audio.load_for_chart(&mut commands, &asset_server, &state.chart, state.playback_rate);
        match load_noteskin_assets(&state.player_strumline.note_style, &asset_server, &mut layouts) {
            Ok(skin_assets) => {
                spawn_strumline_and_pools(&mut commands, &state, &skin_assets);
                spawn_effect_pools(&mut commands, &skin_assets);
                commands.insert_resource(skin_assets);
            }
            Err(err) => {
                state.status_message = format!("Noteskin error: {err}");
            }
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
        state.on_key_press_precise(ev.direction, Some(ev.key_code), ev.song_position_ms, latency);
    }
    for ev in precise.drain_releases() {
        let latency = precise.latency_ms(&ev);
        state.on_key_release_precise(ev.direction, Some(ev.key_code), ev.song_position_ms, latency);
    }
}

fn tick_play_state(
    mut commands: Commands,
    time: Res<Time>,
    mut state: ResMut<PlayState>,
    mut playlist: ResMut<PlayStatePlaylist>,
    mut audio: ResMut<SongAudioPlayer>,
    mut sync: ResMut<AudioSyncState>,
    mut sfx: ResMut<PlaySfxAssets>,
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

    if let Some(step) = state.pending_countdown_step.take() {
        if let Some(handle) = sfx.countdown_sounds.get(step).and_then(|h| h.clone()) {
            spawn_one_shot(&mut commands, handle, sfx.countdown_volume);
        }
    }

    if state.pending_miss_sfx {
        state.pending_miss_sfx = false;
        if !sfx.miss_sounds.is_empty() {
            let idx = sfx.miss_cursor % sfx.miss_sounds.len();
            sfx.miss_cursor = sfx.miss_cursor.wrapping_add(1);
            spawn_one_shot(&mut commands, sfx.miss_sounds[idx].clone(), sfx.miss_volume);
        }
    }

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

fn spawn_one_shot(commands: &mut Commands, handle: Handle<AudioSource>, volume: f32) {
    commands.spawn((
        AudioPlayer(handle),
        PlaybackSettings {
            mode: PlaybackMode::Despawn,
            volume: Volume::Linear(volume.clamp(0.0, 2.0)),
            ..default()
        },
    ));
}

fn sync_hud_text(state: Res<PlayState>, mut hud_text: Query<&mut Text, With<HudTextVisual>>) {
    for mut text in &mut hud_text {
        **text = format!("{}  |  {}", state.score_text, state.status_message);
    }
}
