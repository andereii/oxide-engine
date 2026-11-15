use std::collections::BTreeSet;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::mods::{ModRegistry, ModStore};
use crate::play::characters::loader::LoadedCharacter;
use crate::play::characters::{
    CharacterActor, CharacterRole, CharacterSyncState, CharactersResource,
    load_characters_resource, load_single_character, spawn_characters, spawn_single_character,
};
use crate::play::hud::countdown::{CountdownAssets, load_countdown_assets};
use crate::play::hud::health_icon::{HealthIcon, spawn_health_icons};
use crate::play::hud::healthbar::HealthBarBgVisual;
use crate::play::hud::popups::{PopupAssets, load_popup_assets};
use crate::play::notes::effects::{HoldCoverVisual, NoteSplashVisual, spawn_effect_pools};
use crate::play::notes::render::visuals::{
    NoteVisual, NoteskinAssets, ReceptorVisual, SustainBodyVisual, SustainEndVisual,
    load_noteskin_assets, spawn_strumline_and_pools,
};
use crate::play::notes::skin::NoteStyle;
use crate::play::playlist::PlayStatePlaylist;
use crate::play::plugin::setup::load_and_init_scripts;
use crate::play::plugin::{PendingEcsScriptCommands, PlaySfxAssets};
use crate::play::scene::PlayState;
use crate::play::shaders::systems::StageShaderRuntime;
use crate::play::song::load_song_chart_in;
use crate::play::stage::schema::StageCharacterSlot;
use crate::play::stage::{
    DynamicScriptProp, StageCompositePiece, StagePropVisual, StageResource, StageSyncState,
    load_stage_resource, spawn_stage_props,
};
use crate::scripting::ScriptHost;
use crate::utils::conductor::SongTimeChange;
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};

#[derive(SystemParam)]
pub(super) struct HotReloadRes<'w> {
    pub(super) asset_server: Res<'w, AssetServer>,
    pub(super) mut_layouts: ResMut<'w, Assets<TextureAtlasLayout>>,
    pub(super) mods: ResMut<'w, ModRegistry>,
    pub(super) mod_store: ResMut<'w, ModStore>,
    pub(super) script_host: ResMut<'w, ScriptHost>,
    pub(super) pending_ecs: ResMut<'w, PendingEcsScriptCommands>,
    pub(super) state: ResMut<'w, PlayState>,
    pub(super) playlist: ResMut<'w, PlayStatePlaylist>,
    pub(super) sfx: ResMut<'w, PlaySfxAssets>,
    pub(super) stage_res: Option<ResMut<'w, StageResource>>,
    pub(super) stage_sync: ResMut<'w, StageSyncState>,
    pub(super) chars_res: Option<ResMut<'w, CharactersResource>>,
    pub(super) char_sync: ResMut<'w, CharacterSyncState>,
    pub(super) skin_res: Option<ResMut<'w, NoteskinAssets>>,
    pub(super) countdown_res: Option<ResMut<'w, CountdownAssets>>,
    pub(super) popup_res: Option<ResMut<'w, PopupAssets>>,
    pub(super) shader_runtime: Option<ResMut<'w, StageShaderRuntime>>,
}

#[derive(SystemParam)]
pub(super) struct HotReloadQueries<'w, 's> {
    pub(super) stage_props: Query<'w, 's, Entity, (With<StagePropVisual>, Without<DynamicScriptProp>)>,
    pub(super) stage_pieces: Query<'w, 's, Entity, With<StageCompositePiece>>,
    pub(super) dyn_props: Query<'w, 's, Entity, With<DynamicScriptProp>>,
    pub(super) actors: Query<'w, 's, (Entity, &'static mut CharacterActor)>,
    pub(super) health_icons: Query<'w, 's, Entity, With<HealthIcon>>,
    #[allow(clippy::type_complexity)]
    pub(super) skin_entities: Query<
        'w,
        's,
        Entity,
        Or<(
            With<ReceptorVisual>,
            With<NoteVisual>,
            With<SustainBodyVisual>,
            With<SustainEndVisual>,
            With<NoteSplashVisual>,
            With<HoldCoverVisual>,
        )>,
    >,
    pub(super) health_bar_bg: Query<'w, 's, (&'static mut HealthBarBgVisual, &'static mut Sprite)>,
}

#[derive(Default)]
pub(super) struct ChartReloadDelta {
    pub(super) stage_changed: bool,
    pub(super) chars_changed: bool,
    pub(super) skin_changed: bool,
    pub(super) audio_changed: bool,
}

pub(super) fn reload_asset_paths(asset_server: &AssetServer, server_paths: &[String]) {
    let mut seen = BTreeSet::new();
    for path in server_paths {
        if !path.is_empty() && seen.insert(path.as_str()) {
            asset_server.reload(path.clone());
        }
    }
}

pub(super) fn reload_chart_domain(res: &mut HotReloadRes) -> ChartReloadDelta {
    let song_id = res.state.chart.song_id.clone();
    let diff = res.state.current_difficulty.clone();
    let var = res.state.current_variation.clone();
    let new_chart = match load_song_chart_in(&res.mods, &song_id, &diff, &var) {
        Ok(c) => c,
        Err(err) => {
            warn!("[HMR] Failed to reload chart '{song_id}': {err}");
            res.state.status_message = format!("HMR chart error: {err}");
            return ChartReloadDelta::default();
        }
    };

    let stage_changed = new_chart.stage_id != res.state.chart.stage_id;
    let chars_changed = new_chart.characters.players != res.state.chart.characters.players
        || new_chart.characters.opponents != res.state.chart.characters.opponents
        || new_chart.characters.spectator != res.state.chart.characters.spectator;
    let skin_changed = new_chart.skins.hud != res.state.chart.skins.hud;
    let audio_changed = new_chart.audio.shared_vocal_file != res.state.chart.audio.shared_vocal_file
        || new_chart.audio.instrumental.len() != res.state.chart.audio.instrumental.len();

    if (new_chart.bpm - res.state.chart.bpm).abs() > 1e-4 {
        res.state
            .conductor
            .map_time_changes(vec![SongTimeChange::new(0.0, new_chart.bpm)]);
    }
    res.state.song_events = new_chart.events.clone();
    res.state.chart = new_chart;
    info!("[HMR] Reloaded chart '{song_id}'");
    ChartReloadDelta {
        stage_changed,
        chars_changed,
        skin_changed,
        audio_changed,
    }
}

pub(super) fn reload_stage_domain(
    commands: &mut Commands,
    res: &mut HotReloadRes,
    q: &mut HotReloadQueries,
) -> bool {
    let stage_id = res.state.chart.stage_id.clone();
    if stage_id.trim().is_empty() {
        return false;
    }

    let new_stage = match load_stage_resource(
        &res.mods,
        &stage_id,
        &res.asset_server,
        &mut res.mut_layouts,
    ) {
        Ok(st) => st,
        Err(err) => {
            warn!("[HMR] Failed to reload stage '{stage_id}': {err}");
            res.state.status_message = format!("HMR stage error: {err}");
            return false;
        }
    };

    for entity in &q.stage_props {
        commands.entity(entity).despawn();
    }
    for entity in &q.stage_pieces {
        commands.entity(entity).despawn();
    }

    res.state.camera.stage_zoom = f64::from(new_stage.camera_zoom);
    if res.state.camera.follow_point == [0.0, 0.0] {
        res.state.camera.follow_point =
            [f64::from(SCREEN_WIDTH) * 0.5, f64::from(SCREEN_HEIGHT) * 0.5];
    }

    res.state.script_prop_positions.clear();
    res.state.script_prop_scales.clear();
    res.state.script_prop_angles.clear();
    res.state.script_prop_alphas.clear();
    for prop in &new_stage.props {
        res.state
            .script_prop_positions
            .insert(prop.data.name.clone(), prop.data.position);
        res.state
            .script_prop_scales
            .insert(prop.data.name.clone(), prop.data.scale);
        res.state
            .script_prop_angles
            .insert(prop.data.name.clone(), prop.data.angle);
        res.state
            .script_prop_alphas
            .insert(prop.data.name.clone(), prop.data.opacity);
    }

    for (_, mut actor) in &mut q.actors {
        actor.slot = match actor.role {
            CharacterRole::Spectator => new_stage.characters.spectator.clone(),
            CharacterRole::Opponent => new_stage.characters.opponent.clone(),
            CharacterRole::Player => new_stage.characters.player.clone(),
        };
        actor.recompute_camera_focus();
    }

    res.stage_sync.last_step = None;
    spawn_stage_props(commands, &new_stage);

    if let Some(ref mut sr) = res.stage_res {
        **sr = new_stage;
    } else {
        commands.insert_resource(new_stage);
    }
    if let Some(ref mut shaders) = res.shader_runtime {
        shaders.attached = false;
    }
    info!("[HMR] Reloaded stage '{stage_id}'");
    true
}

fn despawn_health_icons(commands: &mut Commands, q: &HotReloadQueries) {
    for entity in &q.health_icons {
        commands.entity(entity).despawn();
    }
}

pub(super) fn reload_all_characters_domain(
    commands: &mut Commands,
    res: &mut HotReloadRes,
    q: &mut HotReloadQueries,
) {
    for (ent, actor) in &q.actors {
        commands.entity(actor.sheet_entity).despawn();
        for &piece in &actor.piece_entities {
            commands.entity(piece).despawn();
        }
        commands.entity(ent).despawn();
    }
    let new_chars = load_characters_resource(
        &res.mods,
        &res.state.chart.characters,
        res.stage_res.as_deref(),
        &res.asset_server,
        &mut res.mut_layouts,
    );
    spawn_characters(commands, &new_chars);
    if let Some(ref mut chars_res) = res.chars_res {
        **chars_res = new_chars;
    } else {
        commands.insert_resource(new_chars);
    }
    despawn_health_icons(commands, q);
    spawn_health_icons(
        &res.mods,
        commands,
        res.chars_res.as_deref(),
        &res.asset_server,
        &mut res.mut_layouts,
    );
    res.char_sync.last_step = None;
    if let Some(ref mut shaders) = res.shader_runtime {
        shaders.attached = false;
    }
    info!("[HMR] Reloaded all characters");
}

pub(super) fn reload_single_character_domain(
    commands: &mut Commands,
    res: &mut HotReloadRes,
    q: &mut HotReloadQueries,
    char_id: &str,
) {
    let mut existing_entities: Vec<Entity> = Vec::new();
    let mut targets: Vec<(CharacterRole, StageCharacterSlot)> = Vec::new();
    for (ent, actor) in &q.actors {
        if actor.id == char_id {
            existing_entities.push(ent);
            existing_entities.push(actor.sheet_entity);
            existing_entities.extend_from_slice(&actor.piece_entities);
            targets.push((actor.role, actor.slot.clone()));
        }
    }
    if targets.is_empty() {
        let slots = res.stage_res.as_ref().map(|s| &s.characters);
        let role_lists = [
            (
                &res.state.chart.characters.spectator,
                CharacterRole::Spectator,
                slots
                    .map(|s| s.spectator.clone())
                    .unwrap_or_else(StageCharacterSlot::default_for_spectator),
            ),
            (
                &res.state.chart.characters.opponents,
                CharacterRole::Opponent,
                slots
                    .map(|s| s.opponent.clone())
                    .unwrap_or_else(StageCharacterSlot::default_for_opponent),
            ),
            (
                &res.state.chart.characters.players,
                CharacterRole::Player,
                slots
                    .map(|s| s.player.clone())
                    .unwrap_or_else(StageCharacterSlot::default_for_player),
            ),
        ];
        for (ids, role, slot) in role_lists {
            if ids.iter().any(|id| id == char_id) {
                targets.push((role, slot));
            }
        }
    }
    if targets.is_empty() {
        return;
    }

    // Load and validate before despawning so transient JSON/path errors keep the existing actor visible.
    let mut loaded_batch: Vec<LoadedCharacter> = Vec::with_capacity(targets.len());
    for (role, slot) in targets {
        match load_single_character(
            &res.mods,
            char_id,
            role,
            slot,
            &res.asset_server,
            &mut res.mut_layouts,
        ) {
            Ok(loaded) => loaded_batch.push(loaded),
            Err(err) => {
                warn!("[HMR] Failed to reload character '{char_id}': {err}");
                res.state.status_message = format!("HMR char error: {err}");
                return;
            }
        }
    }

    for ent in existing_entities {
        commands.entity(ent).despawn();
    }

    let mut touches_icon = false;
    for loaded in loaded_batch {
        if loaded.role != CharacterRole::Spectator {
            touches_icon = true;
        }
        spawn_single_character(commands, &loaded);
        if let Some(ref mut chars_res) = res.chars_res {
            if let Some(existing) = chars_res
                .characters
                .iter_mut()
                .find(|c| c.id == char_id && c.role == loaded.role)
            {
                *existing = loaded;
            } else {
                chars_res.characters.push(loaded);
            }
        }
    }

    if touches_icon {
        despawn_health_icons(commands, q);
        spawn_health_icons(
            &res.mods,
            commands,
            res.chars_res.as_deref(),
            &res.asset_server,
            &mut res.mut_layouts,
        );
    }

    res.char_sync.last_step = None;
    if let Some(ref mut shaders) = res.shader_runtime {
        shaders.attached = false;
    }
    info!("[HMR] Reloaded character '{char_id}'");
}

pub(super) fn reload_skin_domain(
    commands: &mut Commands,
    res: &mut HotReloadRes,
    q: &mut HotReloadQueries,
) {
    let skin_id = res.state.chart.skins.hud.clone();
    let new_style = match NoteStyle::load_in(&res.mods, &skin_id) {
        Ok(s) => s,
        Err(err) => {
            warn!("[HMR] Failed to reload noteskin '{skin_id}': {err}");
            res.state.status_message = format!("HMR skin error: {err}");
            return;
        }
    };

    res.sfx.miss_sounds = new_style
        .miss_sound_assets_in(&res.mods)
        .into_iter()
        .map(|rel| res.asset_server.load(rel))
        .collect();
    res.sfx.miss_volume = new_style.miss_volume;

    res.state.player_strumline.note_style = new_style.clone();
    res.state.opponent_strumline.note_style = new_style.clone();

    let new_countdown = load_countdown_assets(&res.mods, &new_style, &res.asset_server);
    if let Some(ref mut cd) = res.countdown_res {
        **cd = new_countdown;
    }
    let new_popup = load_popup_assets(&res.mods, &new_style, &res.asset_server);
    if let Some(ref mut pop) = res.popup_res {
        **pop = new_popup;
    }

    let hb_handle: Handle<Image> = res
        .asset_server
        .load(new_style.health_bar_asset_in(&res.mods));
    let hb_scale = new_style.health_bar.scale.max(0.01);
    for (mut bg, mut sprite) in &mut q.health_bar_bg {
        bg.bg_image = hb_handle.clone();
        bg.scale = hb_scale;
        bg.size = [601.0 * hb_scale, 19.0 * hb_scale];
        sprite.image = hb_handle.clone();
        sprite.color = Color::srgba(1.0, 1.0, 1.0, new_style.health_bar.alpha.clamp(0.0, 1.0));
    }

    match load_noteskin_assets(&res.mods, &new_style, &res.asset_server, &mut res.mut_layouts) {
        Ok(skin_assets) => {
            for entity in &q.skin_entities {
                commands.entity(entity).despawn();
            }
            spawn_strumline_and_pools(commands, &res.state, &skin_assets);
            spawn_effect_pools(commands, &skin_assets);
            if let Some(ref mut sr) = res.skin_res {
                **sr = skin_assets;
            } else {
                commands.insert_resource(skin_assets);
            }
            info!("[HMR] Reloaded skin '{skin_id}'");
        }
        Err(err) => {
            warn!("[HMR] Failed to reload skin textures '{skin_id}': {err}");
            res.state.status_message = format!("HMR skin error: {err}");
        }
    }
}

pub(super) fn reload_scripts_domain(
    commands: &mut Commands,
    res: &mut HotReloadRes,
    q: &HotReloadQueries,
) {
    for entity in &q.dyn_props {
        commands.entity(entity).despawn();
    }

    if let Some(stage) = res.stage_res.as_deref() {
        res.state.script_prop_positions.clear();
        res.state.script_prop_scales.clear();
        res.state.script_prop_angles.clear();
        res.state.script_prop_alphas.clear();
        for prop in &stage.props {
            res.state
                .script_prop_positions
                .insert(prop.data.name.clone(), prop.data.position);
            res.state
                .script_prop_scales
                .insert(prop.data.name.clone(), prop.data.scale);
            res.state
                .script_prop_angles
                .insert(prop.data.name.clone(), prop.data.angle);
            res.state
                .script_prop_alphas
                .insert(prop.data.name.clone(), prop.data.opacity);
        }
    }

    *res.script_host = ScriptHost::with_registry(res.mods.clone());
    res.state.hud_visible = true;

    let stage_script_source = res
        .stage_res
        .as_ref()
        .filter(|s| s.scripts)
        .map(|s| (s.stage_id.clone(), s.json_path.clone()));

    let cmds = load_and_init_scripts(
        &res.mods,
        &mut res.mod_store,
        &mut res.script_host,
        &mut res.state,
        &mut res.playlist,
        stage_script_source.as_ref(),
    );
    res.pending_ecs.0.extend(cmds);
    if res.script_host.compile_errors.is_empty() && res.state.status_message.starts_with("HMR ") {
        res.state.status_message.clear();
    }
    info!(
        "[HMR] Reloaded {} script(s) ({} compile error(s))",
        res.script_host.script_count(),
        res.script_host.compile_errors.len()
    );
}
