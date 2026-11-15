use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rhai::{AST, CallFnOptions, Dynamic, Engine, EvalAltResult, Position, Scope};

use crate::mods::ScriptScope;
use crate::utils::constants::{
    MS_PER_SEC, RHAI_MAX_ARRAY_SIZE, RHAI_MAX_CALL_LEVELS, RHAI_MAX_MAP_SIZE, RHAI_MAX_OPERATIONS,
    RHAI_MAX_STRING_SIZE, SCRIPT_EXT_RHAI,
};

use super::api::{
    LogLevel, NewTimerRequest, ScriptCommand, ScriptContext, TweenProp, TweenTarget,
    parse_hex_rgba, script_clamp, script_lerp, script_remap,
};
use super::backend::{ScriptBackend, ScriptError, ScriptInstance, ScriptSource};
use super::events::{HookKind, ScriptEvent};
use super::value::{ScriptLanguage, ScriptValue};

thread_local! {
    static ACTIVE_CTX: RefCell<Option<ScriptContext>> = const { RefCell::new(None) };
}

fn with_tls_context<R>(ctx: ScriptContext, f: impl FnOnce() -> R) -> (ScriptContext, R) {
    struct ResetOnDrop {
        previous: Option<ScriptContext>,
    }

    impl Drop for ResetOnDrop {
        fn drop(&mut self) {
            if let Some(prev) = self.previous.take() {
                ACTIVE_CTX.with(|slot| {
                    *slot.borrow_mut() = Some(prev);
                });
            }
        }
    }

    let previous = ACTIVE_CTX.with(|slot| slot.borrow_mut().replace(ctx));
    let mut guard = ResetOnDrop { previous };
    let result = f();
    let returned = ACTIVE_CTX
        .with(|slot| slot.borrow_mut().take())
        .expect("script TLS context should remain present");
    if let Some(prev) = guard.previous.take() {
        ACTIVE_CTX.with(|slot| {
            *slot.borrow_mut() = Some(prev);
        });
    }
    (returned, result)
}

fn runtime_err(msg: impl Into<String>) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(msg.into().into(), Position::NONE))
}

fn with_ctx<R>(f: impl FnOnce(&mut ScriptContext) -> R) -> Result<R, Box<EvalAltResult>> {
    ACTIVE_CTX.with(|slot| {
        let mut borrow = slot
            .try_borrow_mut()
            .map_err(|_| runtime_err("re-entrant script context borrow"))?;
        let ctx = borrow
            .as_mut()
            .ok_or_else(|| runtime_err("script API called outside active ScriptContext"))?;
        Ok(f(ctx))
    })
}

fn dyn_to_f64(val: &Dynamic) -> Result<f64, Box<EvalAltResult>> {
    if let Ok(v) = val.as_float() {
        return Ok(v);
    }
    if let Ok(v) = val.as_int() {
        return Ok(v as f64);
    }
    Err(runtime_err(format!("expected numeric value, got {}", val.type_name())))
}

fn dyn_to_i64(val: &Dynamic) -> Result<i64, Box<EvalAltResult>> {
    if let Ok(v) = val.as_int() {
        return Ok(v);
    }
    if let Ok(v) = val.as_float() {
        return Ok(v as i64);
    }
    Err(runtime_err(format!("expected integer value, got {}", val.type_name())))
}

fn dyn_to_usize(val: &Dynamic) -> Result<usize, Box<EvalAltResult>> {
    let i = dyn_to_i64(val)?;
    usize::try_from(i).map_err(|_| runtime_err(format!("expected non-negative index, got {i}")))
}

#[derive(Clone)]
pub struct RhaiEvent {
    inner: Arc<Mutex<ScriptEvent>>,
}

impl RhaiEvent {
    fn from_event(event: ScriptEvent) -> Self {
        Self {
            inner: Arc::new(Mutex::new(event)),
        }
    }

    fn snapshot_into(&self, target: &mut ScriptEvent) {
        if let Ok(guard) = self.inner.lock() {
            *target = guard.clone();
        }
    }

    fn with_inner<R>(&self, f: impl FnOnce(&mut ScriptEvent) -> R) -> R {
        match self.inner.lock() {
            Ok(mut guard) => f(&mut guard),
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                f(&mut guard)
            }
        }
    }

    pub fn cancel(&mut self) {
        self.with_inner(ScriptEvent::cancel);
    }

    pub fn stop(&mut self) {
        self.with_inner(ScriptEvent::stop_propagation);
    }

    pub fn is_canceled(&mut self) -> bool {
        self.with_inner(|e| e.canceled)
    }

    pub fn kind_name(&mut self) -> String {
        self.with_inner(|e| e.kind.canonical_name().to_string())
    }

    pub fn has(&mut self, key: &str) -> bool {
        self.with_inner(|e| e.data.contains_key(key))
    }

    pub fn get(&mut self, key: &str) -> Dynamic {
        self.with_inner(|e| {
            let alias = match key {
                "note_kind" => Some("kind"),
                "hold_len" => Some("length"),
                "lane" => Some("dir"),
                _ => None,
            };
            e.data
                .get(key)
                .or_else(|| alias.and_then(|k| e.data.get(k)))
                .cloned()
                .map(ScriptValue::into_rhai)
                .unwrap_or(Dynamic::UNIT)
        })
    }

    pub fn set(&mut self, key: &str, value: Dynamic) {
        let converted = ScriptValue::from_rhai(&value);
        self.with_inner(|e| e.set(key.to_string(), converted));
    }
}

fn register_event_type(engine: &mut Engine) {
    engine.register_type_with_name::<RhaiEvent>("ScriptEvent");
    engine.register_fn("cancel", RhaiEvent::cancel);
    engine.register_fn("stop", RhaiEvent::stop);
    engine.register_fn("stop_propagation", RhaiEvent::stop);
    engine.register_fn("is_canceled", RhaiEvent::is_canceled);
    engine.register_get("canceled", RhaiEvent::is_canceled);
    engine.register_get("cancelled", RhaiEvent::is_canceled);
    engine.register_get("kind", RhaiEvent::kind_name);
    engine.register_fn("has", RhaiEvent::has);
    engine.register_fn("get", RhaiEvent::get);
    engine.register_fn("set", RhaiEvent::set);
    engine.register_indexer_get(|ev: &mut RhaiEvent, key: &str| -> Dynamic { ev.get(key) });
    engine.register_indexer_set(|ev: &mut RhaiEvent, key: &str, val: Dynamic| ev.set(key, val));

    for prop in [
        "id", "dir", "note_kind", "length", "hold_len", "combo", "is_hold", "remaining_ms",
        "health_change", "score_change", "score", "hp", "step", "beat", "measure", "dt", "name",
        "value", "data", "source", "judgement", "lane", "char_id", "time", "count", "is_player",
    ] {
        engine.register_get(prop, move |ev: &mut RhaiEvent| -> Dynamic { ev.get(prop) });
        engine.register_set(prop, move |ev: &mut RhaiEvent, val: Dynamic| ev.set(prop, val));
    }
}

fn register_core_and_song_api(engine: &mut Engine) {
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

fn register_gameplay_api(engine: &mut Engine) {
    engine.register_fn("hp", || with_ctx(|c| c.mutable.health));
    engine.register_fn("set_hp", |v: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let num = dyn_to_f64(&v)?;
        with_ctx(|c| {
            c.mutable.health = num;
            c.mutable.dirty_health = true;
        })
    });
    engine.register_fn("add_hp", |d: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let delta = dyn_to_f64(&d)?;
        with_ctx(|c| {
            c.mutable.health += delta;
            c.mutable.dirty_health = true;
        })
    });

    engine.register_fn("score", || with_ctx(|c| c.mutable.score));
    engine.register_fn("set_score", |v: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let num = dyn_to_f64(&v)?;
        with_ctx(|c| {
            c.mutable.score = num;
            c.mutable.dirty_score = true;
        })
    });
    engine.register_fn("add_score", |d: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let delta = dyn_to_f64(&d)?;
        with_ctx(|c| {
            c.mutable.score += delta;
            c.mutable.dirty_score = true;
        })
    });

    engine.register_fn("combo", || with_ctx(|c| i64::from(c.snapshot.combo)));
    engine.register_fn("misses", || with_ctx(|c| i64::from(c.snapshot.misses)));
    engine.register_fn("note_count", || with_ctx(|c| c.snapshot.note_count as i64));

    engine.register_fn("botplay", || with_ctx(|c| c.mutable.botplay));
    engine.register_fn("set_botplay", |enabled: bool| {
        with_ctx(|c| {
            c.mutable.botplay = enabled;
            c.mutable.dirty_botplay = true;
        })
    });

    engine.register_fn("scroll", || with_ctx(|c| c.mutable.scroll_speed));
    engine.register_fn("set_scroll", |v: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let speed = dyn_to_f64(&v)?;
        with_ctx(|c| {
            c.mutable.scroll_speed = speed;
            c.mutable.dirty_scroll_speed = true;
        })
    });
    engine.register_fn("tween_scroll", |v: Dynamic, secs: Dynamic, ease: &str| -> Result<(), Box<EvalAltResult>> {
        let target = dyn_to_f64(&v)?;
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::TweenScrollSpeed {
                target,
                duration_s: dur.max(0.0),
                ease: ease.to_string(),
            });
        })
    });

    engine.register_fn("rate", || with_ctx(|c| c.mutable.playback_rate));
    engine.register_fn("set_rate", |v: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let rate = dyn_to_f64(&v)?.max(0.05);
        with_ctx(|c| {
            c.mutable.playback_rate = rate;
            c.mutable.dirty_playback_rate = true;
        })
    });

    engine.register_fn("downscroll", || with_ctx(|c| c.snapshot.downscroll));
    engine.register_fn("middlescroll", || with_ctx(|c| c.mutable.middlescroll));
    engine.register_fn("set_middlescroll", |enabled: bool| {
        with_ctx(|c| {
            c.mutable.middlescroll = enabled;
            c.mutable.dirty_middlescroll = true;
        })
    });
    engine.register_fn("swap_strums", |enabled: bool| {
        with_ctx(|c| {
            c.mutable.strums_swapped = enabled;
            c.mutable.dirty_strums_swapped = true;
        })
    });

    engine.register_fn("status", |text: &str| {
        with_ctx(|c| c.commands.push(ScriptCommand::SetStatus(text.to_string())))
    });
    engine.register_fn("set_score_text", |text: &str| {
        with_ctx(|c| {
            let val = if text.is_empty() { None } else { Some(text.to_string()) };
            c.commands.push(ScriptCommand::SetScoreText(val));
        })
    });
    engine.register_fn("clear_score_text", || {
        with_ctx(|c| c.commands.push(ScriptCommand::SetScoreText(None)))
    });
    engine.register_fn("hud_visible", |visible: bool| {
        with_ctx(|c| c.commands.push(ScriptCommand::SetHudVisible(visible)))
    });

    engine.register_fn("inst_vol", |v: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let vol = dyn_to_f64(&v)? as f32;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetAudioVolumes {
                inst: Some(vol),
                vocals: None,
                player_vocal: None,
                opponent_vocal: None,
            });
        })
    });
    engine.register_fn("vocals_vol", |v: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let vol = dyn_to_f64(&v)? as f32;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetAudioVolumes {
                inst: None,
                vocals: Some(vol),
                player_vocal: None,
                opponent_vocal: None,
            });
        })
    });
    engine.register_fn("p_vocal_vol", |v: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let vol = dyn_to_f64(&v)? as f32;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetAudioVolumes {
                inst: None,
                vocals: None,
                player_vocal: Some(vol),
                opponent_vocal: None,
            });
        })
    });
    engine.register_fn("o_vocal_vol", |v: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let vol = dyn_to_f64(&v)? as f32;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetAudioVolumes {
                inst: None,
                vocals: None,
                player_vocal: None,
                opponent_vocal: Some(vol),
            });
        })
    });

    engine.register_fn("add_note", |time_ms: Dynamic, dir: Dynamic, len_ms: Dynamic, kind: &str| -> Result<(), Box<EvalAltResult>> {
        let time_ms = dyn_to_f64(&time_ms)?;
        let dir = dyn_to_usize(&dir)?;
        let length_ms = dyn_to_f64(&len_ms)?.max(0.0);
        with_ctx(|c| {
            c.commands.push(ScriptCommand::AddNote {
                time_ms,
                dir,
                length_ms,
                kind: kind.to_string(),
            });
        })
    });
    engine.register_fn("trigger_event", |name: &str, val: Dynamic| {
        let json = ScriptValue::from_rhai(&val).to_json();
        with_ctx(|c| {
            c.commands.push(ScriptCommand::TriggerSongEvent {
                name: name.to_string(),
                value: json,
            });
        })
    });
    engine.register_fn("sfx", |path: &str, vol: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let volume = dyn_to_f64(&vol)? as f32;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::PlaySfx {
                path: path.to_string(),
                volume,
            });
        })
    });
    engine.register_fn("sfx", |path: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::PlaySfx {
                path: path.to_string(),
                volume: 1.0,
            });
        })
    });
    engine.register_fn("end_song", || with_ctx(|c| c.commands.push(ScriptCommand::EndSong)));
    engine.register_fn("restart_song", || with_ctx(|c| c.commands.push(ScriptCommand::RestartSong)));
    engine.register_fn("pause_song", || with_ctx(|c| c.commands.push(ScriptCommand::PauseSong)));
}

fn register_camera_api(engine: &mut Engine) {
    engine.register_fn("cam_zoom", || with_ctx(|c| c.mutable.cam_zoom));
    engine.register_fn("set_zoom", |z: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let val = dyn_to_f64(&z)?;
        with_ctx(|c| {
            c.mutable.cam_zoom = val;
            c.mutable.dirty_cam_zoom = true;
        })
    });
    engine.register_fn("zoom", |z: Dynamic, secs: Dynamic, ease: &str| -> Result<(), Box<EvalAltResult>> {
        let target = dyn_to_f64(&z)?;
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::TweenCameraZoom {
                target,
                duration_s: dur.max(0.0),
                ease: ease.to_string(),
            });
        })
    });
    engine.register_fn("zoom", |z: Dynamic, secs: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let target = dyn_to_f64(&z)?;
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::TweenCameraZoom {
                target,
                duration_s: dur.max(0.0),
                ease: "quadOut".to_string(),
            });
        })
    });

    engine.register_fn("hud_zoom", || with_ctx(|c| c.mutable.hud_zoom));
    engine.register_fn("set_hud_zoom", |z: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let val = dyn_to_f64(&z)?;
        with_ctx(|c| {
            c.mutable.hud_zoom = val;
            c.mutable.dirty_hud_zoom = true;
        })
    });

    engine.register_fn("cam_pos", || {
        with_ctx(|c| vec![Dynamic::from(c.mutable.cam_x), Dynamic::from(c.mutable.cam_y)])
    });
    engine.register_fn("set_cam", |x: Dynamic, y: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let x = dyn_to_f64(&x)?;
        let y = dyn_to_f64(&y)?;
        with_ctx(|c| {
            c.mutable.cam_x = x;
            c.mutable.cam_y = y;
            c.mutable.dirty_cam_pos = true;
        })
    });
    engine.register_fn("pan_cam", |x: Dynamic, y: Dynamic, secs: Dynamic, ease: &str| -> Result<(), Box<EvalAltResult>> {
        let x = dyn_to_f64(&x)?;
        let y = dyn_to_f64(&y)?;
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::TweenCameraPos {
                x,
                y,
                duration_s: dur.max(0.0),
                ease: ease.to_string(),
            });
        })
    });

    engine.register_fn("focus", |target: &str| {
        with_ctx(|c| {
            c.commands
                .push(ScriptCommand::FocusCameraTarget(target.to_string()))
        })
    });
    engine.register_fn("focus_at", |x: Dynamic, y: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let x = dyn_to_f64(&x)?;
        let y = dyn_to_f64(&y)?;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::TweenCameraPos {
                x,
                y,
                duration_s: 0.4,
                ease: "quadOut".to_string(),
            });
        })
    });

    engine.register_fn("cam_bop", |game_delta: Dynamic, hud_delta: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let g = dyn_to_f64(&game_delta)?;
        let h = dyn_to_f64(&hud_delta)?;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::CameraBop {
                game_delta: g,
                hud_delta: h,
            });
        })
    });
    engine.register_fn("set_bop", |intensity: Dynamic, rate: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let i = dyn_to_f64(&intensity)?;
        let r = dyn_to_f64(&rate)?;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetCameraBopConfig {
                intensity: i,
                rate: r,
            });
        })
    });

    engine.register_fn("cam_angle", || with_ctx(|c| f64::from(c.mutable.cam_angle_deg)));
    engine.register_fn("set_cam_angle", |deg: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let val = dyn_to_f64(&deg)? as f32;
        with_ctx(|c| {
            c.mutable.cam_angle_deg = val;
            c.mutable.dirty_cam_angle = true;
        })
    });
    engine.register_fn("cam_turn", |deg: Dynamic, secs: Dynamic, ease: &str| -> Result<(), Box<EvalAltResult>> {
        let target = dyn_to_f64(&deg)?;
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| c.push_tween(TweenTarget::Camera, TweenProp::Angle, target, dur, ease))
    });
    engine.register_fn("cam_turn", |deg: Dynamic, secs: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let target = dyn_to_f64(&deg)?;
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| c.push_tween(TweenTarget::Camera, TweenProp::Angle, target, dur, "quadOut"))
    });

    engine.register_fn("shake", |intensity: Dynamic, secs: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let i = dyn_to_f64(&intensity)? as f32;
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::CameraShake {
                intensity: i,
                duration_s: dur.max(0.0),
                hud: false,
            });
        })
    });
    engine.register_fn("hud_shake", |intensity: Dynamic, secs: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let i = dyn_to_f64(&intensity)? as f32;
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::CameraShake {
                intensity: i,
                duration_s: dur.max(0.0),
                hud: true,
            });
        })
    });
    engine.register_fn("flash", |hex: &str, secs: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::CameraFlash {
                color: parse_hex_rgba(hex),
                duration_s: dur.max(0.0),
                hud: true,
            });
        })
    });
}

fn register_character_and_modchart_api(engine: &mut Engine) {
    engine.register_fn("char_id", |role: &str| {
        with_ctx(|c| match role.trim().to_ascii_lowercase().as_str() {
            "bf" | "boyfriend" | "player" => c.snapshot.bf_id.clone(),
            "dad" | "opponent" => c.snapshot.dad_id.clone(),
            "gf" | "girlfriend" | "spectator" => c.snapshot.gf_id.clone(),
            _ => String::new(),
        })
    });
    engine.register_fn("anim", |target: &str, name: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::PlayAnim {
                target: target.to_string(),
                anim: name.to_string(),
                force: true,
            });
        })
    });
    engine.register_fn("play_anim", |target: &str, name: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::PlayAnim {
                target: target.to_string(),
                anim: name.to_string(),
                force: true,
            });
        })
    });
    engine.register_fn("bf_anim", |name: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::PlayAnim {
                target: "bf".to_string(),
                anim: name.to_string(),
                force: true,
            });
        })
    });
    engine.register_fn("dad_anim", |name: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::PlayAnim {
                target: "dad".to_string(),
                anim: name.to_string(),
                force: true,
            });
        })
    });
    engine.register_fn("gf_anim", |name: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::PlayAnim {
                target: "gf".to_string(),
                anim: name.to_string(),
                force: true,
            });
        })
    });
    engine.register_fn("char_pos", |target: &str| {
        with_ctx(|c| {
            let p = c.snapshot.char_positions.get(target).copied().unwrap_or([0.0, 0.0]);
            vec![Dynamic::from(f64::from(p[0])), Dynamic::from(f64::from(p[1]))]
        })
    });
    engine.register_fn("set_char_pos", |target: &str, x: Dynamic, y: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let x = dyn_to_f64(&x)? as f32;
        let y = dyn_to_f64(&y)? as f32;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetCharacterPos {
                target: target.to_string(),
                x,
                y,
            });
        })
    });
    engine.register_fn("char_scale", |target: &str, sx: Dynamic, sy: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let sx = dyn_to_f64(&sx)? as f32;
        let sy = dyn_to_f64(&sy)? as f32;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetCharacterScale {
                target: target.to_string(),
                sx,
                sy,
            });
        })
    });
    engine.register_fn("char_tint", |target: &str, hex: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetCharacterTint {
                target: target.to_string(),
                color: parse_hex_rgba(hex),
            });
        })
    });
    engine.register_fn("char_visible", |target: &str, visible: bool| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetCharacterVisible {
                target: target.to_string(),
                visible,
            });
        })
    });
    engine.register_fn("set_dance_every", |target: &str, beats: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let beats = dyn_to_f64(&beats)? as f32;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetCharacterDanceEvery {
                target: target.to_string(),
                beats: beats.max(0.0),
            });
        })
    });

    // Modcharting lane helpers (lanes 0..3 opponent, 4..7 player).
    engine.register_fn("lane_x", |lane: Dynamic, dx: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let l = dyn_to_usize(&lane)?;
        let dx = dyn_to_f64(&dx)? as f32;
        with_ctx(|c| {
            let cur_y = c.snapshot.lane_offsets.get(&l).map_or(0.0, |p| p[1]);
            c.snapshot.lane_offsets.insert(l, [dx, cur_y]);
            c.commands.push(ScriptCommand::SetLaneOffset { lane: l, x: dx, y: cur_y });
        })
    });
    engine.register_fn("lane_y", |lane: Dynamic, dy: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let l = dyn_to_usize(&lane)?;
        let dy = dyn_to_f64(&dy)? as f32;
        with_ctx(|c| {
            let cur_x = c.snapshot.lane_offsets.get(&l).map_or(0.0, |p| p[0]);
            c.snapshot.lane_offsets.insert(l, [cur_x, dy]);
            c.commands.push(ScriptCommand::SetLaneOffset { lane: l, x: cur_x, y: dy });
        })
    });
    engine.register_fn("lane_pos", |lane: Dynamic, dx: Dynamic, dy: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let l = dyn_to_usize(&lane)?;
        let dx = dyn_to_f64(&dx)? as f32;
        let dy = dyn_to_f64(&dy)? as f32;
        with_ctx(|c| {
            c.snapshot.lane_offsets.insert(l, [dx, dy]);
            c.commands.push(ScriptCommand::SetLaneOffset { lane: l, x: dx, y: dy });
        })
    });
    engine.register_fn("lane_angle", |lane: Dynamic, deg: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let l = dyn_to_usize(&lane)?;
        let deg = dyn_to_f64(&deg)? as f32;
        with_ctx(|c| {
            c.snapshot.lane_angles.insert(l, deg);
            c.commands.push(ScriptCommand::SetLaneAngle { lane: l, deg });
        })
    });
    engine.register_fn("lane_scale", |lane: Dynamic, sx: Dynamic, sy: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let l = dyn_to_usize(&lane)?;
        let sx = dyn_to_f64(&sx)? as f32;
        let sy = dyn_to_f64(&sy)? as f32;
        with_ctx(|c| {
            c.snapshot.lane_scales.insert(l, [sx, sy]);
            c.commands.push(ScriptCommand::SetLaneScale { lane: l, sx, sy });
        })
    });
    engine.register_fn("lane_alpha", |lane: Dynamic, alpha: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let l = dyn_to_usize(&lane)?;
        let alpha = (dyn_to_f64(&alpha)? as f32).clamp(0.0, 1.0);
        with_ctx(|c| {
            c.snapshot.lane_alphas.insert(l, alpha);
            c.commands.push(ScriptCommand::SetLaneAlpha { lane: l, alpha });
        })
    });
    engine.register_fn("reset_lanes", || {
        with_ctx(|c| {
            c.snapshot.lane_offsets.clear();
            c.snapshot.lane_angles.clear();
            c.snapshot.lane_scales.clear();
            c.snapshot.lane_alphas.clear();
            c.commands.push(ScriptCommand::ResetLanes);
        })
    });
    engine.register_fn("tween_lane", |lane: Dynamic, prop: &str, to: Dynamic, secs: Dynamic, ease: &str| -> Result<(), Box<EvalAltResult>> {
        let l = dyn_to_usize(&lane)?;
        let target_val = dyn_to_f64(&to)?;
        let dur = dyn_to_f64(&secs)?;
        let p = TweenProp::parse(prop)
            .ok_or_else(|| runtime_err(format!("unknown tween property '{prop}'")))?;
        with_ctx(|c| c.push_tween(TweenTarget::Lane(l), p, target_val, dur, ease))
    });
    engine.register_fn("tween_lane", |lane: Dynamic, prop: &str, to: Dynamic, secs: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let l = dyn_to_usize(&lane)?;
        let target_val = dyn_to_f64(&to)?;
        let dur = dyn_to_f64(&secs)?;
        let p = TweenProp::parse(prop)
            .ok_or_else(|| runtime_err(format!("unknown tween property '{prop}'")))?;
        with_ctx(|c| c.push_tween(TweenTarget::Lane(l), p, target_val, dur, "linear"))
    });
}

fn register_props_tweens_timers_and_store_api(engine: &mut Engine) {
    engine.register_fn("spawn_sprite", |id: &str, path: &str, x: Dynamic, y: Dynamic, z: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let x = dyn_to_f64(&x)? as f32;
        let y = dyn_to_f64(&y)? as f32;
        let z = dyn_to_f64(&z)? as f32;
        with_ctx(|c| {
            c.snapshot.prop_positions.insert(id.to_string(), [x, y]);
            c.snapshot.prop_scales.insert(id.to_string(), [1.0, 1.0]);
            c.snapshot.prop_angles.insert(id.to_string(), 0.0);
            c.snapshot.prop_alphas.insert(id.to_string(), 1.0);
            c.commands.push(ScriptCommand::SpawnProp {
                id: id.to_string(),
                image_path: Some(path.to_string()),
                color: [1.0, 1.0, 1.0, 1.0],
                size: [0.0, 0.0],
                pos: [x, y],
                z,
                hud: false,
            });
        })
    });
    engine.register_fn("spawn_hud_sprite", |id: &str, path: &str, x: Dynamic, y: Dynamic, z: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let x = dyn_to_f64(&x)? as f32;
        let y = dyn_to_f64(&y)? as f32;
        let z = dyn_to_f64(&z)? as f32;
        with_ctx(|c| {
            c.snapshot.prop_positions.insert(id.to_string(), [x, y]);
            c.snapshot.prop_scales.insert(id.to_string(), [1.0, 1.0]);
            c.snapshot.prop_angles.insert(id.to_string(), 0.0);
            c.snapshot.prop_alphas.insert(id.to_string(), 1.0);
            c.commands.push(ScriptCommand::SpawnProp {
                id: id.to_string(),
                image_path: Some(path.to_string()),
                color: [1.0, 1.0, 1.0, 1.0],
                size: [0.0, 0.0],
                pos: [x, y],
                z,
                hud: true,
            });
        })
    });
    engine.register_fn("spawn_rect", |id: &str, w: Dynamic, h: Dynamic, hex: &str, x: Dynamic, y: Dynamic, z: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let w = dyn_to_f64(&w)? as f32;
        let h = dyn_to_f64(&h)? as f32;
        let x = dyn_to_f64(&x)? as f32;
        let y = dyn_to_f64(&y)? as f32;
        let z = dyn_to_f64(&z)? as f32;
        with_ctx(|c| {
            c.snapshot.prop_positions.insert(id.to_string(), [x, y]);
            c.snapshot.prop_scales.insert(id.to_string(), [1.0, 1.0]);
            c.snapshot.prop_angles.insert(id.to_string(), 0.0);
            c.snapshot.prop_alphas.insert(id.to_string(), 1.0);
            c.commands.push(ScriptCommand::SpawnProp {
                id: id.to_string(),
                image_path: None,
                color: parse_hex_rgba(hex),
                size: [w, h],
                pos: [x, y],
                z,
                hud: false,
            });
        })
    });
    engine.register_fn("spawn_hud_rect", |id: &str, w: Dynamic, h: Dynamic, hex: &str, x: Dynamic, y: Dynamic, z: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let w = dyn_to_f64(&w)? as f32;
        let h = dyn_to_f64(&h)? as f32;
        let x = dyn_to_f64(&x)? as f32;
        let y = dyn_to_f64(&y)? as f32;
        let z = dyn_to_f64(&z)? as f32;
        with_ctx(|c| {
            c.snapshot.prop_positions.insert(id.to_string(), [x, y]);
            c.snapshot.prop_scales.insert(id.to_string(), [1.0, 1.0]);
            c.snapshot.prop_angles.insert(id.to_string(), 0.0);
            c.snapshot.prop_alphas.insert(id.to_string(), 1.0);
            c.commands.push(ScriptCommand::SpawnProp {
                id: id.to_string(),
                image_path: None,
                color: parse_hex_rgba(hex),
                size: [w, h],
                pos: [x, y],
                z,
                hud: true,
            });
        })
    });
    engine.register_fn("despawn", |id: &str| {
        with_ctx(|c| c.commands.push(ScriptCommand::DespawnProp(id.to_string())))
    });

    engine.register_fn("set_prop", |id: &str, prop: &str, val: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let v = dyn_to_f64(&val)? as f32;
        let p = TweenProp::parse(prop)
            .ok_or_else(|| runtime_err(format!("unknown prop field '{prop}'")))?;
        with_ctx(|c| {
            match p {
                TweenProp::X => {
                    let y = c.snapshot.prop_positions.get(id).map_or(0.0, |pos| pos[1]);
                    c.snapshot.prop_positions.insert(id.to_string(), [v, y]);
                }
                TweenProp::Y => {
                    let x = c.snapshot.prop_positions.get(id).map_or(0.0, |pos| pos[0]);
                    c.snapshot.prop_positions.insert(id.to_string(), [x, v]);
                }
                TweenProp::ScaleX => {
                    let sy = c.snapshot.prop_scales.get(id).map_or(1.0, |s| s[1]);
                    c.snapshot.prop_scales.insert(id.to_string(), [v, sy]);
                }
                TweenProp::ScaleY => {
                    let sx = c.snapshot.prop_scales.get(id).map_or(1.0, |s| s[0]);
                    c.snapshot.prop_scales.insert(id.to_string(), [sx, v]);
                }
                TweenProp::Angle => {
                    c.snapshot.prop_angles.insert(id.to_string(), v);
                }
                TweenProp::Alpha => {
                    c.snapshot.prop_alphas.insert(id.to_string(), v.clamp(0.0, 1.0));
                }
                _ => {}
            }
            c.commands.push(ScriptCommand::SetPropNumber {
                id: id.to_string(),
                prop: p,
                value: v,
            });
        })
    });
    engine.register_fn("prop_pos", |id: &str, x: Dynamic, y: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let x = dyn_to_f64(&x)? as f32;
        let y = dyn_to_f64(&y)? as f32;
        with_ctx(|c| {
            c.snapshot.prop_positions.insert(id.to_string(), [x, y]);
            c.commands.push(ScriptCommand::SetPropNumber { id: id.to_string(), prop: TweenProp::X, value: x });
            c.commands.push(ScriptCommand::SetPropNumber { id: id.to_string(), prop: TweenProp::Y, value: y });
        })
    });
    engine.register_fn("prop_scale", |id: &str, sx: Dynamic, sy: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let sx = dyn_to_f64(&sx)? as f32;
        let sy = dyn_to_f64(&sy)? as f32;
        with_ctx(|c| {
            c.snapshot.prop_scales.insert(id.to_string(), [sx, sy]);
            c.commands.push(ScriptCommand::SetPropNumber { id: id.to_string(), prop: TweenProp::ScaleX, value: sx });
            c.commands.push(ScriptCommand::SetPropNumber { id: id.to_string(), prop: TweenProp::ScaleY, value: sy });
        })
    });
    engine.register_fn("prop_angle", |id: &str, deg: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let deg = dyn_to_f64(&deg)? as f32;
        with_ctx(|c| {
            c.snapshot.prop_angles.insert(id.to_string(), deg);
            c.commands.push(ScriptCommand::SetPropNumber { id: id.to_string(), prop: TweenProp::Angle, value: deg });
        })
    });
    engine.register_fn("prop_alpha", |id: &str, a: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let a = (dyn_to_f64(&a)? as f32).clamp(0.0, 1.0);
        with_ctx(|c| {
            c.snapshot.prop_alphas.insert(id.to_string(), a);
            c.commands.push(ScriptCommand::SetPropNumber { id: id.to_string(), prop: TweenProp::Alpha, value: a });
        })
    });
    engine.register_fn("prop_tint", |id: &str, hex: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetPropTint {
                id: id.to_string(),
                color: parse_hex_rgba(hex),
            });
        })
    });
    engine.register_fn("prop_visible", |id: &str, visible: bool| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetPropVisible {
                id: id.to_string(),
                visible,
            });
        })
    });

    engine.register_fn("tween", |target: &str, prop: &str, to: Dynamic, secs: Dynamic, ease: &str| -> Result<(), Box<EvalAltResult>> {
        let t = TweenTarget::parse(target);
        let p = TweenProp::parse(prop)
            .ok_or_else(|| runtime_err(format!("unknown tween prop '{prop}'")))?;
        let end = dyn_to_f64(&to)?;
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| c.push_tween(t, p, end, dur, ease))
    });
    engine.register_fn("tween", |target: &str, prop: &str, to: Dynamic, secs: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let t = TweenTarget::parse(target);
        let p = TweenProp::parse(prop)
            .ok_or_else(|| runtime_err(format!("unknown tween prop '{prop}'")))?;
        let end = dyn_to_f64(&to)?;
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| c.push_tween(t, p, end, dur, "linear"))
    });
    engine.register_fn("tween_prop", |id: &str, prop: &str, to: Dynamic, secs: Dynamic, ease: &str| -> Result<(), Box<EvalAltResult>> {
        let p = TweenProp::parse(prop)
            .ok_or_else(|| runtime_err(format!("unknown tween prop '{prop}'")))?;
        let end = dyn_to_f64(&to)?;
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| c.push_tween(TweenTarget::Prop(id.to_string()), p, end, dur, ease))
    });
    engine.register_fn("cancel_tweens", |target: &str| {
        let t = TweenTarget::parse(target);
        with_ctx(|c| c.cancel_tween_targets.push(t))
    });

    engine.register_fn("after", |secs: Dynamic, fn_name: &str| -> Result<(), Box<EvalAltResult>> {
        let dur = dyn_to_f64(&secs)?.max(0.0);
        with_ctx(|c| {
            c.new_timers.push(NewTimerRequest {
                script_id: c.current_script_id.clone(),
                fn_name: fn_name.to_string(),
                interval_s: dur,
                repeats_left: Some(1),
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
            });
        })
    });
    engine.register_fn("cancel_timer", |fn_name: &str| {
        with_ctx(|c| {
            c.cancel_timers_for_fn
                .push((c.current_script_id.clone(), fn_name.to_string()));
        })
    });

    // VFS + ModStore helpers
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
    engine.register_fn("save_flush", || -> Result<bool, Box<EvalAltResult>> {
        with_ctx(|c| {
            let ns = c.current_mod_id.clone();
            c.store.save_namespace(&ns).is_ok()
        })
    });
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
}

fn register_math_and_rng_api(engine: &mut Engine) {
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

fn build_configured_engine() -> Arc<Engine> {
    let mut engine = Engine::new();
    engine.set_max_operations(RHAI_MAX_OPERATIONS);
    engine.set_max_call_levels(RHAI_MAX_CALL_LEVELS);
    engine.set_max_expr_depths(128, 64);
    engine.set_max_array_size(RHAI_MAX_ARRAY_SIZE);
    engine.set_max_map_size(RHAI_MAX_MAP_SIZE);
    engine.set_max_string_size(RHAI_MAX_STRING_SIZE);

    register_event_type(&mut engine);
    register_core_and_song_api(&mut engine);
    register_gameplay_api(&mut engine);
    register_camera_api(&mut engine);
    register_character_and_modchart_api(&mut engine);
    register_props_tweens_timers_and_store_api(&mut engine);
    register_math_and_rng_api(&mut engine);

    Arc::new(engine)
}

pub struct RhaiBackend {
    engine: Arc<Engine>,
}

impl Default for RhaiBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl RhaiBackend {
    pub fn new() -> Self {
        Self {
            engine: build_configured_engine(),
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
        let ast = self
            .engine
            .compile(&source.code)
            .map_err(|err| ScriptError::Compile {
                script_id: source.id.clone(),
                message: err.to_string(),
            })?;

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
