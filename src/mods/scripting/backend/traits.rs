use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::mods::{DiscoveredScript, ScriptScope};

use crate::scripting::api::ScriptContext;
use crate::scripting::events::{HookKind, ScriptEvent};
use crate::scripting::value::{ScriptLanguage, ScriptValue};

/// File and optional 1-based line/column for a script failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptLoc {
    pub file: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

impl std::fmt::Display for ScriptLoc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.line, self.column) {
            (Some(line), Some(column)) => write!(f, "{}:{line}:{column}", self.file),
            (Some(line), None) => write!(f, "{}:{line}", self.file),
            _ => f.write_str(&self.file),
        }
    }
}

#[derive(Debug, Error)]
pub enum ScriptError {
    #[error("failed to read script {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("no scripting backend registered for extension '.{0}'")]
    UnsupportedExtension(String),
    #[error("compile error in {loc}: {message}")]
    Compile {
        script_id: String,
        loc: ScriptLoc,
        message: String,
    },
    #[error("runtime error in {loc} ({hook}): {message}")]
    Runtime {
        script_id: String,
        loc: ScriptLoc,
        hook: String,
        message: String,
    },
}

#[derive(Debug, Clone)]
pub struct ScriptSource {
    pub id: String,
    pub mod_id: String,
    pub name: String,
    pub scope: ScriptScope,
    pub priority: i32,
    pub path: Option<PathBuf>,
    pub code: String,
}

impl ScriptSource {
    pub fn from_discovered(script: &DiscoveredScript) -> Result<Self, ScriptError> {
        let code = fs::read_to_string(&script.path).map_err(|source| ScriptError::Io {
            path: script.path.clone(),
            source,
        })?;
        Ok(Self {
            id: script.id.clone(),
            mod_id: script.mod_id.clone(),
            name: script.name.clone(),
            scope: script.scope.clone(),
            priority: script.priority,
            path: Some(script.path.clone()),
            code,
        })
    }

    pub fn inline(id: impl Into<String>, mod_id: impl Into<String>, code: impl Into<String>) -> Self {
        let id = id.into();
        let mod_id = mod_id.into();
        Self {
            name: id.clone(),
            id,
            mod_id,
            scope: ScriptScope::Global,
            priority: 1000,
            path: None,
            code: code.into(),
        }
    }
}

/// Instance of a compiled script owned by `ScriptHost`.
/// Any future runtime (`Lua`, `Wasm`, etc.) implements this trait to participate identically
/// in hook dispatch, event cancellation, timers, tweens, and hot-reload.
pub trait ScriptInstance: Send + Sync {
    fn id(&self) -> &str;
    fn mod_id(&self) -> &str;
    fn scope(&self) -> &ScriptScope;
    fn priority(&self) -> i32;
    fn language(&self) -> &ScriptLanguage;
    fn path(&self) -> Option<&Path>;
    fn has_hook(&self, hook: &HookKind) -> bool;

    fn call_hook(
        &mut self,
        hook: &HookKind,
        event: &mut ScriptEvent,
        ctx: &mut ScriptContext,
    ) -> Result<ScriptValue, ScriptError>;

    fn call_fn(
        &mut self,
        fn_name: &str,
        args: &[ScriptValue],
        ctx: &mut ScriptContext,
    ) -> Result<ScriptValue, ScriptError>;
}

/// Compiler / VM factory for one scripting language (e.g. Rhai today, Lua / Wasm in the future).
pub trait ScriptBackend: Send + Sync {
    fn language(&self) -> ScriptLanguage;
    fn extensions(&self) -> &[&str];

    fn supports_extension(&self, ext: &str) -> bool {
        let cleaned = ext.trim().trim_start_matches('.');
        self.extensions()
            .iter()
            .any(|supported| supported.eq_ignore_ascii_case(cleaned))
    }

    fn compile(&self, source: ScriptSource) -> Result<Box<dyn ScriptInstance>, ScriptError>;

    fn watched_module_paths(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    fn invalidate_caches(&self) {}
}
