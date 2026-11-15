use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::utils::constants::{
    DEFAULT_MOD_PRIORITY, MOD_ASSET_SOURCE_ID, MOD_MANIFEST_JSON, MOD_MANIFEST_TOML,
    POLYMOD_META_JSON, SCRIPT_DIR_CHARACTERS, SCRIPT_DIR_EVENTS, SCRIPT_DIR_NOTEKINDS,
    SCRIPT_DIR_SONGS, SCRIPT_DIR_STAGES, SCRIPT_DIRS_GLOBAL, SCRIPT_EXT_DISABLED, SCRIPT_EXT_LUA,
    SCRIPT_EXT_RHAI,
};

use super::{ModError, ModRegistry, ModStore, ScriptScope};

struct TempWorkspace {
    root: PathBuf,
    assets: PathBuf,
    mods: PathBuf,
}

impl TempWorkspace {
    fn new(tag: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "oxide_mods_test_{tag}_{}_{}",
            std::process::id(),
            nanos
        ));
        let assets = root.join("assets");
        let mods = root.join("mods");
        fs::create_dir_all(&assets).unwrap();
        fs::create_dir_all(&mods).unwrap();
        Self { root, assets, mods }
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_text(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn write_mod_manifest(mods_dir: &Path, mod_id: &str, priority: i32, enabled: bool) {
    write_text(
        &mods_dir.join(mod_id).join(MOD_MANIFEST_TOML),
        &format!("id = \"{mod_id}\"\npriority = {priority}\nenabled = {enabled}\n"),
    );
}

#[test]
fn discovers_toml_json_polymod_and_loose_mods_without_hardcoding() {
    let ws = TempWorkspace::new("discovery");

    let mod_a_id = "alpha-pack";
    let mod_b_id = "beta-pack";
    let mod_c_id = "gamma-pack";
    let mod_d_folder = "Loose Folder.Mod";
    let mod_disabled_id = "skip-me";

    let low_priority = DEFAULT_MOD_PRIORITY - 50;
    let high_priority = DEFAULT_MOD_PRIORITY + 50;

    write_text(
        &ws.mods.join(mod_a_id).join(MOD_MANIFEST_TOML),
        &format!(
            "id = \"{mod_a_id}\"\ntitle = \"Alpha\"\npriority = {high_priority}\ndependencies = [\"{mod_b_id}\"]\n[config]\ntremor = 1.5\nunknown_field_kept = true\n"
        ),
    );
    write_text(
        &ws.mods.join(mod_b_id).join(MOD_MANIFEST_JSON),
        &format!(
            "{{\"id\":\"{mod_b_id}\",\"priority\":{low_priority},\"authors\":[\"DevOne\"]}}"
        ),
    );
    write_text(
        &ws.mods.join(mod_c_id).join(POLYMOD_META_JSON),
        &format!(
            "{{\"title\":\"Gamma Polymod\",\"mod_version\":\"2.1.0\",\"priority\":{high_priority}}}"
        ),
    );
    write_text(
        &ws.mods.join(mod_d_folder).join("songs/test-song/chart.json"),
        "{}",
    );
    write_text(
        &ws.mods.join(mod_disabled_id).join(MOD_MANIFEST_TOML),
        &format!("id = \"{mod_disabled_id}\"\nenabled = false\n"),
    );

    let registry = ModRegistry::discover(ws.assets.clone(), ws.mods.clone());
    let active_ids = registry.active_mod_ids();

    assert!(active_ids.contains(&mod_a_id));
    assert!(active_ids.contains(&mod_b_id));
    assert!(active_ids.contains(&mod_c_id));
    assert!(active_ids.contains(&"loose-folder-mod"));
    assert!(!active_ids.contains(&mod_disabled_id));

    let pos_b = active_ids.iter().position(|id| *id == mod_b_id).unwrap();
    let pos_a = active_ids.iter().position(|id| *id == mod_a_id).unwrap();
    assert!(pos_b < pos_a, "dependency mod should precede dependent mod");

    assert_eq!(
        registry
            .get_mod_config(mod_a_id, "tremor")
            .and_then(|v| v.as_f64()),
        Some(1.5)
    );
    assert!(registry.list_songs().iter().any(|s| s == "test-song"));
}

#[test]
fn vfs_precedence_asset_paths_and_traversal_rejection() {
    let ws = TempWorkspace::new("vfs");
    let song_rel = "songs/demo/meta.json";
    let base_payload = "base-layer";
    let low_payload = "low-mod-layer";
    let high_payload = "high-mod-layer";

    write_text(&ws.assets.join(song_rel), base_payload);
    write_text(
        &ws.mods.join("low").join(MOD_MANIFEST_TOML),
        &format!("id = \"low\"\npriority = {}\n", DEFAULT_MOD_PRIORITY - 10),
    );
    write_text(&ws.mods.join("low").join(song_rel), low_payload);
    write_text(
        &ws.mods.join("high").join(MOD_MANIFEST_TOML),
        &format!("id = \"high\"\npriority = {}\n", DEFAULT_MOD_PRIORITY + 10),
    );
    write_text(&ws.mods.join("high").join(song_rel), high_payload);

    let mut registry = ModRegistry::discover(ws.assets.clone(), ws.mods.clone());
    assert_eq!(registry.read_to_string(song_rel).unwrap(), high_payload);

    let resolved = registry.resolve_file(song_rel).unwrap();
    let asset_path = registry.to_asset_server_path(&resolved);
    assert_eq!(asset_path, format!("{MOD_ASSET_SOURCE_ID}://high/{song_rel}"));

    assert!(ModRegistry::normalize_rel_path("../secret.txt").is_err());
    assert!(ModRegistry::normalize_rel_path("/etc/passwd").is_err());
    assert!(registry.resolve_file("songs/../../../etc/passwd").is_none());

    // Simulate disabling high mod on disk and hot-reloading.
    write_text(
        &ws.mods.join("high").join(format!(".{SCRIPT_EXT_DISABLED}")),
        "",
    );
    registry.reload().unwrap();
    assert_eq!(registry.read_to_string(song_rel).unwrap(), low_payload);
    assert_eq!(registry.reload_count, 1);
}

#[test]
fn scoped_script_discovery_and_store_roundtrip() {
    let ws = TempWorkspace::new("scripts");
    let mod_id = "modchart";
    let mod_root = ws.mods.join(mod_id);
    write_text(
        &mod_root.join(MOD_MANIFEST_TOML),
        &format!("id = \"{mod_id}\"\n"),
    );

    let target_song = "bopeebo";
    let target_stage = "mainStage";
    let target_char = "bf";
    let target_kind = "hurt";
    let target_event = "CameraShake";

    write_text(
        &mod_root
            .join(SCRIPT_DIRS_GLOBAL[0])
            .join(format!("hud.{SCRIPT_EXT_RHAI}")),
        "// global",
    );
    write_text(
        &mod_root
            .join(SCRIPT_DIR_SONGS)
            .join(format!("{target_song}.{SCRIPT_EXT_RHAI}")),
        "// song",
    );
    write_text(
        &mod_root
            .join(SCRIPT_DIR_STAGES)
            .join(format!("{target_stage}.{SCRIPT_EXT_LUA}")),
        "-- stage",
    );
    write_text(
        &mod_root
            .join(SCRIPT_DIR_CHARACTERS)
            .join(format!("{target_char}.{SCRIPT_EXT_RHAI}")),
        "// char",
    );
    write_text(
        &mod_root
            .join(SCRIPT_DIR_NOTEKINDS)
            .join(format!("{target_kind}.{SCRIPT_EXT_RHAI}")),
        "// notekind",
    );
    write_text(
        &mod_root
            .join(SCRIPT_DIR_EVENTS)
            .join(format!("{target_event}.{SCRIPT_EXT_RHAI}")),
        "// event",
    );
    write_text(
        &mod_root
            .join(SCRIPT_DIR_SONGS)
            .join(format!("other.{SCRIPT_EXT_RHAI}.{SCRIPT_EXT_DISABLED}")),
        "// disabled",
    );

    let registry = ModRegistry::discover(ws.assets.clone(), ws.mods.clone());
    let matched = registry.scripts_for_context(
        target_song,
        Some(target_stage),
        &[target_char],
        &[target_kind],
        &[target_event],
    );

    assert!(matched.iter().any(|s| matches!(s.scope, ScriptScope::Global)));
    assert!(matched.iter().any(|s| s.scope == ScriptScope::Stage(target_stage.to_string())));
    assert!(matched.iter().any(|s| s.scope == ScriptScope::Character(target_char.to_string())));
    assert!(matched.iter().any(|s| s.scope == ScriptScope::NoteKind(target_kind.to_string())));
    assert!(matched.iter().any(|s| s.scope == ScriptScope::Event(target_event.to_string())));
    assert!(matched.iter().any(|s| s.scope == ScriptScope::Song(target_song.to_string())));
    assert!(!matched.iter().any(|s| s.name.contains("other")));

    let mut store = ModStore::with_root(&ws.mods);
    store.set(mod_id, "bestCombo", serde_json::json!(42));
    store.save_namespace(mod_id).unwrap();

    let mut reloaded_store = ModStore::with_root(&ws.mods);
    assert!(reloaded_store.load_namespace(mod_id).unwrap());
    assert_eq!(
        reloaded_store.get(mod_id, "bestCombo").and_then(|v| v.as_i64()),
        Some(42)
    );
}

#[test]
fn polymod_append_applies_in_priority_order_on_override_and_skips_empty_or_disabled() {
    let ws = TempWorkspace::new("polymod_append");
    let dir_name = format!("dir_{}", std::process::id());
    let file_rel = format!("{dir_name}/entries.txt");
    let assets_only_rel = format!("{dir_name}/assets_entries.txt");
    let orphan_rel = format!("{dir_name}/orphan.txt");

    let base_line = format!("base_{}", ws.root.file_name().unwrap().to_string_lossy());
    let override_line = format!("override_{}", DEFAULT_MOD_PRIORITY - 20);
    let low_line = format!("low_{}", DEFAULT_MOD_PRIORITY - 10);
    let high_line = format!("high_{}", DEFAULT_MOD_PRIORITY + 10);
    let disabled_line = "disabled_fragment";

    write_text(&ws.assets.join(&file_rel), &base_line);
    write_text(&ws.assets.join(&assets_only_rel), &base_line);

    let mod_override = "mod-override";
    let mod_low = "mod-low";
    let mod_blank = "mod-blank";
    let mod_high = "mod-high";
    let mod_off = "mod-off";

    write_mod_manifest(&ws.mods, mod_override, DEFAULT_MOD_PRIORITY - 20, true);
    write_text(&ws.mods.join(mod_override).join(&file_rel), &override_line);

    write_mod_manifest(&ws.mods, mod_low, DEFAULT_MOD_PRIORITY - 10, true);
    write_text(
        &ws.mods.join(mod_low).join("_append").join(&file_rel),
        &low_line,
    );
    write_text(
        &ws.mods.join(mod_low).join("_append").join(&assets_only_rel),
        &low_line,
    );

    write_mod_manifest(&ws.mods, mod_blank, DEFAULT_MOD_PRIORITY, true);
    write_text(
        &ws.mods.join(mod_blank).join("_append").join(&file_rel),
        "   \n\t\n",
    );

    write_mod_manifest(&ws.mods, mod_high, DEFAULT_MOD_PRIORITY + 10, true);
    write_text(
        &ws.mods.join(mod_high).join("_append").join(&file_rel),
        &high_line,
    );
    write_text(
        &ws.mods.join(mod_high).join("_append").join(&orphan_rel),
        &high_line,
    );

    write_mod_manifest(&ws.mods, mod_off, DEFAULT_MOD_PRIORITY + 50, false);
    write_text(
        &ws.mods.join(mod_off).join("_append").join(&file_rel),
        disabled_line,
    );

    let registry = ModRegistry::discover(ws.assets.clone(), ws.mods.clone());
    assert_eq!(registry.read_to_string(&file_rel).unwrap(), override_line);

    let expected_override_merged =
        [override_line.as_str(), low_line.as_str(), high_line.as_str()].join("\n");
    assert_eq!(
        registry.read_to_string_merged(&file_rel).unwrap(),
        expected_override_merged
    );

    let expected_assets_merged = [base_line.as_str(), low_line.as_str()].join("\n");
    assert_eq!(
        registry.read_to_string_merged(&assets_only_rel).unwrap(),
        expected_assets_merged
    );

    assert!(matches!(
        registry.read_to_string_merged(&orphan_rel),
        Err(ModError::NotFound(_))
    ));
    assert!(matches!(
        registry.read_to_string_merged(&format!("../{file_rel}")),
        Err(ModError::InvalidPath(_))
    ));
}

#[test]
fn polymod_merge_json_merges_objects_recursively_and_concats_arrays() {
    let ws = TempWorkspace::new("polymod_merge_json");
    let group = format!("cfg_{}", std::process::id());
    let obj_rel = format!("{group}/meta.json");
    let arr_rel = format!("{group}/list.json");
    let bad_rel = format!("{group}/broken.json");

    let base_title = format!("title_{}", DEFAULT_MOD_PRIORITY);
    let base_artist = "base_artist";
    let low_artist = "low_artist";
    let base_bpm = 120;
    let high_bpm = 165;
    let tag_base = "tag_base";
    let tag_low = "tag_low";
    let tag_high = "tag_high";
    let bom_flag = true;

    let base_obj = serde_json::json!({
        "title": base_title,
        "details": {
            "artist": base_artist,
            "bpm": base_bpm
        },
        "tags": [tag_base]
    });
    let low_obj = serde_json::json!({
        "details": {
            "artist": low_artist,
            "bpm": base_bpm + 10,
            "charter": "low_charter"
        },
        "tags": [tag_low]
    });
    let high_obj = serde_json::json!({
        "details": {
            "bpm": high_bpm
        },
        "tags": [tag_high]
    });
    let bom_obj = serde_json::json!({
        "bomHandled": bom_flag
    });

    write_text(&ws.assets.join(&obj_rel), &base_obj.to_string());
    write_text(
        &ws.assets.join(&arr_rel),
        &serde_json::json!([tag_base]).to_string(),
    );
    write_text(&ws.assets.join(&bad_rel), &serde_json::json!({"ok": true}).to_string());

    let mod_low = "json-low";
    let mod_blank = "json-blank";
    let mod_high = "json-high";
    let mod_bom = "json-bom";

    write_mod_manifest(&ws.mods, mod_low, DEFAULT_MOD_PRIORITY - 20, true);
    write_text(
        &ws.mods.join(mod_low).join("_merge").join(&obj_rel),
        &low_obj.to_string(),
    );
    write_text(
        &ws.mods.join(mod_low).join("_merge").join(&arr_rel),
        &serde_json::json!([tag_low]).to_string(),
    );

    write_mod_manifest(&ws.mods, mod_blank, DEFAULT_MOD_PRIORITY - 10, true);
    write_text(
        &ws.mods.join(mod_blank).join("_merge").join(&obj_rel),
        "   \n",
    );

    write_mod_manifest(&ws.mods, mod_high, DEFAULT_MOD_PRIORITY + 10, true);
    write_text(
        &ws.mods.join(mod_high).join("_merge").join(&obj_rel),
        &high_obj.to_string(),
    );
    write_text(
        &ws.mods.join(mod_high).join("_merge").join(&arr_rel),
        &serde_json::json!([tag_high]).to_string(),
    );
    write_text(
        &ws.mods.join(mod_high).join("_merge").join(&bad_rel),
        "{not_valid_json",
    );

    write_mod_manifest(&ws.mods, mod_bom, DEFAULT_MOD_PRIORITY + 20, true);
    write_text(
        &ws.mods.join(mod_bom).join("_merge").join(&obj_rel),
        &format!("\u{feff}{bom_obj}"),
    );

    let registry = ModRegistry::discover(ws.assets.clone(), ws.mods.clone());

    let merged_obj_raw = registry.read_to_string_merged(&obj_rel).unwrap();
    let merged_obj: serde_json::Value = serde_json::from_str(&merged_obj_raw).unwrap();
    let expected_obj = serde_json::json!({
        "title": base_title,
        "details": {
            "artist": low_artist,
            "bpm": high_bpm,
            "charter": "low_charter"
        },
        "tags": [tag_base, tag_low, tag_high],
        "bomHandled": bom_flag
    });
    assert_eq!(merged_obj, expected_obj);

    let merged_arr_raw = registry.read_to_string_merged(&arr_rel).unwrap();
    let merged_arr: serde_json::Value = serde_json::from_str(&merged_arr_raw).unwrap();
    assert_eq!(merged_arr, serde_json::json!([tag_base, tag_low, tag_high]));

    assert!(matches!(
        registry.read_to_string_merged(&bad_rel),
        Err(ModError::InvalidJson { .. })
    ));
}

#[test]
fn polymod_merge_plain_text_tsv_and_mixed_overlays_respect_priority_order() {
    let ws = TempWorkspace::new("polymod_merge_text");
    let dir_name = format!("text_{}", std::process::id());
    let txt_rel = format!("{dir_name}/lines.txt");
    let tsv_rel = format!("{dir_name}/rows.tsv");
    let plain_rel = format!("{dir_name}/notes");

    let txt_base = format!("base_{}", DEFAULT_MOD_PRIORITY);
    let txt_low_1 = "low_merge_1";
    let txt_low_2 = "low_merge_2";
    let txt_high = "high_append_3";

    let tsv_header = "col_a\tcol_b";
    let tsv_base_row = "r0\tv0";
    let tsv_low_row = "r1\tv1";
    let tsv_high_row = "r2\tv2";

    let plain_base = "plain_0";
    let plain_merge = "plain_1";

    write_text(&ws.assets.join(&txt_rel), &format!("{txt_base}\n"));
    write_text(
        &ws.assets.join(&tsv_rel),
        &format!("{tsv_header}\n{tsv_base_row}"),
    );
    write_text(&ws.assets.join(&plain_rel), plain_base);

    let mod_low = "txt-low";
    let mod_high = "txt-high";

    write_mod_manifest(&ws.mods, mod_low, DEFAULT_MOD_PRIORITY - 15, true);
    write_text(
        &ws.mods.join(mod_low).join("_merge").join(&txt_rel),
        &format!("\r\n{txt_low_1}\r\n\r\n{txt_low_2}\r\n"),
    );
    write_text(
        &ws.mods.join(mod_low).join("_merge").join(&tsv_rel),
        &format!("{tsv_low_row}\n"),
    );
    write_text(
        &ws.mods.join(mod_low).join("_merge").join(&plain_rel),
        plain_merge,
    );

    write_mod_manifest(&ws.mods, mod_high, DEFAULT_MOD_PRIORITY + 15, true);
    write_text(
        &ws.mods.join(mod_high).join("_append").join(&txt_rel),
        txt_high,
    );
    write_text(
        &ws.mods.join(mod_high).join("_merge").join(&tsv_rel),
        &format!("\n{tsv_high_row}\n"),
    );

    let registry = ModRegistry::discover(ws.assets.clone(), ws.mods.clone());

    let txt_merged = registry.read_to_string_merged(&txt_rel).unwrap();
    let txt_lines: Vec<&str> = txt_merged.lines().collect();
    assert_eq!(txt_lines, vec![txt_base.as_str(), txt_low_1, txt_low_2, txt_high]);

    let tsv_merged = registry.read_to_string_merged(&tsv_rel).unwrap();
    let tsv_lines: Vec<&str> = tsv_merged.lines().collect();
    assert_eq!(
        tsv_lines,
        vec![tsv_header, tsv_base_row, tsv_low_row, tsv_high_row]
    );

    let plain_merged = registry.read_to_string_merged(&plain_rel).unwrap();
    let plain_lines: Vec<&str> = plain_merged.lines().collect();
    assert_eq!(plain_lines, vec![plain_base, plain_merge]);
}

#[test]
fn list_mod_files_in_deduplicates_normalizes_and_rejects_traversal() {
    let ws = TempWorkspace::new("list_files");
    let root_seg = format!("pack_{}", std::process::id());
    let sub_seg = "charts";
    let nested_seg = "extra";
    let norm_dir = format!("{root_seg}/{sub_seg}");
    let query_dir = format!("./{root_seg}\\{sub_seg}/");

    let shared_rel = format!("{norm_dir}/shared.json");
    let asset_only_rel = format!("{norm_dir}/base.json");
    let mod_a_nested_rel = format!("{norm_dir}/{nested_seg}/a.json");
    let mod_b_rel = format!("{norm_dir}/b.txt");
    let hidden_file_rel = format!("{norm_dir}/.hidden.json");
    let hidden_dir_rel = format!("{norm_dir}/.cache/ignored.json");
    let disabled_mod_rel = format!("{norm_dir}/disabled_only.json");

    write_text(&ws.assets.join(&shared_rel), "{}");
    write_text(&ws.assets.join(&asset_only_rel), "{}");

    let mod_a = "list-a";
    let mod_b = "list-b";
    let mod_off = "list-off";

    write_mod_manifest(&ws.mods, mod_a, DEFAULT_MOD_PRIORITY, true);
    write_text(&ws.mods.join(mod_a).join(&shared_rel), "{\"override\":true}");
    write_text(&ws.mods.join(mod_a).join(&mod_a_nested_rel), "{}");
    write_text(&ws.mods.join(mod_a).join(&hidden_file_rel), "{}");
    write_text(&ws.mods.join(mod_a).join(&hidden_dir_rel), "{}");
    write_text(
        &ws.mods.join(mod_a).join("_append").join(&mod_b_rel),
        "overlay",
    );

    write_mod_manifest(&ws.mods, mod_b, DEFAULT_MOD_PRIORITY + 10, true);
    write_text(&ws.mods.join(mod_b).join(&mod_b_rel), "data");

    write_mod_manifest(&ws.mods, mod_off, DEFAULT_MOD_PRIORITY + 20, false);
    write_text(&ws.mods.join(mod_off).join(&disabled_mod_rel), "{}");

    let registry = ModRegistry::discover(ws.assets.clone(), ws.mods.clone());
    let listed = registry.list_mod_files_in(&query_dir);

    let mut expected = vec![shared_rel, asset_only_rel, mod_a_nested_rel, mod_b_rel];
    expected.sort();
    assert_eq!(listed, expected);

    assert!(registry.list_mod_files_in("../outside").is_empty());
    assert!(registry.list_mod_files_in(&format!("{norm_dir}/../../outside")).is_empty());
    assert!(registry.list_mod_files_in("/etc").is_empty());
    assert!(registry.list_mod_files_in("").is_empty());
    assert!(registry.list_mod_files_in(".").is_empty());
    assert!(registry.list_mod_files_in(&format!("{root_seg}/missing")).is_empty());
}
