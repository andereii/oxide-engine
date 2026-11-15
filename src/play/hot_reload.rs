use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::mods::{ModRegistry, ModStore};
use crate::play::characters::loader::LoadedCharacter;
use crate::play::characters::schema::CharacterData;
use crate::play::characters::{
    CharacterActor, CharacterRole, CharacterSyncState, CharactersResource,
    char_rel_candidates_for_watch, load_characters_resource, load_single_character,
    spawn_characters, spawn_single_character,
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
use crate::play::song::{load_song_chart_in, song_asset_path_in};
use crate::play::stage::paths::{
    resolve_atlas_dir_in, resolve_stage_image_in, stage_rel_candidates_for_watch,
};
use crate::play::stage::schema::{StageCharacterSlot, parse_stage_json};
use crate::play::stage::{
    DynamicScriptProp, StageCompositePiece, StagePropVisual, StageResource, StageSyncState,
    load_stage_resource, spawn_stage_props,
};
use crate::scripting::ScriptHost;
use crate::utils::conductor::SongTimeChange;
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH, STAGES_ASSET_PATH};

const HMR_POLL_INTERVAL_S: f32 = 0.25;
const MAX_SPRITEMAP_WATCH: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileStamp {
    exists: bool,
    len: u64,
    modified: Option<SystemTime>,
}

impl FileStamp {
    fn from_path(path: &Path) -> Self {
        match fs::metadata(path) {
            Ok(meta) if meta.is_file() => Self {
                exists: true,
                len: meta.len(),
                modified: meta.modified().ok(),
            },
            _ => Self {
                exists: false,
                len: 0,
                modified: None,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DirStamp {
    exists: bool,
    entries: Vec<String>,
}

impl DirStamp {
    fn from_path(path: &Path) -> Self {
        let Ok(rd) = fs::read_dir(path) else {
            return Self {
                exists: false,
                entries: Vec::new(),
            };
        };
        let mut entries: Vec<String> = rd
            .filter_map(Result::ok)
            .filter_map(|e| e.file_name().to_str().map(str::to_string))
            .filter(|name| !name.starts_with('.'))
            .collect();
        entries.sort();
        Self {
            exists: true,
            entries,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct DomainWatch {
    json_files: BTreeMap<PathBuf, FileStamp>,
    meta_files: BTreeMap<PathBuf, FileStamp>,
    asset_files: BTreeMap<PathBuf, (FileStamp, Option<String>)>,
    dirs: BTreeMap<PathBuf, DirStamp>,
}

enum DomainDiff {
    Unchanged,
    InPlaceAssets(Vec<String>),
    Full(Vec<String>),
}

impl DomainWatch {
    fn track_json(&mut self, path: PathBuf) {
        self.json_files
            .entry(path.clone())
            .or_insert_with(|| FileStamp::from_path(&path));
    }

    fn track_meta(&mut self, path: PathBuf) {
        self.meta_files
            .entry(path.clone())
            .or_insert_with(|| FileStamp::from_path(&path));
    }

    fn track_asset(&mut self, path: PathBuf, server_path: Option<String>) {
        self.asset_files
            .entry(path.clone())
            .or_insert_with(|| (FileStamp::from_path(&path), server_path));
    }

    fn track_dir(&mut self, dir: PathBuf) {
        self.dirs
            .entry(dir.clone())
            .or_insert_with(|| DirStamp::from_path(&dir));
    }

    fn diff(&self) -> DomainDiff {
        let mut structural = false;
        for (path, old) in &self.json_files {
            if &FileStamp::from_path(path) != old {
                structural = true;
                break;
            }
        }
        if !structural {
            for (path, old) in &self.meta_files {
                if &FileStamp::from_path(path) != old {
                    structural = true;
                    break;
                }
            }
        }
        if !structural {
            for (dir, old) in &self.dirs {
                if &DirStamp::from_path(dir) != old {
                    structural = true;
                    break;
                }
            }
        }

        let mut modified_server_paths = Vec::new();
        for (path, (old, server_opt)) in &self.asset_files {
            let cur = FileStamp::from_path(path);
            if &cur != old {
                if !cur.exists || !old.exists {
                    structural = true;
                }
                if cur.exists
                    && let Some(sp) = server_opt
                {
                    modified_server_paths.push(sp.clone());
                }
            }
        }

        if structural {
            DomainDiff::Full(modified_server_paths)
        } else if !modified_server_paths.is_empty() {
            DomainDiff::InPlaceAssets(modified_server_paths)
        } else {
            DomainDiff::Unchanged
        }
    }

    /// Collects Bevy asset server paths that are new or changed relative to `old`,
    /// ensuring newly re-targeted paths in edited JSON files are reloaded from disk.
    fn collect_changed_or_new_server_paths(&self, old: &DomainWatch) -> Vec<String> {
        let mut out = Vec::new();
        for (path, (new_stamp, server_opt)) in &self.asset_files {
            if !new_stamp.exists {
                continue;
            }
            let Some(sp) = server_opt else {
                continue;
            };
            let needs_reload = match old.asset_files.get(path) {
                Some((old_stamp, _)) => !old_stamp.exists || old_stamp != new_stamp,
                None => true,
            };
            if needs_reload {
                out.push(sp.clone());
            }
        }
        out
    }
}

#[derive(Resource, Default)]
pub struct HotReloadState {
    timer_s: f32,
    initialized: bool,
    mods_dir_stamp: Option<DirStamp>,
    mod_manifest_stamps: BTreeMap<PathBuf, FileStamp>,
    chart_watch: DomainWatch,
    stage_watch: DomainWatch,
    char_watches: BTreeMap<String, DomainWatch>,
    skin_watch: DomainWatch,
    script_watch: DomainWatch,
    audio_watch: DomainWatch,
}

#[derive(SystemParam)]
pub(crate) struct HotReloadRes<'w> {
    time: Res<'w, Time>,
    asset_server: Res<'w, AssetServer>,
    mut_layouts: ResMut<'w, Assets<TextureAtlasLayout>>,
    mods: ResMut<'w, ModRegistry>,
    mod_store: ResMut<'w, ModStore>,
    script_host: ResMut<'w, ScriptHost>,
    pending_ecs: ResMut<'w, PendingEcsScriptCommands>,
    state: ResMut<'w, PlayState>,
    playlist: ResMut<'w, PlayStatePlaylist>,
    sfx: ResMut<'w, PlaySfxAssets>,
    stage_res: Option<ResMut<'w, StageResource>>,
    stage_sync: ResMut<'w, StageSyncState>,
    chars_res: Option<ResMut<'w, CharactersResource>>,
    char_sync: ResMut<'w, CharacterSyncState>,
    skin_res: Option<ResMut<'w, NoteskinAssets>>,
    countdown_res: Option<ResMut<'w, CountdownAssets>>,
    popup_res: Option<ResMut<'w, PopupAssets>>,
    shader_runtime: Option<ResMut<'w, StageShaderRuntime>>,
}

#[derive(SystemParam)]
pub(crate) struct HotReloadQueries<'w, 's> {
    stage_props: Query<'w, 's, Entity, (With<StagePropVisual>, Without<DynamicScriptProp>)>,
    stage_pieces: Query<'w, 's, Entity, With<StageCompositePiece>>,
    dyn_props: Query<'w, 's, Entity, With<DynamicScriptProp>>,
    actors: Query<'w, 's, (Entity, &'static mut CharacterActor)>,
    health_icons: Query<'w, 's, Entity, With<HealthIcon>>,
    #[allow(clippy::type_complexity)]
    skin_entities: Query<
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
    health_bar_bg: Query<'w, 's, (&'static mut HealthBarBgVisual, &'static mut Sprite)>,
}

pub(crate) fn hot_reload_assets_system(
    mut commands: Commands,
    mut hmr: ResMut<HotReloadState>,
    mut res: HotReloadRes,
    mut q: HotReloadQueries,
) {
    if !hmr.initialized {
        refresh_all_watches(&mut hmr, &res);
        hmr.initialized = true;
        return;
    }

    hmr.timer_s += res.time.delta_secs();
    if hmr.timer_s < HMR_POLL_INTERVAL_S {
        return;
    }
    hmr.timer_s = 0.0;

    let mods_changed = check_mods_structure_changed(&hmr, &res.mods);
    if mods_changed {
        let _ = res.mods.reload();
        refresh_mods_watch(&mut hmr, &res.mods);
        info!("[HMR] Refreshed mod registry ({} active mods)", res.mods.mods.len());
    }

    let mut force_stage = mods_changed;
    let mut force_chars = mods_changed;
    let mut force_skin = mods_changed;
    let mut force_audio = mods_changed;
    let mut reload_scripts = matches!(hmr.script_watch.diff(), DomainDiff::Full(_)) || mods_changed;

    if !matches!(hmr.chart_watch.diff(), DomainDiff::Unchanged) {
        let delta = reload_chart_domain(&mut res);
        force_stage |= delta.stage_changed;
        force_chars |= delta.chars_changed;
        force_skin |= delta.skin_changed;
        force_audio |= delta.audio_changed;
        reload_scripts |= delta.stage_changed || delta.chars_changed;
        hmr.chart_watch = build_chart_watch(&res.mods, &res.state);
    }

    if !res.state.is_minimal_mode {
        let stage_diff = if force_stage {
            DomainDiff::Full(Vec::new())
        } else {
            hmr.stage_watch.diff()
        };
        match stage_diff {
            DomainDiff::Unchanged => {}
            DomainDiff::InPlaceAssets(paths) => {
                reload_asset_paths(&res.asset_server, &paths);
                let _ = reload_stage_domain(&mut commands, &mut res, &mut q);
                info!("[HMR] Reloaded {} stage texture(s)", paths.len());
                hmr.stage_watch = build_stage_watch(
                    &res.mods,
                    &res.state.chart.stage_id,
                    res.stage_res.as_deref(),
                );
            }
            DomainDiff::Full(paths) => {
                reload_asset_paths(&res.asset_server, &paths);
                let old_scripts_flag = res.stage_res.as_ref().is_some_and(|s| s.scripts);
                if reload_stage_domain(&mut commands, &mut res, &mut q) {
                    let new_scripts_flag = res.stage_res.as_ref().is_some_and(|s| s.scripts);
                    if old_scripts_flag != new_scripts_flag || new_scripts_flag {
                        reload_scripts = true;
                    }
                }
                let new_watch = build_stage_watch(
                    &res.mods,
                    &res.state.chart.stage_id,
                    res.stage_res.as_deref(),
                );
                let extra_paths = new_watch.collect_changed_or_new_server_paths(&hmr.stage_watch);
                reload_asset_paths(&res.asset_server, &extra_paths);
                hmr.stage_watch = new_watch;
            }
        }

        if force_chars {
            reload_all_characters_domain(&mut commands, &mut res, &mut q);
            rebuild_character_watches(&mut hmr, &res);
        } else {
            let char_ids: Vec<String> = hmr.char_watches.keys().cloned().collect();
            for char_id in char_ids {
                let Some(old_watch) = hmr.char_watches.get(&char_id).cloned() else {
                    continue;
                };
                match old_watch.diff() {
                    DomainDiff::Unchanged => {}
                    DomainDiff::InPlaceAssets(paths) => {
                        reload_asset_paths(&res.asset_server, &paths);
                        reload_single_character_domain(&mut commands, &mut res, &mut q, &char_id);
                        info!("[HMR] Reloaded image(s) for character '{char_id}'");
                        let loaded = find_loaded_char(res.chars_res.as_deref(), &char_id);
                        hmr.char_watches
                            .insert(char_id.clone(), build_character_watch(&res.mods, &char_id, loaded));
                    }
                    DomainDiff::Full(paths) => {
                        reload_asset_paths(&res.asset_server, &paths);
                        reload_single_character_domain(&mut commands, &mut res, &mut q, &char_id);
                        let loaded = find_loaded_char(res.chars_res.as_deref(), &char_id);
                        let new_watch = build_character_watch(&res.mods, &char_id, loaded);
                        let extra_paths = new_watch.collect_changed_or_new_server_paths(&old_watch);
                        reload_asset_paths(&res.asset_server, &extra_paths);
                        hmr.char_watches.insert(char_id, new_watch);
                    }
                }
            }
        }
    }

    let skin_diff = if force_skin {
        DomainDiff::Full(Vec::new())
    } else {
        hmr.skin_watch.diff()
    };
    match skin_diff {
        DomainDiff::Unchanged => {}
        DomainDiff::InPlaceAssets(paths) => {
            reload_asset_paths(&res.asset_server, &paths);
            info!("[HMR] Reloaded {} skin asset(s)", paths.len());
            hmr.skin_watch = build_skin_watch(&res.mods, &res.state.player_strumline.note_style);
        }
        DomainDiff::Full(paths) => {
            reload_asset_paths(&res.asset_server, &paths);
            reload_skin_domain(&mut commands, &mut res, &mut q);
            let new_watch = build_skin_watch(&res.mods, &res.state.player_strumline.note_style);
            let extra_paths = new_watch.collect_changed_or_new_server_paths(&hmr.skin_watch);
            reload_asset_paths(&res.asset_server, &extra_paths);
            hmr.skin_watch = new_watch;
        }
    }

    let audio_diff = if force_audio {
        DomainDiff::Full(Vec::new())
    } else {
        hmr.audio_watch.diff()
    };
    match audio_diff {
        DomainDiff::Unchanged => {}
        DomainDiff::InPlaceAssets(paths) | DomainDiff::Full(paths) => {
            if !paths.is_empty() {
                reload_asset_paths(&res.asset_server, &paths);
                info!("[HMR] Reloaded {} audio track(s)", paths.len());
            }
            let new_watch = build_audio_watch(&res.mods, &res.state);
            let extra_paths = new_watch.collect_changed_or_new_server_paths(&hmr.audio_watch);
            reload_asset_paths(&res.asset_server, &extra_paths);
            hmr.audio_watch = new_watch;
        }
    }

    if reload_scripts {
        reload_scripts_domain(&mut commands, &mut res, &q);
        hmr.script_watch =
            build_script_watch(&res.mods, &res.state, res.stage_res.as_deref(), &res.script_host);
    }
}

fn find_loaded_char<'a>(
    chars: Option<&'a CharactersResource>,
    char_id: &str,
) -> Option<&'a LoadedCharacter> {
    chars.and_then(|cr| cr.characters.iter().find(|c| c.id == char_id))
}

fn reload_asset_paths(asset_server: &AssetServer, server_paths: &[String]) {
    let mut seen = BTreeSet::new();
    for path in server_paths {
        if !path.is_empty() && seen.insert(path.as_str()) {
            asset_server.reload(path.clone());
        }
    }
}

fn check_mods_structure_changed(hmr: &HotReloadState, mods: &ModRegistry) -> bool {
    if let Some(old_dir) = &hmr.mods_dir_stamp
        && &DirStamp::from_path(&mods.mods_dir) != old_dir
    {
        return true;
    }
    for (manifest_path, old_stamp) in &hmr.mod_manifest_stamps {
        if &FileStamp::from_path(manifest_path) != old_stamp {
            return true;
        }
    }
    false
}

fn refresh_mods_watch(hmr: &mut HotReloadState, mods: &ModRegistry) {
    hmr.mods_dir_stamp = Some(DirStamp::from_path(&mods.mods_dir));
    hmr.mod_manifest_stamps.clear();
    for loaded_mod in &mods.mods {
        if let Some(manifest) = &loaded_mod.manifest_path {
            hmr.mod_manifest_stamps
                .insert(manifest.clone(), FileStamp::from_path(manifest));
        }
    }
}

fn refresh_all_watches(hmr: &mut HotReloadState, res: &HotReloadRes) {
    refresh_mods_watch(hmr, &res.mods);
    hmr.chart_watch = build_chart_watch(&res.mods, &res.state);
    hmr.stage_watch = build_stage_watch(
        &res.mods,
        &res.state.chart.stage_id,
        res.stage_res.as_deref(),
    );
    rebuild_character_watches(hmr, res);
    hmr.skin_watch = build_skin_watch(&res.mods, &res.state.player_strumline.note_style);
    hmr.script_watch = build_script_watch(
        &res.mods,
        &res.state,
        res.stage_res.as_deref(),
        &res.script_host,
    );
    hmr.audio_watch = build_audio_watch(&res.mods, &res.state);
}

fn rebuild_character_watches(hmr: &mut HotReloadState, res: &HotReloadRes) {
    hmr.char_watches.clear();
    let mut ids = BTreeSet::new();
    for id in res
        .state
        .chart
        .characters
        .spectator
        .iter()
        .chain(res.state.chart.characters.opponents.iter())
        .chain(res.state.chart.characters.players.iter())
    {
        if !id.trim().is_empty() {
            ids.insert(id.clone());
        }
    }
    if let Some(chars) = &res.chars_res {
        for c in &chars.characters {
            ids.insert(c.id.clone());
        }
    }
    for char_id in ids {
        let loaded = find_loaded_char(res.chars_res.as_deref(), &char_id);
        hmr.char_watches
            .insert(char_id.clone(), build_character_watch(&res.mods, &char_id, loaded));
    }
}

fn track_vfs_file_across_layers(watch: &mut DomainWatch, mods: &ModRegistry, rel: &str, is_json: bool) {
    let Ok(norm) = ModRegistry::normalize_rel_path(rel) else {
        return;
    };
    let track = |w: &mut DomainWatch, path: PathBuf| {
        if is_json {
            w.track_json(path);
        } else {
            w.track_meta(path);
        }
    };
    if !mods.assets_dir.as_os_str().is_empty() {
        track(watch, mods.assets_dir.join(&norm));
    }
    for loaded_mod in &mods.mods {
        track(watch, loaded_mod.root.join(&norm));
        if is_json {
            watch.track_json(loaded_mod.root.join("_merge").join(&norm));
            watch.track_json(loaded_mod.root.join("_append").join(&norm));
        }
    }
}

fn track_vfs_asset_across_layers(watch: &mut DomainWatch, mods: &ModRegistry, rel: &str) {
    let Ok(norm) = ModRegistry::normalize_rel_path(rel) else {
        return;
    };
    if !mods.assets_dir.as_os_str().is_empty() {
        let p = mods.assets_dir.join(&norm);
        watch.track_asset(p.clone(), Some(mods.to_asset_server_path(&p)));
    }
    for loaded_mod in &mods.mods {
        let p = loaded_mod.root.join(&norm);
        watch.track_asset(p.clone(), Some(mods.to_asset_server_path(&p)));
    }
}

fn track_vfs_dir_across_layers(watch: &mut DomainWatch, mods: &ModRegistry, rel_dir: &str) {
    let Ok(norm) = ModRegistry::normalize_rel_path(rel_dir) else {
        return;
    };
    if !mods.assets_dir.as_os_str().is_empty() {
        watch.track_dir(mods.assets_dir.join(&norm));
    }
    for loaded_mod in &mods.mods {
        watch.track_dir(loaded_mod.root.join(&norm));
    }
}

fn track_rel_stem_candidates(
    watch: &mut DomainWatch,
    mods: &ModRegistry,
    candidates: &[String],
    meta_exts: &[&str],
    include_animate_atlas: bool,
) {
    for base in candidates {
        let Ok(norm) = ModRegistry::normalize_rel_path(base) else {
            continue;
        };
        let norm_str = norm.to_string_lossy().replace('\\', "/");
        if let Some(parent) = Path::new(&norm_str).parent().and_then(|p| p.to_str())
            && !parent.is_empty()
        {
            track_vfs_dir_across_layers(watch, mods, parent);
        }

        if include_animate_atlas {
            let dir_rel = Path::new(&norm_str)
                .with_extension("")
                .to_string_lossy()
                .replace('\\', "/");
            track_vfs_dir_across_layers(watch, mods, &dir_rel);
            track_vfs_file_across_layers(watch, mods, &format!("{dir_rel}/Animation.json"), false);
            for i in 1..=MAX_SPRITEMAP_WATCH {
                track_vfs_file_across_layers(watch, mods, &format!("{dir_rel}/spritemap{i}.json"), false);
                track_vfs_asset_across_layers(watch, mods, &format!("{dir_rel}/spritemap{i}.png"));
            }
        }

        for ext in ["png", "jpg", "jpeg", "webp"] {
            let probe = ensure_ext_rel(&norm_str, ext);
            track_vfs_asset_across_layers(watch, mods, &probe);
        }
        for meta_ext in meta_exts {
            let probe = ensure_ext_rel(&norm_str, meta_ext);
            track_vfs_file_across_layers(watch, mods, &probe, false);
        }
    }
}

fn ensure_ext_rel(rel: &str, ext: &str) -> String {
    let path = Path::new(rel);
    if path.extension().is_some() {
        path.with_extension(ext).to_string_lossy().replace('\\', "/")
    } else {
        format!("{rel}.{ext}")
    }
}

fn build_chart_watch(mods: &ModRegistry, state: &PlayState) -> DomainWatch {
    let mut w = DomainWatch::default();
    let song_id = state.chart.song_id.trim();
    if song_id.is_empty() {
        return w;
    }
    for dir_rel in [
        format!("gameplay/songs/{song_id}/chart"),
        format!("songs/{song_id}/chart"),
        format!("songs/{song_id}"),
    ] {
        track_vfs_dir_across_layers(&mut w, mods, &dir_rel);
        for file in ["meta.json", "notes.json", "events.json"] {
            track_vfs_file_across_layers(&mut w, mods, &format!("{dir_rel}/{file}"), true);
        }
    }
    w
}

fn build_stage_watch(
    mods: &ModRegistry,
    stage_id: &str,
    stage_opt: Option<&StageResource>,
) -> DomainWatch {
    let mut w = DomainWatch::default();
    if stage_id.trim().is_empty() {
        return w;
    }
    track_vfs_file_across_layers(&mut w, mods, &format!("{STAGES_ASSET_PATH}/{stage_id}/{stage_id}.json"), true);
    track_vfs_file_across_layers(&mut w, mods, &format!("stages/{stage_id}/{stage_id}.json"), true);
    track_vfs_dir_across_layers(&mut w, mods, &format!("{STAGES_ASSET_PATH}/{stage_id}"));
    track_vfs_dir_across_layers(&mut w, mods, &format!("stages/{stage_id}"));

    let mut prop_files: BTreeSet<String> = BTreeSet::new();
    if let Some(stage) = stage_opt {
        for resolved in &stage.props {
            prop_files.insert(resolved.data.file.clone());
        }
    }
    // Parse disk JSON directly so unresolved/new/re-targeted prop files are also watched.
    let json_path = mods
        .resolve_file(&format!("{STAGES_ASSET_PATH}/{stage_id}/{stage_id}.json"))
        .or_else(|| mods.resolve_file(&format!("stages/{stage_id}/{stage_id}.json")));
    if let Some(path) = json_path
        && let Ok(raw) = fs::read_to_string(&path)
        && let Ok(parsed) = parse_stage_json(stage_id, &raw)
    {
        for prop in parsed.props {
            prop_files.insert(prop.file);
        }
    }

    for raw_file in prop_files {
        let file = raw_file.trim();
        if file.is_empty() || file.starts_with('#') || file.starts_with("0x") {
            continue;
        }
        let cands = stage_rel_candidates_for_watch(stage_id, file);
        track_rel_stem_candidates(&mut w, mods, &cands, &["xml"], true);

        if let Some(atlas_dir) = resolve_atlas_dir_in(mods, stage_id, file) {
            w.track_dir(atlas_dir.clone());
            w.track_meta(atlas_dir.join("Animation.json"));
            for i in 1..=MAX_SPRITEMAP_WATCH {
                w.track_meta(atlas_dir.join(format!("spritemap{i}.json")));
                let spr = atlas_dir.join(format!("spritemap{i}.png"));
                w.track_asset(spr.clone(), Some(mods.to_asset_server_path(&spr)));
            }
        }
        if let Some(img_path) = resolve_stage_image_in(mods, stage_id, file) {
            if let Some(parent) = img_path.parent() {
                w.track_dir(parent.to_path_buf());
            }
            w.track_asset(img_path.clone(), Some(mods.to_asset_server_path(&img_path)));
            w.track_meta(img_path.with_extension("xml"));
        }
    }
    w
}

fn read_char_json_data(mods: &ModRegistry, char_id: &str) -> Option<CharacterData> {
    let json_path = mods
        .resolve_file(&format!("gameplay/characters/{char_id}/{char_id}.json"))
        .or_else(|| mods.resolve_file(&format!("characters/{char_id}/{char_id}.json")))?;
    let raw = fs::read_to_string(json_path).ok()?;
    serde_json::from_str(raw.trim_start_matches('\u{feff}')).ok()
}

fn build_character_watch(
    mods: &ModRegistry,
    char_id: &str,
    loaded_opt: Option<&LoadedCharacter>,
) -> DomainWatch {
    let mut w = DomainWatch::default();
    track_vfs_file_across_layers(&mut w, mods, &format!("gameplay/characters/{char_id}/{char_id}.json"), true);
    track_vfs_file_across_layers(&mut w, mods, &format!("characters/{char_id}/{char_id}.json"), true);
    track_vfs_dir_across_layers(&mut w, mods, &format!("gameplay/characters/{char_id}"));
    track_vfs_dir_across_layers(&mut w, mods, &format!("characters/{char_id}"));

    let disk_data = read_char_json_data(mods, char_id);
    let data_ref = disk_data.as_ref().or_else(|| loaded_opt.map(|l| &l.data));

    let mut raw_assets: BTreeSet<String> = BTreeSet::new();
    if let Some(data) = data_ref {
        if let Some(base_asset) = &data.asset_path {
            raw_assets.insert(base_asset.clone());
        } else {
            raw_assets.insert(char_id.to_string());
        }
        for anim in &data.animations {
            if let Some(sub) = &anim.asset_path {
                raw_assets.insert(sub.clone());
            }
        }
    } else {
        raw_assets.insert(char_id.to_string());
    }

    for raw in raw_assets {
        let cands = char_rel_candidates_for_watch(char_id, &raw);
        track_rel_stem_candidates(&mut w, mods, &cands, &["xml", "txt"], true);
    }

    let icon_id = data_ref
        .and_then(|d| d.health_icon.as_ref())
        .and_then(|hi| hi.id.as_deref())
        .unwrap_or(char_id);
    for icon_rel in [
        format!("gameplay/characters/{char_id}/icon-{icon_id}"),
        format!("gameplay/characters/{icon_id}/icon-{icon_id}"),
        format!("images/icons/icon-{icon_id}"),
    ] {
        track_vfs_asset_across_layers(&mut w, mods, &format!("{icon_rel}.png"));
        track_vfs_file_across_layers(&mut w, mods, &format!("{icon_rel}.xml"), false);
    }
    w
}

fn track_skin_rel(
    watch: &mut DomainWatch,
    mods: &ModRegistry,
    style: &NoteStyle,
    rel: &str,
    has_xml: bool,
    default_ext: &str,
) {
    let cands = style.skin_rel_candidates_for_watch(rel);
    for base in cands {
        if let Some(parent) = Path::new(&base).parent().and_then(|p| p.to_str())
            && !parent.is_empty()
        {
            track_vfs_dir_across_layers(watch, mods, parent);
        }
        let asset_rel = ensure_ext_rel(&base, default_ext);
        track_vfs_asset_across_layers(watch, mods, &asset_rel);
        if has_xml {
            let xml_rel = ensure_ext_rel(&base, "xml");
            track_vfs_file_across_layers(watch, mods, &xml_rel, false);
        }
    }
}

fn build_skin_watch(mods: &ModRegistry, style: &NoteStyle) -> DomainWatch {
    let mut w = DomainWatch::default();
    let id = &style.id;
    track_vfs_file_across_layers(&mut w, mods, &format!("gameplay/noteskins/{id}/{id}.json"), true);
    track_vfs_file_across_layers(&mut w, mods, &format!("noteskins/{id}/{id}.json"), true);
    track_vfs_dir_across_layers(&mut w, mods, &format!("gameplay/noteskins/{id}"));
    track_vfs_dir_across_layers(&mut w, mods, &format!("noteskins/{id}"));

    track_skin_rel(&mut w, mods, style, &style.strum_path, true, "png");
    track_skin_rel(&mut w, mods, style, &style.notes_path, true, "png");
    track_skin_rel(&mut w, mods, style, &style.sustains_path, true, "png");
    track_skin_rel(&mut w, mods, style, &style.splash_path, true, "png");
    for dir_cfg in &style.hold_cover_dirs {
        track_skin_rel(&mut w, mods, style, &dir_cfg.path, true, "png");
    }
    if let Some(hb) = &style.health_bar.path {
        track_skin_rel(&mut w, mods, style, hb, false, "png");
    }
    for item in style.popup.judgments.iter().chain(style.countdown_images.iter()) {
        if let Some(rel) = &item.path {
            track_skin_rel(&mut w, mods, style, rel, false, "png");
        }
    }
    for rel in style.popup.combo_digits.iter().flatten() {
        track_skin_rel(&mut w, mods, style, rel, false, "png");
    }
    for rel in style.countdown_sounds.iter().flatten() {
        track_skin_rel(&mut w, mods, style, rel, false, "ogg");
    }
    for rel in &style.miss_sounds {
        track_skin_rel(&mut w, mods, style, rel, false, "ogg");
    }
    w
}

fn is_script_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "rhai" | "lua" | "hxc"))
}

fn track_script_dir_tree(watch: &mut DomainWatch, dir: &Path, depth: u8) {
    watch.track_dir(dir.to_path_buf());
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let hidden = p
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('.'));
        if hidden {
            continue;
        }
        if p.is_dir() && depth > 0 {
            track_script_dir_tree(watch, &p, depth - 1);
        } else if p.is_file() && is_script_file(&p) {
            watch.track_meta(p);
        }
    }
}

fn build_script_watch(
    mods: &ModRegistry,
    state: &PlayState,
    stage: Option<&StageResource>,
    script_host: &ScriptHost,
) -> DomainWatch {
    let mut w = DomainWatch::default();
    for path in script_host.watched_script_paths() {
        if let Some(parent) = path.parent() {
            w.track_dir(parent.to_path_buf());
        }
        w.track_meta(path);
    }
    if let Some(st) = stage {
        for ext in ["rhai", "lua", "hxc"] {
            w.track_meta(st.json_path.with_extension(ext));
        }
        if let Some(parent) = st.json_path.parent() {
            track_script_dir_tree(&mut w, parent, 2);
        }
    }
    let stage_id = &state.chart.stage_id;
    let song_id = &state.chart.song_id;

    if !mods.assets_dir.as_os_str().is_empty() {
        track_script_dir_tree(&mut w, &mods.assets_dir.join("scripts"), 4);
        track_script_dir_tree(&mut w, &mods.assets_dir.join("gameplay").join("scripts"), 4);
        if !stage_id.is_empty() {
            track_script_dir_tree(&mut w, &mods.assets_dir.join(STAGES_ASSET_PATH).join(stage_id), 2);
        }
        if !song_id.is_empty() {
            track_script_dir_tree(&mut w, &mods.assets_dir.join("gameplay").join("songs").join(song_id), 2);
        }
    }
    for loaded_mod in &mods.mods {
        track_script_dir_tree(&mut w, &loaded_mod.root.join("scripts"), 4);
        track_script_dir_tree(&mut w, &loaded_mod.root.join("gameplay").join("scripts"), 4);
        if !stage_id.is_empty() {
            track_script_dir_tree(&mut w, &loaded_mod.root.join("stages").join(stage_id), 2);
            track_script_dir_tree(&mut w, &loaded_mod.root.join(STAGES_ASSET_PATH).join(stage_id), 2);
        }
        if !song_id.is_empty() {
            track_script_dir_tree(&mut w, &loaded_mod.root.join("songs").join(song_id), 2);
            track_script_dir_tree(&mut w, &loaded_mod.root.join("gameplay").join("songs").join(song_id), 2);
        }
    }
    w
}

fn build_audio_watch(mods: &ModRegistry, state: &PlayState) -> DomainWatch {
    let mut w = DomainWatch::default();
    let song_id = &state.chart.song_id;
    if song_id.trim().is_empty() {
        return w;
    }
    for rel in [
        format!("gameplay/songs/{song_id}/song/Inst.ogg"),
        format!("songs/{song_id}/song/Inst.ogg"),
        format!("songs/{song_id}/Inst.ogg"),
        song_asset_path_in(mods, song_id, "Inst.ogg"),
    ] {
        track_vfs_asset_across_layers(&mut w, mods, &rel);
    }
    for track in state.chart.audio.instrumental.values() {
        track_vfs_asset_across_layers(&mut w, mods, &song_asset_path_in(mods, song_id, &track.file));
    }
    for vocal in state
        .chart
        .audio
        .player_vocal
        .iter()
        .chain(state.chart.audio.opponent_vocal.iter())
        .chain(state.chart.audio.spectator_vocal.iter())
    {
        track_vfs_asset_across_layers(&mut w, mods, &song_asset_path_in(mods, song_id, &vocal.file));
    }
    if !state.chart.audio.shared_vocal_file.trim().is_empty() {
        track_vfs_asset_across_layers(
            &mut w,
            mods,
            &song_asset_path_in(mods, song_id, &state.chart.audio.shared_vocal_file),
        );
    }
    w
}

#[derive(Default)]
struct ChartReloadDelta {
    stage_changed: bool,
    chars_changed: bool,
    skin_changed: bool,
    audio_changed: bool,
}

fn reload_chart_domain(res: &mut HotReloadRes) -> ChartReloadDelta {
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

fn reload_stage_domain(
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

fn reload_all_characters_domain(
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

fn reload_single_character_domain(
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

fn reload_skin_domain(
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

fn reload_scripts_domain(
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
