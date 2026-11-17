use std::collections::BTreeSet;

use bevy::prelude::*;

use crate::mods::{ModRegistry, ModStore};
use crate::scripting::{HookKind, ScriptCommand, ScriptEvent, ScriptHost, ScriptValue};
use crate::ui::LoadingScreenState;
use crate::utils::constants::SCREEN_HEIGHT;

use super::super::camera::hud::{hud_render_layer, spawn_hud_camera};
use super::super::characters::{
    CharacterRole, CharactersResource, append_character_sidecar_scripts,
};
use super::super::events::append_notekind_sidecar_scripts;
use super::super::playlist::PlayStatePlaylist;
use super::super::preload::PlayPreloadTracker;
use super::super::scene::PlayState;
use super::super::song::append_song_sidecar_scripts;
use super::super::stage::{StageResource, append_stage_sidecar_scripts};
use super::HudTextVisual;
use super::load_pipeline::{PlayLoadPipeline, begin_play_scene_load};

pub(crate) fn seed_stage_and_chars_into_state(
    state: &mut PlayState,
    stage: Option<&StageResource>,
    chars: &CharactersResource,
) {
    if let Some(stage) = stage {
        for (idx, prop) in stage.props.iter().enumerate() {
            let z = super::super::stage::schema::stage_layer_z(prop.data.layer, idx);
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
            state.script_prop_z.insert(prop.data.name.clone(), z);
            state
                .script_prop_scrolls
                .insert(prop.data.name.clone(), prop.data.scroll_factor);
            state
                .script_prop_flips
                .insert(prop.data.name.clone(), prop.data.flip);
            state
                .script_prop_vis
                .insert(prop.data.name.clone(), prop.data.visible);
        }
    }
    for c in &chars.characters {
        let total_scale = [
            c.data.scale[0] * c.slot.scale[0],
            c.data.scale[1] * c.slot.scale[1],
        ];
        let tl_x = c.slot.position[0]
            + c.data.offsets[0] * (1.0 + total_scale[0])
            - c.idle_size[0] * total_scale[0] * 0.5;
        let tl_y = c.slot.position[1]
            + c.data.offsets[1] * (1.0 + total_scale[1])
            - c.idle_size[1] * total_scale[1];
        let char_z = super::super::stage::schema::stage_layer_z(c.slot.layer, 0);
        let focus_x = c.slot.position[0] + c.data.offsets[0];
        let focus_y =
            c.slot.position[1] - (c.idle_size[1] * total_scale[1]) * 0.5 + c.data.offsets[1];
        let focus = [
            f64::from(focus_x + c.data.camera_offsets[0] + c.slot.camera_offset[0]),
            f64::from(focus_y + c.data.camera_offsets[1] + c.slot.camera_offset[1]),
        ];

        state
            .script_char_positions
            .insert(c.id.clone(), c.slot.position);
        state
            .script_char_top_left
            .insert(c.id.clone(), [tl_x, tl_y]);
        state.script_char_z.insert(c.id.clone(), char_z);
        state.script_char_scales.insert(c.id.clone(), c.slot.scale);
        state
            .script_char_alphas
            .insert(c.id.clone(), c.slot.opacity);
        state.script_char_flips.insert(c.id.clone(), c.slot.flip);
        state.script_char_vis.insert(c.id.clone(), c.slot.visible);
        state.script_char_sizes.insert(c.id.clone(), c.idle_size);
        state.script_char_focus.insert(c.id.clone(), focus);

        let (alias, role_idx) = match c.role {
            CharacterRole::Player => ("bf", 0),
            CharacterRole::Opponent => ("dad", 1),
            CharacterRole::Spectator => ("gf", 2),
        };
        state.camera.focus_points[role_idx] = Some(focus);
        state
            .script_char_positions
            .entry(alias.to_string())
            .or_insert(c.slot.position);
        state
            .script_char_top_left
            .entry(alias.to_string())
            .or_insert([tl_x, tl_y]);
        state.script_char_z.entry(alias.to_string()).or_insert(char_z);
        state
            .script_char_scales
            .entry(alias.to_string())
            .or_insert(c.slot.scale);
        state
            .script_char_alphas
            .entry(alias.to_string())
            .or_insert(c.slot.opacity);
        state
            .script_char_flips
            .entry(alias.to_string())
            .or_insert(c.slot.flip);
        state
            .script_char_vis
            .entry(alias.to_string())
            .or_insert(c.slot.visible);
        state
            .script_char_sizes
            .entry(alias.to_string())
            .or_insert(c.idle_size);
        state
            .script_char_focus
            .entry(alias.to_string())
            .or_insert(focus);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn setup_play_scene(
    mut commands: Commands,
    mods: Res<ModRegistry>,
    asset_server: Res<AssetServer>,
    mut state: ResMut<PlayState>,
    mut preload: ResMut<PlayPreloadTracker>,
    mut loading_screen: ResMut<LoadingScreenState>,
    mut load_pipeline: ResMut<PlayLoadPipeline>,
) {
    spawn_hud_camera(&mut commands);
    begin_play_scene_load(
        &mut state,
        Some(&mut preload),
        &mut loading_screen,
        &mut load_pipeline,
    );

    let score_font_path = mods
        .resolve_asset_server_path("ui/fonts/VCR OSD Mono.ttf")
        .unwrap_or_else(|| "ui/fonts/VCR OSD Mono.ttf".to_string());
    let score_font: Handle<Font> = asset_server.load(score_font_path);
    commands.spawn((
        Text2d::new(""),
        TextFont {
            font: score_font.into(),
            font_size: FontSize::Px(16.0),
            font_smoothing: bevy::text::FontSmoothing::None,
            ..default()
        },
        TextLayout::justify(Justify::Right),
        bevy::sprite::Text2dShadow {
            offset: Vec2::new(1.0, -1.0),
            color: Color::BLACK,
        },
        bevy::sprite::Anchor::TOP_LEFT,
        Transform::from_xyz(601.0 * 0.5 - 190.0, -SCREEN_HEIGHT * 0.4 - 30.0, 10.8),
        hud_render_layer(),
        HudTextVisual,
    ));
}

pub(crate) fn load_and_init_scripts(
    mods: &ModRegistry,
    mod_store: &mut ModStore,
    script_host: &mut ScriptHost,
    state: &mut PlayState,
    playlist: &mut PlayStatePlaylist,
    stage_script_source: Option<&(String, std::path::PathBuf)>,
    char_script_sources: &[(String, std::path::PathBuf)],
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
    let supported = script_host.supported_extensions();
    if let Some((stage_id, json_path)) = stage_script_source {
        append_stage_sidecar_scripts(mods, stage_id, json_path, &supported, &mut discovered);
    }
    for (char_id, json_path) in char_script_sources {
        append_character_sidecar_scripts(mods, char_id, json_path, &supported, &mut discovered);
    }
    append_notekind_sidecar_scripts(mods, &note_kind_vec, &supported, &mut discovered);
    append_song_sidecar_scripts(mods, &state.chart, &supported, &mut discovered);
    script_host.load_discovered(&discovered);
    for err in &script_host.compile_errors {
        warn!("Script compile error: {err}");
    }

    // Run Create first so scripted NoteKind registrations update note flags before SongLoaded.
    let mut ecs_cmds = state.run_script_hooks(
        vec![(HookKind::Create, ScriptEvent::new(HookKind::Create))],
        false,
        0.0,
        playlist,
        script_host,
        mods,
        mod_store,
    );

    let song_loaded_ev = {
        let mut ev_list = Vec::with_capacity(state.chart.events.len());
        for ev in &state.chart.events {
            let mut m = std::collections::BTreeMap::new();
            m.insert("time".to_string(), ScriptValue::Float(ev.time_ms));
            m.insert("kind".to_string(), ScriptValue::String(ev.kind.clone()));
            m.insert("name".to_string(), ScriptValue::String(ev.kind.clone()));
            m.insert("value".to_string(), ScriptValue::from_json(&ev.raw_value));
            ev_list.push(ScriptValue::Map(m));
        }
        let mut note_list = Vec::with_capacity(
            state.player_strumline.notes.len() + state.opponent_strumline.notes.len(),
        );
        for (is_player, notes) in [
            (true, &state.player_strumline.notes),
            (false, &state.opponent_strumline.notes),
        ] {
            for n in notes {
                let mut m = std::collections::BTreeMap::new();
                m.insert("id".to_string(), ScriptValue::Int(n.data.id as i64));
                m.insert("time".to_string(), ScriptValue::Float(n.data.time_ms));
                m.insert("dir".to_string(), ScriptValue::Int(n.lane.index() as i64));
                m.insert("len".to_string(), ScriptValue::Float(n.data.sustain_length_ms));
                m.insert(
                    "kind".to_string(),
                    ScriptValue::String(n.data.kind.clone().unwrap_or_default()),
                );
                m.insert("is_player".to_string(), ScriptValue::Bool(is_player));
                m.insert("scoreable".to_string(), ScriptValue::Bool(n.data.scoreable));
                note_list.push(ScriptValue::Map(m));
            }
        }
        ScriptEvent::new(HookKind::SongLoaded)
            .with_field("song", state.chart.song_id.clone())
            .with_field("bpm", state.conductor.bpm())
            .with_field("notes", ScriptValue::List(note_list))
            .with_field("events", ScriptValue::List(ev_list))
            .with_field(
                "count",
                ScriptValue::Int(
                    (state.player_strumline.notes.len()
                        + state.opponent_strumline.notes.len()) as i64,
                ),
            )
    };
    ecs_cmds.extend(state.run_script_hooks(
        vec![(HookKind::SongLoaded, song_loaded_ev)],
        false,
        0.0,
        playlist,
        script_host,
        mods,
        mod_store,
    ));
    ecs_cmds
}
