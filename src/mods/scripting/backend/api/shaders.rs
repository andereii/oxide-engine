use crate::scripting::api::{ScriptCommand, parse_hex_rgba};
use crate::scripting::value::ScriptValue;

use super::bindings::{ScriptApiEntry, arg_bool, arg_f32, arg_f64, arg_str, arg_str_or_i64};
use super::context::ScriptContext;

fn parse_rgb_hex3(hex: &str) -> [f32; 3] {
    let [r, g, b, _] = parse_hex_rgba(hex);
    [r, g, b]
}

fn api_adjust_color(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetShaderAdjustColor {
        target: arg_str(args, 0)?.to_string(),
        hue: arg_f32(args, 1)?,
        saturation: arg_f32(args, 2)?,
        brightness: arg_f32(args, 3)?,
        contrast: arg_f32(args, 4)?,
    });
    Ok(ScriptValue::Null)
}

fn api_drop_shadow(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let target = arg_str(args, 0)?.to_string();
    let color = parse_rgb_hex3(arg_str(args, 1)?);
    let angle_deg = arg_f32(args, 2)?;
    let distance = arg_f32(args, 3)?;
    let strength = arg_f32(args, 4)?;
    let threshold = arg_f32(args, 5)?;

    let (hue, saturation, brightness, contrast) = if args.len() >= 10 {
        (
            arg_f32(args, 6)?,
            arg_f32(args, 7)?,
            arg_f32(args, 8)?,
            arg_f32(args, 9)?,
        )
    } else {
        (0.0, 0.0, 0.0, 0.0)
    };

    let (alt_mask_path, mask_threshold) = if args.len() >= 12 {
        let mask_raw = arg_str(args, 10)?;
        let mask = (!mask_raw.trim().is_empty()).then(|| mask_raw.to_string());
        (mask, arg_f32(args, 11)?)
    } else {
        (None, 0.5)
    };

    ctx.commands.push(ScriptCommand::SetShaderDropShadow {
        target,
        color,
        angle_deg,
        distance,
        strength,
        threshold,
        hue,
        saturation,
        brightness,
        contrast,
        use_alt_mask: alt_mask_path.is_some(),
        alt_mask_path,
        mask_threshold,
    });
    Ok(ScriptValue::Null)
}

fn api_wiggle(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetShaderWiggle {
        target: arg_str(args, 0)?.to_string(),
        effect: arg_str_or_i64(args, 1)?,
        speed: arg_f32(args, 2)?,
        frequency: arg_f32(args, 3)?,
        amplitude: arg_f32(args, 4)?,
    });
    Ok(ScriptValue::Null)
}

fn api_rain(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let scale = arg_f32(args, 0)?;
    let start_intensity = arg_f32(args, 1)?;
    let (end_intensity, color, time_scale_decay) = match args.len() {
        3 => (start_intensity, parse_rgb_hex3(arg_str(args, 2)?), false),
        4 => (
            start_intensity,
            parse_rgb_hex3(arg_str(args, 2)?),
            arg_bool(args, 3)?,
        ),
        _ => (
            arg_f32(args, 2)?,
            parse_rgb_hex3(arg_str(args, 3)?),
            arg_bool(args, 4)?,
        ),
    };
    ctx.commands.push(ScriptCommand::SetCameraRain {
        scale,
        start_intensity,
        end_intensity,
        color,
        time_scale_decay,
    });
    Ok(ScriptValue::Null)
}

fn api_overlay_blend(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetShaderOverlayBlend {
        target: arg_str(args, 0)?.to_string(),
        aux_path: arg_str(args, 1)?.to_string(),
    });
    Ok(ScriptValue::Null)
}

fn api_rain_sprite(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetShaderRainSprite {
        target: arg_str(args, 0)?.to_string(),
        scale: arg_f32(args, 1)?,
        intensity: arg_f32(args, 2)?,
        color: parse_rgb_hex3(arg_str(args, 3)?),
    });
    Ok(ScriptValue::Null)
}

fn api_building(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let alpha_shit = if args.len() >= 2 {
        arg_f32(args, 1)?
    } else {
        1.0
    };
    ctx.commands.push(ScriptCommand::SetShaderBuilding {
        target: arg_str(args, 0)?.to_string(),
        alpha_shit,
    });
    Ok(ScriptValue::Null)
}

fn api_blend(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetBlendMode {
        target: arg_str(args, 0)?.to_string(),
        mode: arg_str(args, 1)?.to_string(),
    });
    Ok(ScriptValue::Null)
}

fn api_shader(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetCustomShader {
        target: arg_str(args, 0)?.to_string(),
        path: arg_str(args, 1)?.to_string(),
    });
    Ok(ScriptValue::Null)
}

fn api_shader_val(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let v = arg_f64(args, 2)? as f32;
    ctx.commands.push(ScriptCommand::SetShaderParam {
        target: arg_str(args, 0)?.to_string(),
        key: arg_str(args, 1)?.to_string(),
        values: [v, 0.0, 0.0, 0.0],
    });
    Ok(ScriptValue::Null)
}

fn api_shader_vec(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetShaderParam {
        target: arg_str(args, 0)?.to_string(),
        key: arg_str(args, 1)?.to_string(),
        values: [
            arg_f32(args, 2)?,
            arg_f32(args, 3)?,
            arg_f32(args, 4)?,
            arg_f32(args, 5)?,
        ],
    });
    Ok(ScriptValue::Null)
}

fn api_shader_tex(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetShaderTex {
        target: arg_str(args, 0)?.to_string(),
        path: arg_str(args, 1)?.to_string(),
    });
    Ok(ScriptValue::Null)
}

fn api_clear_shader(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::ClearShader {
        target: arg_str(args, 0)?.to_string(),
    });
    Ok(ScriptValue::Null)
}

pub(crate) fn api_entries() -> Vec<ScriptApiEntry> {
    vec![
        ScriptApiEntry::new(&["set_adjust_color", "adjust_color"], &[5], api_adjust_color),
        ScriptApiEntry::new(&["set_drop_shadow", "drop_shadow"], &[6, 10, 12], api_drop_shadow),
        ScriptApiEntry::new(&["set_wiggle", "wiggle"], &[5], api_wiggle),
        ScriptApiEntry::new(&["set_rain", "rain"], &[3, 4, 5], api_rain),
        ScriptApiEntry::new(&["set_overlay_blend", "overlay_blend"], &[2], api_overlay_blend),
        ScriptApiEntry::new(&["set_rain_sprite", "rain_sprite"], &[4], api_rain_sprite),
        ScriptApiEntry::new(&["set_building", "building"], &[1, 2], api_building),
        ScriptApiEntry::new(&["blend", "set_blend"], &[2], api_blend),
        ScriptApiEntry::new(&["shader", "set_shader"], &[2], api_shader),
        ScriptApiEntry::new(&["shader_val"], &[3], api_shader_val),
        ScriptApiEntry::new(&["shader_vec"], &[6], api_shader_vec),
        ScriptApiEntry::new(&["shader_tex"], &[2], api_shader_tex),
        ScriptApiEntry::new(&["clear_shader"], &[1], api_clear_shader),
    ]
}
