use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::mods::ModRegistry;
use crate::play::characters::char_rel_candidates_for_watch;
use crate::play::characters::loader::LoadedCharacter;
use crate::play::characters::schema::CharacterData;
use crate::play::characters::CharactersResource;
use crate::play::notes::skin::NoteStyle;
use crate::play::scene::PlayState;
use crate::play::song::song_asset_path_in;
use crate::play::stage::StageResource;
use crate::play::stage::paths::{
    resolve_atlas_dir_in, resolve_stage_image_in, stage_rel_candidates_for_watch,
};
use crate::play::stage::schema::parse_stage_json;
use crate::scripting::ScriptHost;
use crate::utils::constants::STAGES_ASSET_PATH;

use super::reloaders::HotReloadRes;
use super::types::{DirStamp, DomainWatch, FileStamp, MAX_SPRITEMAP_WATCH, WatchPlan};

pub(super) fn build_watch_plan(res: &HotReloadRes) -> WatchPlan {
    let mods_dir = (!res.mods.mods_dir.as_os_str().is_empty())
        .then(|| (res.mods.mods_dir.clone(), DirStamp::from_path(&res.mods.mods_dir)));

    let mut mod_manifests = BTreeMap::new();
    for loaded_mod in &res.mods.mods {
        if let Some(manifest) = &loaded_mod.manifest_path {
            mod_manifests.insert(manifest.clone(), FileStamp::from_path(manifest));
        }
    }

    WatchPlan {
        mods_dir,
        mod_manifests,
        chart: build_chart_watch(&res.mods, &res.state),
        stage: build_stage_watch(
            &res.mods,
            &res.state.chart.stage_id,
            res.stage_res.as_deref(),
        ),
        chars: build_all_character_watches(&res.mods, &res.state, res.chars_res.as_deref()),
        skin: build_skin_watch(&res.mods, &res.state.player_strumline.note_style),
        scripts: build_script_watch(
            &res.mods,
            &res.state,
            res.stage_res.as_deref(),
            res.chars_res.as_deref(),
            &res.script_host,
        ),
        audio: build_audio_watch(&res.mods, &res.state),
        parent_dirs: BTreeMap::new(),
    }
}

pub(super) fn build_all_character_watches(
    mods: &ModRegistry,
    state: &PlayState,
    chars_res: Option<&CharactersResource>,
) -> BTreeMap<String, DomainWatch> {
    let mut watches = BTreeMap::new();
    let mut ids = BTreeSet::new();
    for id in state
        .chart
        .characters
        .spectator
        .iter()
        .chain(state.chart.characters.opponents.iter())
        .chain(state.chart.characters.players.iter())
    {
        if !id.trim().is_empty() {
            ids.insert(id.clone());
        }
    }
    if let Some(chars) = chars_res {
        for c in &chars.characters {
            ids.insert(c.id.clone());
        }
    }
    for char_id in ids {
        let loaded = find_loaded_char(chars_res, &char_id);
        watches.insert(char_id.clone(), build_character_watch(mods, &char_id, loaded));
    }
    watches
}

pub(super) fn find_loaded_char<'a>(
    chars: Option<&'a CharactersResource>,
    char_id: &str,
) -> Option<&'a LoadedCharacter> {
    chars.and_then(|cr| cr.characters.iter().find(|c| c.id == char_id))
}

fn track_vfs_file_across_layers(
    watch: &mut DomainWatch,
    mods: &ModRegistry,
    rel: &str,
    is_json: bool,
) {
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
                track_vfs_file_across_layers(
                    watch,
                    mods,
                    &format!("{dir_rel}/spritemap{i}.json"),
                    false,
                );
                track_vfs_asset_across_layers(
                    watch,
                    mods,
                    &format!("{dir_rel}/spritemap{i}.png"),
                );
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

pub(super) fn build_chart_watch(mods: &ModRegistry, state: &PlayState) -> DomainWatch {
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

pub(super) fn build_stage_watch(
    mods: &ModRegistry,
    stage_id: &str,
    stage_opt: Option<&StageResource>,
) -> DomainWatch {
    let mut w = DomainWatch::default();
    if stage_id.trim().is_empty() {
        return w;
    }
    track_vfs_file_across_layers(
        &mut w,
        mods,
        &format!("{STAGES_ASSET_PATH}/{stage_id}/{stage_id}.json"),
        true,
    );
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

pub(super) fn build_character_watch(
    mods: &ModRegistry,
    char_id: &str,
    loaded_opt: Option<&LoadedCharacter>,
) -> DomainWatch {
    let mut w = DomainWatch::default();
    track_vfs_file_across_layers(
        &mut w,
        mods,
        &format!("gameplay/characters/{char_id}/{char_id}.json"),
        true,
    );
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

pub(super) fn build_skin_watch(mods: &ModRegistry, style: &NoteStyle) -> DomainWatch {
    let mut w = DomainWatch::default();
    let id = &style.id;
    track_vfs_file_across_layers(
        &mut w,
        mods,
        &format!("gameplay/noteskins/{id}/{id}.json"),
        true,
    );
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
    for item in style
        .popup
        .judgments
        .iter()
        .chain(style.countdown_images.iter())
    {
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

pub(super) fn build_script_watch(
    mods: &ModRegistry,
    state: &PlayState,
    stage: Option<&StageResource>,
    chars: Option<&CharactersResource>,
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
    if let Some(chars_res) = chars {
        for ch in &chars_res.characters {
            for ext in ["rhai", "lua", "hxc"] {
                w.track_meta(ch.json_path.with_extension(ext));
            }
            if let Some(parent) = ch.json_path.parent() {
                track_script_dir_tree(&mut w, parent, 2);
            }
        }
    }
    let stage_id = &state.chart.stage_id;
    let song_id = &state.chart.song_id;

    if !mods.assets_dir.as_os_str().is_empty() {
        track_script_dir_tree(&mut w, &mods.assets_dir.join("scripts"), 4);
        track_script_dir_tree(&mut w, &mods.assets_dir.join("gameplay").join("scripts"), 4);
        if !stage_id.is_empty() {
            track_script_dir_tree(
                &mut w,
                &mods.assets_dir.join(STAGES_ASSET_PATH).join(stage_id),
                2,
            );
        }
        if !song_id.is_empty() {
            track_script_dir_tree(
                &mut w,
                &mods.assets_dir.join("gameplay").join("songs").join(song_id),
                2,
            );
        }
    }
    for loaded_mod in &mods.mods {
        track_script_dir_tree(&mut w, &loaded_mod.root.join("scripts"), 4);
        track_script_dir_tree(&mut w, &loaded_mod.root.join("gameplay").join("scripts"), 4);
        if !stage_id.is_empty() {
            track_script_dir_tree(&mut w, &loaded_mod.root.join("stages").join(stage_id), 2);
            track_script_dir_tree(
                &mut w,
                &loaded_mod.root.join(STAGES_ASSET_PATH).join(stage_id),
                2,
            );
        }
        if !song_id.is_empty() {
            track_script_dir_tree(&mut w, &loaded_mod.root.join("songs").join(song_id), 2);
            track_script_dir_tree(
                &mut w,
                &loaded_mod.root.join("gameplay").join("songs").join(song_id),
                2,
            );
        }
    }
    w
}

pub(super) fn build_audio_watch(mods: &ModRegistry, state: &PlayState) -> DomainWatch {
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
        track_vfs_asset_across_layers(
            &mut w,
            mods,
            &song_asset_path_in(mods, song_id, &track.file),
        );
    }
    for vocal in state
        .chart
        .audio
        .player_vocal
        .iter()
        .chain(state.chart.audio.opponent_vocal.iter())
        .chain(state.chart.audio.spectator_vocal.iter())
    {
        track_vfs_asset_across_layers(
            &mut w,
            mods,
            &song_asset_path_in(mods, song_id, &vocal.file),
        );
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
