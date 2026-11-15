// Public scripting API surface exposed for Rhai and future Lua/Wasm backends.
#![allow(dead_code)]

pub mod api;
pub mod backend;
pub mod events;
pub mod host;
pub mod rhai_backend;
pub mod value;

// Public scripting API surface exposed for current Rhai and future Lua/Wasm backends.
#[allow(unused_imports)]
pub use api::{
    LogLevel, PropertyTween, SCRIPT_API_CATALOG, ScriptApiDoc, ScriptCommand, ScriptContext,
    ScriptMutableState, ScriptStateSnapshot, TweenProp, TweenTarget,
};
#[allow(unused_imports)]
pub use backend::{ScriptBackend, ScriptError, ScriptInstance, ScriptSource};
pub use events::{HookKind, ScriptEvent};
pub use host::ScriptHost;
#[allow(unused_imports)]
pub use rhai_backend::RhaiBackend;
#[allow(unused_imports)]
pub use value::{ScriptLanguage, ScriptValue};
