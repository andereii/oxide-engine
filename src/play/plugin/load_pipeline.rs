use std::path::PathBuf;

use bevy::prelude::*;

use crate::ui::{LoadingScreenConfig, LoadingScreenState};
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};

use super::super::characters::{
    load_characters_resource, spawn_characters,
};
use super::super::hud::countdown::load_countdown_assets;
use super::super::hud::healthbar::spawn_health_bar;
use super::super::hud::popups::{load_popup_assets, spawn_popup_pool};
use super::super::notes::effects::spawn_effect_pools;
use super::super::notes::render::{load_noteskin_assets, spawn_strumline_and_pools};
use super::super::playlist::PlayStatePlaylist;
use super::super::preload::PlayPreloadTracker;
use super::super::scene::PlayState;
use super::super::stage::{StageResource, load_stage_resource, spawn_stage_props};
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
    Stage,
    WaitStage,
    Characters,
    WaitCharacters,
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
    stage_script_source: Option<(String, PathBuf)>,
    char_script_sources: Vec<(String, PathBuf)>,
}

impl PlayLoadPipeline {
    fn reset_for_load(&mut self) {
        self.step = PlayLoadStep::WaitLoadingUi;
        self.loaded_stage = None;
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
            if step_assets_ready(scripts, loading_screen, 0.0, 0.2) {
                pipeline.step = if state.is_minimal_mode {
                    PlayLoadStep::Audio
                } else {
                    PlayLoadStep::Stage
                };
            }
        }
        PlayLoadStep::Stage => {
            load_stage_step(commands, state, scripts, pipeline);
            pipeline.step = PlayLoadStep::WaitStage;
        }
        PlayLoadStep::WaitStage => {
            if step_assets_ready(scripts, loading_screen, 0.2, 0.45) {
                pipeline.step = PlayLoadStep::Characters;
            }
        }
        PlayLoadStep::Characters => {
            load_characters_step(commands, state, scripts, pipeline);
            pipeline.step = PlayLoadStep::WaitCharacters;
        }
        PlayLoadStep::WaitCharacters => {
            if step_assets_ready(scripts, loading_screen, 0.45, 0.7) {
                pipeline.step = PlayLoadStep::Audio;
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

fn load_stage_step(
    commands: &mut Commands,
    state: &mut PlayState,
    scripts: &mut ScriptTickParams,
    pipeline: &mut PlayLoadPipeline,
) {
    let stage_id = state.chart.stage_id.clone();
    match load_stage_resource(
        &scripts.mods,
        &stage_id,
        &scripts.asset_server,
        &mut scripts.layouts,
    ) {
        Ok(stage) => {
            state.camera.stage_zoom = f64::from(stage.camera_zoom);
            if state.camera.follow_point == [0.0, 0.0] {
                state.camera.follow_point = [
                    f64::from(SCREEN_WIDTH) * 0.5,
                    f64::from(SCREEN_HEIGHT) * 0.5,
                ];
            }
            state.reset_camera(true, true, true);
            spawn_stage_props(commands, &stage);
            if stage.scripts {
                pipeline.stage_script_source =
                    Some((stage.stage_id.clone(), stage.json_path.clone()));
            }
            if let Some(tr) = scripts.preload.as_deref_mut() {
                tr.register_stage(&stage);
            }
            pipeline.loaded_stage = Some(stage);
        }
        Err(err) => {
            state.status_message = format!("Stage error: {err}");
            pipeline.loaded_stage = None;
        }
    }
}

fn load_characters_step(
    commands: &mut Commands,
    state: &mut PlayState,
    scripts: &mut ScriptTickParams,
    pipeline: &mut PlayLoadPipeline,
) {
    let chars = load_characters_resource(
        &scripts.mods,
        &state.chart.characters,
        pipeline.loaded_stage.as_ref(),
        &scripts.asset_server,
        &mut scripts.layouts,
    );
    for c in &chars.characters {
        if c.data.script {
            pipeline
                .char_script_sources
                .push((c.id.clone(), c.json_path.clone()));
        }
    }
    seed_stage_and_chars_into_state(state, pipeline.loaded_stage.as_ref(), &chars);
    spawn_characters(commands, &chars);
    spawn_health_bar(
        &scripts.mods,
        commands,
        Some(&chars),
        &state.player_strumline.note_style,
        &scripts.asset_server,
        &mut scripts.layouts,
    );
    if let Some(tr) = scripts.preload.as_deref_mut() {
        tr.register_characters(&chars);
    }
    commands.insert_resource(chars);
    if let Some(stage) = pipeline.loaded_stage.take() {
        commands.insert_resource(stage);
    }
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
