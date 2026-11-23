use std::collections::BTreeMap;

use crate::scripting::api::{ScriptCommand, TweenProp, TweenTarget};
use crate::scripting::value::ScriptValue;

use super::bindings::{
    ScriptApiEntry, arg_bool, arg_f32, arg_f64, arg_map, arg_str, arg_usize, val_f64, val_usize,
    vec2_f32_list,
};
use super::context::ScriptContext;

fn map_opt_str(fields: &BTreeMap<String, ScriptValue>, keys: &[&str]) -> Option<String> {
    for &key in keys {
        let Some(val) = fields.get(key) else {
            continue;
        };
        if matches!(val, ScriptValue::Null) {
            continue;
        }
        if let Some(s) = val.as_str() {
            return Some(s.to_string());
        }
    }
    None
}

fn map_opt_bool(fields: &BTreeMap<String, ScriptValue>, keys: &[&str]) -> Result<Option<bool>, String> {
    for &key in keys {
        let Some(val) = fields.get(key) else {
            continue;
        };
        if matches!(val, ScriptValue::Null) {
            continue;
        }
        return val
            .as_bool()
            .map(Some)
            .ok_or_else(|| format!("expected bool for '{key}', got {}", val.type_name()));
    }
    Ok(None)
}

fn emit_register_notekind(
    ctx: &mut ScriptContext,
    id: &str,
    fields: &BTreeMap<String, ScriptValue>,
) -> Result<(), String> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err("register_notekind: id cannot be empty".to_string());
    }
    let description = map_opt_str(fields, &["description", "desc"]).unwrap_or_default();
    let note_style_id = map_opt_str(fields, &["note_style", "style", "noteStyle", "note_style_id"])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let no_anim = map_opt_bool(fields, &["no_anim", "noAnim"])?.unwrap_or(false);
    let suffix = map_opt_str(fields, &["suffix", "anim_suffix", "animSuffix"]).unwrap_or_default();
    let scoreable = map_opt_bool(fields, &["scoreable"])?.unwrap_or(true);
    let low_priority = map_opt_bool(fields, &["low_priority", "lowPriority"])?.unwrap_or(false);

    ctx.commands.push(ScriptCommand::RegisterNoteKind {
        id: trimmed.to_string(),
        description,
        note_style_id,
        no_anim,
        suffix,
        scoreable,
        low_priority,
    });
    Ok(())
}

fn api_lane_x(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let l = arg_usize(args, 0)?;
    let dx = arg_f32(args, 1)?;
    let cur_y = ctx.snapshot.lane_offsets.get(&l).map_or(0.0, |p| p[1]);
    ctx.snapshot.lane_offsets.insert(l, [dx, cur_y]);
    ctx.commands.push(ScriptCommand::SetLaneOffset { lane: l, x: dx, y: cur_y });
    Ok(ScriptValue::Null)
}

fn api_lane_y(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let l = arg_usize(args, 0)?;
    let dy = arg_f32(args, 1)?;
    let cur_x = ctx.snapshot.lane_offsets.get(&l).map_or(0.0, |p| p[0]);
    ctx.snapshot.lane_offsets.insert(l, [cur_x, dy]);
    ctx.commands.push(ScriptCommand::SetLaneOffset { lane: l, x: cur_x, y: dy });
    Ok(ScriptValue::Null)
}

fn api_lane_pos(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let l = arg_usize(args, 0)?;
    let dx = arg_f32(args, 1)?;
    let dy = arg_f32(args, 2)?;
    ctx.snapshot.lane_offsets.insert(l, [dx, dy]);
    ctx.commands.push(ScriptCommand::SetLaneOffset { lane: l, x: dx, y: dy });
    Ok(ScriptValue::Null)
}

fn api_lane_angle(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let l = arg_usize(args, 0)?;
    let deg = arg_f32(args, 1)?;
    ctx.snapshot.lane_angles.insert(l, deg);
    ctx.commands.push(ScriptCommand::SetLaneAngle { lane: l, deg });
    Ok(ScriptValue::Null)
}

fn api_lane_scale(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let l = arg_usize(args, 0)?;
    let sx = arg_f32(args, 1)?;
    let sy = arg_f32(args, 2)?;
    ctx.snapshot.lane_scales.insert(l, [sx, sy]);
    ctx.commands.push(ScriptCommand::SetLaneScale { lane: l, sx, sy });
    Ok(ScriptValue::Null)
}

fn api_lane_alpha(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let l = arg_usize(args, 0)?;
    let alpha = arg_f32(args, 1)?.clamp(0.0, 1.0);
    ctx.snapshot.lane_alphas.insert(l, alpha);
    ctx.commands.push(ScriptCommand::SetLaneAlpha { lane: l, alpha });
    Ok(ScriptValue::Null)
}

fn api_get_lane_pos(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let l = arg_usize(args, 0)?;
    let p = ctx.snapshot.lane_offsets.get(&l).copied().unwrap_or([0.0, 0.0]);
    Ok(vec2_f32_list(p))
}

fn api_get_lane_angle(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let l = arg_usize(args, 0)?;
    Ok(ScriptValue::Float(f64::from(
        ctx.snapshot.lane_angles.get(&l).copied().unwrap_or(0.0),
    )))
}

fn api_get_lane_scale(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let l = arg_usize(args, 0)?;
    let s = ctx.snapshot.lane_scales.get(&l).copied().unwrap_or([1.0, 1.0]);
    Ok(vec2_f32_list(s))
}

fn api_get_lane_alpha(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let l = arg_usize(args, 0)?;
    Ok(ScriptValue::Float(f64::from(
        ctx.snapshot.lane_alphas.get(&l).copied().unwrap_or(1.0),
    )))
}

fn api_reset_lanes(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.snapshot.lane_offsets.clear();
    ctx.snapshot.lane_angles.clear();
    ctx.snapshot.lane_scales.clear();
    ctx.snapshot.lane_alphas.clear();
    ctx.commands.push(ScriptCommand::ResetLanes);
    Ok(ScriptValue::Null)
}

fn api_tween_lane(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let l = arg_usize(args, 0)?;
    let prop = arg_str(args, 1)?;
    let target_val = arg_f64(args, 2)?;
    let dur = arg_f64(args, 3)?;
    let ease = if args.len() >= 5 { arg_str(args, 4)? } else { "linear" };
    let p = TweenProp::parse(prop).ok_or_else(|| format!("unknown tween property '{prop}'"))?;
    ctx.push_tween(TweenTarget::Lane(l), p, target_val, dur, ease);
    Ok(ScriptValue::Null)
}

fn api_strum_anim(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::StrumAnim {
        lane: arg_usize(args, 0)?,
        state: arg_str(args, 1)?.to_string(),
    });
    Ok(ScriptValue::Null)
}

fn api_splash(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SpawnSplash {
        lane: arg_usize(args, 0)?,
    });
    Ok(ScriptValue::Null)
}

fn api_splashes_vis(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands
        .push(ScriptCommand::SetNotesplashVisible(arg_bool(args, 0)?));
    Ok(ScriptValue::Null)
}

fn api_strum_style(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetStrumStyle {
        side: arg_str(args, 0)?.to_string(),
        scale: arg_f32(args, 1)?,
        spacing: arg_f32(args, 2)?,
    });
    Ok(ScriptValue::Null)
}

fn api_vwoosh(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::VwooshNotes {
        side: arg_str(args, 0)?.to_string(),
        fade_in: arg_bool(args, 1)?,
    });
    Ok(ScriptValue::Null)
}

fn api_strum_visible(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let side = arg_str(args, 0)?;
    let visible = arg_bool(args, 1)?;
    let alpha = if visible { 1.0_f32 } else { 0.0_f32 };
    let key_count = crate::utils::constants::KEY_COUNT;
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
            ctx.snapshot.lane_alphas.insert(lane, alpha);
        }
    }
    if update_opp {
        for lane in key_count..(key_count * 2) {
            ctx.snapshot.lane_alphas.insert(lane, alpha);
        }
    }
    ctx.commands.push(ScriptCommand::SetStrumVisible {
        side: side.to_string(),
        visible,
    });
    Ok(ScriptValue::Null)
}

fn api_register_notekind(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let id = arg_str(args, 0)?;
    let empty = BTreeMap::new();
    let fields = if args.len() >= 2 {
        arg_map(args, 1)?
    } else {
        &empty
    };
    emit_register_notekind(ctx, id, fields)?;
    Ok(ScriptValue::Null)
}

fn api_notes(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    let mut out = Vec::with_capacity(ctx.snapshot.active_notes.len());
    for n in &ctx.snapshot.active_notes {
        let mut m = BTreeMap::new();
        m.insert("id".to_string(), ScriptValue::Int(n.id as i64));
        m.insert("time".to_string(), ScriptValue::Float(n.time_ms));
        m.insert("dir".to_string(), ScriptValue::Int(n.dir as i64));
        m.insert("len".to_string(), ScriptValue::Float(n.length_ms));
        m.insert("kind".to_string(), ScriptValue::String(n.kind.clone()));
        m.insert("is_player".to_string(), ScriptValue::Bool(n.is_player));
        m.insert("hit".to_string(), ScriptValue::Bool(n.hit));
        m.insert("missed".to_string(), ScriptValue::Bool(n.missed));
        m.insert("holding".to_string(), ScriptValue::Bool(n.holding));
        m.insert("rem_ms".to_string(), ScriptValue::Float(n.rem_ms));
        out.push(ScriptValue::Map(m));
    }
    Ok(ScriptValue::List(out))
}

fn api_kill_note(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::KillNote {
        id: arg_usize(args, 0)?,
    });
    Ok(ScriptValue::Null)
}

fn api_set_note(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let note_id = arg_usize(args, 0)?;
    let fields = arg_map(args, 1)?;
    let time_ms = fields.get("time").map(val_f64).transpose()?;
    let dir = fields.get("dir").map(val_usize).transpose()?;
    let length_ms = fields
        .get("len")
        .or_else(|| fields.get("length"))
        .map(val_f64)
        .transpose()?;
    let kind = fields.get("kind").and_then(|v| v.as_str().map(str::to_string));
    let scoreable = fields.get("scoreable").and_then(ScriptValue::as_bool);
    ctx.commands.push(ScriptCommand::ModifyNote {
        id: note_id,
        time_ms,
        dir,
        length_ms,
        kind,
        scoreable,
    });
    Ok(ScriptValue::Null)
}

pub(crate) fn api_entries() -> Vec<ScriptApiEntry> {
    vec![
        ScriptApiEntry::new(&["lane_x"], &[2], api_lane_x),
        ScriptApiEntry::new(&["lane_y"], &[2], api_lane_y),
        ScriptApiEntry::new(&["lane_pos"], &[3], api_lane_pos),
        ScriptApiEntry::new(&["lane_angle"], &[2], api_lane_angle),
        ScriptApiEntry::new(&["lane_scale"], &[3], api_lane_scale),
        ScriptApiEntry::new(&["lane_alpha"], &[2], api_lane_alpha),
        ScriptApiEntry::new(&["get_lane_pos"], &[1], api_get_lane_pos),
        ScriptApiEntry::new(&["get_lane_angle"], &[1], api_get_lane_angle),
        ScriptApiEntry::new(&["get_lane_scale"], &[1], api_get_lane_scale),
        ScriptApiEntry::new(&["get_lane_alpha"], &[1], api_get_lane_alpha),
        ScriptApiEntry::new(&["reset_lanes"], &[0], api_reset_lanes),
        ScriptApiEntry::new(&["tween_lane"], &[4, 5], api_tween_lane),
        ScriptApiEntry::new(&["strum_anim"], &[2], api_strum_anim),
        ScriptApiEntry::new(&["splash"], &[1], api_splash),
        ScriptApiEntry::new(&["splashes_vis"], &[1], api_splashes_vis),
        ScriptApiEntry::new(&["strum_style"], &[3], api_strum_style),
        ScriptApiEntry::new(&["vwoosh"], &[2], api_vwoosh),
        ScriptApiEntry::new(&["strum_visible"], &[2], api_strum_visible),
        ScriptApiEntry::new(&["register_notekind", "notekind"], &[1, 2], api_register_notekind),
        ScriptApiEntry::new(&["notes"], &[0], api_notes),
        ScriptApiEntry::new(&["kill_note", "remove_note"], &[1], api_kill_note),
        ScriptApiEntry::new(&["set_note"], &[2], api_set_note),
    ]
}
