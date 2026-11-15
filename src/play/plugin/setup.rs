use bevy::prelude::*;

use crate::audio::SongAudioPlayer;
use crate::cli::LaunchFlags;
use crate::mods::{ModRegistry, ModStore};
use crate::scripting::{HookKind, ScriptEvent, ScriptHost};
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};

use super::super::camera::hud::{hud_render_layer, spawn_hud_camera};
use super::super::characters::{load_characters_resource, spawn_characters};
use super::super::hud::countdown::load_countdown_assets;
use super::super::hud::healthbar::spawn_health_bar;
use super::super::hud::popups::load_popup_assets;
use super::super::notes::NoteStyle;
use super::super::notes::effects::spawn_effect_pools;
use super::super::notes::render::{load_noteskin_assets, spawn_strumline_and_pools};
use super::super::playlist::PlayStatePlaylist;
use super::super::scene::PlayState;
use super::super::song::{SongSelection, load_song_chart_in};
use super::super::stage::{
    StageResource, append_stage_sidecar_scripts, load_stage_resource, spawn_stage_props,
};
use super::super::types::{PlayStateParams, SongChart};
use super::{
    AudioSyncState, HudTextVisual, PendingEcsScriptCommands, PlaySfxAssets, apply_ecs_script_commands,
};

pub(super) fn setup_play_scene(
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
        let mut stage_script_source: Option<(String, std::path::PathBuf)> = None;
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
                if stage.scripts {
                    stage_script_source = Some((stage.stage_id.clone(), stage.json_path.clone()));
                }
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
            stage_script_source.as_ref(),
        );
        pending_ecs.0.extend(cmds);
    }

    commands.spawn((
        Text2d::new(""),
        TextFont { font_size: FontSize::Px(20.0), ..default() },
        bevy::sprite::Anchor::BOTTOM_LEFT,
        Transform::from_xyz(-SCREEN_WIDTH * 0.5 + 24.0, -SCREEN_HEIGHT * 0.5 + 20.0, 10.8),
        super::camera::hud::hud_render_layer(),
        HudTextVisual,
    ));
}

pub(super) fn load_and_init_scripts(
    mods: &ModRegistry,
    mod_store: &mut ModStore,
    script_host: &mut ScriptHost,
    state: &mut PlayState,
    playlist: &mut PlayStatePlaylist,
    stage_script_source: Option<&(String, std::path::PathBuf)>,
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

    let _ = mod_store.load_namespace("global");
    for mod_id in mods.active_mod_ids() {
        let _ = mod_store.load_namespace(mod_id);
    }

    let mut discovered = mods.scripts_for_context(
        &state.chart.song_id,
        (!state.chart.stage_id.is_empty()).then_some(state.chart.stage_id.as_str()),
        &char_ids,
        &note_kind_vec,
        &event_vec,
    );
    if let Some((stage_id, json_path)) = stage_script_source {
        let supported = script_host.supported_extensions();
        append_stage_sidecar_scripts(mods, stage_id, json_path, &supported, &mut discovered);
    }
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
