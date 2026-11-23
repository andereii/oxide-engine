use crate::play::hud::subtitles::{
    SubtitleConfigPatch, SubtitleHighlightMode, SubtitleTemplate, parse_subtitle_color_str,
};
use crate::scripting::api::{
    LogLevel, NewTimerRequest, ScriptCommand, TweenProp, TweenTarget, parse_hex_rgba,
};
use crate::scripting::value::ScriptValue;

use super::bindings::{ScriptApiEntry, arg, arg_f32, arg_f64, arg_i64, arg_str};
use super::context::ScriptContext;

fn spawn_text_impl(
    ctx: &mut ScriptContext,
    args: &[ScriptValue],
    hud: bool,
) -> Result<ScriptValue, String> {
    let id = arg_str(args, 0)?;
    let text = arg_str(args, 1)?;
    let px = arg_f32(args, 2)?;
    let py = arg_f32(args, 3)?;
    let (pz, sz, color, align) = if args.len() >= 8 {
        (
            arg_f32(args, 4)?,
            arg_f32(args, 5)?.max(1.0),
            parse_hex_rgba(arg_str(args, 6)?),
            arg_str(args, 7)?,
        )
    } else {
        (
            if hud { 15.0 } else { 5.0 },
            arg_f32(args, 4)?.max(1.0),
            parse_hex_rgba(arg_str(args, 5)?),
            "left",
        )
    };
    ctx.snapshot.texts.insert(id.to_string(), text.to_string());
    ctx.snapshot.text_positions.insert(id.to_string(), [px, py]);
    ctx.snapshot.text_alphas.insert(id.to_string(), color[3]);
    ctx.commands.push(ScriptCommand::SpawnText {
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
    Ok(ScriptValue::Null)
}

fn api_world_text(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    spawn_text_impl(ctx, args, false)
}
fn api_hud_text(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    spawn_text_impl(ctx, args, true)
}

fn api_set_text(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let id = arg_str(args, 0)?;
    let text = arg_str(args, 1)?;
    ctx.snapshot.texts.insert(id.to_string(), text.to_string());
    ctx.commands.push(ScriptCommand::SetText {
        id: id.to_string(),
        text: text.to_string(),
    });
    Ok(ScriptValue::Null)
}

fn api_get_text(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::String(
        ctx.snapshot
            .texts
            .get(arg_str(args, 0)?)
            .cloned()
            .unwrap_or_default(),
    ))
}

fn api_text_style(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let id = arg_str(args, 0)?;
    let sz = arg_f32(args, 1)?.max(1.0);
    let color = parse_hex_rgba(arg_str(args, 2)?);
    let align = if args.len() >= 4 {
        Some(arg_str(args, 3)?.to_string())
    } else {
        None
    };
    ctx.commands.push(ScriptCommand::SetTextStyle {
        id: id.to_string(),
        size: Some(sz),
        color: Some(color),
        align,
    });
    Ok(ScriptValue::Null)
}

fn api_text_font(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetTextFont {
        id: arg_str(args, 0)?.to_string(),
        font: arg_str(args, 1)?.to_string(),
    });
    Ok(ScriptValue::Null)
}

fn load_subtitle_file(
    ctx: &mut ScriptContext,
    args: &[ScriptValue],
    fn_name: &str,
    default_ext: Option<&str>,
    default_sync_to_song: bool,
) -> Result<ScriptValue, String> {
    let path = arg_str(args, 0)?;
    let sync_to_song = if args.len() >= 2 {
        match arg(args, 1)? {
            ScriptValue::Bool(b) => *b,
            ScriptValue::String(s) => s.eq_ignore_ascii_case("song"),
            _ => default_sync_to_song,
        }
    } else {
        default_sync_to_song
    };
    let path_hint = match default_ext {
        Some(ext) if !path.contains('.') => Some(format!("{path}.{ext}")),
        _ => Some(path.to_string()),
    };
    match ctx.registry.read_to_string(path) {
        Ok(raw) => ctx.commands.push(ScriptCommand::LoadSubtitlesText {
            raw,
            path_hint,
            sync_to_song,
        }),
        Err(err) => ctx.commands.push(ScriptCommand::Log {
            level: LogLevel::Warn,
            mod_id: ctx.current_mod_id.clone(),
            message: format!("{fn_name}({path}): {err}"),
        }),
    }
    Ok(ScriptValue::Null)
}

fn api_subtitle_srt(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    load_subtitle_file(ctx, args, "subtitle_srt", Some("srt"), false)
}

fn api_subtitle_lrc(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    load_subtitle_file(ctx, args, "subtitle_lrc", Some("lrc"), true)
}

fn api_subtitle_load(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let is_lrc = arg_str(args, 0)?
        .trim()
        .to_ascii_lowercase()
        .ends_with(".lrc");
    load_subtitle_file(ctx, args, "subtitle_load", None, is_lrc)
}

fn api_subtitle_config(
    ctx: &mut ScriptContext,
    args: &[ScriptValue],
) -> Result<ScriptValue, String> {
    let json = arg(args, 0)?.to_json();
    let patch = SubtitleConfigPatch::from_json_value(&json);
    ctx.commands.push(ScriptCommand::ConfigureSubtitles { patch });
    Ok(ScriptValue::Null)
}

fn api_subtitle_style(
    ctx: &mut ScriptContext,
    args: &[ScriptValue],
) -> Result<ScriptValue, String> {
    let template = SubtitleTemplate::from_name(arg_str(args, 0)?);
    let highlight_mode = if args.len() >= 2 {
        SubtitleHighlightMode::from_name(arg_str(args, 1)?)
    } else {
        None
    };
    ctx.commands.push(ScriptCommand::ConfigureSubtitles {
        patch: SubtitleConfigPatch {
            template,
            highlight_mode,
            ..Default::default()
        },
    });
    Ok(ScriptValue::Null)
}

fn api_subtitle_colors(
    ctx: &mut ScriptContext,
    args: &[ScriptValue],
) -> Result<ScriptValue, String> {
    let active_color = parse_subtitle_color_str(arg_str(args, 0)?);
    let inactive_color = parse_subtitle_color_str(arg_str(args, 1)?);
    ctx.commands.push(ScriptCommand::ConfigureSubtitles {
        patch: SubtitleConfigPatch {
            active_color,
            inactive_color,
            ..Default::default()
        },
    });
    Ok(ScriptValue::Null)
}

fn api_subtitle_reset_config(
    ctx: &mut ScriptContext,
    _: &[ScriptValue],
) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::ResetSubtitleConfig);
    Ok(ScriptValue::Null)
}

fn api_subtitle_clear(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::ClearSubtitles);
    Ok(ScriptValue::Null)
}

fn api_tween(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let target = TweenTarget::parse(arg_str(args, 0)?);
    let prop_name = arg_str(args, 1)?;
    let prop =
        TweenProp::parse(prop_name).ok_or_else(|| format!("unknown tween prop '{prop_name}'"))?;
    let end = arg_f64(args, 2)?;
    let dur = arg_f64(args, 3)?;
    let ease = if args.len() >= 5 { arg_str(args, 4)? } else { "linear" };
    let cb = if args.len() >= 6 {
        Some(arg_str(args, 5)?.to_string())
    } else {
        None
    };
    ctx.push_tween_cb(target, prop, end, dur, ease, cb);
    Ok(ScriptValue::Null)
}

fn api_tween_prop(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let id = arg_str(args, 0)?;
    let prop_name = arg_str(args, 1)?;
    let prop =
        TweenProp::parse(prop_name).ok_or_else(|| format!("unknown tween prop '{prop_name}'"))?;
    let end = arg_f64(args, 2)?;
    let dur = arg_f64(args, 3)?;
    let ease = arg_str(args, 4)?;
    let cb = if args.len() >= 6 {
        Some(arg_str(args, 5)?.to_string())
    } else {
        None
    };
    ctx.push_tween_cb(TweenTarget::Prop(id.to_string()), prop, end, dur, ease, cb);
    Ok(ScriptValue::Null)
}

fn api_cancel_tweens(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.cancel_tween_targets
        .push(TweenTarget::parse(arg_str(args, 0)?));
    Ok(ScriptValue::Null)
}

fn api_cancel_tween(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let target = TweenTarget::parse(arg_str(args, 0)?);
    let prop_name = arg_str(args, 1)?;
    let prop =
        TweenProp::parse(prop_name).ok_or_else(|| format!("unknown tween prop '{prop_name}'"))?;
    ctx.cancel_tween_props.push((target, prop));
    Ok(ScriptValue::Null)
}

fn api_after(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let dur = arg_f64(args, 0)?.max(0.0);
    let fn_name = arg_str(args, 1)?.to_string();
    let payload = if args.len() >= 3 {
        vec![arg(args, 2)?.clone()]
    } else {
        Vec::new()
    };
    ctx.new_timers.push(NewTimerRequest {
        script_id: ctx.current_script_id.clone(),
        fn_name,
        interval_s: dur,
        repeats_left: Some(1),
        args: payload,
    });
    Ok(ScriptValue::Null)
}

fn api_every(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let dur = arg_f64(args, 0)?.max(0.001);
    let fn_name = arg_str(args, 1)?.to_string();
    let cnt = arg_i64(args, 2)?;
    let repeats_left = if cnt <= 0 { None } else { Some(cnt as u32) };
    let payload = if args.len() >= 4 {
        vec![arg(args, 3)?.clone()]
    } else {
        Vec::new()
    };
    ctx.new_timers.push(NewTimerRequest {
        script_id: ctx.current_script_id.clone(),
        fn_name,
        interval_s: dur,
        repeats_left,
        args: payload,
    });
    Ok(ScriptValue::Null)
}

fn api_cancel_timer(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.cancel_timers_for_fn
        .push((ctx.current_script_id.clone(), arg_str(args, 0)?.to_string()));
    Ok(ScriptValue::Null)
}

fn api_file_exists(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(ctx.registry.file_exists(arg_str(args, 0)?)))
}

fn api_read_file(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(match ctx.registry.read_to_string(arg_str(args, 0)?) {
        Ok(txt) => ScriptValue::String(txt),
        Err(_) => ScriptValue::Null,
    })
}

fn api_read_json(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let Ok(txt) = ctx.registry.read_to_string(arg_str(args, 0)?) else {
        return Ok(ScriptValue::Null);
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&txt) else {
        return Ok(ScriptValue::Null);
    };
    if args.len() >= 2 {
        let key = arg_str(args, 1)?;
        Ok(json
            .get(key)
            .map(ScriptValue::from_json)
            .unwrap_or(ScriptValue::Null))
    } else {
        Ok(ScriptValue::from_json(&json))
    }
}

fn store_has(ctx: &ScriptContext, ns: &str, key: &str) -> ScriptValue {
    ScriptValue::Bool(ctx.store.has(ns, key))
}
fn store_get(ctx: &ScriptContext, ns: &str, key: &str) -> ScriptValue {
    ctx.store
        .get(ns, key)
        .map(ScriptValue::from_json)
        .unwrap_or(ScriptValue::Null)
}
fn store_set(ctx: &mut ScriptContext, ns: &str, key: &str, val: &ScriptValue) {
    ctx.store.set(ns, key, val.to_json());
}
fn store_del(ctx: &mut ScriptContext, ns: &str, key: &str) -> ScriptValue {
    ScriptValue::Bool(ctx.store.remove(ns, key).is_some())
}

fn api_save_has(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(store_has(ctx, &ctx.current_mod_id, arg_str(args, 0)?))
}
fn api_save_get(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(store_get(ctx, &ctx.current_mod_id, arg_str(args, 0)?))
}
fn api_save_set(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let ns = ctx.current_mod_id.clone();
    store_set(ctx, &ns, arg_str(args, 0)?, arg(args, 1)?);
    Ok(ScriptValue::Null)
}
fn api_save_del(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let ns = ctx.current_mod_id.clone();
    Ok(store_del(ctx, &ns, arg_str(args, 0)?))
}
fn api_save_keys(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::List(
        ctx.store
            .keys(&ctx.current_mod_id)
            .into_iter()
            .map(ScriptValue::String)
            .collect(),
    ))
}
fn api_save_flush(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(
        ctx.store.save_namespace(&ctx.current_mod_id).is_ok(),
    ))
}

fn api_global_has(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(store_has(ctx, "global", arg_str(args, 0)?))
}
fn api_global_get(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(store_get(ctx, "global", arg_str(args, 0)?))
}
fn api_global_set(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    store_set(ctx, "global", arg_str(args, 0)?, arg(args, 1)?);
    Ok(ScriptValue::Null)
}
fn api_global_del(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(store_del(ctx, "global", arg_str(args, 0)?))
}
fn api_global_flush(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(ctx.store.save_namespace("global").is_ok()))
}

pub(crate) fn api_entries() -> Vec<ScriptApiEntry> {
    vec![
        ScriptApiEntry::new(&["text", "spawn_text"], &[6, 8], api_world_text),
        ScriptApiEntry::new(&["hud_text", "spawn_hud_text"], &[6, 8], api_hud_text),
        ScriptApiEntry::new(&["set_text"], &[2], api_set_text),
        ScriptApiEntry::new(&["get_text"], &[1], api_get_text),
        ScriptApiEntry::new(&["text_style"], &[3, 4], api_text_style),
        ScriptApiEntry::new(&["text_font"], &[2], api_text_font),
        ScriptApiEntry::new(&["subtitle_srt", "subtitles_srt"], &[1, 2], api_subtitle_srt),
        ScriptApiEntry::new(&["subtitle_lrc", "subtitles_lrc"], &[1, 2], api_subtitle_lrc),
        ScriptApiEntry::new(&["subtitle_load", "subtitles_load"], &[1, 2], api_subtitle_load),
        ScriptApiEntry::new(
            &["subtitle_config", "subtitles_config"],
            &[1],
            api_subtitle_config,
        ),
        ScriptApiEntry::new(
            &["subtitle_style", "subtitles_style"],
            &[1, 2],
            api_subtitle_style,
        ),
        ScriptApiEntry::new(
            &["subtitle_colors", "subtitles_colors"],
            &[2],
            api_subtitle_colors,
        ),
        ScriptApiEntry::new(
            &["subtitle_reset_config", "subtitles_reset_config"],
            &[0],
            api_subtitle_reset_config,
        ),
        ScriptApiEntry::new(&["subtitle_clear", "subtitles_clear"], &[0], api_subtitle_clear),
        ScriptApiEntry::new(&["tween"], &[4, 5, 6], api_tween),
        ScriptApiEntry::new(&["tween_prop"], &[5, 6], api_tween_prop),
        ScriptApiEntry::new(&["cancel_tweens"], &[1], api_cancel_tweens),
        ScriptApiEntry::new(&["cancel_tween"], &[2], api_cancel_tween),
        ScriptApiEntry::new(&["after"], &[2, 3], api_after),
        ScriptApiEntry::new(&["every"], &[3, 4], api_every),
        ScriptApiEntry::new(&["cancel_timer"], &[1], api_cancel_timer),
        ScriptApiEntry::new(&["file_exists"], &[1], api_file_exists),
        ScriptApiEntry::new(&["read_file"], &[1], api_read_file),
        ScriptApiEntry::new(&["read_json"], &[1, 2], api_read_json),
        ScriptApiEntry::new(&["save_has"], &[1], api_save_has),
        ScriptApiEntry::new(&["save_get"], &[1], api_save_get),
        ScriptApiEntry::new(&["save_set"], &[2], api_save_set),
        ScriptApiEntry::new(&["save_del"], &[1], api_save_del),
        ScriptApiEntry::new(&["save_keys"], &[0], api_save_keys),
        ScriptApiEntry::new(&["save_flush"], &[0], api_save_flush),
        ScriptApiEntry::new(&["global_has"], &[1], api_global_has),
        ScriptApiEntry::new(&["global_get"], &[1], api_global_get),
        ScriptApiEntry::new(&["global_set"], &[2], api_global_set),
        ScriptApiEntry::new(&["global_del"], &[1], api_global_del),
        ScriptApiEntry::new(&["global_flush"], &[0], api_global_flush),
    ]
}
