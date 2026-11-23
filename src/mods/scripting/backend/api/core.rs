use crate::scripting::api::{LogLevel, ScriptCommand, script_clamp, script_lerp, script_remap};
use crate::scripting::value::ScriptValue;
use crate::utils::constants::{MS_PER_SEC, SCREEN_HEIGHT, SCREEN_WIDTH};

use super::bindings::{ScriptApiEntry, arg, arg_f64, arg_i64, arg_str};
use super::context::ScriptContext;

fn push_log(ctx: &mut ScriptContext, level: LogLevel, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let msg = arg(args, 0)?.to_display_string();
    ctx.commands.push(ScriptCommand::Log {
        level,
        mod_id: ctx.current_mod_id.clone(),
        message: msg,
    });
    Ok(ScriptValue::Null)
}

fn api_log(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    push_log(ctx, LogLevel::Info, args)
}
fn api_warn(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    push_log(ctx, LogLevel::Warn, args)
}
fn api_err(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    push_log(ctx, LogLevel::Error, args)
}

fn api_song(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::String(ctx.snapshot.song_id.clone()))
}
fn api_song_len(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(ctx.snapshot.song_length_ms))
}
fn api_start_ms(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(ctx.snapshot.start_timestamp_ms))
}
fn api_diff(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::String(ctx.snapshot.difficulty.clone()))
}
fn api_variation(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::String(ctx.snapshot.variation.clone()))
}
fn api_stage(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::String(ctx.snapshot.stage_id.clone()))
}
fn api_mod_id(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::String(ctx.current_mod_id.clone()))
}
fn api_script_id(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::String(ctx.current_script_id.clone()))
}
fn api_time(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(ctx.snapshot.song_position_ms))
}
fn api_bpm(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(ctx.snapshot.bpm))
}
fn api_step(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Int(i64::from(ctx.snapshot.step)))
}
fn api_beat(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Int(i64::from(ctx.snapshot.beat)))
}
fn api_measure(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Int(i64::from(ctx.snapshot.measure)))
}
fn api_step_ms(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(ctx.snapshot.step_length_ms))
}
fn api_beat_ms(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(ctx.snapshot.beat_length_ms))
}
fn api_beat_f(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(ctx.snapshot.beat_f()))
}
fn api_step_f(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(ctx.snapshot.step_f()))
}
fn api_dt(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(ctx.snapshot.dt_s))
}

fn api_emit(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let trimmed = arg_str(args, 0)?.trim();
    if trimmed.is_empty() {
        return Err("custom hook name cannot be empty".to_string());
    }
    let data = args.get(1).cloned().unwrap_or(ScriptValue::Null);
    ctx.emit_custom_hook(trimmed, data);
    Ok(ScriptValue::Null)
}

fn api_mods(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::List(
        ctx.registry
            .active_mod_ids()
            .into_iter()
            .map(|id| ScriptValue::String(id.to_string()))
            .collect(),
    ))
}

fn api_mod_exists(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let id = arg_str(args, 0)?;
    Ok(ScriptValue::Bool(ctx.registry.active_mod_ids().contains(&id)))
}

fn api_mod_cfg(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let key = arg_str(args, 0)?;
    Ok(ctx
        .registry
        .get_mod_config(&ctx.current_mod_id, key)
        .map(ScriptValue::from_json)
        .unwrap_or(ScriptValue::Null))
}

fn api_cfg(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let mod_id = arg_str(args, 0)?;
    let key = arg_str(args, 1)?;
    Ok(ctx
        .registry
        .get_mod_config(mod_id, key)
        .map(ScriptValue::from_json)
        .unwrap_or(ScriptValue::Null))
}

fn api_dir_exists(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(ctx.registry.resolve_dir(arg_str(args, 0)?).is_some()))
}

fn api_ls(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::List(
        ctx.registry
            .list_dir(arg_str(args, 0)?)
            .into_iter()
            .map(ScriptValue::String)
            .collect(),
    ))
}

fn api_read_merged(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(match ctx.registry.read_to_string_merged(arg_str(args, 0)?) {
        Ok(txt) => ScriptValue::String(txt),
        Err(_) => ScriptValue::Null,
    })
}

fn api_from_json(_: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let raw = arg_str(args, 0)?;
    Ok(serde_json::from_str::<serde_json::Value>(raw)
        .map(|v| ScriptValue::from_json(&v))
        .unwrap_or(ScriptValue::Null))
}

fn api_to_json(_: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let json = arg(args, 0)?.to_json();
    Ok(ScriptValue::String(serde_json::to_string(&json).unwrap_or_default()))
}

fn api_screen_w(_: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(f64::from(SCREEN_WIDTH)))
}
fn api_screen_h(_: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(f64::from(SCREEN_HEIGHT)))
}
fn api_screen_size(_: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::List(vec![
        ScriptValue::Float(f64::from(SCREEN_WIDTH)),
        ScriptValue::Float(f64::from(SCREEN_HEIGHT)),
    ]))
}

fn has_ci(items: &[String], needle: &str) -> bool {
    let q = needle.trim();
    !q.is_empty() && items.iter().any(|item| item.eq_ignore_ascii_case(q))
}

fn api_key_down(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(has_ci(&ctx.snapshot.keys_down, arg_str(args, 0)?)))
}
fn api_key_hit(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(has_ci(&ctx.snapshot.keys_hit, arg_str(args, 0)?)))
}
fn api_key_up(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(has_ci(&ctx.snapshot.keys_up, arg_str(args, 0)?)))
}
fn api_ctrl_down(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(has_ci(&ctx.snapshot.ctrls_down, arg_str(args, 0)?)))
}
fn api_ctrl_hit(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(has_ci(&ctx.snapshot.ctrls_hit, arg_str(args, 0)?)))
}
fn api_ctrl_up(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(has_ci(&ctx.snapshot.ctrls_up, arg_str(args, 0)?)))
}

fn api_screenshot(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let name = if args.is_empty() {
        None
    } else {
        let trimmed = arg_str(args, 0)?.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    };
    ctx.commands.push(ScriptCommand::TakeScreenshot(name));
    Ok(ScriptValue::Null)
}

fn api_pi(_: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(std::f64::consts::PI))
}

fn api_lerp(_: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(script_lerp(
        arg_f64(args, 0)?,
        arg_f64(args, 1)?,
        arg_f64(args, 2)?,
    )))
}

fn api_clamp(_: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(script_clamp(
        arg_f64(args, 0)?,
        arg_f64(args, 1)?,
        arg_f64(args, 2)?,
    )))
}

fn api_remap(_: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(script_remap(
        arg_f64(args, 0)?,
        arg_f64(args, 1)?,
        arg_f64(args, 2)?,
        arg_f64(args, 3)?,
        arg_f64(args, 4)?,
    )))
}

fn api_sin_beat(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(
        ctx.snapshot.sin_beat(arg_f64(args, 0)?, arg_f64(args, 1)?),
    ))
}

fn api_cos_beat(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(
        ctx.snapshot.cos_beat(arg_f64(args, 0)?, arg_f64(args, 1)?),
    ))
}

fn api_rand(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(ctx.next_rand_f64()))
}

fn api_rand_range(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let lo = arg_f64(args, 0)?;
    let hi = arg_f64(args, 1)?;
    Ok(ScriptValue::Float(lo + (hi - lo) * ctx.next_rand_f64()))
}

fn api_rand_int(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let lo = arg_i64(args, 0)?;
    let hi = arg_i64(args, 1)?;
    if hi <= lo {
        return Ok(ScriptValue::Int(lo));
    }
    let span = (hi - lo + 1) as u64;
    Ok(ScriptValue::Int(lo + (ctx.next_rand_u64() % span) as i64))
}

fn api_sec_to_ms(_: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(arg_f64(args, 0)? * MS_PER_SEC))
}

pub(crate) fn api_entries() -> Vec<ScriptApiEntry> {
    vec![
        ScriptApiEntry::new(&["log", "trace"], &[1], api_log),
        ScriptApiEntry::new(&["warn"], &[1], api_warn),
        ScriptApiEntry::new(&["err"], &[1], api_err),
        ScriptApiEntry::new(&["song"], &[0], api_song),
        ScriptApiEntry::new(&["song_len"], &[0], api_song_len),
        ScriptApiEntry::new(&["start_ms"], &[0], api_start_ms),
        ScriptApiEntry::new(&["diff"], &[0], api_diff),
        ScriptApiEntry::new(&["variation"], &[0], api_variation),
        ScriptApiEntry::new(&["stage"], &[0], api_stage),
        ScriptApiEntry::new(&["mod_id"], &[0], api_mod_id),
        ScriptApiEntry::new(&["script_id"], &[0], api_script_id),
        ScriptApiEntry::new(&["time"], &[0], api_time),
        ScriptApiEntry::new(&["bpm"], &[0], api_bpm),
        ScriptApiEntry::new(&["step"], &[0], api_step),
        ScriptApiEntry::new(&["beat"], &[0], api_beat),
        ScriptApiEntry::new(&["measure"], &[0], api_measure),
        ScriptApiEntry::new(&["step_ms"], &[0], api_step_ms),
        ScriptApiEntry::new(&["beat_ms"], &[0], api_beat_ms),
        ScriptApiEntry::new(&["beat_f"], &[0], api_beat_f),
        ScriptApiEntry::new(&["step_f"], &[0], api_step_f),
        ScriptApiEntry::new(&["dt"], &[0], api_dt),
        ScriptApiEntry::new(&["emit"], &[1, 2], api_emit),
        ScriptApiEntry::new(&["mods"], &[0], api_mods),
        ScriptApiEntry::new(&["mod_exists"], &[1], api_mod_exists),
        ScriptApiEntry::new(&["mod_cfg"], &[1], api_mod_cfg),
        ScriptApiEntry::new(&["cfg"], &[2], api_cfg),
        ScriptApiEntry::new(&["dir_exists"], &[1], api_dir_exists),
        ScriptApiEntry::new(&["ls"], &[1], api_ls),
        ScriptApiEntry::new(&["read_merged", "read_file_merged"], &[1], api_read_merged),
        ScriptApiEntry::new(&["from_json", "json_parse"], &[1], api_from_json),
        ScriptApiEntry::new(&["to_json", "json_stringify"], &[1], api_to_json),
        ScriptApiEntry::new(&["screen_w"], &[0], api_screen_w),
        ScriptApiEntry::new(&["screen_h"], &[0], api_screen_h),
        ScriptApiEntry::new(&["screen_size"], &[0], api_screen_size),
        ScriptApiEntry::new(&["key_down"], &[1], api_key_down),
        ScriptApiEntry::new(&["key_hit"], &[1], api_key_hit),
        ScriptApiEntry::new(&["key_up"], &[1], api_key_up),
        ScriptApiEntry::new(&["ctrl_down"], &[1], api_ctrl_down),
        ScriptApiEntry::new(&["ctrl_hit"], &[1], api_ctrl_hit),
        ScriptApiEntry::new(&["ctrl_up"], &[1], api_ctrl_up),
        ScriptApiEntry::new(&["screenshot"], &[0, 1], api_screenshot),
        ScriptApiEntry::new(&["pi"], &[0], api_pi),
        ScriptApiEntry::new(&["lerp"], &[3], api_lerp),
        ScriptApiEntry::new(&["clamp"], &[3], api_clamp),
        ScriptApiEntry::new(&["remap"], &[5], api_remap),
        ScriptApiEntry::new(&["sin_beat"], &[2], api_sin_beat),
        ScriptApiEntry::new(&["cos_beat"], &[2], api_cos_beat),
        ScriptApiEntry::new(&["rand"], &[0], api_rand),
        ScriptApiEntry::new(&["rand_range"], &[2], api_rand_range),
        ScriptApiEntry::new(&["rand_int"], &[2], api_rand_int),
        ScriptApiEntry::new(&["sec_to_ms"], &[1], api_sec_to_ms),
    ]
}
