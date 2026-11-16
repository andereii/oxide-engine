use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use rhai::{CallFnOptions, Dynamic, Engine, ModuleResolver, Position, Scope};

use crate::mods::{ModRegistry, ModStore};

use super::api::{ScriptContext, ScriptStateSnapshot};
use super::context::{with_ctx, with_tls_context};
use super::rhai_resolver::{SharedVfsModuleResolver, VfsModuleResolver};

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "oxide_rhai_resolver_{label}_{}_{}",
            std::process::id(),
            seq
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp test dir");
        Self(dir)
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn resolve_hits_cache_after_file_removed_and_clears_on_invalidate() {
    let temp = TempDirGuard::new("cache_hit");
    let assets_dir = temp.0.join("assets");
    let mods_dir = temp.0.join("mods");
    let scripts_dir = assets_dir.join("scripts");
    fs::create_dir_all(&scripts_dir).expect("create scripts dir");

    let helper_path = scripts_dir.join("helper.rhai");
    fs::write(&helper_path, "fn answer() { 42 }\n").expect("write helper module");

    let registry = ModRegistry::discover(assets_dir, mods_dir);
    let resolver = VfsModuleResolver::new(registry);
    let engine = Engine::new();

    let first = resolver
        .resolve(&engine, Some("scripts/main.rhai"), "./helper", Position::NONE)
        .expect("initial resolve should succeed");
    assert_eq!(resolver.cached_paths(), vec![helper_path.clone()]);

    // Remove the module on disk; repeated resolve with the same (source_path, path)
    // must hit resolution_cache + module cache without touching the filesystem.
    fs::remove_file(&helper_path).expect("remove helper file");

    let second = resolver
        .resolve(&engine, Some("scripts/main.rhai"), "./helper", Position::NONE)
        .expect("cached resolve should succeed even after file removal");
    assert!(Arc::ptr_eq(&first, &second));

    resolver.clear_caches();
    assert!(resolver.cached_paths().is_empty());
    assert!(
        resolver
            .resolve(&engine, Some("scripts/main.rhai"), "./helper", Position::NONE)
            .is_err()
    );

    fs::write(&helper_path, "fn answer() { 99 }\n").expect("recreate helper module");
    let reloaded = resolver
        .resolve(&engine, Some("scripts/main.rhai"), "./helper", Position::NONE)
        .expect("resolve should succeed once recreated after clear_caches");
    assert!(!Arc::ptr_eq(&first, &reloaded));
}

#[test]
fn repeated_hook_imports_hit_resolution_cache_until_cleared() {
    let temp = TempDirGuard::new("hook_import");
    let assets_dir = temp.0.join("assets");
    let mods_dir = temp.0.join("mods");
    let scripts_dir = assets_dir.join("scripts");
    fs::create_dir_all(&scripts_dir).expect("create scripts dir");

    let initial_val = 42_i64;
    let updated_val = 99_i64;
    let helper_path = scripts_dir.join("helper.rhai");
    fs::write(&helper_path, format!("fn val() {{ {initial_val} }}\n")).expect("write helper module");

    let registry = ModRegistry::discover(assets_dir, mods_dir);
    let resolver = Arc::new(VfsModuleResolver::new(registry));
    let mut engine = Engine::new();
    engine.set_module_resolver(SharedVfsModuleResolver(Arc::clone(&resolver)));

    let mut ast = engine
        .compile("fn get_val() { import \"./helper\" as h; h::val() }")
        .expect("compile test script");
    ast.set_source("scripts/main.rhai");

    let mut scope = Scope::new();
    let first = engine
        .call_fn_with_options::<Dynamic>(
            CallFnOptions::new().eval_ast(false),
            &mut scope,
            &ast,
            "get_val",
            (),
        )
        .expect("first call should resolve module");
    assert_eq!(first.as_int().expect("first int"), initial_val);

    // Secondary source path hits module cache and also populates resolution_cache.
    resolver
        .resolve(&engine, Some("scripts/other.rhai"), "./helper", Position::NONE)
        .expect("secondary source path should resolve via module cache");

    fs::remove_file(&helper_path).expect("remove helper file");

    let second = engine
        .call_fn_with_options::<Dynamic>(
            CallFnOptions::new().eval_ast(false),
            &mut scope,
            &ast,
            "get_val",
            (),
        )
        .expect("repeated hook import should hit resolution cache");
    assert_eq!(second.as_int().expect("second int"), initial_val);
    assert!(
        resolver
            .resolve(&engine, Some("scripts/other.rhai"), "./helper", Position::NONE)
            .is_ok()
    );

    fs::write(&helper_path, format!("fn val() {{ {updated_val} }}\n"))
        .expect("write updated helper module");
    resolver.clear_caches();

    let reloaded = engine
        .call_fn_with_options::<Dynamic>(
            CallFnOptions::new().eval_ast(false),
            &mut scope,
            &ast,
            "get_val",
            (),
        )
        .expect("call after clear_caches should reload updated module");
    assert_eq!(reloaded.as_int().expect("reloaded int"), updated_val);
}

#[test]
fn with_tls_context_clears_slot_on_panic() {
    let registry = Arc::new(ModRegistry::discover(PathBuf::new(), PathBuf::new()));
    let ctx = ScriptContext::new(
        ScriptStateSnapshot::empty_placeholder(),
        registry,
        ModStore::default(),
        1,
    );
    let res = catch_unwind(AssertUnwindSafe(|| {
        with_tls_context(ctx, || {
            panic!("simulated script panic");
        });
    }));
    assert!(res.is_err());
    assert!(with_ctx(|_| ()).is_err());
}
