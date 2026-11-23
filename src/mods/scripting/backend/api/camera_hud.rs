use crate::scripting::api::{ScriptCommand, TweenProp, TweenTarget, parse_hex_rgba};
use crate::scripting::value::ScriptValue;

use super::bindings::{
    ScriptApiEntry, arg_bool, arg_f32, arg_f64, arg_i64, arg_str, vec2_f64_list,
};
use super::context::ScriptContext;

fn api_cam_zoom(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(ctx.mutable.cam_zoom))
}

fn api_stage_zoom(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(ctx.snapshot.stage_zoom))
}

fn api_set_zoom(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.mutable.cam_zoom = arg_f64(args, 0)?;
    ctx.mutable.dirty_cam_zoom = true;
    Ok(ScriptValue::Null)
}

fn api_zoom(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let target = arg_f64(args, 0)?;
    let dur = arg_f64(args, 1)?.max(0.0);
    let ease = if args.len() >= 3 { arg_str(args, 2)? } else { "quadOut" };
    ctx.commands.push(ScriptCommand::TweenCameraZoom {
        target,
        duration_s: dur,
        ease: ease.to_string(),
    });
    Ok(ScriptValue::Null)
}

fn api_hud_zoom(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(ctx.mutable.hud_zoom))
}

fn api_set_hud_zoom(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.mutable.hud_zoom = arg_f64(args, 0)?;
    ctx.mutable.dirty_hud_zoom = true;
    Ok(ScriptValue::Null)
}

fn api_cam_pos(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(vec2_f64_list([ctx.mutable.cam_x, ctx.mutable.cam_y]))
}

fn api_set_cam(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.mutable.cam_x = arg_f64(args, 0)?;
    ctx.mutable.cam_y = arg_f64(args, 1)?;
    ctx.mutable.dirty_cam_pos = true;
    Ok(ScriptValue::Null)
}

fn api_pan_cam(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::TweenCameraPos {
        x: arg_f64(args, 0)?,
        y: arg_f64(args, 1)?,
        duration_s: arg_f64(args, 2)?.max(0.0),
        ease: arg_str(args, 3)?.to_string(),
    });
    Ok(ScriptValue::Null)
}

fn api_focus(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands
        .push(ScriptCommand::FocusCameraTarget(arg_str(args, 0)?.to_string()));
    Ok(ScriptValue::Null)
}

fn api_focus_at(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::TweenCameraPos {
        x: arg_f64(args, 0)?,
        y: arg_f64(args, 1)?,
        duration_s: 0.4,
        ease: "quadOut".to_string(),
    });
    Ok(ScriptValue::Null)
}

fn api_reset_cam(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::ResetCamera {
        zoom: true,
        tweens: true,
        snap: true,
    });
    Ok(ScriptValue::Null)
}

fn api_stop_cam(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::CancelCameraTweens);
    Ok(ScriptValue::Null)
}

fn api_cam_bop(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::CameraBop {
        game_delta: arg_f64(args, 0)?,
        hud_delta: arg_f64(args, 1)?,
    });
    Ok(ScriptValue::Null)
}

fn api_set_bop(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetCameraBopConfig {
        intensity: arg_f64(args, 0)?,
        rate: arg_f64(args, 1)?,
    });
    Ok(ScriptValue::Null)
}

fn api_cam_angle(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(f64::from(ctx.mutable.cam_angle_deg)))
}

fn api_set_cam_angle(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.mutable.cam_angle_deg = arg_f32(args, 0)?;
    ctx.mutable.dirty_cam_angle = true;
    Ok(ScriptValue::Null)
}

fn api_cam_turn(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let ease = if args.len() >= 3 { arg_str(args, 2)? } else { "quadOut" };
    ctx.push_tween(TweenTarget::Camera, TweenProp::Angle, arg_f64(args, 0)?, arg_f64(args, 1)?, ease);
    Ok(ScriptValue::Null)
}

fn api_hud_angle(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(f64::from(ctx.mutable.hud_angle_deg)))
}

fn api_set_hud_angle(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.mutable.hud_angle_deg = arg_f32(args, 0)?;
    ctx.mutable.dirty_hud_angle = true;
    Ok(ScriptValue::Null)
}

fn api_hud_turn(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let ease = if args.len() >= 3 { arg_str(args, 2)? } else { "quadOut" };
    ctx.push_tween(
        TweenTarget::Camera,
        TweenProp::HudAngle,
        arg_f64(args, 0)?,
        arg_f64(args, 1)?,
        ease,
    );
    Ok(ScriptValue::Null)
}

fn push_shake(ctx: &mut ScriptContext, args: &[ScriptValue], hud: bool) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::CameraShake {
        intensity: arg_f32(args, 0)?,
        duration_s: arg_f64(args, 1)?.max(0.0),
        hud,
    });
    Ok(ScriptValue::Null)
}

fn api_shake(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    push_shake(ctx, args, false)
}
fn api_hud_shake(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    push_shake(ctx, args, true)
}

fn push_flash(ctx: &mut ScriptContext, args: &[ScriptValue], hud: bool) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::CameraFlash {
        color: parse_hex_rgba(arg_str(args, 0)?),
        duration_s: arg_f64(args, 1)?.max(0.0),
        hud,
    });
    Ok(ScriptValue::Null)
}

fn api_flash(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    push_flash(ctx, args, true)
}
fn api_game_flash(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    push_flash(ctx, args, false)
}

fn api_fade(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let hud = if args.len() >= 4 { arg_bool(args, 3)? } else { true };
    ctx.commands.push(ScriptCommand::CameraFade {
        color: parse_hex_rgba(arg_str(args, 0)?),
        duration_s: arg_f64(args, 1)?.max(0.0),
        fade_in: arg_bool(args, 2)?,
        hud,
    });
    Ok(ScriptValue::Null)
}

fn api_overlay_flash(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::OverlayFlash {
        color: parse_hex_rgba(arg_str(args, 0)?),
        duration_s: arg_f64(args, 1)?.max(0.0),
    });
    Ok(ScriptValue::Null)
}

fn api_overlay_fade(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::OverlayFade {
        color: parse_hex_rgba(arg_str(args, 0)?),
        duration_s: arg_f64(args, 1)?.max(0.0),
        fade_in: arg_bool(args, 2)?,
    });
    Ok(ScriptValue::Null)
}

fn api_bar_colors(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetBarColors {
        opp: parse_hex_rgba(arg_str(args, 0)?),
        ply: parse_hex_rgba(arg_str(args, 1)?),
    });
    Ok(ScriptValue::Null)
}

fn api_set_icon(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetIcon {
        side: arg_str(args, 0)?.to_string(),
        char_id: arg_str(args, 1)?.to_string(),
    });
    Ok(ScriptValue::Null)
}

fn api_icon_flip(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetIconFlip {
        side: arg_str(args, 0)?.to_string(),
        flip_x: arg_bool(args, 1)?,
    });
    Ok(ScriptValue::Null)
}

fn api_icon_visible(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetIconVisible {
        side: arg_str(args, 0)?.to_string(),
        visible: arg_bool(args, 1)?,
    });
    Ok(ScriptValue::Null)
}

fn api_icon_scale(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetIconScale {
        side: arg_str(args, 0)?.to_string(),
        scale: arg_f32(args, 1)?,
    });
    Ok(ScriptValue::Null)
}

fn api_icon_bop(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let (every, angle_deg) = if args.len() >= 4 {
        (arg_i64(args, 2)? as i32, arg_f32(args, 3)?)
    } else {
        (4, 0.0)
    };
    ctx.commands.push(ScriptCommand::SetIconBop {
        side: arg_str(args, 0)?.to_string(),
        enabled: arg_bool(args, 1)?,
        every,
        angle_deg,
    });
    Ok(ScriptValue::Null)
}

fn api_popup(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let combo = if args.len() >= 2 {
        let cmb = arg_i64(args, 1)?;
        (cmb >= 0).then_some(cmb as u32)
    } else {
        None
    };
    ctx.commands.push(ScriptCommand::SpawnPopup {
        judgement: arg_str(args, 0)?.to_string(),
        combo,
    });
    Ok(ScriptValue::Null)
}

fn api_popups_vis(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetPopupsConfig {
        visible: Some(arg_bool(args, 0)?),
        offset: None,
    });
    Ok(ScriptValue::Null)
}

fn api_popups_off(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetPopupsConfig {
        visible: None,
        offset: Some([arg_f32(args, 0)?, arg_f32(args, 1)?]),
    });
    Ok(ScriptValue::Null)
}

pub(crate) fn api_entries() -> Vec<ScriptApiEntry> {
    vec![
        ScriptApiEntry::new(&["cam_zoom"], &[0], api_cam_zoom),
        ScriptApiEntry::new(&["stage_zoom"], &[0], api_stage_zoom),
        ScriptApiEntry::new(&["set_zoom"], &[1], api_set_zoom),
        ScriptApiEntry::new(&["zoom"], &[2, 3], api_zoom),
        ScriptApiEntry::new(&["hud_zoom"], &[0], api_hud_zoom),
        ScriptApiEntry::new(&["set_hud_zoom"], &[1], api_set_hud_zoom),
        ScriptApiEntry::new(&["cam_pos"], &[0], api_cam_pos),
        ScriptApiEntry::new(&["set_cam"], &[2], api_set_cam),
        ScriptApiEntry::new(&["pan_cam"], &[4], api_pan_cam),
        ScriptApiEntry::new(&["focus"], &[1], api_focus),
        ScriptApiEntry::new(&["focus_at"], &[2], api_focus_at),
        ScriptApiEntry::new(&["reset_cam"], &[0], api_reset_cam),
        ScriptApiEntry::new(&["stop_cam"], &[0], api_stop_cam),
        ScriptApiEntry::new(&["cam_bop"], &[2], api_cam_bop),
        ScriptApiEntry::new(&["set_bop"], &[2], api_set_bop),
        ScriptApiEntry::new(&["cam_angle"], &[0], api_cam_angle),
        ScriptApiEntry::new(&["set_cam_angle"], &[1], api_set_cam_angle),
        ScriptApiEntry::new(&["cam_turn"], &[2, 3], api_cam_turn),
        ScriptApiEntry::new(&["hud_angle"], &[0], api_hud_angle),
        ScriptApiEntry::new(&["set_hud_angle"], &[1], api_set_hud_angle),
        ScriptApiEntry::new(&["hud_turn"], &[2, 3], api_hud_turn),
        ScriptApiEntry::new(&["shake"], &[2], api_shake),
        ScriptApiEntry::new(&["hud_shake"], &[2], api_hud_shake),
        ScriptApiEntry::new(&["flash"], &[2], api_flash),
        ScriptApiEntry::new(&["game_flash"], &[2], api_game_flash),
        ScriptApiEntry::new(&["fade"], &[3, 4], api_fade),
        ScriptApiEntry::new(&["overlay_flash", "cutscene_flash"], &[2], api_overlay_flash),
        ScriptApiEntry::new(&["overlay_fade", "cutscene_fade"], &[3], api_overlay_fade),
        ScriptApiEntry::new(&["bar_colors"], &[2], api_bar_colors),
        ScriptApiEntry::new(&["set_icon"], &[2], api_set_icon),
        ScriptApiEntry::new(&["icon_flip"], &[2], api_icon_flip),
        ScriptApiEntry::new(&["icon_visible", "icon_vis"], &[2], api_icon_visible),
        ScriptApiEntry::new(&["icon_scale"], &[2], api_icon_scale),
        ScriptApiEntry::new(&["icon_bop"], &[2, 4], api_icon_bop),
        ScriptApiEntry::new(&["popup"], &[1, 2], api_popup),
        ScriptApiEntry::new(&["popups_vis"], &[1], api_popups_vis),
        ScriptApiEntry::new(&["popups_off"], &[2], api_popups_off),
    ]
}
