use bevy::prelude::*;

use crate::input::{Action, Controls, PreciseInputManager};
use crate::mods::{ModRegistry, ModStore};
use crate::scripting::{HookKind, ScriptCommand, ScriptEvent, ScriptHost};

use super::super::playlist::PlayStatePlaylist;
use super::super::scene::PlayState;
use super::super::stage::{DynamicScriptProp, StageResource};
use super::super::types::{PauseMode, PlayPhase};
use super::PendingEcsScriptCommands;
use super::setup::load_and_init_scripts;

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_play_input(
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
    stage_res: Option<Res<StageResource>>,
    dyn_props: Query<Entity, With<DynamicScriptProp>>,
) {
    if controls.just_pressed(&keys, Action::ReloadMods) {
        let destroy_cmds = state.run_script_hooks(
            vec![(HookKind::Destroy, ScriptEvent::new(HookKind::Destroy))],
            false,
            0.0,
            &mut playlist,
            &mut script_host,
            &mods,
            &mut mod_store,
        );
        pending_ecs.0.extend(destroy_cmds);
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
        let stage_script_source = stage_res
            .as_deref()
            .filter(|s| s.scripts)
            .map(|s| (s.stage_id.clone(), s.json_path.clone()));
        let cmds = load_and_init_scripts(
            &mods,
            &mut mod_store,
            &mut script_host,
            &mut state,
            &mut playlist,
            stage_script_source.as_ref(),
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

    if state.is_bot_play_mode { return; }
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

