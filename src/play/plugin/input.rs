use bevy::prelude::*;

use crate::input::{
    Action, Controls, PreciseInputManager, keycode_aliases, primary_keycode_name,
};
use crate::mods::{ModRegistry, ModStore};
use crate::scripting::{HookKind, ScriptCommand, ScriptEvent, ScriptHost};
use crate::ui::script_errors::ScriptErrorOverlay;

use super::super::characters::CharactersResource;
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
    chars_res: Option<Res<CharactersResource>>,
    dyn_props: Query<Entity, With<DynamicScriptProp>>,
    mut overlay: ResMut<ScriptErrorOverlay>,
) {
    // Shift+S opens the script log. S is also Note Down, so that chord must not hit a lane.
    let swallow_note_s = overlay.swallow_note_s;
    if swallow_note_s && !keys.pressed(KeyCode::KeyS) {
        overlay.swallow_note_s = false;
    }
    let swallow_backspace = overlay.swallow_backspace;

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
        state.set_hud_visible(true, true);
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
        let char_script_sources: Vec<(String, std::path::PathBuf)> = chars_res
            .as_deref()
            .map(|cr| {
                cr.characters
                    .iter()
                    .filter(|c| c.data.script)
                    .map(|c| (c.id.clone(), c.json_path.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let cmds = load_and_init_scripts(
            &mods,
            &mut mod_store,
            &mut script_host,
            &mut state,
            &mut playlist,
            stage_script_source.as_ref(),
            &char_script_sources,
        );
        pending_ecs.0.extend(cmds);
        state.status_message = format!(
            "Reloaded {} mod(s), {} script(s)",
            mods.active_mod_ids().len(),
            script_host.script_count()
        );
        return;
    }

    if state.phase == PlayPhase::Loading || matches!(state.phase, PlayPhase::Paused(_)) {
        return;
    }

    if controls.just_pressed(&keys, Action::WindowScreenshot) {
        pending_ecs.0.push(ScriptCommand::TakeScreenshot(None));
    }

    state.script_keys_down.clear();
    for k in keys.get_pressed() {
        state.script_keys_down.extend(keycode_aliases(*k));
    }
    state.script_keys_hit.clear();
    for k in keys.get_just_pressed() {
        state.script_keys_hit.extend(keycode_aliases(*k));
    }
    state.script_keys_up.clear();
    for k in keys.get_just_released() {
        state.script_keys_up.extend(keycode_aliases(*k));
    }
    let (c_down, c_hit, c_up) = controls.collect_active_names(&keys);
    state.script_ctrls_down = c_down;
    state.script_ctrls_hit = c_hit;
    state.script_ctrls_up = c_up;
    if swallow_note_s {
        retain_key(&mut state.script_keys_down, "KeyS", "S");
        retain_key(&mut state.script_keys_hit, "KeyS", "S");
        retain_key(&mut state.script_keys_up, "KeyS", "S");
        if !keys.pressed(KeyCode::ArrowDown) {
            state.script_ctrls_down.retain(|name| !is_down_control(name));
        }
        if !keys.just_pressed(KeyCode::ArrowDown) {
            state.script_ctrls_hit.retain(|name| !is_down_control(name));
        }
        if !keys.just_released(KeyCode::ArrowDown) {
            state.script_ctrls_up.retain(|name| !is_down_control(name));
        }
    }
    if swallow_backspace {
        state.script_keys_down.retain(|name| name != "Backspace");
        state.script_keys_hit.retain(|name| name != "Backspace");
        state.script_keys_up.retain(|name| name != "Backspace");
        let other_down = keys.pressed(KeyCode::KeyX) || keys.pressed(KeyCode::Escape);
        let other_hit = keys.just_pressed(KeyCode::KeyX) || keys.just_pressed(KeyCode::Escape);
        let other_up = keys.just_released(KeyCode::KeyX) || keys.just_released(KeyCode::Escape);
        if !other_down {
            state.script_ctrls_down.retain(|name| name != "BACK");
        }
        if !other_hit {
            state.script_ctrls_hit.retain(|name| name != "BACK");
        }
        if !other_up {
            state.script_ctrls_up.retain(|name| name != "BACK");
        }
    }

    let note_lanes_active = !state.is_bot_play_mode
        && !state.disable_keys
        && !state.is_in_cutscene
        && !matches!(state.phase, PlayPhase::Paused(_));
    let emit_press = script_host.has_hook_listeners(&HookKind::KeyPress);
    let emit_release = script_host.has_hook_listeners(&HookKind::KeyRelease);
    if emit_press {
        let ctrl_name = state.script_ctrls_hit.first().cloned().unwrap_or_default();
        for k in keys.get_just_pressed() {
            if swallowed_overlay_key(*k, swallow_note_s, swallow_backspace) {
                continue;
            }
            if note_lanes_active && controls.is_note_key(*k) {
                continue;
            }
            let mut ev = ScriptEvent::new(HookKind::KeyPress)
                .with_field("key", primary_keycode_name(*k));
            if !ctrl_name.is_empty() {
                ev = ev.with_field("ctrl", ctrl_name.as_str());
            }
            state.pending_hooks.push((HookKind::KeyPress, ev));
        }
    }
    if emit_release {
        let ctrl_name = state.script_ctrls_up.first().cloned().unwrap_or_default();
        for k in keys.get_just_released() {
            if swallowed_overlay_key(*k, swallow_note_s, swallow_backspace) {
                continue;
            }
            if note_lanes_active && controls.is_note_key(*k) {
                continue;
            }
            let mut ev = ScriptEvent::new(HookKind::KeyRelease)
                .with_field("key", primary_keycode_name(*k));
            if !ctrl_name.is_empty() {
                ev = ev.with_field("ctrl", ctrl_name.as_str());
            }
            state.pending_hooks.push((HookKind::KeyRelease, ev));
        }
    }

    if controls.just_pressed(&keys, Action::Pause) {
        if matches!(state.phase, PlayPhase::Paused(_)) {
            state.resume();
            return;
        } else if state.may_pause_game {
            state.pause(PauseMode::Standard);
            return;
        }
    }

    if controls.just_pressed(&keys, Action::Reset) {
        state.needs_reset = true;
        return;
    }

    if state.is_bot_play_mode || state.disable_keys {
        return;
    }
    let pos_ms = state.conductor.song_position_ms;
    precise.poll_keyboard(&keys, pos_ms);

    for ev in precise.drain_presses() {
        if swallow_note_s && ev.key_code == KeyCode::KeyS {
            continue;
        }
        let latency = precise.latency_ms(&ev);
        state.on_key_press_precise(ev.direction, Some(ev.key_code), ev.song_position_ms, latency);
    }
    for ev in precise.drain_releases() {
        if swallow_note_s && ev.key_code == KeyCode::KeyS {
            continue;
        }
        let latency = precise.latency_ms(&ev);
        state.on_key_release_precise(ev.direction, Some(ev.key_code), ev.song_position_ms, latency);
    }
}

fn retain_key(names: &mut Vec<String>, raw: &str, short: &str) {
    names.retain(|name| name != raw && name != short);
}

fn is_down_control(name: &str) -> bool {
    matches!(name, "NOTE_DOWN" | "UI_DOWN" | "DOWN")
}

fn swallowed_overlay_key(key: KeyCode, swallow_note_s: bool, swallow_backspace: bool) -> bool {
    (swallow_note_s && key == KeyCode::KeyS) || (swallow_backspace && key == KeyCode::Backspace)
}

