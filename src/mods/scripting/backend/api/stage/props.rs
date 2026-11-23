use std::collections::BTreeMap;

use crate::animations::BaseAnimationData;
use crate::scripting::api::{
    ScriptCommand, ScriptPropAnimKind, TweenProp, parse_hex_rgba,
};
use crate::scripting::value::ScriptValue;

use super::super::bindings::{
    ScriptApiEntry, arg_bool, arg_f32, arg_list, arg_map, arg_str, val_f64, val_usize,
    vec2_bool_list, vec2_f32_list,
};
use super::super::context::ScriptContext;

fn record_prop_defaults(
    ctx: &mut ScriptContext,
    id: &str,
    pos: [f32; 2],
    alpha: f32,
    z: f32,
    hud: bool,
) {
    ctx.snapshot.prop_positions.insert(id.to_string(), pos);
    ctx.snapshot.prop_scales.insert(id.to_string(), [1.0, 1.0]);
    ctx.snapshot.prop_angles.insert(id.to_string(), 0.0);
    ctx.snapshot.prop_alphas.insert(id.to_string(), alpha);
    ctx.snapshot.prop_z.insert(id.to_string(), z);
    ctx.snapshot
        .prop_scrolls
        .insert(id.to_string(), if hud { [0.0, 0.0] } else { [1.0, 1.0] });
    ctx.snapshot.prop_flips.insert(id.to_string(), [false, false]);
    ctx.snapshot.prop_vis.insert(id.to_string(), true);
}

fn push_spawn_prop(
    ctx: &mut ScriptContext,
    id: &str,
    image_path: Option<String>,
    color: [f32; 4],
    size: [f32; 2],
    pos: [f32; 2],
    z: f32,
    hud: bool,
) {
    record_prop_defaults(ctx, id, pos, color[3], z, hud);
    ctx.commands.push(ScriptCommand::SpawnProp {
        id: id.to_string(),
        image_path,
        color,
        size,
        pos,
        z,
        hud,
    });
}

fn push_spawn_anim_prop(
    ctx: &mut ScriptContext,
    id: &str,
    path: String,
    kind: ScriptPropAnimKind,
    anims: Vec<BaseAnimationData>,
    color: [f32; 4],
    size: [f32; 2],
    pos: [f32; 2],
    z: f32,
    hud: bool,
    top_left: bool,
) {
    record_prop_defaults(ctx, id, pos, color[3], z, hud);
    ctx.commands.push(ScriptCommand::SpawnAnimatedProp {
        id: id.to_string(),
        path,
        kind,
        anims,
        color,
        size,
        pos,
        z,
        hud,
        top_left,
    });
}

fn api_spawn_sprite(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    push_spawn_prop(
        ctx,
        arg_str(args, 0)?,
        Some(arg_str(args, 1)?.to_string()),
        [1.0; 4],
        [0.0, 0.0],
        [arg_f32(args, 2)?, arg_f32(args, 3)?],
        arg_f32(args, 4)?,
        false,
    );
    Ok(ScriptValue::Null)
}

fn api_spawn_hud_sprite(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    push_spawn_prop(
        ctx,
        arg_str(args, 0)?,
        Some(arg_str(args, 1)?.to_string()),
        [1.0; 4],
        [0.0, 0.0],
        [arg_f32(args, 2)?, arg_f32(args, 3)?],
        arg_f32(args, 4)?,
        true,
    );
    Ok(ScriptValue::Null)
}

fn api_spawn_rect(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    push_spawn_prop(
        ctx,
        arg_str(args, 0)?,
        None,
        parse_hex_rgba(arg_str(args, 3)?),
        [arg_f32(args, 1)?, arg_f32(args, 2)?],
        [arg_f32(args, 4)?, arg_f32(args, 5)?],
        arg_f32(args, 6)?,
        false,
    );
    Ok(ScriptValue::Null)
}

fn api_spawn_hud_rect(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    push_spawn_prop(
        ctx,
        arg_str(args, 0)?,
        None,
        parse_hex_rgba(arg_str(args, 3)?),
        [arg_f32(args, 1)?, arg_f32(args, 2)?],
        [arg_f32(args, 4)?, arg_f32(args, 5)?],
        arg_f32(args, 6)?,
        true,
    );
    Ok(ScriptValue::Null)
}

fn api_spawn_sprite_tl(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    push_spawn_anim_prop(
        ctx,
        arg_str(args, 0)?,
        arg_str(args, 1)?.to_string(),
        ScriptPropAnimKind::Static,
        Vec::new(),
        [1.0; 4],
        [0.0, 0.0],
        [arg_f32(args, 2)?, arg_f32(args, 3)?],
        arg_f32(args, 4)?,
        false,
        true,
    );
    Ok(ScriptValue::Null)
}

fn api_spawn_rect_tl(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    push_spawn_anim_prop(
        ctx,
        arg_str(args, 0)?,
        String::new(),
        ScriptPropAnimKind::Rect,
        Vec::new(),
        parse_hex_rgba(arg_str(args, 3)?),
        [arg_f32(args, 1)?, arg_f32(args, 2)?],
        [arg_f32(args, 4)?, arg_f32(args, 5)?],
        arg_f32(args, 6)?,
        false,
        true,
    );
    Ok(ScriptValue::Null)
}

fn spawn_anim_prop_kind(
    ctx: &mut ScriptContext,
    args: &[ScriptValue],
    kind: ScriptPropAnimKind,
) -> Result<ScriptValue, String> {
    let hud = if args.len() >= 7 { arg_bool(args, 6)? } else { false };
    push_spawn_anim_prop(
        ctx,
        arg_str(args, 0)?,
        arg_str(args, 1)?.to_string(),
        kind,
        parse_script_anims(arg_list(args, 5)?)?,
        [1.0; 4],
        [0.0, 0.0],
        [arg_f32(args, 2)?, arg_f32(args, 3)?],
        arg_f32(args, 4)?,
        hud,
        true,
    );
    Ok(ScriptValue::Null)
}

fn api_spawn_sparrow(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    spawn_anim_prop_kind(ctx, args, ScriptPropAnimKind::Sparrow)
}
fn api_spawn_atlas(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    spawn_anim_prop_kind(ctx, args, ScriptPropAnimKind::Atlas)
}

fn api_despawn(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let id = arg_str(args, 0)?;
    ctx.snapshot.prop_positions.remove(id);
    ctx.snapshot.prop_vis.remove(id);
    ctx.commands.push(ScriptCommand::DespawnProp(id.to_string()));
    Ok(ScriptValue::Null)
}

fn api_add_anim(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let anim = parse_single_script_anim(arg_map(args, 1)?)?;
    ctx.commands.push(ScriptCommand::AddPropAnim {
        id: arg_str(args, 0)?.to_string(),
        anim,
    });
    Ok(ScriptValue::Null)
}

fn api_prop_exists(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(
        ctx.snapshot.prop_positions.contains_key(arg_str(args, 0)?),
    ))
}

fn api_get_prop(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let id = arg_str(args, 0)?;
    let prop_name = arg_str(args, 1)?;
    let p = TweenProp::parse(prop_name).ok_or_else(|| format!("unknown prop field '{prop_name}'"))?;
    let val = match p {
        TweenProp::X => f64::from(ctx.snapshot.prop_positions.get(id).map_or(0.0, |v| v[0])),
        TweenProp::Y => f64::from(ctx.snapshot.prop_positions.get(id).map_or(0.0, |v| v[1])),
        TweenProp::Z => f64::from(ctx.snapshot.prop_z.get(id).copied().unwrap_or(0.0)),
        TweenProp::VelX => f64::from(ctx.snapshot.prop_velocities.get(id).map_or(0.0, |v| v[0])),
        TweenProp::VelY => f64::from(ctx.snapshot.prop_velocities.get(id).map_or(0.0, |v| v[1])),
        TweenProp::ScaleX => f64::from(ctx.snapshot.prop_scales.get(id).map_or(1.0, |v| v[0])),
        TweenProp::ScaleY => f64::from(ctx.snapshot.prop_scales.get(id).map_or(1.0, |v| v[1])),
        TweenProp::Angle => f64::from(ctx.snapshot.prop_angles.get(id).copied().unwrap_or(0.0)),
        TweenProp::Alpha => f64::from(ctx.snapshot.prop_alphas.get(id).copied().unwrap_or(1.0)),
        _ => 0.0,
    };
    Ok(ScriptValue::Float(val))
}

fn api_set_prop(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let id = arg_str(args, 0)?;
    let prop_name = arg_str(args, 1)?;
    let v = arg_f32(args, 2)?;
    let p = TweenProp::parse(prop_name).ok_or_else(|| format!("unknown prop field '{prop_name}'"))?;
    match p {
        TweenProp::X => {
            let y = ctx.snapshot.prop_positions.get(id).map_or(0.0, |pos| pos[1]);
            ctx.snapshot.prop_positions.insert(id.to_string(), [v, y]);
        }
        TweenProp::Y => {
            let x = ctx.snapshot.prop_positions.get(id).map_or(0.0, |pos| pos[0]);
            ctx.snapshot.prop_positions.insert(id.to_string(), [x, v]);
        }
        TweenProp::Z => {
            ctx.snapshot.prop_z.insert(id.to_string(), v);
        }
        TweenProp::VelX => {
            let vy = ctx.snapshot.prop_velocities.get(id).map_or(0.0, |vel| vel[1]);
            ctx.snapshot.prop_velocities.insert(id.to_string(), [v, vy]);
        }
        TweenProp::VelY => {
            let vx = ctx.snapshot.prop_velocities.get(id).map_or(0.0, |vel| vel[0]);
            ctx.snapshot.prop_velocities.insert(id.to_string(), [vx, v]);
        }
        TweenProp::ScaleX => {
            let sy = ctx.snapshot.prop_scales.get(id).map_or(1.0, |s| s[1]);
            ctx.snapshot.prop_scales.insert(id.to_string(), [v, sy]);
        }
        TweenProp::ScaleY => {
            let sx = ctx.snapshot.prop_scales.get(id).map_or(1.0, |s| s[0]);
            ctx.snapshot.prop_scales.insert(id.to_string(), [sx, v]);
        }
        TweenProp::Angle => {
            ctx.snapshot.prop_angles.insert(id.to_string(), v);
        }
        TweenProp::Alpha => {
            ctx.snapshot
                .prop_alphas
                .insert(id.to_string(), v.clamp(0.0, 1.0));
        }
        _ => {}
    }
    ctx.commands.push(ScriptCommand::SetPropNumber {
        id: id.to_string(),
        prop: p,
        value: v,
    });
    Ok(ScriptValue::Null)
}

fn api_get_prop_pos(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let p = ctx
        .snapshot
        .prop_positions
        .get(arg_str(args, 0)?)
        .copied()
        .unwrap_or([0.0, 0.0]);
    Ok(vec2_f32_list(p))
}
fn api_prop_pos(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let id = arg_str(args, 0)?;
    let x = arg_f32(args, 1)?;
    let y = arg_f32(args, 2)?;
    ctx.snapshot.prop_positions.insert(id.to_string(), [x, y]);
    ctx.commands.push(ScriptCommand::SetPropNumber {
        id: id.to_string(),
        prop: TweenProp::X,
        value: x,
    });
    ctx.commands.push(ScriptCommand::SetPropNumber {
        id: id.to_string(),
        prop: TweenProp::Y,
        value: y,
    });
    Ok(ScriptValue::Null)
}

fn api_get_prop_scale(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let s = ctx
        .snapshot
        .prop_scales
        .get(arg_str(args, 0)?)
        .copied()
        .unwrap_or([1.0, 1.0]);
    Ok(vec2_f32_list(s))
}
fn api_prop_scale(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let id = arg_str(args, 0)?;
    let sx = arg_f32(args, 1)?;
    let sy = arg_f32(args, 2)?;
    ctx.snapshot.prop_scales.insert(id.to_string(), [sx, sy]);
    ctx.commands.push(ScriptCommand::SetPropNumber {
        id: id.to_string(),
        prop: TweenProp::ScaleX,
        value: sx,
    });
    ctx.commands.push(ScriptCommand::SetPropNumber {
        id: id.to_string(),
        prop: TweenProp::ScaleY,
        value: sy,
    });
    Ok(ScriptValue::Null)
}

fn api_prop_size(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let s = ctx
        .snapshot
        .prop_sizes
        .get(arg_str(args, 0)?)
        .copied()
        .unwrap_or([0.0, 0.0]);
    Ok(vec2_f32_list(s))
}
fn api_set_prop_size(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let id = arg_str(args, 0)?;
    let w = arg_f32(args, 1)?;
    let h = arg_f32(args, 2)?;
    ctx.snapshot.prop_sizes.insert(id.to_string(), [w, h]);
    ctx.commands.push(ScriptCommand::SetPropSize {
        id: id.to_string(),
        w,
        h,
    });
    Ok(ScriptValue::Null)
}

fn api_get_prop_z(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(f64::from(
        ctx.snapshot.prop_z.get(arg_str(args, 0)?).copied().unwrap_or(0.0),
    )))
}
fn api_prop_z(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let id = arg_str(args, 0)?;
    let z = arg_f32(args, 1)?;
    ctx.snapshot.prop_z.insert(id.to_string(), z);
    ctx.commands.push(ScriptCommand::SetPropZ {
        id: id.to_string(),
        z,
    });
    Ok(ScriptValue::Null)
}

fn api_get_prop_scroll(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let s = ctx
        .snapshot
        .prop_scrolls
        .get(arg_str(args, 0)?)
        .copied()
        .unwrap_or([1.0, 1.0]);
    Ok(vec2_f32_list(s))
}
fn api_prop_scroll(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let id = arg_str(args, 0)?;
    let sx = arg_f32(args, 1)?;
    let sy = arg_f32(args, 2)?;
    ctx.snapshot.prop_scrolls.insert(id.to_string(), [sx, sy]);
    ctx.commands.push(ScriptCommand::SetPropScroll {
        id: id.to_string(),
        sx,
        sy,
    });
    Ok(ScriptValue::Null)
}

fn api_get_prop_flip(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let f = ctx
        .snapshot
        .prop_flips
        .get(arg_str(args, 0)?)
        .copied()
        .unwrap_or([false, false]);
    Ok(vec2_bool_list(f))
}
fn api_prop_flip(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let id = arg_str(args, 0)?;
    let flip_x = arg_bool(args, 1)?;
    let flip_y = arg_bool(args, 2)?;
    ctx.snapshot
        .prop_flips
        .insert(id.to_string(), [flip_x, flip_y]);
    ctx.commands.push(ScriptCommand::SetPropFlip {
        id: id.to_string(),
        flip_x,
        flip_y,
    });
    Ok(ScriptValue::Null)
}

fn api_prop_vel(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let id = arg_str(args, 0)?;
    let vx = arg_f32(args, 1)?;
    let vy = arg_f32(args, 2)?;
    let (ax, ay) = if args.len() >= 5 {
        (arg_f32(args, 3)?, arg_f32(args, 4)?)
    } else {
        (0.0, 0.0)
    };
    ctx.snapshot
        .prop_velocities
        .insert(id.to_string(), [vx, vy]);
    ctx.commands.push(ScriptCommand::SetPropVelocity {
        id: id.to_string(),
        vx,
        vy,
        ax,
        ay,
    });
    Ok(ScriptValue::Null)
}

fn api_get_prop_angle(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(f64::from(
        ctx.snapshot
            .prop_angles
            .get(arg_str(args, 0)?)
            .copied()
            .unwrap_or(0.0),
    )))
}
fn api_prop_angle(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let id = arg_str(args, 0)?;
    let deg = arg_f32(args, 1)?;
    ctx.snapshot.prop_angles.insert(id.to_string(), deg);
    ctx.commands.push(ScriptCommand::SetPropNumber {
        id: id.to_string(),
        prop: TweenProp::Angle,
        value: deg,
    });
    Ok(ScriptValue::Null)
}

fn api_get_prop_alpha(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(f64::from(
        ctx.snapshot
            .prop_alphas
            .get(arg_str(args, 0)?)
            .copied()
            .unwrap_or(1.0),
    )))
}
fn api_prop_alpha(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let id = arg_str(args, 0)?;
    let a = arg_f32(args, 1)?.clamp(0.0, 1.0);
    ctx.snapshot.prop_alphas.insert(id.to_string(), a);
    ctx.commands.push(ScriptCommand::SetPropNumber {
        id: id.to_string(),
        prop: TweenProp::Alpha,
        value: a,
    });
    Ok(ScriptValue::Null)
}

fn api_prop_tint(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetPropTint {
        id: arg_str(args, 0)?.to_string(),
        color: parse_hex_rgba(arg_str(args, 1)?),
    });
    Ok(ScriptValue::Null)
}

fn api_prop_visible(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let id = arg_str(args, 0)?;
    let visible = arg_bool(args, 1)?;
    ctx.snapshot.prop_vis.insert(id.to_string(), visible);
    ctx.commands.push(ScriptCommand::SetPropVisible {
        id: id.to_string(),
        visible,
    });
    Ok(ScriptValue::Null)
}

fn api_is_prop_vis(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(
        ctx.snapshot
            .prop_vis
            .get(arg_str(args, 0)?)
            .copied()
            .unwrap_or(true),
    ))
}

fn api_prop_anim(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::String(
        ctx.snapshot
            .prop_anims
            .get(arg_str(args, 0)?)
            .cloned()
            .unwrap_or_default(),
    ))
}

fn api_prop_anim_done(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(
        ctx.snapshot
            .prop_anim_done
            .get(arg_str(args, 0)?)
            .copied()
            .unwrap_or(false),
    ))
}

fn api_prop_repeat(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetPropRepeat {
        id: arg_str(args, 0)?.to_string(),
        repeat_x: arg_bool(args, 1)?,
        repeat_y: arg_bool(args, 2)?,
    });
    Ok(ScriptValue::Null)
}

fn api_prop_matrix(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let matrix = [
        arg_f32(args, 1)?,
        arg_f32(args, 2)?,
        arg_f32(args, 3)?,
        arg_f32(args, 4)?,
        arg_f32(args, 5)?,
        arg_f32(args, 6)?,
    ];
    ctx.commands.push(ScriptCommand::SetPropAffine {
        id: arg_str(args, 0)?.to_string(),
        matrix: Some(matrix),
    });
    Ok(ScriptValue::Null)
}

fn api_prop_matrix_clear(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetPropAffine {
        id: arg_str(args, 0)?.to_string(),
        matrix: None,
    });
    Ok(ScriptValue::Null)
}

fn api_prop_tex(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetPropTexture {
        id: arg_str(args, 0)?.to_string(),
        path: arg_str(args, 1)?.to_string(),
    });
    Ok(ScriptValue::Null)
}

fn api_prop_overlay(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetPropOverlay {
        id: arg_str(args, 0)?.to_string(),
        overlay: arg_bool(args, 1)?,
    });
    Ok(ScriptValue::Null)
}

fn parse_single_script_anim(
    map: &BTreeMap<String, ScriptValue>,
) -> Result<BaseAnimationData, String> {
    let mut data = BaseAnimationData::default();
    if let Some(v) = map.get("name").and_then(ScriptValue::as_str) {
        data.name = v.to_string();
    }
    if let Some(v) = map.get("prefix").and_then(ScriptValue::as_str) {
        data.prefix = v.to_string();
    }
    if let Some(v) = map.get("fps") {
        data.frame_rate = val_f64(v)? as f32;
    }
    if let Some(v) = map.get("looped").and_then(ScriptValue::as_bool) {
        data.looped = v;
    }
    if let Some(v) = map.get("flip_x").and_then(ScriptValue::as_bool) {
        data.flip_x = Some(v);
    }
    if let Some(v) = map.get("flip_y").and_then(ScriptValue::as_bool) {
        data.flip_y = Some(v);
    }
    if let Some(arr) = map.get("offsets").and_then(ScriptValue::as_list)
        && arr.len() >= 2
    {
        data.offsets = [val_f64(&arr[0])? as f32, val_f64(&arr[1])? as f32];
    }
    if let Some(arr) = map.get("indices").and_then(ScriptValue::as_list) {
        for idx in arr {
            data.frame_indices.push(val_usize(idx)?);
        }
    } else if let (Some(s), Some(e)) = (map.get("start"), map.get("end")) {
        let start = val_usize(s)?;
        let end = val_usize(e)?;
        if start <= end {
            data.frame_indices.extend(start..=end);
        } else {
            data.frame_indices.extend((end..=start).rev());
        }
    }
    Ok(data)
}

fn parse_script_anims(anims: &[ScriptValue]) -> Result<Vec<BaseAnimationData>, String> {
    let mut out = Vec::with_capacity(anims.len());
    for item in anims {
        let Some(map) = item.as_map() else {
            return Err("animation entry must be an object map".to_string());
        };
        out.push(parse_single_script_anim(map)?);
    }
    Ok(out)
}

pub(crate) fn api_entries() -> Vec<ScriptApiEntry> {
    vec![
        ScriptApiEntry::new(&["spawn_sprite"], &[5], api_spawn_sprite),
        ScriptApiEntry::new(&["spawn_hud_sprite"], &[5], api_spawn_hud_sprite),
        ScriptApiEntry::new(&["spawn_rect"], &[7], api_spawn_rect),
        ScriptApiEntry::new(&["spawn_hud_rect"], &[7], api_spawn_hud_rect),
        ScriptApiEntry::new(&["spawn_sprite_tl"], &[5], api_spawn_sprite_tl),
        ScriptApiEntry::new(&["spawn_rect_tl"], &[7], api_spawn_rect_tl),
        ScriptApiEntry::new(&["spawn_sparrow"], &[6, 7], api_spawn_sparrow),
        ScriptApiEntry::new(&["spawn_atlas"], &[6, 7], api_spawn_atlas),
        ScriptApiEntry::new(&["despawn"], &[1], api_despawn),
        ScriptApiEntry::new(&["add_anim"], &[2], api_add_anim),
        ScriptApiEntry::new(&["prop_exists"], &[1], api_prop_exists),
        ScriptApiEntry::new(&["get_prop"], &[2], api_get_prop),
        ScriptApiEntry::new(&["set_prop"], &[3], api_set_prop),
        ScriptApiEntry::new(&["get_prop_pos"], &[1], api_get_prop_pos),
        ScriptApiEntry::new(&["prop_pos"], &[3], api_prop_pos),
        ScriptApiEntry::new(&["get_prop_scale"], &[1], api_get_prop_scale),
        ScriptApiEntry::new(&["prop_scale"], &[3], api_prop_scale),
        ScriptApiEntry::new(&["prop_size"], &[1], api_prop_size),
        ScriptApiEntry::new(&["set_prop_size"], &[3], api_set_prop_size),
        ScriptApiEntry::new(&["get_prop_z"], &[1], api_get_prop_z),
        ScriptApiEntry::new(&["prop_z"], &[2], api_prop_z),
        ScriptApiEntry::new(&["get_prop_scroll"], &[1], api_get_prop_scroll),
        ScriptApiEntry::new(&["prop_scroll"], &[3], api_prop_scroll),
        ScriptApiEntry::new(&["get_prop_flip"], &[1], api_get_prop_flip),
        ScriptApiEntry::new(&["prop_flip"], &[3], api_prop_flip),
        ScriptApiEntry::new(&["prop_vel"], &[3, 5], api_prop_vel),
        ScriptApiEntry::new(&["get_prop_angle"], &[1], api_get_prop_angle),
        ScriptApiEntry::new(&["prop_angle"], &[2], api_prop_angle),
        ScriptApiEntry::new(&["get_prop_alpha"], &[1], api_get_prop_alpha),
        ScriptApiEntry::new(&["prop_alpha"], &[2], api_prop_alpha),
        ScriptApiEntry::new(&["prop_tint"], &[2], api_prop_tint),
        ScriptApiEntry::new(&["prop_visible", "prop_vis"], &[2], api_prop_visible),
        ScriptApiEntry::new(&["is_prop_vis"], &[1], api_is_prop_vis),
        ScriptApiEntry::new(&["prop_anim"], &[1], api_prop_anim),
        ScriptApiEntry::new(&["prop_anim_done"], &[1], api_prop_anim_done),
        ScriptApiEntry::new(&["prop_repeat"], &[3], api_prop_repeat),
        ScriptApiEntry::new(&["prop_matrix"], &[7], api_prop_matrix),
        ScriptApiEntry::new(&["prop_matrix_clear"], &[1], api_prop_matrix_clear),
        ScriptApiEntry::new(&["prop_tex"], &[2], api_prop_tex),
        ScriptApiEntry::new(&["prop_overlay", "text_overlay"], &[2], api_prop_overlay),
    ]
}
