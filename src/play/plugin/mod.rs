mod capture;
mod hud_audio;
mod input;
mod load_pipeline;
mod reload;
mod script_commands;
pub(crate) mod setup;
mod tick;

use capture::{ParityCaptureState, drive_parity_capture};
use hud_audio::{sync_abot_visualizer_audio, sync_hud_text};
use input::handle_play_input;
use setup::setup_play_scene;
use tick::tick_play_state;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::audio::{OxideAudioPlugin, PcmSfxAudio, PcmSfxCache, SongAudioPlayer, SyncedSongAudio};
use crate::cli::LaunchFlags;
use crate::input::{Controls, CursorState, PreciseInputManager};
use crate::mods::{ModRegistry, ModStore};
use crate::scripting::{ScriptCommand, ScriptHost};
use crate::ui::script_errors::{
    ScriptErrorOverlay, spawn_script_error_overlay, update_script_error_overlay,
};

use super::camera::hud::sync_hud_camera;
use super::characters::{
    CharacterActor, CharacterRole, CharacterSyncState, apply_composite_affine_transforms,
    sync_characters,
};
use super::hot_reload::{HotReloadState, hot_reload_assets_system};
use super::hud::countdown::sync_countdown;
use super::hud::healthbar::sync_health_bar;
use super::hud::popups::sync_popups;
use super::hud::subtitles::sync_subtitles;
use super::notes::NoteStyle;
use super::notes::effects::sync_note_effects;
use super::notes::render::sync_arrow_visuals;
use super::pausesunscene::PauseSubScenePlugin;
use super::playlist::PlayStatePlaylist;
use super::preload::PlayPreloadTracker;
use super::scene::PlayState;
use super::song::{SongSelection, load_song_chart_in};
use super::stage::{StagePropVisual, StageSyncState, sync_script_texts, sync_stage_props};
use super::types::{PlayStateParams, SongChart};

#[derive(Default)]
pub struct PlayPlugin {
    pub flags: LaunchFlags,
}

impl PlayPlugin {
    pub fn with_flags(flags: LaunchFlags) -> Self {
        Self { flags }
    }
}

#[derive(Component)]
pub(super) struct HudTextVisual;

#[derive(Resource, Default)]
pub(super) struct AudioSyncState {
    pub(super) was_countdown: bool,
    was_paused: bool,
}

#[derive(Resource, Default)]
pub(crate) struct PlaySfxAssets {
    pub(crate) miss_sounds: Vec<Handle<AudioSource>>,
    pub(crate) preloaded_sounds: Vec<Handle<AudioSource>>,
    pub(crate) miss_volume: f32,
    pub(crate) miss_cursor: usize,
}

#[derive(Resource, Default)]
pub(crate) struct PendingEcsScriptCommands(pub(crate) Vec<ScriptCommand>);

#[derive(SystemParam)]
pub(super) struct AudioTickParams<'w, 's> {
    pub(super) audio: ResMut<'w, SongAudioPlayer>,
    pub(super) sync: ResMut<'w, AudioSyncState>,
    pub(super) sfx: ResMut<'w, PlaySfxAssets>,
    pub(super) sinks: Query<'w, 's, &'static mut AudioSink>,
    pub(super) audio_sources: Res<'w, Assets<AudioSource>>,
    pub(super) synced_songs: ResMut<'w, Assets<SyncedSongAudio>>,
    pub(super) pcm_sfx: ResMut<'w, Assets<PcmSfxAudio>>,
    pub(super) sfx_cache: ResMut<'w, PcmSfxCache>,
}

#[derive(Resource, Default)]
pub(crate) struct PendingSongReload {
    pub(crate) request: Option<(String, Option<String>, Option<String>)>,
}

#[derive(Resource, Default)]
pub(crate) struct PendingSongSeek(pub(crate) Option<f64>);

#[derive(SystemParam)]
pub(super) struct ScriptTickParams<'w, 's> {
    pub(super) asset_server: Res<'w, AssetServer>,
    pub(super) images: ResMut<'w, Assets<Image>>,
    pub(super) layouts: ResMut<'w, Assets<TextureAtlasLayout>>,
    pub(super) mods: Res<'w, ModRegistry>,
    pub(super) mod_store: ResMut<'w, ModStore>,
    pub(super) script_host: ResMut<'w, ScriptHost>,
    pub(super) pending_ecs: ResMut<'w, PendingEcsScriptCommands>,
    pub(super) pending_shaders: ResMut<'w, super::shaders::systems::PendingShaderCommands>,
    pub(super) preload: Option<ResMut<'w, PlayPreloadTracker>>,
    pub(super) actors: Query<'w, 's, (Entity, &'static mut CharacterActor)>,
    pub(super) props: script_commands::PropsQuery<'w, 's>,
    pub(super) sparrows: Query<'w, 's, &'static script_commands::ScriptSparrowData>,
}

#[derive(SystemParam)]
pub(super) struct ScriptHudAndTextParams<'w, 's> {
    pub(super) texts: script_commands::TextsQuery<'w, 's>,
    pub(super) tagged_sounds: Query<'w, 's, (Entity, &'static mut hud_audio::TaggedScriptSound)>,
    pub(super) bar_fills: Query<
        'w,
        's,
        (&'static mut super::hud::healthbar::HealthBarFillVisual, &'static mut Sprite),
        (Without<StagePropVisual>, Without<super::hud::health_icon::HealthIcon>),
    >,
    pub(super) icons: Query<
        'w,
        's,
        (
            &'static mut super::hud::health_icon::HealthIcon,
            &'static mut crate::animations::AnimationController,
            &'static mut Sprite,
        ),
        (Without<StagePropVisual>, Without<super::hud::healthbar::HealthBarFillVisual>),
    >,
    pub(super) pending_reload: ResMut<'w, PendingSongReload>,
    pub(super) pending_seek: ResMut<'w, PendingSongSeek>,
    pub(super) preloaded_icons: ResMut<'w, super::hud::health_icon::PreloadedHealthIcons>,
    pub(super) dialogue: ResMut<'w, super::cutscene::DialogueRuntime>,
    pub(super) video: ResMut<'w, super::cutscene::VideoCutsceneState>,
}

impl Plugin for PlayPlugin {
    fn build(&self, app: &mut App) {
        let mods = ModRegistry::from_default_dirs();
        let mod_store = ModStore::with_root(&mods.mods_dir);
        let script_host = ScriptHost::with_registry(mods.clone());

        let mut selection = SongSelection::default();
        if let Some(song) = &self.flags.song_id {
            selection.song_id.clone_from(song);
        }
        if let Some(diff) = &self.flags.difficulty {
            selection.difficulty.clone_from(diff);
        }
        if let Some(var) = &self.flags.variation {
            selection.variation.clone_from(var);
        }
        let (chart, note_style, load_error) = if self.flags.has_play_flags() {
            match load_song_chart_in(
                &mods,
                &selection.song_id,
                &selection.difficulty,
                &selection.variation,
            ) {
                Ok(chart) => {
                    let style = NoteStyle::load_in(&mods, &chart.skins.hud).unwrap_or_default();
                    (chart, style, None)
                }
                Err(err) => (SongChart::default(), NoteStyle::default(), Some(err)),
            }
        } else {
            (SongChart::default(), NoteStyle::default(), None)
        };

        let mut state = PlayState::new(PlayStateParams {
            chart,
            note_style,
            practice_mode: self.flags.practice_mode,
            bot_play_mode: self.flags.bot_play_mode,
            minimal_mode: self.flags.minimal_mode,
            downscroll: self.flags.downscroll.unwrap_or(false),
            loop_song: self.flags.loop_song,
            start_timestamp_ms: self.flags.start_timestamp_ms,
            playback_rate: self.flags.playback_rate,
            ..default()
        });
        if !self.flags.has_play_flags() {
            state.enter_loading_phase();
        }
        if let Some(err) = load_error {
            state.critical_failure = true;
            state.status_message = err;
        }

        let controls = Controls::default();
        let mut precise_input = PreciseInputManager::default();
        precise_input.initialize_keys(&controls);

        app.insert_resource(mods)
            .insert_resource(mod_store)
            .insert_resource(script_host)
            .insert_resource(PendingEcsScriptCommands::default())
            .insert_resource(PendingSongReload::default())
            .insert_resource(PendingSongSeek::default())
            .insert_resource(selection)
            .insert_resource(controls)
            .insert_resource(precise_input)
            .insert_resource(CursorState::default())
            .insert_resource(PlayStatePlaylist::default())
            .insert_resource(SongAudioPlayer::default())
            .insert_resource(PlaySfxAssets::default())
            .insert_resource(StageSyncState::default())
            .insert_resource(CharacterSyncState::default())
            .insert_resource(HotReloadState::default())
            .insert_resource(PlayPreloadTracker::default())
            .init_resource::<super::hud::health_icon::PreloadedHealthIcons>()
            .init_resource::<load_pipeline::PlayLoadPipeline>()
            .init_resource::<ScriptErrorOverlay>()
            .insert_resource(AudioSyncState {
                was_countdown: state.is_in_countdown,
                was_paused: false,
            })
            .insert_resource(state)
            .add_plugins((
                OxideAudioPlugin,
                PauseSubScenePlugin,
                super::cutscene::CutscenePlugin,
                super::shaders::FunkinShadersPlugin {
                    enabled: !self.flags.no_shaders,
                },
            ))
            .add_systems(Startup, (spawn_script_error_overlay, setup_play_scene))
            .add_systems(
                Update,
                (
                    manage_play_scene_transitions,
                    update_script_error_overlay,
                    handle_play_input,
                    hot_reload_assets_system,
                    tick_play_state,
                    sync_subtitles,
                    ApplyDeferred,
                )
                    .chain()
                    .before(crate::animations::sprite::step_animations)
                    .run_if(crate::ui::in_play_scene),
            )
            .add_systems(
                Update,
                (
                    sync_stage_props,
                    sync_abot_visualizer_audio,
                    sync_script_texts,
                    sync_characters,
                    sync_arrow_visuals,
                    sync_note_effects,
                    sync_countdown,
                    sync_popups,
                    sync_health_bar,
                    sync_hud_camera,
                    sync_hud_text,
                )
                    .chain()
                    .after(crate::animations::sprite::step_animations)
                    .run_if(crate::ui::in_play_scene),
            )
            .add_systems(
                PostUpdate,
                apply_composite_affine_transforms
                    .after(TransformSystems::Propagate)
                    .run_if(crate::ui::in_play_scene),
            );

        if let Some(capture_state) = ParityCaptureState::from_flags(&self.flags) {
            app.insert_resource(capture_state)
                .add_systems(Update, drive_parity_capture.after(sync_hud_text));
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn manage_play_scene_transitions(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    controls: Res<Controls>,
    mut app_scene: ResMut<crate::ui::AppScene>,
    mut state: ResMut<PlayState>,
    mut playlist: ResMut<PlayStatePlaylist>,
    mut selection: ResMut<SongSelection>,
    mut audio_res: AudioTickParams,
    mut scripts: ScriptTickParams,
    mut hud_texts: ScriptHudAndTextParams,
    reload_cleanup: reload::ReloadCleanupQuery,
    mut loading_screen: ResMut<crate::ui::LoadingScreenState>,
    mut load_pipeline: ResMut<load_pipeline::PlayLoadPipeline>,
    mut hud_cam_q: Query<
        &mut Camera,
        (With<super::camera::hud::HudCamera>, Without<IsDefaultUiCamera>),
    >,
    mut world_cam_q: Query<
        (&mut Transform, &mut Projection),
        (
            With<Camera2d>,
            With<IsDefaultUiCamera>,
            Without<super::camera::hud::HudCamera>,
            Without<CharacterActor>,
            Without<StagePropVisual>,
            Without<super::stage::DynamicScriptText>,
        ),
    >,
) {
    if let Some(launch) = app_scene.pending_launch.take() {
        setup::ensure_play_hud_spawned(
            &mut commands,
            &scripts.mods,
            &scripts.asset_server,
            &mut app_scene,
        );
        for mut cam in &mut hud_cam_q {
            cam.is_active = true;
        }
        playlist.is_story_mode = launch.story_mode;
        playlist.playlist_song_ids = launch.playlist_songs;
        playlist.campaign_score = 0;

        state.is_bot_play_mode = app_scene.options.botplay;
        state.is_practice_mode = app_scene.options.practice_mode;
        state.player_strumline.is_downscroll = app_scene.options.downscroll;
        state.opponent_strumline.is_downscroll = app_scene.options.downscroll;
        state.player_strumline.ghost_tap_enabled = app_scene.options.ghost_tap;

        reload::execute_song_reload(
            &mut commands,
            launch.song_id,
            Some(launch.difficulty),
            Some(launch.variation),
            &mut state,
            &mut playlist,
            &mut selection,
            &mut audio_res,
            &mut scripts,
            &mut hud_texts,
            &reload_cleanup,
            &mut loading_screen,
            &mut load_pipeline,
        );
        state.player_strumline.ghost_tap_enabled = app_scene.options.ghost_tap;
        return;
    }

    if state.phase == super::types::PlayPhase::Ended && !app_scene.started_from_cli {
        if playlist.is_story_mode && !playlist.playlist_song_ids.is_empty() {
            let next_id = playlist.playlist_song_ids[0].clone();
            hud_texts.pending_reload.request = Some((next_id, None, None));
            return;
        }
        let exit_pressed = controls.just_pressed(&keys, crate::input::Action::Accept)
            || controls.just_pressed(&keys, crate::input::Action::Back)
            || keys.just_pressed(KeyCode::Enter)
            || keys.just_pressed(KeyCode::Space)
            || keys.just_pressed(KeyCode::Escape);
        if exit_pressed {
            app_scene.exit_to_menu_requested = true;
        }
    }

    if app_scene.exit_to_menu_requested && !app_scene.started_from_cli {
        app_scene.exit_to_menu_requested = false;
        let _ = state.run_script_hooks(
            vec![(
                crate::scripting::HookKind::Destroy,
                crate::scripting::ScriptEvent::new(crate::scripting::HookKind::Destroy),
            )],
            false,
            0.0,
            &mut playlist,
            &mut scripts.script_host,
            &scripts.mods,
            &mut scripts.mod_store,
        );
        scripts.script_host.clear_scripts();
        scripts.pending_ecs.0.clear();
        scripts.pending_shaders.0.clear();
        scripts
            .pending_shaders
            .0
            .push(super::shaders::systems::ShaderScriptCommand::ClearShader {
                target: "__reload__".to_string(),
            });
        for (ent, _) in &hud_texts.tagged_sounds {
            commands.entity(ent).despawn();
        }
        for (ent, _, _, _, _, _, _) in &hud_texts.texts {
            commands.entity(ent).despawn();
        }
        for (ent, _, _, _, _, comp_opt) in &scripts.props {
            if let Some(comp) = comp_opt {
                comp.despawn_composite_rt(&mut commands);
            }
            commands.entity(ent).despawn();
        }
        for (ent, actor) in &scripts.actors {
            commands.entity(actor.sheet_entity).despawn();
            for &piece in &actor.piece_entities {
                commands.entity(piece).despawn();
            }
            actor.despawn_composite_rt(&mut commands);
            for &trail_ent in &actor.trail_entities {
                commands.entity(trail_ent).despawn();
            }
            commands.entity(ent).despawn();
        }
        for ent in &reload_cleanup {
            commands.entity(ent).despawn();
        }
        audio_res.audio.despawn_all(&mut commands);
        audio_res.sfx.preloaded_sounds.clear();
        hud_texts.dialogue.queue_cleanup();
        hud_texts.video.queue_cleanup();
        loading_screen.hide();
        state.enter_loading_phase();
        for mut cam in &mut hud_cam_q {
            cam.is_active = false;
        }
        for (mut tf, mut proj) in &mut world_cam_q {
            tf.translation = Vec3::ZERO;
            tf.rotation = Quat::IDENTITY;
            if let Projection::Orthographic(ref mut ortho) = *proj {
                ortho.scale = 1.0;
            }
        }
        app_scene.current = crate::ui::ActiveScene::MainMenu;
    }
}

pub(crate) fn actor_matches_target(actor: &CharacterActor, target: &str) -> bool {
    if actor.mirror_of.is_some() {
        return false;
    }
    let lower = target.trim().to_ascii_lowercase();
    match lower.as_str() {
        "bf" | "boyfriend" | "player" | "0" => actor.role == CharacterRole::Player,
        "dad" | "opponent" | "1" => actor.role == CharacterRole::Opponent,
        "gf" | "girlfriend" | "spectator" | "2" => actor.role == CharacterRole::Spectator,
        _ => actor.id.eq_ignore_ascii_case(target.trim()),
    }
}
