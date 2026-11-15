// Public scripting API surface exposed for Rhai and future Lua/Wasm backends.
#![allow(dead_code)]

pub mod backend;
pub mod events;
pub mod host;
pub mod value;

pub use backend::api;

// Public scripting API surface exposed for current Rhai and future Lua/Wasm backends.
#[allow(unused_imports)]
pub use api::{
    LogLevel, PropertyTween, SCRIPT_API_CATALOG, ScriptApiDoc, ScriptCommand, ScriptContext,
    ScriptMutableState, ScriptPropAnimKind, ScriptStateSnapshot, TweenProp, TweenTarget,
};
#[allow(unused_imports)]
pub use backend::{RhaiBackend, ScriptBackend, ScriptError, ScriptInstance, ScriptSource};
pub use events::{HookKind, ScriptEvent};
pub use host::ScriptHost;
#[allow(unused_imports)]
pub use value::{ScriptLanguage, ScriptValue};
