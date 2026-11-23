use rhai::{Dynamic, Engine, EvalAltResult};

use crate::play::hud::subtitles::parse_srt;
use crate::scripting::api::{
    LogLevel, NewTimerRequest, ScriptCommand, TweenProp, TweenTarget, parse_hex_rgba,
};
use crate::scripting::backend::context::{dyn_to_f64, dyn_to_i64, runtime_err, with_ctx};
use crate::scripting::value::ScriptValue;

fn register_text_spawn_pair(engine: &mut Engine, name: &str, hud: bool) {
    engine.register_fn(
        name,
        move |id: &str, text: &str, x: Dynamic, y: Dynamic, size: Dynamic, hex: &str| -> Result<(), Box<EvalAltResult>> {
            let px = dyn_to_f64(&x)? as f32;
            let py = dyn_to_f64(&y)? as f32;
            let sz = (dyn_to_f64(&size)? as f32).max(1.0);
            let color = parse_hex_rgba(hex);
            with_ctx(|c| {
                c.snapshot.texts.insert(id.to_string(), text.to_string());
                c.snapshot.text_positions.insert(id.to_string(), [px, py]);
                c.snapshot.text_alphas.insert(id.to_string(), color[3]);
                c.commands.push(ScriptCommand::SpawnText {
                    id: id.to_string(),
                    text: text.to_string(),
                    pos: [px, py],
                    z: if hud { 15.0 } else { 5.0 },
                    size: sz,
                    color,
                    font: None,
                    align: "left".to_string(),
                    hud,
                });
            })
        },
    );
    engine.register_fn(
        name,
        move |id: &str, text: &str, x: Dynamic, y: Dynamic, z: Dynamic, size: Dynamic, hex: &str, align: &str| -> Result<(), Box<EvalAltResult>> {
            let px = dyn_to_f64(&x)? as f32;
            let py = dyn_to_f64(&y)? as f32;
            let pz = dyn_to_f64(&z)? as f32;
            let sz = (dyn_to_f64(&size)? as f32).max(1.0);
            let color = parse_hex_rgba(hex);
            with_ctx(|c| {
                c.snapshot.texts.insert(id.to_string(), text.to_string());
                c.snapshot.text_positions.insert(id.to_string(), [px, py]);
                c.snapshot.text_alphas.insert(id.to_string(), color[3]);
                c.commands.push(ScriptCommand::SpawnText {
                    id: id.to_string(),
                    text: text.to_string(),
                    pos: [px, py],
                    z: pz,
                    size: sz,
                    color,
                    font: None,
                    align: align.to_string(),
                    hud,
                });
            })
        },
    );
}

pub fn register_text_tweens_timers_and_store_api(engine: &mut Engine) {
    // Text API
    register_text_spawn_pair(engine, "text", false);
    register_text_spawn_pair(engine, "spawn_text", false);
    register_text_spawn_pair(engine, "hud_text", true);
    register_text_spawn_pair(engine, "spawn_hud_text", true);

    engine.register_fn("set_text", |id: &str, text: &str| {
        with_ctx(|c| {
            c.snapshot.texts.insert(id.to_string(), text.to_string());
            c.commands.push(ScriptCommand::SetText {
                id: id.to_string(),
                text: text.to_string(),
            });
        })
    });
    engine.register_fn("get_text", |id: &str| {
        with_ctx(|c| c.snapshot.texts.get(id).cloned().unwrap_or_default())
    });
    engine.register_fn("text_style", |id: &str, size: Dynamic, hex: &str| -> Result<(), Box<EvalAltResult>> {
        let sz = (dyn_to_f64(&size)? as f32).max(1.0);
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetTextStyle {
                id: id.to_string(),
                size: Some(sz),
                color: Some(parse_hex_rgba(hex)),
                align: None,
            });
        })
    });
    engine.register_fn("text_style", |id: &str, size: Dynamic, hex: &str, align: &str| -> Result<(), Box<EvalAltResult>> {
        let sz = (dyn_to_f64(&size)? as f32).max(1.0);
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetTextStyle {
                id: id.to_string(),
                size: Some(sz),
                color: Some(parse_hex_rgba(hex)),
                align: Some(align.to_string()),
            });
        })
    });
    engine.register_fn("text_font", |id: &str, font: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetTextFont {
                id: id.to_string(),
                font: font.to_string(),
            });
        })
    });

    // Subtitles (SRT tracks timed from the moment they are assigned, like V-Slice's
    // `Subtitles.assignSubtitles(sound)` behaviour adapted to the script sandbox).
    for name in ["subtitle_srt", "subtitles_srt"] {
        engine.register_fn(name, |path: &str| {
            with_ctx(|c| {
                match c.registry.read_to_string(path) {
                    Ok(raw) => c.commands.push(ScriptCommand::LoadSubtitles {
                        cues: parse_srt(&raw),
                    }),
                    Err(err) => c.commands.push(ScriptCommand::Log {
                        level: LogLevel::Warn,
                        mod_id: c.current_mod_id.clone(),
                        message: format!("subtitle_srt({path}): {err}"),
                    }),
                }
            })
        });
    }
    for name in ["subtitle_clear", "subtitles_clear"] {
        engine.register_fn(name, || {
            with_ctx(|c| c.commands.push(ScriptCommand::ClearSubtitles))
        });
    }

    // Unified Tweens (camera/char/prop/lane/sound/text + optional onComplete callback)
    engine.register_fn("tween", |target: &str, prop: &str, to: Dynamic, secs: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let t = TweenTarget::parse(target);
        let p = TweenProp::parse(prop)
            .ok_or_else(|| runtime_err(format!("unknown tween prop '{prop}'")))?;
        let end = dyn_to_f64(&to)?;
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| c.push_tween(t, p, end, dur, "linear"))
    });
    engine.register_fn("tween", |target: &str, prop: &str, to: Dynamic, secs: Dynamic, ease: &str| -> Result<(), Box<EvalAltResult>> {
        let t = TweenTarget::parse(target);
        let p = TweenProp::parse(prop)
            .ok_or_else(|| runtime_err(format!("unknown tween prop '{prop}'")))?;
        let end = dyn_to_f64(&to)?;
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| c.push_tween(t, p, end, dur, ease))
    });
    engine.register_fn("tween", |target: &str, prop: &str, to: Dynamic, secs: Dynamic, ease: &str, done_fn: &str| -> Result<(), Box<EvalAltResult>> {
        let t = TweenTarget::parse(target);
        let p = TweenProp::parse(prop)
            .ok_or_else(|| runtime_err(format!("unknown tween prop '{prop}'")))?;
        let end = dyn_to_f64(&to)?;
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| c.push_tween_cb(t, p, end, dur, ease, Some(done_fn.to_string())))
    });
    engine.register_fn("tween_prop", |id: &str, prop: &str, to: Dynamic, secs: Dynamic, ease: &str| -> Result<(), Box<EvalAltResult>> {
        let p = TweenProp::parse(prop)
            .ok_or_else(|| runtime_err(format!("unknown tween prop '{prop}'")))?;
        let end = dyn_to_f64(&to)?;
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| c.push_tween(TweenTarget::Prop(id.to_string()), p, end, dur, ease))
    });
    engine.register_fn("tween_prop", |id: &str, prop: &str, to: Dynamic, secs: Dynamic, ease: &str, done_fn: &str| -> Result<(), Box<EvalAltResult>> {
        let p = TweenProp::parse(prop)
            .ok_or_else(|| runtime_err(format!("unknown tween prop '{prop}'")))?;
        let end = dyn_to_f64(&to)?;
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| c.push_tween_cb(TweenTarget::Prop(id.to_string()), p, end, dur, ease, Some(done_fn.to_string())))
    });
    engine.register_fn("cancel_tweens", |target: &str| {
        let t = TweenTarget::parse(target);
        with_ctx(|c| c.cancel_tween_targets.push(t))
    });
    engine.register_fn("cancel_tween", |target: &str, prop: &str| -> Result<(), Box<EvalAltResult>> {
        let t = TweenTarget::parse(target);
        let p = TweenProp::parse(prop)
            .ok_or_else(|| runtime_err(format!("unknown tween prop '{prop}'")))?;
        with_ctx(|c| c.cancel_tween_props.push((t, p)))
    });

    // Timers (with optional payload arg passed into callback)
    engine.register_fn("after", |secs: Dynamic, fn_name: &str| -> Result<(), Box<EvalAltResult>> {
        let dur = dyn_to_f64(&secs)?.max(0.0);
        with_ctx(|c| {
            c.new_timers.push(NewTimerRequest {
                script_id: c.current_script_id.clone(),
                fn_name: fn_name.to_string(),
                interval_s: dur,
                repeats_left: Some(1),
                args: Vec::new(),
            });
        })
    });
    engine.register_fn("after", |secs: Dynamic, fn_name: &str, arg: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let dur = dyn_to_f64(&secs)?.max(0.0);
        let sv = ScriptValue::from_rhai(&arg);
        with_ctx(|c| {
            c.new_timers.push(NewTimerRequest {
                script_id: c.current_script_id.clone(),
                fn_name: fn_name.to_string(),
                interval_s: dur,
                repeats_left: Some(1),
                args: vec![sv],
            });
        })
    });
    engine.register_fn("every", |secs: Dynamic, fn_name: &str, count: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let dur = dyn_to_f64(&secs)?.max(0.001);
        let cnt = dyn_to_i64(&count)?;
        let repeats = if cnt <= 0 { None } else { Some(cnt as u32) };
        with_ctx(|c| {
            c.new_timers.push(NewTimerRequest {
                script_id: c.current_script_id.clone(),
                fn_name: fn_name.to_string(),
                interval_s: dur,
                repeats_left: repeats,
                args: Vec::new(),
            });
        })
    });
    engine.register_fn("every", |secs: Dynamic, fn_name: &str, count: Dynamic, arg: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let dur = dyn_to_f64(&secs)?.max(0.001);
        let cnt = dyn_to_i64(&count)?;
        let repeats = if cnt <= 0 { None } else { Some(cnt as u32) };
        let sv = ScriptValue::from_rhai(&arg);
        with_ctx(|c| {
            c.new_timers.push(NewTimerRequest {
                script_id: c.current_script_id.clone(),
                fn_name: fn_name.to_string(),
                interval_s: dur,
                repeats_left: repeats,
                args: vec![sv],
            });
        })
    });
    engine.register_fn("cancel_timer", |fn_name: &str| {
        with_ctx(|c| {
            c.cancel_timers_for_fn
                .push((c.current_script_id.clone(), fn_name.to_string()));
        })
    });

    // Basic VFS + ModStore helpers
    engine.register_fn("file_exists", |rel: &str| with_ctx(|c| c.registry.file_exists(rel)));
    engine.register_fn("read_file", |rel: &str| -> Result<Dynamic, Box<EvalAltResult>> {
        with_ctx(|c| match c.registry.read_to_string(rel) {
            Ok(txt) => Dynamic::from(txt),
            Err(_) => Dynamic::UNIT,
        })
    });
    engine.register_fn("read_json", |rel: &str| -> Result<Dynamic, Box<EvalAltResult>> {
        with_ctx(|c| {
            let Ok(txt) = c.registry.read_to_string(rel) else {
                return Dynamic::UNIT;
            };
            let Ok(json) = serde_json::from_str::<serde_json::Value>(&txt) else {
                return Dynamic::UNIT;
            };
            ScriptValue::from_json(&json).into_rhai()
        })
    });
    engine.register_fn("read_json", |rel: &str, key: &str| -> Result<Dynamic, Box<EvalAltResult>> {
        with_ctx(|c| {
            let Ok(txt) = c.registry.read_to_string(rel) else {
                return Dynamic::UNIT;
            };
            let Ok(json) = serde_json::from_str::<serde_json::Value>(&txt) else {
                return Dynamic::UNIT;
            };
            json.get(key)
                .map(|sub| ScriptValue::from_json(sub).into_rhai())
                .unwrap_or(Dynamic::UNIT)
        })
    });

    engine.register_fn("save_has", |key: &str| {
        with_ctx(|c| c.store.has(&c.current_mod_id.clone(), key))
    });
    engine.register_fn("save_get", |key: &str| {
        with_ctx(|c| {
            let ns = c.current_mod_id.clone();
            c.store
                .get(&ns, key)
                .map(|v| ScriptValue::from_json(v).into_rhai())
                .unwrap_or(Dynamic::UNIT)
        })
    });
    engine.register_fn("save_set", |key: &str, val: Dynamic| {
        let json = ScriptValue::from_rhai(&val).to_json();
        with_ctx(|c| {
            let ns = c.current_mod_id.clone();
            c.store.set(&ns, key, json);
        })
    });
    engine.register_fn("save_del", |key: &str| {
        with_ctx(|c| {
            let ns = c.current_mod_id.clone();
            c.store.remove(&ns, key).is_some()
        })
    });
    engine.register_fn("save_keys", || {
        with_ctx(|c| {
            let ns = c.current_mod_id.clone();
            c.store
                .keys(&ns)
                .into_iter()
                .map(Dynamic::from)
                .collect::<rhai::Array>()
        })
    });
    engine.register_fn("save_flush", || -> Result<bool, Box<EvalAltResult>> {
        with_ctx(|c| {
            let ns = c.current_mod_id.clone();
            c.store.save_namespace(&ns).is_ok()
        })
    });
    engine.register_fn("global_has", |key: &str| with_ctx(|c| c.store.has("global", key)));
    engine.register_fn("global_get", |key: &str| {
        with_ctx(|c| {
            c.store
                .get("global", key)
                .map(|v| ScriptValue::from_json(v).into_rhai())
                .unwrap_or(Dynamic::UNIT)
        })
    });
    engine.register_fn("global_set", |key: &str, val: Dynamic| {
        let json = ScriptValue::from_rhai(&val).to_json();
        with_ctx(|c| c.store.set("global", key, json))
    });
    engine.register_fn("global_del", |key: &str| {
        with_ctx(|c| c.store.remove("global", key).is_some())
    });
    engine.register_fn("global_flush", || -> Result<bool, Box<EvalAltResult>> {
        with_ctx(|c| c.store.save_namespace("global").is_ok())
    });
}
