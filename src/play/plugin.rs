use std::collections::BTreeSet;

use bevy::audio::{AudioPlayer, AudioSink, PlaybackMode, PlaybackSettings, Volume};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::animations::AnimationController;
use crate::audio::SongAudioPlayer;
use crate::input::{Action, Controls, CursorState, PreciseInputManager};
use crate::mods::{ModRegistry, ModStore};
use crate::scripting::{
    HookKind, LogLevel, ScriptCommand, ScriptEvent, ScriptHost, ScriptValue, TweenProp,
};
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};

use super::characters::{
    CharacterActor, CharacterRole, CharacterSyncState, apply_composite_affine_transforms,
    load_characters_resource, spawn_characters, sync_characters,
};
use super::camera::hud::{hud_render_layer, spawn_hud_camera, sync_hud_camera};
use super::hud::countdown::{load_countdown_assets, sync_countdown};
use super::hud::healthbar::{spawn_health_bar, sync_health_bar};
use super::hud::popups::{load_popup_assets, sync_popups};
use super::notes::NoteStyle;
use super::notes::effects::{spawn_effect_pools, sync_note_effects};
use super::notes::render::{load_noteskin_assets, spawn_strumline_and_pools, sync_arrow_visuals};
use super::playlist::PlayStatePlaylist;
use super::scene::PlayState;
use super::song::{SongSelection, load_song_chart_in};
use super::stage::schema::StageCoordMode;
use super::stage::{
    DynamicScriptProp, StagePropVisual, StageSyncState, load_stage_resource, spawn_stage_props,
    sync_stage_props,
};
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
    miss_cursor: usize,
}

#[derive(Resource, Default)]
struct PendingEcsScriptCommands(Vec<ScriptCommand>);

#[derive(SystemParam)]
struct ScriptTickParams<'w, 's> {
    asset_server: Res<'w, AssetServer>,
    mods: Res<'w, ModRegistry>,
    mod_store: ResMut<'w, ModStore>,
    script_host: ResMut<'w, ScriptHost>,
    pending_ecs: ResMut<'w, PendingEcsScriptCommands>,
    actors: Query<'w, 's, &'static mut CharacterActor>,
    props: Query<
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

        let selection = SongSelection::default();
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
            .add_systems(Startup, setup_play_scene)
            .add_systems(Update, (handle_play_input, tick_play_state).chain().before(crate::animations::sprite::step_animations))
            .add_systems(Update, (sync_stage_props, sync_characters, sync_arrow_visuals, sync_note_effects, sync_countdown, sync_popups, sync_health_bar, sync_hud_camera, sync_hud_text).chain().after(crate::animations::sprite::step_animations))
            .add_systems(PostUpdate, apply_composite_affine_transforms.after(TransformSystems::Propagate));
    }
}

#[allow(clippy::too_many_arguments)]
fn setup_play_scene(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mods: Res<ModRegistry>,
    mut mod_store: ResMut<ModStore>,
    mut script_host: ResMut<ScriptHost>,
    mut pending_ecs: ResMut<PendingEcsScriptCommands>,
    mut state: ResMut<PlayState>,
    mut playlist: ResMut<PlayStatePlaylist>,
    mut audio: ResMut<SongAudioPlayer>,
    mut sfx: ResMut<PlaySfxAssets>,
) {
    let style = &state.player_strumline.note_style;
    sfx.miss_sounds = style
        .miss_sound_assets_in(&mods)
        .into_iter()
        .map(|rel| asset_server.load(rel))
        .collect();
    sfx.miss_volume = style.miss_volume;

    spawn_hud_camera(&mut commands);

    if !state.critical_failure {
        if !state.is_minimal_mode {
            let stage_id = state.chart.stage_id.clone();
            let loaded_stage = match load_stage_resource(&mods, &stage_id, &asset_server, &mut layouts) {
                Ok(stage) => {
                    state.camera.stage_zoom = f64::from(stage.camera_zoom);
                    if state.camera.follow_point == [0.0, 0.0] {
                        state.camera.follow_point = [
                            f64::from(SCREEN_WIDTH) * 0.5,
                            f64::from(SCREEN_HEIGHT) * 0.5,
                        ];
                    }
                    state.reset_camera(true, true, true);
                    for prop in &stage.props {
                        state
                            .script_prop_positions
                            .insert(prop.data.name.clone(), prop.data.position);
                        state
                            .script_prop_scales
                            .insert(prop.data.name.clone(), prop.data.scale);
                        state
                            .script_prop_angles
                            .insert(prop.data.name.clone(), prop.data.angle);
                        state
                            .script_prop_alphas
                            .insert(prop.data.name.clone(), prop.data.opacity);
                    }
                    spawn_stage_props(&mut commands, &stage);
                    Some(stage)
                }
                Err(err) => {
                    state.status_message = format!("Stage error: {err}");
                    None
                }
            };
            let chars = load_characters_resource(
                &mods,
                &state.chart.characters,
                loaded_stage.as_ref(),
                &asset_server,
                &mut layouts,
            );
            for c in &chars.characters {
                state
                    .script_char_positions
                    .insert(c.id.clone(), c.slot.position);
                let alias = match c.role {
                    CharacterRole::Player => "bf",
                    CharacterRole::Opponent => "dad",
                    CharacterRole::Spectator => "gf",
                };
                state
                    .script_char_positions
                    .entry(alias.to_string())
                    .or_insert(c.slot.position);
            }
            spawn_characters(&mut commands, &chars);
            spawn_health_bar(
                &mods,
                &mut commands,
                Some(&chars),
                &state.player_strumline.note_style,
                &asset_server,
                &mut layouts,
            );
            commands.insert_resource(chars);
            if let Some(stage) = loaded_stage {
                commands.insert_resource(stage);
            }
        }
        commands.insert_resource(load_countdown_assets(
            &mods,
            &state.player_strumline.note_style,
            &asset_server,
        ));
        commands.insert_resource(load_popup_assets(
            &mods,
            &state.player_strumline.note_style,
            &asset_server,
        ));
        audio.load_for_chart_with_mods(
            &mods,
            &mut commands,
            &asset_server,
            &state.chart,
            state.playback_rate,
        );
        match load_noteskin_assets(
            &mods,
            &state.player_strumline.note_style,
            &asset_server,
            &mut layouts,
        ) {
            Ok(skin_assets) => {
                spawn_strumline_and_pools(&mut commands, &state, &skin_assets);
                spawn_effect_pools(&mut commands, &skin_assets);
                commands.insert_resource(skin_assets);
            }
            Err(err) => {
                state.status_message = format!("Noteskin error: {err}");
            }
        }

        let cmds = load_and_init_scripts(
            &mods,
            &mut mod_store,
            &mut script_host,
            &mut state,
            &mut playlist,
        );
        pending_ecs.0.extend(cmds);
    }

    commands.spawn((
        Text::new(""),
        TextFont { font_size: FontSize::Px(20.0), ..default() },
        Node { position_type: PositionType::Absolute, left: Val::Px(24.0), bottom: Val::Px(20.0), ..default() },
        HudTextVisual,
    ));
}

fn load_and_init_scripts(
    mods: &ModRegistry,
    mod_store: &mut ModStore,
    script_host: &mut ScriptHost,
    state: &mut PlayState,
    playlist: &mut PlayStatePlaylist,
) -> Vec<ScriptCommand> {
    let mut char_ids: Vec<&str> = Vec::new();
    for id in &state.chart.characters.players {
        char_ids.push(id.as_str());
    }
    for id in &state.chart.characters.opponents {
        char_ids.push(id.as_str());
    }
    for id in &state.chart.characters.spectator {
        char_ids.push(id.as_str());
    }

    let mut note_kinds = BTreeSet::new();
    for note in state
        .chart
        .player_notes
        .iter()
        .chain(state.chart.opponent_notes.iter())
    {
        if let Some(kind) = &note.kind
            && !kind.trim().is_empty()
        {
            note_kinds.insert(kind.as_str());
        }
    }
    let note_kind_vec: Vec<&str> = note_kinds.into_iter().collect();

    let mut event_names = BTreeSet::new();
    for ev in &state.chart.events {
        if !ev.kind.trim().is_empty() {
            event_names.insert(ev.kind.as_str());
        }
    }
    let event_vec: Vec<&str> = event_names.into_iter().collect();

    let discovered = mods.scripts_for_context(
        &state.chart.song_id,
        (!state.chart.stage_id.is_empty()).then_some(state.chart.stage_id.as_str()),
        &char_ids,
        &note_kind_vec,
        &event_vec,
    );
    script_host.load_discovered(&discovered);
    for err in &script_host.compile_errors {
        warn!("Script compile error: {err}");
    }

    let init_hooks = vec![
        (HookKind::Create, ScriptEvent::new(HookKind::Create)),
        (
            HookKind::SongLoaded,
            ScriptEvent::new(HookKind::SongLoaded)
                .with_field("song", state.chart.song_id.clone())
                .with_field("bpm", state.conductor.bpm())
                .with_field(
                    "notes",
                    ScriptValue::Int(
                        (state.player_strumline.notes.len()
                            + state.opponent_strumline.notes.len()) as i64,
                    ),
                ),
        ),
    ];
    state.run_script_hooks(
        init_hooks,
        false,
        0.0,
        playlist,
        script_host,
        mods,
        mod_store,
    )
}

#[allow(clippy::too_many_arguments)]
fn handle_play_input(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    controls: Res<Controls>,
    mut precise: ResMut<PreciseInputManager>,
    mut mods: ResMut<ModRegistry>,
    mut mod_store: ResMut<ModStore>,
    mut script_host: ResMut<ScriptHost>,
    mut pending_ecs: ResMut<PendingEcsScriptCommands>,
    mut state: ResMut<PlayState>,
    mut playlist: ResMut<PlayStatePlaylist>,
    dyn_props: Query<Entity, With<DynamicScriptProp>>,
) {
    if controls.just_pressed(&keys, Action::ReloadMods) {
        for entity in &dyn_props {
            commands.entity(entity).despawn();
        }
        state.apply_state_script_command(&ScriptCommand::ResetLanes, &mut playlist);
        state.custom_score_text = None;
        state.hud_visible = true;
        if let Err(err) = mods.reload() {
            warn!("Mod reload warning: {err}");
        }
        for (_, err) in &mods.disabled_mods {
            warn!("Disabled mod during reload: {err}");
        }
        let cmds = load_and_init_scripts(
            &mods,
            &mut mod_store,
            &mut script_host,
            &mut state,
            &mut playlist,
        );
        pending_ecs.0.extend(cmds);
        state.status_message = format!(
            "Reloaded {} mod(s), {} script(s)",
            mods.active_mod_ids().len(),
            script_host.script_count()
        );
        return;
    }

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
    mut scripts: ScriptTickParams,
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

    for actor in &scripts.actors {
        state
            .script_char_positions
            .insert(actor.id.clone(), actor.slot.position);
        let alias = match actor.role {
            CharacterRole::Player => "bf",
            CharacterRole::Opponent => "dad",
            CharacterRole::Spectator => "gf",
        };
        state
            .script_char_positions
            .entry(alias.to_string())
            .or_insert(actor.slot.position);
    }
    for (_, vis, sprite, _, _) in &scripts.props {
        state
            .script_prop_positions
            .insert(vis.name.clone(), vis.base_position);
        state
            .script_prop_scales
            .insert(vis.name.clone(), vis.base_scale);
        state
            .script_prop_angles
            .insert(vis.name.clone(), vis.base_angle_deg);
        state
            .script_prop_alphas
            .insert(vis.name.clone(), sprite.color.alpha());
    }

    let dt = f64::from(time.delta_secs()).min(0.1);
    let master_ms = audio.master_time_ms(&sinks);
    let mut ecs_cmds: Vec<ScriptCommand> = scripts.pending_ecs.0.drain(..).collect();
    let frame_cmds = state.update_with_scripts(
        dt,
        master_ms,
        &mut playlist,
        &mut scripts.script_host,
        &scripts.mods,
        &mut scripts.mod_store,
    );
    ecs_cmds.extend(frame_cmds);
    apply_ecs_script_commands(
        &mut commands,
        &scripts.asset_server,
        &scripts.mods,
        &mut scripts.actors,
        &mut scripts.props,
        &ecs_cmds,
    );

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
        let sync_ms = (state.conductor.song_position_ms - state.conductor.combined_offset()).max(0.0);
        audio.tick(sync_ms, dt, &mut sinks);
    }
}

fn actor_matches_target(actor: &CharacterActor, target: &str) -> bool {
    let lower = target.trim().to_ascii_lowercase();
    match lower.as_str() {
        "bf" | "boyfriend" | "player" | "0" => actor.role == CharacterRole::Player,
        "dad" | "opponent" | "1" => actor.role == CharacterRole::Opponent,
        "gf" | "girlfriend" | "spectator" | "2" => actor.role == CharacterRole::Spectator,
        _ => actor.id.eq_ignore_ascii_case(target.trim()),
    }
}

fn apply_ecs_script_commands(
    commands: &mut Commands,
    asset_server: &AssetServer,
    mods: &ModRegistry,
    actors: &mut Query<&mut CharacterActor>,
    props: &mut Query<(
        Entity,
        &mut StagePropVisual,
        &mut Sprite,
        Option<&mut AnimationController>,
        Option<&DynamicScriptProp>,
    )>,
    cmds: &[ScriptCommand],
) {
    for cmd in cmds {
        match cmd {
            ScriptCommand::Log { level, mod_id, message } => match level {
                LogLevel::Info => info!("[mod:{mod_id}] {message}"),
                LogLevel::Warn => warn!("[mod:{mod_id}] {message}"),
                LogLevel::Error => error!("[mod:{mod_id}] {message}"),
            },
            ScriptCommand::PlayAnim { target, anim, force } => {
                let mut matched_actor = false;
                for mut actor in actors.iter_mut() {
                    if actor_matches_target(&actor, target) {
                        actor.controller.play(anim, *force, false, 0);
                        actor.hold_timer = 0.0;
                        matched_actor = true;
                    }
                }
                if !matched_actor {
                    for (_, vis, _, anim_opt, _) in props.iter_mut() {
                        if vis.name.eq_ignore_ascii_case(target)
                            && let Some(mut ctrl) = anim_opt
                        {
                            ctrl.play(anim, *force, false, 0);
                        }
                    }
                }
            }
            ScriptCommand::SetCharacterPos { target, x, y } => {
                for mut actor in actors.iter_mut() {
                    if actor_matches_target(&actor, target) {
                        actor.slot.position = [*x, *y];
                        actor.recompute_camera_focus();
                    }
                }
            }
            ScriptCommand::SetCharacterScale { target, sx, sy } => {
                for mut actor in actors.iter_mut() {
                    if actor_matches_target(&actor, target) {
                        actor.slot.scale = [*sx, *sy];
                    }
                }
            }
            ScriptCommand::SetCharacterTint { target, color } => {
                for mut actor in actors.iter_mut() {
                    if actor_matches_target(&actor, target) {
                        actor.mod_tint = *color;
                    }
                }
            }
            ScriptCommand::SetCharacterVisible { target, visible } => {
                for mut actor in actors.iter_mut() {
                    if actor_matches_target(&actor, target) {
                        actor.mod_visible = *visible;
                    }
                }
            }
            ScriptCommand::SetCharacterDanceEvery { target, beats } => {
                for mut actor in actors.iter_mut() {
                    if actor_matches_target(&actor, target) {
                        actor.bopper.dance_every = *beats;
                    }
                }
            }
            ScriptCommand::SpawnProp { id, image_path, color, size, pos, z, hud } => {
                for (entity, _, _, _, dyn_opt) in props.iter() {
                    if dyn_opt.is_some_and(|d| d.id == *id) {
                        commands.entity(entity).despawn();
                    }
                }
                let mut sprite = match image_path {
                    Some(rel) => {
                        let resolved = mods
                            .resolve_asset_server_path(rel)
                            .unwrap_or_else(|| rel.clone());
                        let mut spr = Sprite::from_image(asset_server.load(resolved));
                        spr.color = Color::srgba(color[0], color[1], color[2], color[3]);
                        if size[0] > 0.0 && size[1] > 0.0 {
                            spr.custom_size = Some(Vec2::new(size[0], size[1]));
                        }
                        spr
                    }
                    None => Sprite::from_color(
                        Color::srgba(color[0], color[1], color[2], color[3]),
                        Vec2::new(size[0].max(1.0), size[1].max(1.0)),
                    ),
                };
                sprite.color = Color::srgba(color[0], color[1], color[2], color[3]);
                let vis = StagePropVisual {
                    name: id.clone(),
                    base_position: *pos,
                    base_scale: [1.0, 1.0],
                    scroll_factor: if *hud { [0.0, 0.0] } else { [1.0, 1.0] },
                    base_flip: [false, false],
                    base_angle_deg: 0.0,
                    z_layer: *z,
                    visible: true,
                    coord_mode: StageCoordMode::Center,
                    raw_size: *size,
                };
                let mut ec = commands.spawn((
                    sprite,
                    Transform::from_xyz(0.0, 0.0, *z),
                    Visibility::Visible,
                    vis,
                    DynamicScriptProp {
                        id: id.clone(),
                        hud: *hud,
                    },
                ));
                if *hud {
                    ec.insert(hud_render_layer());
                }
            }
            ScriptCommand::DespawnProp(id) => {
                for (entity, vis, _, _, dyn_opt) in props.iter() {
                    if dyn_opt.is_some_and(|d| d.id == *id) || vis.name == *id {
                        commands.entity(entity).despawn();
                    }
                }
            }
            ScriptCommand::SetPropNumber { id, prop, value } => {
                for (_, mut vis, mut sprite, _, _) in props.iter_mut() {
                    if vis.name == *id {
                        match prop {
                            TweenProp::X => vis.base_position[0] = *value,
                            TweenProp::Y => vis.base_position[1] = *value,
                            TweenProp::ScaleX => vis.base_scale[0] = *value,
                            TweenProp::ScaleY => vis.base_scale[1] = *value,
                            TweenProp::Angle => vis.base_angle_deg = *value,
                            TweenProp::Alpha => {
                                sprite.color = sprite.color.with_alpha(value.clamp(0.0, 1.0));
                            }
                            _ => {}
                        }
                    }
                }
            }
            ScriptCommand::SetPropTint { id, color } => {
                for (_, vis, mut sprite, _, _) in props.iter_mut() {
                    if vis.name == *id {
                        sprite.color = Color::srgba(color[0], color[1], color[2], color[3]);
                    }
                }
            }
            ScriptCommand::SetPropVisible { id, visible } => {
                for (_, mut vis, _, _, _) in props.iter_mut() {
                    if vis.name == *id {
                        vis.visible = *visible;
                    }
                }
            }
            ScriptCommand::PlaySfx { path, volume } => {
                let resolved = mods
                    .resolve_asset_server_path(path)
                    .unwrap_or_else(|| path.clone());
                let handle: Handle<AudioSource> = asset_server.load(resolved);
                spawn_one_shot(commands, handle, *volume);
            }
            _ => {}
        }
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

fn sync_hud_text(
    state: Res<PlayState>,
    mut cache: Local<(String, String, bool)>,
    mut hud_text: Query<(&mut Text, &mut Visibility), With<HudTextVisual>>,
) {
    if cache.0 == state.score_text && cache.1 == state.status_message && cache.2 == state.hud_visible {
        return;
    }
    cache.0.clone_from(&state.score_text);
    cache.1.clone_from(&state.status_message);
    cache.2 = state.hud_visible;
    let combined = format!("{}  |  {}", cache.0, cache.1);
    let vis = if state.hud_visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for (mut text, mut visibility) in &mut hud_text {
        **text = combined.clone();
        if *visibility != vis {
            *visibility = vis;
        }
    }
}
