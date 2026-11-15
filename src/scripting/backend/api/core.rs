use rhai::{Dynamic, Engine, EvalAltResult};

use crate::scripting::api::LogLevel;
use crate::scripting::value::ScriptValue;

use crate::scripting::backend::context::{dyn_to_f64, dyn_to_i64, with_ctx};

pub fn register_core_and_song_api(engine: &mut Engine) {
    let push_log = |level: LogLevel, msg: &str| {
        let _ = with_ctx(|ctx| {
            ctx.commands.push(ScriptCommand::Log {
                level,
                mod_id: ctx.current_mod_id.clone(),
                message: msg.to_string(),
            });
        });
    };

    engine.register_fn("log", move |msg: Dynamic| {
        push_log(LogLevel::Info, &msg.to_string());
    });
    engine.register_fn("trace", move |msg: Dynamic| {
        push_log(LogLevel::Info, &msg.to_string());
    });
    engine.register_fn("warn", move |msg: Dynamic| {
        push_log(LogLevel::Warn, &msg.to_string());
    });
    engine.register_fn("err", move |msg: Dynamic| {
        push_log(LogLevel::Error, &msg.to_string());
    });

    engine.register_fn("song", || with_ctx(|c| c.snapshot.song_id.clone()));
    engine.register_fn("diff", || with_ctx(|c| c.snapshot.difficulty.clone()));
    engine.register_fn("variation", || with_ctx(|c| c.snapshot.variation.clone()));
    engine.register_fn("stage", || with_ctx(|c| c.snapshot.stage_id.clone()));
    engine.register_fn("time", || with_ctx(|c| c.snapshot.song_position_ms));
    engine.register_fn("bpm", || with_ctx(|c| c.snapshot.bpm));
    engine.register_fn("step", || with_ctx(|c| i64::from(c.snapshot.step)));
    engine.register_fn("beat", || with_ctx(|c| i64::from(c.snapshot.beat)));
    engine.register_fn("measure", || with_ctx(|c| i64::from(c.snapshot.measure)));
    engine.register_fn("step_ms", || with_ctx(|c| c.snapshot.step_length_ms));
    engine.register_fn("beat_ms", || with_ctx(|c| c.snapshot.beat_length_ms));
    engine.register_fn("beat_f", || with_ctx(|c| c.snapshot.beat_f()));
    engine.register_fn("step_f", || with_ctx(|c| c.snapshot.step_f()));
    engine.register_fn("dt", || with_ctx(|c| c.snapshot.dt_s));

    engine.register_fn("emit", |name: &str| -> Result<(), Box<EvalAltResult>> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(runtime_err("custom hook name cannot be empty"));
        }
        with_ctx(|c| c.emit_custom_hook(trimmed, ScriptValue::Null))
    });
    engine.register_fn("emit", |name: &str, val: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(runtime_err("custom hook name cannot be empty"));
        }
        let data = ScriptValue::from_rhai(&val);
        with_ctx(|c| c.emit_custom_hook(trimmed, data))
    });

    engine.register_fn("mods", || {
        with_ctx(|c| {
            let list: rhai::Array = c
                .registry
                .active_mod_ids()
                .into_iter()
                .map(|id| Dynamic::from(id.to_string()))
                .collect();
            list
        })
    });
    engine.register_fn("mod_exists", |id: &str| {
        with_ctx(|c| c.registry.active_mod_ids().contains(&id))
    });
    engine.register_fn("mod_cfg", |key: &str| {
        with_ctx(|c| {
            c.registry
                .get_mod_config(&c.current_mod_id, key)
                .map(|v| ScriptValue::from_json(v).into_rhai())
                .unwrap_or(Dynamic::UNIT)
        })
    });
    engine.register_fn("cfg", |mod_id: &str, key: &str| {
        with_ctx(|c| {
            c.registry
                .get_mod_config(mod_id, key)
                .map(|v| ScriptValue::from_json(v).into_rhai())
                .unwrap_or(Dynamic::UNIT)
        })
    });
}


pub fn register_math_and_rng_api(engine: &mut Engine) {
    engine.register_fn("pi", || std::f64::consts::PI);
    engine.register_fn("lerp", |a: Dynamic, b: Dynamic, t: Dynamic| -> Result<f64, Box<EvalAltResult>> {
        let a = dyn_to_f64(&a)?;
        let b = dyn_to_f64(&b)?;
        let t = dyn_to_f64(&t)?;
        Ok(script_lerp(a, b, t))
    });
    engine.register_fn("clamp", |v: Dynamic, lo: Dynamic, hi: Dynamic| -> Result<f64, Box<EvalAltResult>> {
        let v = dyn_to_f64(&v)?;
        let lo = dyn_to_f64(&lo)?;
        let hi = dyn_to_f64(&hi)?;
        Ok(script_clamp(v, lo, hi))
    });
    engine.register_fn(
        "remap",
        |v: Dynamic, in0: Dynamic, in1: Dynamic, out0: Dynamic, out1: Dynamic| -> Result<f64, Box<EvalAltResult>> {
            let v = dyn_to_f64(&v)?;
            let in0 = dyn_to_f64(&in0)?;
            let in1 = dyn_to_f64(&in1)?;
            let out0 = dyn_to_f64(&out0)?;
            let out1 = dyn_to_f64(&out1)?;
            Ok(script_remap(v, in0, in1, out0, out1))
        },
    );
    engine.register_fn("sin_beat", |mult: Dynamic, amp: Dynamic| -> Result<f64, Box<EvalAltResult>> {
        let mult = dyn_to_f64(&mult)?;
        let amp = dyn_to_f64(&amp)?;
        with_ctx(|c| c.snapshot.sin_beat(mult, amp))
    });
    engine.register_fn("cos_beat", |mult: Dynamic, amp: Dynamic| -> Result<f64, Box<EvalAltResult>> {
        let mult = dyn_to_f64(&mult)?;
        let amp = dyn_to_f64(&amp)?;
        with_ctx(|c| c.snapshot.cos_beat(mult, amp))
    });
    engine.register_fn("rand", || with_ctx(|c| c.next_rand_f64()));
    engine.register_fn("rand_range", |lo: Dynamic, hi: Dynamic| -> Result<f64, Box<EvalAltResult>> {
        let lo = dyn_to_f64(&lo)?;
        let hi = dyn_to_f64(&hi)?;
        with_ctx(|c| lo + (hi - lo) * c.next_rand_f64())
    });
    engine.register_fn("rand_int", |lo: Dynamic, hi: Dynamic| -> Result<i64, Box<EvalAltResult>> {
        let lo = dyn_to_i64(&lo)?;
        let hi = dyn_to_i64(&hi)?;
        if hi <= lo {
            return Ok(lo);
        }
        let span = (hi - lo + 1) as u64;
        with_ctx(|c| lo + (c.next_rand_u64() % span) as i64)
    });
    engine.register_fn("sec_to_ms", |s: Dynamic| -> Result<f64, Box<EvalAltResult>> {
        Ok(dyn_to_f64(&s)? * MS_PER_SEC)
    });
}

