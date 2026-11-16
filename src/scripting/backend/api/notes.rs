use rhai::{Dynamic, Engine, EvalAltResult, Map};

use crate::scripting::api::{ScriptCommand, TweenProp, TweenTarget};
use crate::scripting::backend::context::{dyn_to_f64, dyn_to_usize, runtime_err, with_ctx};

fn map_opt_str(fields: &Map, keys: &[&str]) -> Option<String> {
    for &key in keys {
        let Some(val) = fields.get(key) else {
            continue;
        };
        if val.is_unit() {
            continue;
        }
        if let Ok(s) = val.clone().into_string() {
            return Some(s);
        }
    }
    None
}

fn map_opt_bool(fields: &Map, keys: &[&str]) -> Result<Option<bool>, Box<EvalAltResult>> {
    for &key in keys {
        let Some(val) = fields.get(key) else {
            continue;
        };
        if val.is_unit() {
            continue;
        }
        return val
            .as_bool()
            .map(Some)
            .map_err(|_| runtime_err(format!("expected bool for '{key}', got {}", val.type_name())));
    }
    Ok(None)
}

fn emit_register_notekind(id: &str, fields: &Map) -> Result<(), Box<EvalAltResult>> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(runtime_err("register_notekind: id cannot be empty"));
    }
    let description = map_opt_str(fields, &["description", "desc"]).unwrap_or_default();
    let note_style_id = map_opt_str(fields, &["note_style", "style", "noteStyle", "note_style_id"])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let no_anim = map_opt_bool(fields, &["no_anim", "noAnim"])?.unwrap_or(false);
    let suffix = map_opt_str(fields, &["suffix", "anim_suffix", "animSuffix"]).unwrap_or_default();
    let scoreable = map_opt_bool(fields, &["scoreable"])?.unwrap_or(true);
    let low_priority = map_opt_bool(fields, &["low_priority", "lowPriority"])?.unwrap_or(false);

    with_ctx(|c| {
        c.commands.push(ScriptCommand::RegisterNoteKind {
            id: trimmed.to_string(),
            description,
            note_style_id,
            no_anim,
            suffix,
            scoreable,
            low_priority,
        });
    })
}

pub fn register_notes_api(engine: &mut Engine) {
    // Modcharting lane helpers (lanes 0..3 player, 4..7 opponent).
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

    engine.register_fn("get_lane_pos", |lane: Dynamic| -> Result<rhai::Array, Box<EvalAltResult>> {
        let l = dyn_to_usize(&lane)?;
        with_ctx(|c| {
            let p = c.snapshot.lane_offsets.get(&l).copied().unwrap_or([0.0, 0.0]);
            vec![Dynamic::from(f64::from(p[0])), Dynamic::from(f64::from(p[1]))]
        })
    });
    engine.register_fn("get_lane_angle", |lane: Dynamic| -> Result<f64, Box<EvalAltResult>> {
        let l = dyn_to_usize(&lane)?;
        with_ctx(|c| f64::from(c.snapshot.lane_angles.get(&l).copied().unwrap_or(0.0)))
    });
    engine.register_fn("get_lane_scale", |lane: Dynamic| -> Result<rhai::Array, Box<EvalAltResult>> {
        let l = dyn_to_usize(&lane)?;
        with_ctx(|c| {
            let s = c.snapshot.lane_scales.get(&l).copied().unwrap_or([1.0, 1.0]);
            vec![Dynamic::from(f64::from(s[0])), Dynamic::from(f64::from(s[1]))]
        })
    });
    engine.register_fn("get_lane_alpha", |lane: Dynamic| -> Result<f64, Box<EvalAltResult>> {
        let l = dyn_to_usize(&lane)?;
        with_ctx(|c| f64::from(c.snapshot.lane_alphas.get(&l).copied().unwrap_or(1.0)))
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

    // Strumline & Note runtime control
    engine.register_fn("strum_anim", |lane: Dynamic, state: &str| -> Result<(), Box<EvalAltResult>> {
        let l = dyn_to_usize(&lane)?;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::StrumAnim {
                lane: l,
                state: state.to_string(),
            });
        })
    });
    engine.register_fn("splash", |lane: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let l = dyn_to_usize(&lane)?;
        with_ctx(|c| c.commands.push(ScriptCommand::SpawnSplash { lane: l }))
    });
    engine.register_fn("splashes_vis", |visible: bool| {
        with_ctx(|c| c.commands.push(ScriptCommand::SetNotesplashVisible(visible)))
    });
    engine.register_fn("strum_style", |side: &str, scale: Dynamic, spacing: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let scale = dyn_to_f64(&scale)? as f32;
        let spacing = dyn_to_f64(&spacing)? as f32;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetStrumStyle {
                side: side.to_string(),
                scale,
                spacing,
            });
        })
    });
    engine.register_fn("vwoosh", |side: &str, fade_in: bool| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::VwooshNotes {
                side: side.to_string(),
                fade_in,
            });
        })
    });
    engine.register_fn("strum_visible", |side: &str, visible: bool| {
        let alpha = if visible { 1.0_f32 } else { 0.0_f32 };
        let key_count = crate::utils::constants::KEY_COUNT;
        with_ctx(|c| {
            let lower = side.trim().to_ascii_lowercase();
            let update_player = matches!(
                lower.as_str(),
                "player" | "ply" | "bf" | "boyfriend" | "p1" | "0" | "both" | "all"
            );
            let update_opp = matches!(
                lower.as_str(),
                "opponent" | "opp" | "dad" | "p2" | "1" | "both" | "all"
            );
            if update_player {
                for lane in 0..key_count {
                    c.snapshot.lane_alphas.insert(lane, alpha);
                }
            }
            if update_opp {
                for lane in key_count..(key_count * 2) {
                    c.snapshot.lane_alphas.insert(lane, alpha);
                }
            }
            c.commands.push(ScriptCommand::SetStrumVisible {
                side: side.to_string(),
                visible,
            });
        })
    });

    for name in ["register_notekind", "notekind"] {
        engine.register_fn(name, |id: &str| -> Result<(), Box<EvalAltResult>> {
            emit_register_notekind(id, &Map::new())
        });
        engine.register_fn(name, |id: &str, fields: Map| -> Result<(), Box<EvalAltResult>> {
            emit_register_notekind(id, &fields)
        });
    }

    engine.register_fn("notes", || {
        with_ctx(|c| {
            let mut out = rhai::Array::with_capacity(c.snapshot.active_notes.len());
            for n in &c.snapshot.active_notes {
                let mut m = Map::new();
                m.insert("id".into(), Dynamic::from(n.id as i64));
                m.insert("time".into(), Dynamic::from(n.time_ms));
                m.insert("dir".into(), Dynamic::from(n.dir as i64));
                m.insert("len".into(), Dynamic::from(n.length_ms));
                m.insert("kind".into(), Dynamic::from(n.kind.clone()));
                m.insert("is_player".into(), Dynamic::from(n.is_player));
                m.insert("hit".into(), Dynamic::from(n.hit));
                m.insert("missed".into(), Dynamic::from(n.missed));
                m.insert("holding".into(), Dynamic::from(n.holding));
                m.insert("rem_ms".into(), Dynamic::from(n.rem_ms));
                out.push(Dynamic::from_map(m));
            }
            out
        })
    });
    for name in ["kill_note", "remove_note"] {
        engine.register_fn(name, |id: Dynamic| -> Result<(), Box<EvalAltResult>> {
            let note_id = dyn_to_usize(&id)?;
            with_ctx(|c| c.commands.push(ScriptCommand::KillNote { id: note_id }))
        });
    }
    engine.register_fn("set_note", |id: Dynamic, fields: Map| -> Result<(), Box<EvalAltResult>> {
        let note_id = dyn_to_usize(&id)?;
        let time_ms = fields.get("time").map(dyn_to_f64).transpose()?;
        let dir = fields.get("dir").map(dyn_to_usize).transpose()?;
        let length_ms = fields
            .get("len")
            .or_else(|| fields.get("length"))
            .map(dyn_to_f64)
            .transpose()?;
        let kind = fields
            .get("kind")
            .and_then(|v| v.clone().into_string().ok());
        let scoreable = fields.get("scoreable").and_then(|v| v.as_bool().ok());
        with_ctx(|c| {
            c.commands.push(ScriptCommand::ModifyNote {
                id: note_id,
                time_ms,
                dir,
                length_ms,
                kind,
                scoreable,
            });
        })
    });
}
