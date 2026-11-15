use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::utils::constants::{
    DEFAULT_MOD_PRIORITY, MOD_ASSET_SOURCE_ID, MOD_MANIFEST_JSON, MOD_MANIFEST_TOML,
    POLYMOD_META_JSON, SCRIPT_DIR_CHARACTERS, SCRIPT_DIR_EVENTS, SCRIPT_DIR_NOTEKINDS,
    SCRIPT_DIR_SONGS, SCRIPT_DIR_STAGES, SCRIPT_DIRS_GLOBAL, SCRIPT_EXT_DISABLED, SCRIPT_EXT_LUA,
    SCRIPT_EXT_RHAI,
};

use super::{ModRegistry, ModStore, ScriptScope};

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
