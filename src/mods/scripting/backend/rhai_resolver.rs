use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rhai::{Engine, EvalAltResult, Module, ModuleResolver, Position, Scope, Shared};

use crate::mods::ModRegistry;
use crate::utils::constants::SCRIPT_EXT_RHAI;

pub(super) struct SharedVfsModuleResolver(pub(super) Arc<VfsModuleResolver>);

impl ModuleResolver for SharedVfsModuleResolver {
    fn resolve(
        &self,
        engine: &Engine,
        source_path: Option<&str>,
        path: &str,
        pos: Position,
    ) -> Result<Shared<Module>, Box<EvalAltResult>> {
        self.0.resolve(engine, source_path, path, pos)
    }
}

pub(super) struct VfsModuleResolver {
    registry: ModRegistry,
    cache: Mutex<HashMap<PathBuf, Shared<Module>>>,
    resolution_cache: Mutex<HashMap<(Option<String>, String), PathBuf>>,
    misses: Mutex<HashSet<(Option<String>, String)>>,
    loading: Mutex<HashSet<PathBuf>>,
}

struct LoadingGuard<'a> {
    loading: &'a Mutex<HashSet<PathBuf>>,
    path: PathBuf,
}

impl Drop for LoadingGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut set) = self.loading.lock() {
            set.remove(&self.path);
        }
    }
}

impl VfsModuleResolver {
    pub(super) fn new(registry: ModRegistry) -> Self {
        Self {
            registry,
            cache: Mutex::new(HashMap::new()),
            resolution_cache: Mutex::new(HashMap::new()),
            misses: Mutex::new(HashSet::new()),
            loading: Mutex::new(HashSet::new()),
        }
    }

    pub(super) fn cached_paths(&self) -> Vec<PathBuf> {
        self.cache
            .lock()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub(super) fn clear_caches(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
        if let Ok(mut resolution_cache) = self.resolution_cache.lock() {
            resolution_cache.clear();
        }
        if let Ok(mut misses) = self.misses.lock() {
            misses.clear();
        }
        self.registry.clear_script_io_memo();
    }

    fn remember_miss(&self, key: (Option<String>, String)) {
        let Ok(mut misses) = self.misses.lock() else {
            return;
        };
        const MISS_CAP: usize = 256;
        if misses.len() >= MISS_CAP && !misses.contains(&key) {
            return;
        }
        misses.insert(key);
    }

    fn candidate_paths(source_path: Option<&str>, raw_path: &str) -> Vec<String> {
        let cleaned = raw_path.trim().replace('\\', "/");
        let cleaned = cleaned.strip_prefix("./").unwrap_or(&cleaned);
        let mut candidates = Vec::new();
        if let Some(src) = source_path
            && let Some((parent, _)) = src.rsplit_once('/')
            && !parent.is_empty()
        {
            candidates.push(format!("{parent}/{cleaned}"));
        }
        candidates.push(cleaned.to_string());
        if !cleaned.starts_with("../") && !cleaned.starts_with("modules/") {
            candidates.push(format!("modules/{cleaned}"));
        }
        if !cleaned.starts_with("../") && !cleaned.starts_with("scripts/") {
            candidates.push(format!("scripts/{cleaned}"));
        }
        candidates
    }

    fn resolve_candidate(&self, candidate: &str) -> Option<(PathBuf, String)> {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            return None;
        }
        let mut probes = vec![trimmed.to_string()];
        let has_rhai_ext = Path::new(trimmed)
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case(SCRIPT_EXT_RHAI));
        if !has_rhai_ext {
            probes.push(format!("{trimmed}.{SCRIPT_EXT_RHAI}"));
        }
        for probe in probes {
            if let Ok(norm) = ModRegistry::normalize_rel_path(&probe)
                && let Some(disk_path) = self.registry.resolve_file(&probe)
            {
                let vfs_rel = norm.to_string_lossy().replace('\\', "/");
                return Some((disk_path, vfs_rel));
            }
        }
        None
    }

    fn resolve_vfs_target(
        &self,
        source_path: Option<&str>,
        raw_path: &str,
    ) -> Option<(PathBuf, String)> {
        for cand in Self::candidate_paths(source_path, raw_path) {
            if let Some(hit) = self.resolve_candidate(&cand) {
                return Some(hit);
            }
        }
        None
    }
}

impl ModuleResolver for VfsModuleResolver {
    fn resolve(
        &self,
        engine: &Engine,
        source_path: Option<&str>,
        path: &str,
        pos: Position,
    ) -> Result<Shared<Module>, Box<EvalAltResult>> {
        let lookup_key = (source_path.map(str::to_string), path.to_string());
        if self
            .misses
            .lock()
            .ok()
            .is_some_and(|misses| misses.contains(&lookup_key))
        {
            return Err(Box::new(EvalAltResult::ErrorModuleNotFound(
                path.to_string(),
                pos,
            )));
        }
        let cached_disk_path = self
            .resolution_cache
            .lock()
            .ok()
            .and_then(|res_cache| res_cache.get(&lookup_key).cloned());
        if let Some(cached_path) = cached_disk_path
            && let Ok(cache) = self.cache.lock()
            && let Some(cached) = cache.get(&cached_path)
        {
            return Ok(cached.clone());
        }

        let Some((disk_path, vfs_rel)) = self.resolve_vfs_target(source_path, path) else {
            self.remember_miss(lookup_key);
            return Err(Box::new(EvalAltResult::ErrorModuleNotFound(
                path.to_string(),
                pos,
            )));
        };

        if let Ok(cache) = self.cache.lock()
            && let Some(cached) = cache.get(&disk_path)
        {
            let module = cached.clone();
            drop(cache);
            if let Ok(mut res_cache) = self.resolution_cache.lock() {
                res_cache.insert(lookup_key, disk_path);
            }
            return Ok(module);
        }

        {
            let mut loading = self.loading.lock().map_err(|_| {
                Box::new(EvalAltResult::ErrorInModule(
                    vfs_rel.clone(),
                    "module loader mutex poisoned".into(),
                    pos,
                ))
            })?;
            if !loading.insert(disk_path.clone()) {
                return Err(Box::new(EvalAltResult::ErrorInModule(
                    vfs_rel,
                    format!("circular module import detected for '{path}'").into(),
                    pos,
                )));
            }
        }
        let _guard = LoadingGuard {
            loading: &self.loading,
            path: disk_path.clone(),
        };

        let code = match fs::read_to_string(&disk_path) {
            Ok(code) => code,
            Err(err) => {
                self.remember_miss(lookup_key);
                return Err(Box::new(EvalAltResult::ErrorInModule(
                    vfs_rel.clone(),
                    format!("failed to read module '{}': {err}", disk_path.display()).into(),
                    pos,
                )));
            }
        };

        let mut module_ast = match engine.compile(&code) {
            Ok(ast) => ast,
            Err(err) => {
                self.remember_miss(lookup_key);
                return Err(Box::new(EvalAltResult::ErrorInModule(
                    vfs_rel.clone(),
                    Box::new(EvalAltResult::from(err)),
                    pos,
                )));
            }
        };
        module_ast.set_source(vfs_rel.clone());

        let mut module = match Module::eval_ast_as_new(Scope::new(), &module_ast, engine) {
            Ok(module) => module,
            Err(err) => {
                self.remember_miss(lookup_key);
                return Err(Box::new(EvalAltResult::ErrorInModule(vfs_rel, err, pos)));
            }
        };
        module.build_index();

        let shared = Shared::new(module);
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(disk_path.clone(), shared.clone());
        }
        if let Ok(mut res_cache) = self.resolution_cache.lock() {
            res_cache.insert(lookup_key, disk_path);
        }
        Ok(shared)
    }
}

pub(super) fn vfs_rel_for_script(
    registry: Option<&ModRegistry>,
    script_path: Option<&Path>,
    fallback_id: &str,
) -> String {
    let (Some(mods), Some(disk_path)) = (registry, script_path) else {
        return fallback_id.to_string();
    };
    for loaded_mod in mods.mods.iter().rev() {
        if let Ok(rel) = disk_path.strip_prefix(&loaded_mod.root) {
            return rel.to_string_lossy().replace('\\', "/");
        }
    }
    if let Ok(rel) = disk_path.strip_prefix(&mods.assets_dir) {
        return rel.to_string_lossy().replace('\\', "/");
    }
    if let Ok(canon_disk) = disk_path.canonicalize() {
        for loaded_mod in mods.mods.iter().rev() {
            if let Ok(canon_root) = loaded_mod.root.canonicalize()
                && let Ok(rel) = canon_disk.strip_prefix(&canon_root)
            {
                return rel.to_string_lossy().replace('\\', "/");
            }
        }
        if let Ok(canon_assets) = mods.assets_dir.canonicalize()
            && let Ok(rel) = canon_disk.strip_prefix(&canon_assets)
        {
            return rel.to_string_lossy().replace('\\', "/");
        }
    }
    fallback_id.to_string()
}
