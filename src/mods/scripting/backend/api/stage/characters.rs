use crate::scripting::api::{ScriptCommand, parse_hex_rgba};
use crate::scripting::value::ScriptValue;

use super::super::bindings::{
    ScriptApiEntry, arg_bool, arg_f32, arg_f64, arg_str, arg_usize, vec2_bool_list, vec2_f32_list,
    vec2_f64_list,
};
use super::super::context::ScriptContext;

fn api_char_id(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let id = match arg_str(args, 0)?.trim().to_ascii_lowercase().as_str() {
        "bf" | "boyfriend" | "player" => ctx.snapshot.bf_id.clone(),
        "dad" | "opponent" => ctx.snapshot.dad_id.clone(),
        "gf" | "girlfriend" | "spectator" => ctx.snapshot.gf_id.clone(),
        _ => String::new(),
    };
    Ok(ScriptValue::String(id))
}

fn api_char_exists(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(
        ctx.snapshot.char_positions.contains_key(arg_str(args, 0)?),
    ))
}

fn push_play_anim(ctx: &mut ScriptContext, target: &str, name: &str) {
    ctx.commands.push(ScriptCommand::PlayAnim {
        target: target.to_string(),
        anim: name.to_string(),
        force: true,
    });
}

fn api_play_anim(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    push_play_anim(ctx, arg_str(args, 0)?, arg_str(args, 1)?);
    Ok(ScriptValue::Null)
}

fn api_play_anim_frame(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::PlayAnimFrame {
        target: arg_str(args, 0)?.to_string(),
        anim: arg_str(args, 1)?.to_string(),
        force: true,
        frame: arg_usize(args, 2)?,
    });
    Ok(ScriptValue::Null)
}

fn api_play_sing(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let (miss, suffix) = if args.len() >= 4 {
        (arg_bool(args, 2)?, arg_str(args, 3)?.to_string())
    } else {
        (false, String::new())
    };
    ctx.commands.push(ScriptCommand::PlaySing {
        target: arg_str(args, 0)?.to_string(),
        dir: arg_usize(args, 1)?,
        miss,
        suffix,
    });
    Ok(ScriptValue::Null)
}

fn api_dance(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let force = if args.len() >= 2 { arg_bool(args, 1)? } else { false };
    ctx.commands.push(ScriptCommand::CharDance {
        target: arg_str(args, 0)?.to_string(),
        force,
    });
    Ok(ScriptValue::Null)
}

fn api_char_anim(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::String(
        ctx.snapshot
            .char_anims
            .get(arg_str(args, 0)?)
            .cloned()
            .unwrap_or_default(),
    ))
}

fn api_char_frame(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let frame = ctx
        .snapshot
        .char_frames
        .get(arg_str(args, 0)?)
        .copied()
        .unwrap_or(0) as u32;
    Ok(ScriptValue::Int(i64::from(frame)))
}

fn api_char_anim_finished(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(
        ctx.snapshot
            .char_anim_finished
            .get(arg_str(args, 0)?)
            .copied()
            .unwrap_or(false),
    ))
}

fn api_bf_anim(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    push_play_anim(ctx, "bf", arg_str(args, 0)?);
    Ok(ScriptValue::Null)
}
fn api_dad_anim(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    push_play_anim(ctx, "dad", arg_str(args, 0)?);
    Ok(ScriptValue::Null)
}
fn api_gf_anim(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    push_play_anim(ctx, "gf", arg_str(args, 0)?);
    Ok(ScriptValue::Null)
}

fn api_char_pos(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let p = ctx
        .snapshot
        .char_positions
        .get(arg_str(args, 0)?)
        .copied()
        .unwrap_or([0.0, 0.0]);
    Ok(vec2_f32_list(p))
}
fn api_char_top_left(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let p = ctx
        .snapshot
        .char_top_left
        .get(arg_str(args, 0)?)
        .copied()
        .unwrap_or([0.0, 0.0]);
    Ok(vec2_f32_list(p))
}
fn api_char_focus(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let p = ctx
        .snapshot
        .char_focus
        .get(arg_str(args, 0)?)
        .copied()
        .unwrap_or([0.0, 0.0]);
    Ok(vec2_f64_list(p))
}
fn api_char_size(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let p = ctx
        .snapshot
        .char_sizes
        .get(arg_str(args, 0)?)
        .copied()
        .unwrap_or([0.0, 0.0]);
    Ok(vec2_f32_list(p))
}
fn api_char_z(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(f64::from(
        ctx.snapshot
            .char_z
            .get(arg_str(args, 0)?)
            .copied()
            .unwrap_or(-500.0),
    )))
}
fn api_set_char_z(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let target = arg_str(args, 0)?;
    let z = arg_f32(args, 1)?;
    ctx.snapshot.char_z.insert(target.to_string(), z);
    ctx.commands.push(ScriptCommand::SetCharZ {
        target: target.to_string(),
        z,
    });
    Ok(ScriptValue::Null)
}
fn api_set_char_pos(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let target = arg_str(args, 0)?;
    let x = arg_f32(args, 1)?;
    let y = arg_f32(args, 2)?;
    ctx.snapshot.char_positions.insert(target.to_string(), [x, y]);
    ctx.commands.push(ScriptCommand::SetCharacterPos {
        target: target.to_string(),
        x,
        y,
    });
    Ok(ScriptValue::Null)
}
fn api_get_char_scale(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let s = ctx
        .snapshot
        .char_scales
        .get(arg_str(args, 0)?)
        .copied()
        .unwrap_or([1.0, 1.0]);
    Ok(vec2_f32_list(s))
}
fn api_char_scale(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let target = arg_str(args, 0)?;
    let sx = arg_f32(args, 1)?;
    let sy = arg_f32(args, 2)?;
    ctx.snapshot.char_scales.insert(target.to_string(), [sx, sy]);
    ctx.commands.push(ScriptCommand::SetCharacterScale {
        target: target.to_string(),
        sx,
        sy,
    });
    Ok(ScriptValue::Null)
}
fn api_char_flip(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let target = arg_str(args, 0)?;
    let flip_x = arg_bool(args, 1)?;
    let flip_y = arg_bool(args, 2)?;
    ctx.snapshot
        .char_flips
        .insert(target.to_string(), [flip_x, flip_y]);
    ctx.commands.push(ScriptCommand::SetCharFlip {
        target: target.to_string(),
        flip_x,
        flip_y,
    });
    Ok(ScriptValue::Null)
}
fn api_get_char_flip(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let f = ctx
        .snapshot
        .char_flips
        .get(arg_str(args, 0)?)
        .copied()
        .unwrap_or([false, false]);
    Ok(vec2_bool_list(f))
}
fn api_char_tint(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetCharacterTint {
        target: arg_str(args, 0)?.to_string(),
        color: parse_hex_rgba(arg_str(args, 1)?),
    });
    Ok(ScriptValue::Null)
}
fn api_get_char_alpha(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(f64::from(
        ctx.snapshot
            .char_alphas
            .get(arg_str(args, 0)?)
            .copied()
            .unwrap_or(1.0),
    )))
}
fn api_char_alpha(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let target = arg_str(args, 0)?;
    let alpha = arg_f32(args, 1)?.clamp(0.0, 1.0);
    ctx.snapshot.char_alphas.insert(target.to_string(), alpha);
    ctx.commands.push(ScriptCommand::SetCharacterAlpha {
        target: target.to_string(),
        alpha,
    });
    Ok(ScriptValue::Null)
}
fn api_char_visible(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let target = arg_str(args, 0)?;
    let visible = arg_bool(args, 1)?;
    ctx.snapshot.char_vis.insert(target.to_string(), visible);
    ctx.commands.push(ScriptCommand::SetCharacterVisible {
        target: target.to_string(),
        visible,
    });
    Ok(ScriptValue::Null)
}
fn api_is_char_vis(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(
        ctx.snapshot
            .char_vis
            .get(arg_str(args, 0)?)
            .copied()
            .unwrap_or(true),
    ))
}
fn api_set_dance_every(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetCharacterDanceEvery {
        target: arg_str(args, 0)?.to_string(),
        beats: arg_f32(args, 1)?.max(0.0),
    });
    Ok(ScriptValue::Null)
}
fn api_swap_char(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SwapChar {
        role: arg_str(args, 0)?.to_string(),
        char_id: arg_str(args, 1)?.to_string(),
    });
    Ok(ScriptValue::Null)
}

fn api_char_trail(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let z_layer = if args.len() >= 6 {
        Some(arg_f64(args, 5)?.round() as i32)
    } else {
        None
    };
    ctx.commands.push(ScriptCommand::SetCharTrail {
        target: arg_str(args, 0)?.to_string(),
        length: arg_usize(args, 1)?.clamp(1, 32),
        delay_s: arg_f64(args, 2)?.max(0.0),
        alpha: arg_f32(args, 3)?.clamp(0.0, 1.0),
        diff: arg_f32(args, 4)?.clamp(0.0, 1.0),
        z_layer,
    });
    Ok(ScriptValue::Null)
}

fn api_clear_char_trail(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::ClearCharTrail {
        target: arg_str(args, 0)?.to_string(),
    });
    Ok(ScriptValue::Null)
}

fn api_char_underlay(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let offset = if args.len() >= 4 {
        [arg_f32(args, 2)?, arg_f32(args, 3)?]
    } else {
        [0.0, 0.0]
    };
    let z_offset = if args.len() >= 5 {
        arg_f64(args, 4)?.round() as i32
    } else {
        -1
    };
    ctx.commands.push(ScriptCommand::SetCharUnderlay {
        target: arg_str(args, 0)?.to_string(),
        char_id: arg_str(args, 1)?.to_string(),
        offset,
        z_offset,
    });
    Ok(ScriptValue::Null)
}

fn api_clear_char_underlay(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::ClearCharUnderlay {
        target: arg_str(args, 0)?.to_string(),
    });
    Ok(ScriptValue::Null)
}

fn api_char_spawn(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let id = arg_str(args, 0)?;
    let char_id = arg_str(args, 1)?;
    let px = arg_f32(args, 2)?;
    let py = arg_f32(args, 3)?;
    let zl = arg_f64(args, 4)?.round() as i32;
    let scroll = if args.len() >= 7 {
        [arg_f32(args, 5)?, arg_f32(args, 6)?]
    } else {
        [1.0, 1.0]
    };
    ctx.snapshot.char_positions.insert(id.to_string(), [px, py]);
    ctx.snapshot.char_vis.insert(id.to_string(), true);
    ctx.commands.push(ScriptCommand::SpawnChar {
        id: id.to_string(),
        char_id: char_id.to_string(),
        pos: [px, py],
        z_layer: zl,
        scroll,
        visible: true,
    });
    Ok(ScriptValue::Null)
}

fn api_char_sing_type(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetCharSingType {
        target: arg_str(args, 0)?.to_string(),
        role: arg_str(args, 1)?.to_string(),
    });
    Ok(ScriptValue::Null)
}

fn api_char_suffix(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetCharAnimSuffix {
        target: arg_str(args, 0)?.to_string(),
        suffix: arg_str(args, 1)?.to_string(),
    });
    Ok(ScriptValue::Null)
}

fn api_char_attach_atlas(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::AttachCharAtlas {
        target: arg_str(args, 0)?.to_string(),
        attach_id: arg_str(args, 1)?.to_string(),
        symbol_keyword: arg_str(args, 2)?.to_string(),
        atlas_path: arg_str(args, 3)?.to_string(),
    });
    Ok(ScriptValue::Null)
}

fn api_char_attach_frame(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetCharAttachFrame {
        target: arg_str(args, 0)?.to_string(),
        attach_id: arg_str(args, 1)?.to_string(),
        frame: arg_usize(args, 2)?,
    });
    Ok(ScriptValue::Null)
}

fn api_char_attach_pose(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetCharAttachPose {
        target: arg_str(args, 0)?.to_string(),
        attach_id: arg_str(args, 1)?.to_string(),
        offset: [arg_f32(args, 2)?, arg_f32(args, 3)?],
        angle_deg: arg_f32(args, 4)?,
    });
    Ok(ScriptValue::Null)
}

fn api_char_attach_style(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetCharAttachStyle {
        target: arg_str(args, 0)?.to_string(),
        attach_id: arg_str(args, 1)?.to_string(),
        flip_x: arg_bool(args, 2)?,
        alpha: arg_f32(args, 3)?.clamp(0.0, 1.0),
    });
    Ok(ScriptValue::Null)
}

fn api_char_attach_vis(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetCharAttachVis {
        target: arg_str(args, 0)?.to_string(),
        attach_id: arg_str(args, 1)?.to_string(),
        visible: arg_bool(args, 2)?,
    });
    Ok(ScriptValue::Null)
}

pub(crate) fn api_entries() -> Vec<ScriptApiEntry> {
    vec![
        ScriptApiEntry::new(&["char_id"], &[1], api_char_id),
        ScriptApiEntry::new(&["char_exists"], &[1], api_char_exists),
        ScriptApiEntry::new(&["anim", "play_anim"], &[2], api_play_anim),
        ScriptApiEntry::new(&["play_anim_frame"], &[3], api_play_anim_frame),
        ScriptApiEntry::new(&["play_sing"], &[2, 4], api_play_sing),
        ScriptApiEntry::new(&["dance"], &[1, 2], api_dance),
        ScriptApiEntry::new(&["char_anim"], &[1], api_char_anim),
        ScriptApiEntry::new(&["char_frame"], &[1], api_char_frame),
        ScriptApiEntry::new(&["char_anim_finished", "char_anim_done"], &[1], api_char_anim_finished),
        ScriptApiEntry::new(&["bf_anim"], &[1], api_bf_anim),
        ScriptApiEntry::new(&["dad_anim"], &[1], api_dad_anim),
        ScriptApiEntry::new(&["gf_anim"], &[1], api_gf_anim),
        ScriptApiEntry::new(&["char_pos"], &[1], api_char_pos),
        ScriptApiEntry::new(&["char_top_left"], &[1], api_char_top_left),
        ScriptApiEntry::new(&["char_focus"], &[1], api_char_focus),
        ScriptApiEntry::new(&["char_size"], &[1], api_char_size),
        ScriptApiEntry::new(&["char_z"], &[1], api_char_z),
        ScriptApiEntry::new(&["set_char_z"], &[2], api_set_char_z),
        ScriptApiEntry::new(&["set_char_pos"], &[3], api_set_char_pos),
        ScriptApiEntry::new(&["get_char_scale"], &[1], api_get_char_scale),
        ScriptApiEntry::new(&["char_scale"], &[3], api_char_scale),
        ScriptApiEntry::new(&["char_flip"], &[3], api_char_flip),
        ScriptApiEntry::new(&["get_char_flip"], &[1], api_get_char_flip),
        ScriptApiEntry::new(&["char_tint"], &[2], api_char_tint),
        ScriptApiEntry::new(&["get_char_alpha"], &[1], api_get_char_alpha),
        ScriptApiEntry::new(&["char_alpha"], &[2], api_char_alpha),
        ScriptApiEntry::new(&["char_visible", "char_vis"], &[2], api_char_visible),
        ScriptApiEntry::new(&["is_char_vis"], &[1], api_is_char_vis),
        ScriptApiEntry::new(&["set_dance_every"], &[2], api_set_dance_every),
        ScriptApiEntry::new(&["swap_char"], &[2], api_swap_char),
        ScriptApiEntry::new(&["char_trail"], &[5, 6], api_char_trail),
        ScriptApiEntry::new(&["clear_char_trail"], &[1], api_clear_char_trail),
        ScriptApiEntry::new(&["char_underlay"], &[2, 4, 5], api_char_underlay),
        ScriptApiEntry::new(&["clear_char_underlay"], &[1], api_clear_char_underlay),
        ScriptApiEntry::new(&["char_spawn"], &[5, 7], api_char_spawn),
        ScriptApiEntry::new(&["char_sing_type", "char_role"], &[2], api_char_sing_type),
        ScriptApiEntry::new(&["char_suffix"], &[2], api_char_suffix),
        ScriptApiEntry::new(&["char_attach_atlas"], &[4], api_char_attach_atlas),
        ScriptApiEntry::new(&["char_attach_frame"], &[3], api_char_attach_frame),
        ScriptApiEntry::new(&["char_attach_pose"], &[5], api_char_attach_pose),
        ScriptApiEntry::new(&["char_attach_style"], &[4], api_char_attach_style),
        ScriptApiEntry::new(&["char_attach_vis"], &[3], api_char_attach_vis),
    ]
}
