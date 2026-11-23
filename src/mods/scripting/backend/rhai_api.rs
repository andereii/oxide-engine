use rhai::{Dynamic, Engine};

use crate::scripting::api::{ScriptApiEntry, ScriptApiFn};
use crate::scripting::value::ScriptValue;

use super::context::{script_value_from_rhai, script_value_into_rhai, with_ctx};

fn invoke_rhai_handler(
    handler: ScriptApiFn,
    args: &[ScriptValue],
) -> Result<Dynamic, Box<rhai::EvalAltResult>> {
    let res = with_ctx(|ctx| handler(ctx, args))?;
    res.map(script_value_into_rhai)
        .map_err(super::context::runtime_err)
}

macro_rules! register_rhai_arity {
    ($engine:expr, $name:expr, $handler:expr, 0) => {{
        let handler = $handler;
        $engine.register_fn($name, move || invoke_rhai_handler(handler, &[]));
    }};
    ($engine:expr, $name:expr, $handler:expr, $($arg:ident),+) => {{
        let handler = $handler;
        $engine.register_fn($name, move |$($arg: Dynamic),+| {
            let args = [$(script_value_from_rhai(&$arg)),+];
            invoke_rhai_handler(handler, &args)
        });
    }};
}

/// Binds every language-agnostic [`ScriptApiEntry`] onto a Rhai `Engine`.
pub(super) fn register_api_bindings(engine: &mut Engine, entries: &[ScriptApiEntry]) {
    for entry in entries {
        for &name in entry.names {
            for &arity in entry.arities {
                match arity {
                    0 => register_rhai_arity!(engine, name, entry.handler, 0),
                    1 => register_rhai_arity!(engine, name, entry.handler, a0),
                    2 => register_rhai_arity!(engine, name, entry.handler, a0, a1),
                    3 => register_rhai_arity!(engine, name, entry.handler, a0, a1, a2),
                    4 => register_rhai_arity!(engine, name, entry.handler, a0, a1, a2, a3),
                    5 => register_rhai_arity!(engine, name, entry.handler, a0, a1, a2, a3, a4),
                    6 => register_rhai_arity!(engine, name, entry.handler, a0, a1, a2, a3, a4, a5),
                    7 => {
                        register_rhai_arity!(engine, name, entry.handler, a0, a1, a2, a3, a4, a5, a6)
                    }
                    8 => register_rhai_arity!(
                        engine,
                        name,
                        entry.handler,
                        a0,
                        a1,
                        a2,
                        a3,
                        a4,
                        a5,
                        a6,
                        a7
                    ),
                    9 => register_rhai_arity!(
                        engine,
                        name,
                        entry.handler,
                        a0,
                        a1,
                        a2,
                        a3,
                        a4,
                        a5,
                        a6,
                        a7,
                        a8
                    ),
                    10 => register_rhai_arity!(
                        engine,
                        name,
                        entry.handler,
                        a0,
                        a1,
                        a2,
                        a3,
                        a4,
                        a5,
                        a6,
                        a7,
                        a8,
                        a9
                    ),
                    11 => register_rhai_arity!(
                        engine,
                        name,
                        entry.handler,
                        a0,
                        a1,
                        a2,
                        a3,
                        a4,
                        a5,
                        a6,
                        a7,
                        a8,
                        a9,
                        a10
                    ),
                    12 => register_rhai_arity!(
                        engine,
                        name,
                        entry.handler,
                        a0,
                        a1,
                        a2,
                        a3,
                        a4,
                        a5,
                        a6,
                        a7,
                        a8,
                        a9,
                        a10,
                        a11
                    ),
                    _ => debug_assert!(false, "unsupported script API arity {arity} for '{name}'"),
                }
            }
        }
    }
}
