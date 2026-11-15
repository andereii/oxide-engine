use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::mods::{ModRegistry, ModStore};
use crate::play::characters::loader::{
    CharactersResource, LoadedCharacter, load_single_character,
};
use crate::play::characters::spawn::spawn_single_character;
use crate::play::characters::{CharacterActor, CharacterRole, CharacterSyncState};
use crate::play::hud::countdown::{CountdownAssets, load_countdown_assets};
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
use crate::play::song::song_asset_path_in;
use crate::play::stage::paths::{resolve_atlas_dir_in, resolve_stage_image_in};
use crate::play::stage::{
    DynamicScriptProp, StageCompositePiece, StagePropVisual, StageResource, StageSyncState,
    load_stage_resource, spawn_stage_props,
};
use crate::scripting::ScriptHost;
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH, STAGES_ASSET_PATH};

const HMR_POLL_INTERVAL_S: f32 = 0.25;

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
}

#[derive(Resource, Default)]
pub struct HotReloadState {
    timer_s: f32,
    initialized: bool,
    mods_dir_stamp: Option<DirStamp>,
    mod_manifest_stamps: BTreeMap<PathBuf, FileStamp>,
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

    let stage_diff = if !res.state.is_minimal_mode {
        hmr.stage_watch.diff()
    } else {
        DomainDiff::Unchanged
    };

    let mut reload_scripts = matches!(hmr.script_watch.diff(), DomainDiff::Full(_)) || mods_changed;

    match stage_diff {
        DomainDiff::Unchanged => {}
        DomainDiff::InPlaceAssets(paths) => {
            reload_asset_paths(&res.asset_server, &paths);
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
            hmr.stage_watch = build_stage_watch(
                &res.mods,
                &res.state.chart.stage_id,
                res.stage_res.as_deref(),
            );
        }
    }

    if !res.state.is_minimal_mode {
        let char_ids: Vec<String> = hmr.char_watches.keys().cloned().collect();
        for char_id in char_ids {
            let Some(watch) = hmr.char_watches.get(&char_id) else {
                continue;
            };
            match watch.diff() {
                DomainDiff::Unchanged => {}
                DomainDiff::InPlaceAssets(paths) => {
                    reload_asset_paths(&res.asset_server, &paths);
                    info!("[HMR] Reloaded image(s) for character '{char_id}'");
                    if let Some(loaded) = res
                        .chars_res
                        .as_ref()
                        .and_then(|cr| cr.characters.iter().find(|c| c.id == char_id))
                    {
                        let w = build_character_watch(&res.mods, loaded);
                        hmr.char_watches.insert(char_id, w);
                    }
                }
                DomainDiff::Full(paths) => {
                    reload_asset_paths(&res.asset_server, &paths);
                    reload_single_character_domain(&mut commands, &mut res, &mut q, &char_id);
                    if let Some(loaded) = res
                        .chars_res
                        .as_ref()
                        .and_then(|cr| cr.characters.iter().find(|c| c.id == char_id))
                    {
                        let w = build_character_watch(&res.mods, loaded);
                        hmr.char_watches.insert(char_id, w);
                    }
                }
            }
        }
    }

    match hmr.skin_watch.diff() {
        DomainDiff::Unchanged => {}
        DomainDiff::InPlaceAssets(paths) => {
            reload_asset_paths(&res.asset_server, &paths);
            info!("[HMR] Reloaded {} skin asset(s)", paths.len());
            hmr.skin_watch = build_skin_watch(&res.mods, &res.state.player_strumline.note_style);
        }
        DomainDiff::Full(paths) => {
            reload_asset_paths(&res.asset_server, &paths);
            reload_skin_domain(&mut commands, &mut res, &mut q);
            hmr.skin_watch = build_skin_watch(&res.mods, &res.state.player_strumline.note_style);
        }
    }

    match hmr.audio_watch.diff() {
        DomainDiff::Unchanged => {}
        DomainDiff::InPlaceAssets(paths) | DomainDiff::Full(paths) => {
            if !paths.is_empty() {
                reload_asset_paths(&res.asset_server, &paths);
                info!("[HMR] Reloaded {} audio track(s)", paths.len());
            }
            hmr.audio_watch = build_audio_watch(&res.mods, &res.state);
        }
    }

    if reload_scripts {
        reload_scripts_domain(&mut commands, &mut res, &q);
        hmr.script_watch =
            build_script_watch(&res.mods, &res.state, res.stage_res.as_deref(), &res.script_host);
    }
}

fn reload_asset_paths(asset_server: &AssetServer, server_paths: &[String]) {
    let mut seen = BTreeSet::new();
    for path in server_paths {
        if seen.insert(path.as_str()) {
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
    hmr.stage_watch = build_stage_watch(
        &res.mods,
        &res.state.chart.stage_id,
        res.stage_res.as_deref(),
    );
    hmr.char_watches.clear();
    if let Some(chars) = &res.chars_res {
        for c in &chars.characters {
            hmr.char_watches
                .insert(c.id.clone(), build_character_watch(&res.mods, c));
        }
    }
    hmr.skin_watch = build_skin_watch(&res.mods, &res.state.player_strumline.note_style);
    hmr.script_watch = build_script_watch(
        &res.mods,
        &res.state,
        res.stage_res.as_deref(),
        &res.script_host,
    );
    hmr.audio_watch = build_audio_watch(&res.mods, &res.state);
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

    if let Some(stage) = stage_opt {
        for resolved in &stage.props {
            let file = resolved.data.file.trim();
            if file.is_empty() || file.starts_with('#') || file.starts_with("0x") {
                continue;
            }
            if let Some(atlas_dir) = resolve_atlas_dir_in(mods, stage_id, file) {
                w.track_dir(atlas_dir.clone());
                w.track_meta(atlas_dir.join("Animation.json"));
                w.track_meta(atlas_dir.join("spritemap1.json"));
                let spr = atlas_dir.join("spritemap1.png");
                if spr.is_file() {
                    w.track_asset(spr.clone(), Some(mods.to_asset_server_path(&spr)));
                }
            }
            if let Some(img_path) = resolve_stage_image_in(mods, stage_id, file) {
                if let Some(parent) = img_path.parent() {
                    w.track_dir(parent.to_path_buf());
                }
                w.track_asset(img_path.clone(), Some(mods.to_asset_server_path(&img_path)));
                let xml = img_path.with_extension("xml");
                if xml.is_file() {
                    w.track_meta(xml);
                }
            }
        }
    }
    w
}

fn track_char_raw_asset(watch: &mut DomainWatch, mods: &ModRegistry, char_id: &str, raw: &str) {
    let cleaned = raw.trim().replace('\\', "/");
    let cleaned = cleaned.trim_start_matches("./").trim_start_matches('/');
    let cands = if cleaned.starts_with("gameplay/") || cleaned.starts_with("characters/") {
        vec![cleaned.to_string(), format!("gameplay/characters/{char_id}/{cleaned}")]
    } else {
        vec![
            format!("gameplay/characters/{char_id}/{cleaned}"),
            format!("gameplay/characters/{cleaned}"),
            format!("characters/{char_id}/{cleaned}"),
            format!("characters/{cleaned}"),
        ]
    };

    for base in cands {
        if let Some(dir) = mods.resolve_dir(&base) {
            watch.track_dir(dir.clone());
            let anim_json = dir.join("Animation.json");
            if anim_json.is_file() {
                watch.track_meta(anim_json);
                let map_json = dir.join("spritemap1.json");
                if map_json.is_file() {
                    watch.track_meta(map_json);
                }
                let spr = dir.join("spritemap1.png");
                if spr.is_file() {
                    watch.track_asset(spr.clone(), Some(mods.to_asset_server_path(&spr)));
                }
            }
        }
        for ext in ["png", "jpg", "jpeg", "webp"] {
            let probe = if Path::new(&base).extension().is_some() {
                Path::new(&base)
                    .with_extension(ext)
                    .to_string_lossy()
                    .replace('\\', "/")
            } else {
                format!("{base}.{ext}")
            };
            if let Some(img) = mods.resolve_file(&probe) {
                if let Some(parent) = img.parent() {
                    watch.track_dir(parent.to_path_buf());
                }
                watch.track_asset(img.clone(), Some(mods.to_asset_server_path(&img)));
            }
        }
        for meta_ext in ["xml", "txt"] {
            let probe = if Path::new(&base).extension().is_some() {
                Path::new(&base)
                    .with_extension(meta_ext)
                    .to_string_lossy()
                    .replace('\\', "/")
            } else {
                format!("{base}.{meta_ext}")
            };
            if let Some(meta) = mods.resolve_file(&probe) {
                watch.track_meta(meta);
            }
        }
    }
}

fn build_character_watch(mods: &ModRegistry, loaded: &LoadedCharacter) -> DomainWatch {
    let mut w = DomainWatch::default();
    let char_id = &loaded.id;
    track_vfs_file_across_layers(&mut w, mods, &format!("gameplay/characters/{char_id}/{char_id}.json"), true);
    track_vfs_file_across_layers(&mut w, mods, &format!("characters/{char_id}/{char_id}.json"), true);
    track_vfs_dir_across_layers(&mut w, mods, &format!("gameplay/characters/{char_id}"));
    track_vfs_dir_across_layers(&mut w, mods, &format!("characters/{char_id}"));

    if let Some(base_asset) = &loaded.data.asset_path {
        track_char_raw_asset(&mut w, mods, char_id, base_asset);
    } else {
        track_char_raw_asset(&mut w, mods, char_id, char_id);
    }
    for anim in &loaded.data.animations {
        if let Some(sub) = &anim.asset_path {
            track_char_raw_asset(&mut w, mods, char_id, sub);
        }
    }
    let icon_id = loaded
        .data
        .health_icon
        .id
        .as_deref()
        .unwrap_or(char_id.as_str());
    for icon_rel in [
        format!("gameplay/characters/{char_id}/icon-{icon_id}.png"),
        format!("images/icons/icon-{icon_id}.png"),
    ] {
        if let Some(icon_path) = mods.resolve_file(&icon_rel) {
            w.track_asset(icon_path.clone(), Some(mods.to_asset_server_path(&icon_path)));
        }
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
    let cleaned = rel.trim().replace('\\', "/");
    let cleaned = cleaned.trim_start_matches("./").trim_start_matches('/');
    for base in [
        format!("gameplay/noteskins/{}/{cleaned}", style.id),
        format!("noteskins/{}/{cleaned}", style.id),
        cleaned.to_string(),
    ] {
        let asset_rel = if Path::new(&base).extension().is_some() {
            base.clone()
        } else {
            format!("{base}.{default_ext}")
        };
        if let Some(p) = mods.resolve_file(&asset_rel) {
            if let Some(parent) = p.parent() {
                watch.track_dir(parent.to_path_buf());
            }
            watch.track_asset(p.clone(), Some(mods.to_asset_server_path(&p)));
        }
    }
    if has_xml {
        let xml = style.disk_xml_in(mods, rel);
        if xml.is_file() {
            watch.track_meta(xml);
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

fn build_script_watch(
    mods: &ModRegistry,
    state: &PlayState,
    stage: Option<&StageResource>,
    script_host: &ScriptHost,
) -> DomainWatch {
    let mut w = DomainWatch::default();
    for path in script_host.watched_script_paths() {
        w.track_meta(path);
    }
    if let Some(st) = stage {
        let sidecar = st.json_path.with_extension("rhai");
        w.track_meta(sidecar);
        if let Some(parent) = st.json_path.parent() {
            w.track_dir(parent.to_path_buf());
        }
    }
    let stage_id = &state.chart.stage_id;
    let song_id = &state.chart.song_id;
    for loaded_mod in &mods.mods {
        w.track_dir(loaded_mod.root.join("scripts"));
        if !stage_id.is_empty() {
            w.track_dir(loaded_mod.root.join("stages").join(stage_id));
            w.track_dir(loaded_mod.root.join(STAGES_ASSET_PATH).join(stage_id));
        }
        if !song_id.is_empty() {
            w.track_dir(loaded_mod.root.join("songs").join(song_id));
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
        if let Some(p) = mods.resolve_file(&rel) {
            w.track_asset(p.clone(), Some(mods.to_asset_server_path(&p)));
        }
    }
    for track in &state.chart.audio.tracks {
        if let Some(p) = mods.resolve_file(&song_asset_path_in(mods, song_id, &track.file)) {
            w.track_asset(p.clone(), Some(mods.to_asset_server_path(&p)));
        }
    }
    for vocal in &state.chart.audio.player_vocals {
        if let Some(p) = mods.resolve_file(&song_asset_path_in(mods, song_id, &vocal.file)) {
            w.track_asset(p.clone(), Some(mods.to_asset_server_path(&p)));
        }
    }
    for vocal in &state.chart.audio.opponent_vocals {
        if let Some(p) = mods.resolve_file(&song_asset_path_in(mods, song_id, &vocal.file)) {
            w.track_asset(p.clone(), Some(mods.to_asset_server_path(&p)));
        }
    }
    if !state.chart.audio.shared_vocal_file.trim().is_empty()
        && let Some(p) = mods.resolve_file(&song_asset_path_in(
            mods,
            song_id,
            &state.chart.audio.shared_vocal_file,
        ))
    {
        w.track_asset(p.clone(), Some(mods.to_asset_server_path(&p)));
    }
    w
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

fn reload_single_character_domain(
    commands: &mut Commands,
    res: &mut HotReloadRes,
    q: &mut HotReloadQueries,
    char_id: &str,
) {
    let mut targets: Vec<(Entity, CharacterRole, crate::play::stage::schema::StageCharacterSlot)> =
        Vec::new();
    for (ent, actor) in &q.actors {
        if actor.id == char_id {
            commands.entity(actor.sheet_entity).despawn();
            for &piece in &actor.piece_entities {
                commands.entity(piece).despawn();
            }
            commands.entity(ent).despawn();
            targets.push((ent, actor.role, actor.slot.clone()));
        }
    }
    if targets.is_empty() {
        return;
    }

    for (_, role, slot) in targets {
        match load_single_character(
            &res.mods,
            char_id,
            role,
            slot,
            &res.asset_server,
            &mut res.mut_layouts,
        ) {
            Ok(loaded) => {
                spawn_single_character(commands, &loaded);
                if let Some(ref mut chars_res) = res.chars_res {
                    if let Some(existing) = chars_res
                        .characters
                        .iter_mut()
                        .find(|c| c.id == char_id && c.role == role)
                    {
                        *existing = loaded;
                    } else {
                        chars_res.characters.push(loaded);
                    }
                }
            }
            Err(err) => {
                warn!("[HMR] Failed to reload character '{char_id}': {err}");
                res.state.status_message = format!("HMR char error: {err}");
                return;
            }
        }
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
