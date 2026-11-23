use bevy::prelude::*;

use crate::scripting::{HookKind, ScriptEvent};
use crate::ui::LoadingScreenState;

use super::super::characters::CharacterTrailVisual;
use super::super::hud::countdown::CountdownGraphicVisual;
use super::super::hud::health_icon::HealthIcon;
use super::super::hud::healthbar::{HealthBarBgVisual, HealthBarFillVisual};
use super::super::hud::popups::PopupVisual;
use super::super::notes::NoteStyle;
use super::super::notes::effects::{HoldCoverVisual, NoteSplashVisual};
use super::super::notes::render::visuals::{
    NoteVisual, ReceptorVisual, SustainBodyVisual, SustainEndVisual,
};
use super::super::playlist::PlayStatePlaylist;
use super::super::scene::PlayState;
use super::super::shaders::systems::ShaderScriptCommand;
use super::super::song::{SongSelection, load_song_chart_in};
use super::super::stage::{StageCompositePiece, StageCompositeQuad};
use super::super::types::{PlayPhase, PlayStateParams, SongChart};
use super::load_pipeline::{PlayLoadPipeline, begin_play_scene_load};
use super::{AudioTickParams, ScriptHudAndTextParams, ScriptTickParams};

pub(super) type ReloadCleanupQuery<'w, 's> = Query<
    'w,
    's,
    Entity,
    Or<(
        With<StageCompositePiece>,
        With<StageCompositeQuad>,
        With<CountdownGraphicVisual>,
        With<HealthBarBgVisual>,
        With<HealthBarFillVisual>,
        With<HealthIcon>,
        With<ReceptorVisual>,
        With<NoteVisual>,
        With<SustainBodyVisual>,
        With<SustainEndVisual>,
        With<NoteSplashVisual>,
        With<HoldCoverVisual>,
        With<PopupVisual>,
        With<CharacterTrailVisual>,
    )>,
>;

pub(super) fn apply_song_seek_state(state: &mut PlayState, seek_ms: f64) {
    let clamped = seek_ms.max(0.0);
    let prev_pos = state.conductor.song_position_ms;
    let was_starting = state.starting_song;
    state.starting_song = false;
    state.is_in_countdown = false;
    state.countdown_paused = false;
    if state.phase == PlayPhase::Countdown {
        state.phase = PlayPhase::Playing;
    }
    state.conductor.update(clamped);
    state.last_raw_audio_ms = None;
    if was_starting {
        state.pending_hooks.push((
            HookKind::CountdownEnd,
            ScriptEvent::new(HookKind::CountdownEnd),
        ));
        state.pending_hooks.push((
            HookKind::SongStart,
            ScriptEvent::new(HookKind::SongStart)
                .with_field("song", state.chart.song_id.clone())
                .with_field("bpm", state.conductor.bpm()),
        ));
        state.status_message = format!(
            "Playing: {} [{}]",
            state.chart.song_name, state.current_difficulty
        );
    }
    if clamped < prev_pos {
        for ev in &mut state.song_events {
            if ev.time_ms >= clamped {
                ev.triggered = false;
            }
        }
    } else {
        // Run skipped song events chronologically so camera/zoom/script state matches the target timestamp.
        state.process_song_events();
    }
    state.player_strumline.seek_to(clamped);
    state.opponent_strumline.seek_to(clamped);
}

pub(super) fn apply_song_seek(
    state: &mut PlayState,
    audio_res: &mut AudioTickParams,
    seek_ms: f64,
) {
    apply_song_seek_state(state, seek_ms);
    let sync_ms = (state.conductor.song_position_ms - state.conductor.combined_offset()).max(0.0);
    audio_res.sync.was_countdown = false;
    audio_res.audio.play_from(sync_ms, &mut audio_res.sinks);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_song_reload(
    commands: &mut Commands,
    song_id: String,
    diff_opt: Option<String>,
    var_opt: Option<String>,
    state: &mut PlayState,
    playlist: &mut PlayStatePlaylist,
    selection: &mut SongSelection,
    audio_res: &mut AudioTickParams,
    scripts: &mut ScriptTickParams,
    hud_texts: &mut ScriptHudAndTextParams,
    reload_cleanup: &ReloadCleanupQuery,
    loading_screen: &mut LoadingScreenState,
    load_pipeline: &mut PlayLoadPipeline,
) {
    let _ = state.run_script_hooks(
        vec![(HookKind::Destroy, ScriptEvent::new(HookKind::Destroy))],
        false,
        0.0,
        playlist,
        &mut scripts.script_host,
        &scripts.mods,
        &mut scripts.mod_store,
    );
    scripts.script_host.clear_scripts();
    scripts.pending_ecs.0.clear();
    scripts.pending_shaders.0.clear();
    scripts.pending_shaders.0.push(ShaderScriptCommand::ClearShader {
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
            comp.despawn_composite_rt(commands);
        }
        commands.entity(ent).despawn();
    }
    for (ent, actor) in &scripts.actors {
        commands.entity(actor.sheet_entity).despawn();
        for &piece in &actor.piece_entities {
            commands.entity(piece).despawn();
        }
        actor.despawn_composite_rt(commands);
        for &trail_ent in &actor.trail_entities {
            commands.entity(trail_ent).despawn();
        }
        commands.entity(ent).despawn();
    }
    for ent in reload_cleanup {
        commands.entity(ent).despawn();
    }
    audio_res.audio.despawn_all(commands);
    audio_res.sfx.preloaded_sounds.clear();

    selection.song_id = song_id;
    if let Some(d) = diff_opt {
        selection.difficulty = d;
    }
    if let Some(v) = var_opt {
        selection.variation = v;
    }

    let (chart, note_style, load_error) = match load_song_chart_in(
        &scripts.mods,
        &selection.song_id,
        &selection.difficulty,
        &selection.variation,
    ) {
        Ok(chart) => {
            let style = NoteStyle::load_in(&scripts.mods, &chart.skins.hud).unwrap_or_default();
            (chart, style, None)
        }
        Err(err) => (SongChart::default(), NoteStyle::default(), Some(err)),
    };

    let prev_deaths = state.death_counter;
    let mut next_state = PlayState::new(PlayStateParams {
        chart,
        note_style,
        practice_mode: state.is_practice_mode,
        bot_play_mode: state.is_bot_play_mode,
        minimal_mode: state.is_minimal_mode,
        downscroll: state.player_strumline.is_downscroll,
        start_timestamp_ms: 0.0,
        playback_rate: state.playback_rate,
        ..default()
    });
    next_state.death_counter = prev_deaths;
    if let Some(err) = load_error {
        next_state.critical_failure = true;
        next_state.status_message = err;
    }
    *state = next_state;
    hud_texts.dialogue.queue_cleanup();
    hud_texts.video.queue_cleanup();
    audio_res.sync.was_countdown = state.is_in_countdown;
    audio_res.sync.was_paused = false;

    begin_play_scene_load(
        state,
        scripts.preload.as_deref_mut(),
        loading_screen,
        load_pipeline,
    );
}
