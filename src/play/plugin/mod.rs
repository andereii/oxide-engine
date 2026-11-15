mod hud_audio;
mod input;
mod script_commands;
mod setup;
mod tick;

use hud_audio::sync_hud_text;
use input::handle_play_input;
use setup::setup_play_scene;
use tick::tick_play_state;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::animations::AnimationController;
use crate::audio::SongAudioPlayer;
use crate::cli::LaunchFlags;
use crate::input::{Controls, CursorState, PreciseInputManager};
use crate::mods::{ModRegistry, ModStore};
use crate::scripting::{ScriptCommand, ScriptHost};

use super::camera::hud::sync_hud_camera;
use super::characters::{
    CharacterActor, CharacterRole, CharacterSyncState, apply_composite_affine_transforms,
    sync_characters,
};
use super::hud::countdown::sync_countdown;
use super::hud::healthbar::sync_health_bar;
use super::hud::popups::sync_popups;
use super::notes::NoteStyle;
use super::notes::effects::sync_note_effects;
use super::notes::render::sync_arrow_visuals;
use super::playlist::PlayStatePlaylist;
use super::scene::PlayState;
use super::song::{SongSelection, load_song_chart_in};
use super::stage::{DynamicScriptProp, StagePropVisual, StageSyncState, sync_stage_props};
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
pub(super) struct PlaySfxAssets {
    pub(super) miss_sounds: Vec<Handle<AudioSource>>,
    miss_volume: f32,
    pub(super) miss_cursor: usize,
}

#[derive(Resource, Default)]
pub(super) struct PendingEcsScriptCommands(pub(super) Vec<ScriptCommand>);

#[derive(SystemParam)]
pub(super) struct ScriptTickParams<'w, 's> {
    pub(super) asset_server: Res<'w, AssetServer>,
    pub(super) mods: Res<'w, ModRegistry>,
    pub(super) mod_store: ResMut<'w, ModStore>,
    pub(super) script_host: ResMut<'w, ScriptHost>,
    pub(super) pending_ecs: ResMut<'w, PendingEcsScriptCommands>,
    pub(super) pending_shaders: ResMut<'w, super::shaders::systems::PendingShaderCommands>,
    pub(super) actors: Query<'w, 's, &'static mut CharacterActor>,
    pub(super) props: Query<
        'w,
        's,
        (
            Entity,
            &'static mut StagePropVisual,
            &'static mut Sprite,
            Option<&'static mut AnimationController>,
            Option<&'static DynamicScriptProp>,
        ),
    >,
}

impl Plugin for PlayPlugin {
    fn build(&self, app: &mut App) {
        let mods = ModRegistry::from_default_dirs();
        let mod_store = ModStore::with_root(&mods.mods_dir);
        let script_host = ScriptHost::with_default_backends();

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
        let (chart, note_style, load_error) = match load_song_chart_in(
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
        };

        let mut state = PlayState::new(PlayStateParams {
            chart,
            note_style,
            practice_mode: self.flags.practice_mode,
            bot_play_mode: self.flags.bot_play_mode,
            minimal_mode: self.flags.minimal_mode,
            start_timestamp_ms: self.flags.start_timestamp_ms,
            playback_rate: self.flags.playback_rate,
            ..default()
        });
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
            .insert_resource(selection)
            .insert_resource(controls)
            .insert_resource(precise_input)
            .insert_resource(CursorState::default())
            .insert_resource(PlayStatePlaylist::default())
            .insert_resource(SongAudioPlayer::default())
            .insert_resource(PlaySfxAssets::default())
            .insert_resource(StageSyncState::default())
            .insert_resource(CharacterSyncState::default())
            .insert_resource(AudioSyncState {
                was_countdown: state.is_in_countdown,
                was_paused: false,
            })
            .insert_resource(state)
            .add_plugins(super::shaders::FunkinShadersPlugin)
            .add_systems(Startup, setup_play_scene)
            .add_systems(Update, (handle_play_input, tick_play_state).chain().before(crate::animations::sprite::step_animations))
            .add_systems(Update, (sync_stage_props, sync_characters, sync_arrow_visuals, sync_note_effects, sync_countdown, sync_popups, sync_health_bar, sync_hud_camera, sync_hud_text).chain().after(crate::animations::sprite::step_animations))
            .add_systems(PostUpdate, apply_composite_affine_transforms.after(TransformSystems::Propagate));
    }
}

pub(crate) fn actor_matches_target(actor: &CharacterActor, target: &str) -> bool {
    let lower = target.trim().to_ascii_lowercase();
    match lower.as_str() {
        "bf" | "boyfriend" | "player" | "0" => actor.role == CharacterRole::Player,
        "dad" | "opponent" | "1" => actor.role == CharacterRole::Opponent,
        "gf" | "girlfriend" | "spectator" | "2" => actor.role == CharacterRole::Spectator,
        _ => actor.id.eq_ignore_ascii_case(target.trim()),
    }
}
