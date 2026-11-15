use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rhai::{AST, CallFnOptions, Dynamic, Engine, Scope};

use crate::mods::{ModRegistry, ScriptScope};
use crate::scripting::api::{ScriptContext, script_clamp, script_lerp, script_remap};
use crate::scripting::events::{HookKind, ScriptEvent};
use crate::scripting::value::{ScriptLanguage, ScriptValue};
use crate::utils::constants::{
    RHAI_MAX_ARRAY_SIZE, RHAI_MAX_CALL_LEVELS, RHAI_MAX_MAP_SIZE, RHAI_MAX_OPERATIONS,
    RHAI_MAX_STRING_SIZE, SCRIPT_EXT_RHAI,
};

use super::api::core::{register_core_and_song_api, register_math_and_rng_api};
use super::api::gameplay::{register_camera_api, register_gameplay_api};
use super::api::shaders::register_shader_api;
use super::api::stage::{
    register_character_and_modchart_api, register_props_tweens_timers_and_store_api,
};
use super::context::{RhaiEvent, register_event_type, with_tls_context};
use super::rhai_resolver::{VfsModuleResolver, vfs_rel_for_script};
use super::traits::{ScriptBackend, ScriptError, ScriptInstance, ScriptSource};

fn build_configured_engine(registry: Option<ModRegistry>) -> Arc<Engine> {
    let mut engine = Engine::new();
    engine.set_max_operations(RHAI_MAX_OPERATIONS);
    engine.set_max_call_levels(RHAI_MAX_CALL_LEVELS);
    engine.set_max_expr_depths(128, 64);
    engine.set_max_array_size(RHAI_MAX_ARRAY_SIZE);
    engine.set_max_map_size(RHAI_MAX_MAP_SIZE);
    engine.set_max_string_size(RHAI_MAX_STRING_SIZE);

    if let Some(mods) = registry {
        engine.set_module_resolver(VfsModuleResolver::new(mods));
    }

    register_event_type(&mut engine);
    register_core_and_song_api(&mut engine);
    register_gameplay_api(&mut engine);
    register_camera_api(&mut engine);
    register_character_and_modchart_api(&mut engine);
    register_props_tweens_timers_and_store_api(&mut engine);
    register_math_and_rng_api(&mut engine);
    register_shader_api(&mut engine);

    Arc::new(engine)
}

pub struct RhaiBackend {
    engine: Arc<Engine>,
    registry: Option<ModRegistry>,
}

impl Default for RhaiBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl RhaiBackend {
    pub fn new() -> Self {
        Self {
            engine: build_configured_engine(None),
            registry: None,
        }
    }

    pub fn with_registry(registry: ModRegistry) -> Self {
        Self {
            engine: build_configured_engine(Some(registry.clone())),
            registry: Some(registry),
        }
    }
}

impl ScriptBackend for RhaiBackend {
    fn language(&self) -> ScriptLanguage {
        ScriptLanguage::Rhai
    }

    fn extensions(&self) -> &[&str] {
        &[SCRIPT_EXT_RHAI]
    }

    fn compile(&self, source: ScriptSource) -> Result<Box<dyn ScriptInstance>, ScriptError> {
        let mut ast = self
            .engine
            .compile(&source.code)
            .map_err(|err| ScriptError::Compile {
                script_id: source.id.clone(),
                message: err.to_string(),
            })?;
        let source_label =
            vfs_rel_for_script(self.registry.as_ref(), source.path.as_deref(), &source.id);
        ast.set_source(source_label);

        let mut fn_arities = HashMap::new();
        for fn_meta in ast.iter_functions() {
            fn_arities.insert(fn_meta.name.to_string(), fn_meta.params.len());
        }

        Ok(Box::new(RhaiScriptInstance {
            engine: Arc::clone(&self.engine),
            id: source.id,
            mod_id: source.mod_id,
            scope_filter: source.scope,
            priority: source.priority,
            path: source.path,
            ast,
            rhai_scope: Scope::new(),
            script_state: Dynamic::from_map(rhai::Map::new()),
            fn_arities,
            initialized: false,
        }))
    }
}

pub struct RhaiScriptInstance {
    engine: Arc<Engine>,
    id: String,
    mod_id: String,
    scope_filter: ScriptScope,
    priority: i32,
    path: Option<PathBuf>,
    ast: AST,
    rhai_scope: Scope<'static>,
    script_state: Dynamic,
    fn_arities: HashMap<String, usize>,
    initialized: bool,
}

impl RhaiScriptInstance {
    fn ensure_initialized(&mut self, ctx: &mut ScriptContext) -> Result<(), ScriptError> {
        if self.initialized {
            return Ok(());
        }
        self.initialized = true;
        ctx.current_script_id = self.id.clone();
        ctx.current_mod_id = self.mod_id.clone();

        let owned_ctx = std::mem::replace(
            ctx,
            ScriptContext::new(
                ctx.snapshot.clone(),
                ctx.registry.clone(),
                ctx.store.clone(),
                ctx.rng_state,
            ),
        );
        let engine = &self.engine;
        let ast = &self.ast;
        let rhai_scope = &mut self.rhai_scope;
        let (returned_ctx, res) = with_tls_context(owned_ctx, || {
            engine.run_ast_with_scope(rhai_scope, ast)
        });
        *ctx = returned_ctx;
        res.map_err(|err| ScriptError::Runtime {
            script_id: self.id.clone(),
            hook: "top_level".to_string(),
            message: err.to_string(),
        })
    }

    fn resolve_hook_target(&self, hook: &HookKind) -> Option<(String, usize)> {
        for candidate in hook.candidate_names() {
            if let Some(&arity) = self.fn_arities.get(candidate) {
                return Some((candidate.to_string(), arity));
            }
        }
        None
    }
}

impl ScriptInstance for RhaiScriptInstance {
    fn id(&self) -> &str {
        &self.id
    }

    fn mod_id(&self) -> &str {
        &self.mod_id
    }

    fn scope(&self) -> &ScriptScope {
        &self.scope_filter
    }

    fn priority(&self) -> i32 {
        self.priority
    }

    fn language(&self) -> &ScriptLanguage {
        &ScriptLanguage::Rhai
    }

    fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn has_hook(&self, hook: &HookKind) -> bool {
        self.resolve_hook_target(hook).is_some()
    }

    fn call_hook(
        &mut self,
        hook: &HookKind,
        event: &mut ScriptEvent,
        ctx: &mut ScriptContext,
    ) -> Result<ScriptValue, ScriptError> {
        if matches!(hook, HookKind::Create) {
            self.ensure_initialized(ctx)?;
        }

        let Some((fn_name, arity)) = self.resolve_hook_target(hook) else {
            return Ok(ScriptValue::Null);
        };
        self.ensure_initialized(ctx)?;

        ctx.current_script_id = self.id.clone();
        ctx.current_mod_id = self.mod_id.clone();

        let shared_event = RhaiEvent::from_event(event.clone());
        let owned_ctx = std::mem::replace(
            ctx,
            ScriptContext::new(
                ctx.snapshot.clone(),
                ctx.registry.clone(),
                ctx.store.clone(),
                ctx.rng_state,
            ),
        );

        let engine = &self.engine;
        let ast = &self.ast;
        let scope = &mut self.rhai_scope;
        let state = &mut self.script_state;
        let ev_clone = shared_event.clone();

        let (returned_ctx, call_res) = with_tls_context(owned_ctx, || {
            let options = CallFnOptions::new()
                .eval_ast(false)
                .bind_this_ptr(state);
            if arity == 0 {
                engine.call_fn_with_options::<Dynamic>(options, scope, ast, &fn_name, ())
            } else {
                engine.call_fn_with_options::<Dynamic>(options, scope, ast, &fn_name, (ev_clone,))
            }
        });
        *ctx = returned_ctx;
        shared_event.snapshot_into(event);

        match call_res {
            Ok(val) => Ok(ScriptValue::from_rhai(&val)),
            Err(err) => Err(ScriptError::Runtime {
                script_id: self.id.clone(),
                hook: fn_name,
                message: err.to_string(),
            }),
        }
    }

    fn call_fn(
        &mut self,
        fn_name: &str,
        args: &[ScriptValue],
        ctx: &mut ScriptContext,
    ) -> Result<ScriptValue, ScriptError> {
        let Some(&arity) = self.fn_arities.get(fn_name) else {
            return Ok(ScriptValue::Null);
        };
        self.ensure_initialized(ctx)?;

        ctx.current_script_id = self.id.clone();
        ctx.current_mod_id = self.mod_id.clone();

        let owned_ctx = std::mem::replace(
            ctx,
            ScriptContext::new(
                ctx.snapshot.clone(),
                ctx.registry.clone(),
                ctx.store.clone(),
                ctx.rng_state,
            ),
        );

        let engine = &self.engine;
        let ast = &self.ast;
        let scope = &mut self.rhai_scope;
        let state = &mut self.script_state;
        let dyn_args: Vec<Dynamic> = args.iter().cloned().map(ScriptValue::into_rhai).collect();

        let (returned_ctx, call_res) = with_tls_context(owned_ctx, || {
            let options = CallFnOptions::new()
                .eval_ast(false)
                .bind_this_ptr(state);
            if arity == 0 || dyn_args.is_empty() {
                engine.call_fn_with_options::<Dynamic>(options, scope, ast, fn_name, ())
            } else if dyn_args.len() == 1 {
                engine.call_fn_with_options::<Dynamic>(
                    options,
                    scope,
                    ast,
                    fn_name,
                    (dyn_args[0].clone(),),
                )
            } else {
                engine.call_fn_with_options::<Dynamic>(options, scope, ast, fn_name, (dyn_args,))
            }
        });
        *ctx = returned_ctx;

        match call_res {
            Ok(val) => Ok(ScriptValue::from_rhai(&val)),
            Err(err) => Err(ScriptError::Runtime {
                script_id: self.id.clone(),
                hook: fn_name.to_string(),
                message: err.to_string(),
            }),
        }
    }
}
