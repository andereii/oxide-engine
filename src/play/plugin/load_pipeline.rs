use std::collections::VecDeque;
use std::path::PathBuf;

use bevy::prelude::*;

use crate::ui::{LoadingScreenConfig, LoadingScreenState};
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};

use super::super::characters::{
    CharacterRole, CharactersResource, LoadedCharacter, load_single_character, spawn_characters,
};
use super::super::hud::countdown::load_countdown_assets;
use super::super::hud::healthbar::spawn_health_bar;
use super::super::hud::popups::{load_popup_assets, spawn_popup_pool};
use super::super::notes::effects::spawn_effect_pools;
use super::super::notes::render::{load_noteskin_assets, spawn_strumline_and_pools};
use super::super::playlist::PlayStatePlaylist;
use super::super::preload::PlayPreloadTracker;
use super::super::scene::PlayState;
use super::super::stage::schema::{StageCharacterSlot, StagePropData};
use super::super::stage::{
    StageResource, begin_stage_load, resolve_stage_prop, spawn_stage_props,
};
use super::hud_audio::prewarm_sfx_handles;
use super::script_commands::apply_ecs_script_commands;
use super::setup::{load_and_init_scripts, seed_stage_and_chars_into_state};
use super::{AudioTickParams, ScriptHudAndTextParams, ScriptTickParams};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) enum PlayLoadStep {
    #[default]
    Idle,
    /// Wait for the loading screen UI assets (`mic-stand.png` + character) to appear first.
    WaitLoadingUi,
    HudAndNoteskin,
    WaitHudAndNoteskin,
    StageBegin,
    StageNextProp,
    WaitStageProp,
    CharactersBegin,
    CharacterNext,
    WaitCharacter,
    Audio,
    Scripts,
    WaitAll,
    /// Brief hold at 100% so the progress bar finishes sliding before starting the countdown.
    HoldComplete(f32),
}

/// Phased loader for `PlayScene` that staggers heavy asset decoding across frames
/// behind the global black loading screen to avoid memory spikes.
#[derive(Resource, Default)]
pub(crate) struct PlayLoadPipeline {
    pub(crate) step: PlayLoadStep,
    loaded_stage: Option<StageResource>,
    pending_props: VecDeque<StagePropData>,
    total_props: usize,
    pending_chars: VecDeque<(String, CharacterRole, StageCharacterSlot)>,
    loaded_chars: Vec<LoadedCharacter>,
    total_chars: usize,
    stage_script_source: Option<(String, PathBuf)>,
    char_script_sources: Vec<(String, PathBuf)>,
}

impl PlayLoadPipeline {
    fn reset_for_load(&mut self) {
        self.step = PlayLoadStep::WaitLoadingUi;
        self.loaded_stage = None;
        self.pending_props.clear();
        self.total_props = 0;
        self.pending_chars.clear();
        self.loaded_chars.clear();
        self.total_chars = 0;
        self.stage_script_source = None;
        self.char_script_sources.clear();
    }
}

pub(crate) fn begin_play_scene_load(
    state: &mut PlayState,
    preload: Option<&mut PlayPreloadTracker>,
    loading_screen: &mut LoadingScreenState,
    pipeline: &mut PlayLoadPipeline,
) {
    if let Some(tr) = preload {
        tr.clear();
    }
    state.enter_loading_phase();
    if state.critical_failure {
        pipeline.step = PlayLoadStep::Idle;
        loading_screen.hide();
        return;
    }
    loading_screen.show(LoadingScreenConfig::default());
    pipeline.reset_for_load();
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn step_play_load_pipeline(
    dt: f32,
    commands: &mut Commands,
    state: &mut PlayState,
    playlist: &mut PlayStatePlaylist,
    audio_res: &mut AudioTickParams,
    scripts: &mut ScriptTickParams,
    hud_texts: &mut ScriptHudAndTextParams,
    loading_screen: &mut LoadingScreenState,
    pipeline: &mut PlayLoadPipeline,
) {
    match pipeline.step {
        PlayLoadStep::Idle => {}
        PlayLoadStep::WaitLoadingUi => {
            if loading_screen.is_ui_ready() {
                pipeline.step = PlayLoadStep::HudAndNoteskin;
            }
        }
        PlayLoadStep::HudAndNoteskin => {
            load_hud_and_noteskin_step(commands, state, audio_res, scripts);
            pipeline.step = PlayLoadStep::WaitHudAndNoteskin;
        }
        PlayLoadStep::WaitHudAndNoteskin => {
            if step_assets_ready(scripts, loading_screen, 0.0, 0.15) {
                pipeline.step = if state.is_minimal_mode {
                    PlayLoadStep::Audio
                } else {
                    PlayLoadStep::StageBegin
                };
            }
        }
        PlayLoadStep::StageBegin => {
            begin_stage_step(state, scripts, pipeline);
        }
        PlayLoadStep::StageNextProp => {
            load_next_stage_prop_step(commands, scripts, pipeline);
        }
        PlayLoadStep::WaitStageProp => {
            let frac = stage_progress_fraction(pipeline);
            if step_assets_ready(scripts, loading_screen, frac.0, frac.1) {
                pipeline.step = PlayLoadStep::StageNextProp;
            }
        }
        PlayLoadStep::CharactersBegin => {
            begin_characters_step(state, pipeline);
        }
        PlayLoadStep::CharacterNext => {
            load_next_character_step(commands, state, scripts, pipeline);
        }
        PlayLoadStep::WaitCharacter => {
            let frac = character_progress_fraction(pipeline);
            if step_assets_ready(scripts, loading_screen, frac.0, frac.1) {
                pipeline.step = PlayLoadStep::CharacterNext;
            }
        }
        PlayLoadStep::Audio => {
            load_audio_step(commands, state, audio_res, scripts);
            pipeline.step = PlayLoadStep::Scripts;
        }
        PlayLoadStep::Scripts => {
            load_scripts_step(commands, state, playlist, audio_res, scripts, hud_texts, pipeline);
            pipeline.step = PlayLoadStep::WaitAll;
        }
        PlayLoadStep::WaitAll => {
            finish_or_poll_all(
                commands,
                state,
                audio_res,
                scripts,
                hud_texts,
                loading_screen,
                pipeline,
            );
        }
        PlayLoadStep::HoldComplete(remaining) => {
            let next = remaining - dt.min(0.1);
            if next <= 0.0 {
                pipeline.step = PlayLoadStep::Idle;
                loading_screen.hide();
                state.start_countdown();
                audio_res.sync.was_countdown = true;
                audio_res.sync.was_paused = false;
            } else {
                pipeline.step = PlayLoadStep::HoldComplete(next);
            }
        }
    }
}

fn step_assets_ready(
    scripts: &mut ScriptTickParams,
    loading_screen: &mut LoadingScreenState,
    min_frac: f32,
    max_frac: f32,
) -> bool {
    let Some(tr) = scripts.preload.as_deref_mut() else {
        loading_screen.set_progress(max_frac);
        return true;
    };
    let progress = tr.poll(&scripts.asset_server, false);
    let local = if progress.total_assets == 0 {
        1.0
    } else {
        (progress.loaded_assets as f32 / progress.total_assets as f32).clamp(0.0, 1.0)
    };
    loading_screen.set_progress(min_frac + (max_frac - min_frac) * local);
    progress.loaded_assets >= progress.total_assets
}

fn stage_progress_fraction(pipeline: &PlayLoadPipeline) -> (f32, f32) {
    if pipeline.total_props == 0 {
        return (0.15, 0.40);
    }
    let done = pipeline.total_props.saturating_sub(pipeline.pending_props.len());
    let span = 0.25 / pipeline.total_props as f32;
    let start = 0.15 + span * done.saturating_sub(1) as f32;
    let end = 0.15 + span * done as f32;
    (start, end)
}

fn character_progress_fraction(pipeline: &PlayLoadPipeline) -> (f32, f32) {
    if pipeline.total_chars == 0 {
        return (0.40, 0.70);
    }
    let done = pipeline.total_chars.saturating_sub(pipeline.pending_chars.len());
    let span = 0.30 / pipeline.total_chars as f32;
    let start = 0.40 + span * done.saturating_sub(1) as f32;
    let end = 0.40 + span * done as f32;
    (start, end)
}

fn load_hud_and_noteskin_step(
    commands: &mut Commands,
    state: &mut PlayState,
    audio_res: &mut AudioTickParams,
    scripts: &mut ScriptTickParams,
) {
    let style = &state.player_strumline.note_style;
    audio_res.sfx.miss_sounds = style
        .miss_sound_assets_in(&scripts.mods)
        .into_iter()
        .map(|rel| scripts.asset_server.load(rel))
        .collect();
    audio_res.sfx.miss_volume = style.miss_volume;
    audio_res.sfx.miss_cursor = 0;
    if let Some(tr) = scripts.preload.as_deref_mut() {
        tr.register_sfx(&audio_res.sfx.miss_sounds);
    }

    let countdown_assets =
        load_countdown_assets(&scripts.mods, style, &scripts.asset_server);
    if let Some(tr) = scripts.preload.as_deref_mut() {
        tr.register_countdown(&countdown_assets);
    }
    commands.insert_resource(countdown_assets);

    let popup_assets = load_popup_assets(&scripts.mods, style, &scripts.asset_server);
    if let Some(tr) = scripts.preload.as_deref_mut() {
        tr.register_popups(&popup_assets);
    }
    spawn_popup_pool(commands, &popup_assets);
    commands.insert_resource(popup_assets);

    match load_noteskin_assets(
        &scripts.mods,
        &state.player_strumline.note_style,
        &scripts.asset_server,
        &mut scripts.layouts,
    ) {
        Ok(skin_assets) => {
            if let Some(tr) = scripts.preload.as_deref_mut() {
                tr.register_noteskin(&skin_assets);
            }
            spawn_strumline_and_pools(commands, state, &skin_assets);
            spawn_effect_pools(commands, &skin_assets);
            commands.insert_resource(skin_assets);
        }
        Err(err) => {
            state.status_message = format!("Noteskin error: {err}");
        }
    }
}

fn begin_stage_step(
    state: &mut PlayState,
    scripts: &ScriptTickParams,
    pipeline: &mut PlayLoadPipeline,
) {
    let stage_id = state.chart.stage_id.clone();
    match begin_stage_load(&scripts.mods, &stage_id) {
        Ok((stage, ordered_props)) => {
            state.camera.stage_zoom = f64::from(stage.camera_zoom);
            if state.camera.follow_point == [0.0, 0.0] {
                state.camera.follow_point = [
                    f64::from(SCREEN_WIDTH) * 0.5,
                    f64::from(SCREEN_HEIGHT) * 0.5,
                ];
            }
            state.reset_camera(true, true, true);
            if stage.scripts {
                pipeline.stage_script_source =
                    Some((stage.stage_id.clone(), stage.json_path.clone()));
            }
            pipeline.total_props = ordered_props.len();
            pipeline.pending_props = ordered_props.into();
            pipeline.loaded_stage = Some(stage);
            pipeline.step = PlayLoadStep::StageNextProp;
        }
        Err(err) => {
            state.status_message = format!("Stage error: {err}");
            pipeline.loaded_stage = None;
            pipeline.pending_props.clear();
            pipeline.step = PlayLoadStep::CharactersBegin;
        }
    }
}

fn load_next_stage_prop_step(
    commands: &mut Commands,
    scripts: &mut ScriptTickParams,
    pipeline: &mut PlayLoadPipeline,
) {
    let Some(prop_data) = pipeline.pending_props.pop_front() else {
        if let Some(ref stage) = pipeline.loaded_stage {
            spawn_stage_props(commands, stage);
        }
        pipeline.step = PlayLoadStep::CharactersBegin;
        return;
    };

    if let Some(ref mut stage) = pipeline.loaded_stage
        && let Some(resolved) = resolve_stage_prop(
            &scripts.mods,
            &stage.stage_id,
            prop_data,
            &scripts.asset_server,
            &mut scripts.layouts,
        )
    {
        if let Some(tr) = scripts.preload.as_deref_mut() {
            tr.register_stage_prop(&resolved);
        }
        stage.props.push(resolved);
    }
    pipeline.step = PlayLoadStep::WaitStageProp;
}

fn begin_characters_step(state: &PlayState, pipeline: &mut PlayLoadPipeline) {
    let slots = pipeline.loaded_stage.as_ref().map(|s| &s.characters);
    let meta = &state.chart.characters;

    let groups = [
        (
            &meta.spectator,
            CharacterRole::Spectator,
            slots
                .map(|s| s.spectator.clone())
                .unwrap_or_else(StageCharacterSlot::default_for_spectator),
        ),
        (
            &meta.opponents,
            CharacterRole::Opponent,
            slots
                .map(|s| s.opponent.clone())
                .unwrap_or_else(StageCharacterSlot::default_for_opponent),
        ),
        (
            &meta.players,
            CharacterRole::Player,
            slots
                .map(|s| s.player.clone())
                .unwrap_or_else(StageCharacterSlot::default_for_player),
        ),
    ];

    pipeline.pending_chars.clear();
    pipeline.loaded_chars.clear();
    for (ids, role, slot) in groups {
        for char_id in ids {
            pipeline
                .pending_chars
                .push_back((char_id.clone(), role, slot.clone()));
        }
    }
    pipeline.total_chars = pipeline.pending_chars.len();
    pipeline.step = PlayLoadStep::CharacterNext;
}

fn load_next_character_step(
    commands: &mut Commands,
    state: &mut PlayState,
    scripts: &mut ScriptTickParams,
    pipeline: &mut PlayLoadPipeline,
) {
    let Some((char_id, role, slot)) = pipeline.pending_chars.pop_front() else {
        let chars = CharactersResource {
            characters: std::mem::take(&mut pipeline.loaded_chars),
        };
        seed_stage_and_chars_into_state(state, pipeline.loaded_stage.as_ref(), &chars);
        if let Some(pt) = state.camera.focus_points[1]
            .or(state.camera.focus_points[2])
            .or(state.camera.focus_points[0])
        {
            state.camera.follow_point = pt;
            state.camera.follow_target = Some(pt);
        }
        let initial_events: Vec<_> = state
            .song_events
            .iter()
            .filter(|ev| ev.time_ms <= 0.0)
            .cloned()
            .collect();
        for ev in &initial_events {
            state.apply_due_song_event(ev);
        }
        if let Some(target) = state.camera.follow_target {
            state.camera.follow_point = target;
        }
        spawn_characters(commands, &chars);
        spawn_health_bar(
            &scripts.mods,
            commands,
            Some(&chars),
            &state.player_strumline.note_style,
            &scripts.asset_server,
            &mut scripts.layouts,
        );
        commands.insert_resource(chars);
        if let Some(stage) = pipeline.loaded_stage.take() {
            commands.insert_resource(stage);
        }
        pipeline.step = PlayLoadStep::Audio;
        return;
    };

    if let Ok(loaded) = load_single_character(
        &scripts.mods,
        &char_id,
        role,
        slot,
        &scripts.asset_server,
        &mut scripts.layouts,
    ) {
        if loaded.data.script {
            pipeline
                .char_script_sources
                .push((loaded.id.clone(), loaded.json_path.clone()));
        }
        if let Some(tr) = scripts.preload.as_deref_mut() {
            tr.register_character(&loaded);
        }
        pipeline.loaded_chars.push(loaded);
    }
    pipeline.step = PlayLoadStep::WaitCharacter;
}

fn load_audio_step(
    commands: &mut Commands,
    state: &PlayState,
    audio_res: &mut AudioTickParams,
    scripts: &mut ScriptTickParams,
) {
    audio_res.audio.load_for_chart_with_mods(
        &scripts.mods,
        commands,
        &scripts.asset_server,
        &state.chart,
        state.playback_rate,
    );
    if let Some(tr) = scripts.preload.as_deref_mut() {
        tr.register_song_audio(&audio_res.audio);
    }
}

fn load_scripts_step(
    commands: &mut Commands,
    state: &mut PlayState,
    playlist: &mut PlayStatePlaylist,
    audio_res: &mut AudioTickParams,
    scripts: &mut ScriptTickParams,
    hud_texts: &mut ScriptHudAndTextParams,
    pipeline: &PlayLoadPipeline,
) {
    let cmds = load_and_init_scripts(
        &scripts.mods,
        &mut scripts.mod_store,
        &mut scripts.script_host,
        state,
        playlist,
        pipeline.stage_script_source.as_ref(),
        &pipeline.char_script_sources,
    );
    scripts.pending_ecs.0.extend(cmds);
    flush_pending_script_cmds(commands, audio_res, scripts, hud_texts);
}

fn flush_pending_script_cmds(
    commands: &mut Commands,
    audio_res: &mut AudioTickParams,
    scripts: &mut ScriptTickParams,
    hud_texts: &mut ScriptHudAndTextParams,
) {
    if !scripts.pending_ecs.0.is_empty() {
        let init_cmds: Vec<_> = scripts.pending_ecs.0.drain(..).collect();
        apply_ecs_script_commands(commands, audio_res, scripts, hud_texts, &init_cmds);
    }
}

fn finish_or_poll_all(
    commands: &mut Commands,
    state: &mut PlayState,
    audio_res: &mut AudioTickParams,
    scripts: &mut ScriptTickParams,
    hud_texts: &mut ScriptHudAndTextParams,
    loading_screen: &mut LoadingScreenState,
    pipeline: &mut PlayLoadPipeline,
) {
    flush_pending_script_cmds(commands, audio_res, scripts, hud_texts);
    prewarm_sfx_handles(
        &audio_res.sfx.miss_sounds,
        &audio_res.audio_sources,
        &mut audio_res.pcm_sfx,
        &mut audio_res.sfx_cache,
    );
    prewarm_sfx_handles(
        &audio_res.sfx.preloaded_sounds,
        &audio_res.audio_sources,
        &mut audio_res.pcm_sfx,
        &mut audio_res.sfx_cache,
    );

    let Some(ref mut preload) = scripts.preload else {
        return;
    };
    let progress = preload.poll(&scripts.asset_server, audio_res.audio.is_ready());
    loading_screen.set_progress(0.7 + 0.3 * progress.fraction());
    state.status_message = format!(
        "Loading... {:.0}% ({}/{})",
        progress.fraction() * 100.0,
        progress.loaded_assets,
        progress.total_assets,
    );
    if progress.is_complete() {
        loading_screen.set_progress(1.0);
        pipeline.step = PlayLoadStep::HoldComplete(0.35);
    }
}
