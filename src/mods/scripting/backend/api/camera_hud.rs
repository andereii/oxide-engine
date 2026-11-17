use rhai::{Dynamic, Engine, EvalAltResult};

use crate::scripting::api::{ScriptCommand, TweenProp, TweenTarget, parse_hex_rgba};
use crate::scripting::backend::context::{dyn_to_f64, dyn_to_i64, with_ctx};

pub fn register_camera_hud_api(engine: &mut Engine) {
    engine.register_fn("cam_zoom", || with_ctx(|c| c.mutable.cam_zoom));
    engine.register_fn("stage_zoom", || with_ctx(|c| c.snapshot.stage_zoom));
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
    engine.register_fn("reset_cam", || {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::ResetCamera {
                zoom: true,
                tweens: true,
                snap: true,
            });
        })
    });
    engine.register_fn("stop_cam", || {
        with_ctx(|c| c.commands.push(ScriptCommand::CancelCameraTweens))
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

    engine.register_fn("hud_angle", || with_ctx(|c| f64::from(c.mutable.hud_angle_deg)));
    engine.register_fn("set_hud_angle", |deg: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let val = dyn_to_f64(&deg)? as f32;
        with_ctx(|c| {
            c.mutable.hud_angle_deg = val;
            c.mutable.dirty_hud_angle = true;
        })
    });
    engine.register_fn("hud_turn", |deg: Dynamic, secs: Dynamic, ease: &str| -> Result<(), Box<EvalAltResult>> {
        let target = dyn_to_f64(&deg)?;
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| c.push_tween(TweenTarget::Camera, TweenProp::HudAngle, target, dur, ease))
    });
    engine.register_fn("hud_turn", |deg: Dynamic, secs: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let target = dyn_to_f64(&deg)?;
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| c.push_tween(TweenTarget::Camera, TweenProp::HudAngle, target, dur, "quadOut"))
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
    engine.register_fn("game_flash", |hex: &str, secs: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::CameraFlash {
                color: parse_hex_rgba(hex),
                duration_s: dur.max(0.0),
                hud: false,
            });
        })
    });
    engine.register_fn("fade", |hex: &str, secs: Dynamic, fade_in: bool| -> Result<(), Box<EvalAltResult>> {
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::CameraFade {
                color: parse_hex_rgba(hex),
                duration_s: dur.max(0.0),
                fade_in,
                hud: true,
            });
        })
    });
    engine.register_fn("fade", |hex: &str, secs: Dynamic, fade_in: bool, hud: bool| -> Result<(), Box<EvalAltResult>> {
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::CameraFade {
                color: parse_hex_rgba(hex),
                duration_s: dur.max(0.0),
                fade_in,
                hud,
            });
        })
    });

    // Health bar, icons & judgement popups
    engine.register_fn("bar_colors", |opp_hex: &str, ply_hex: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetBarColors {
                opp: parse_hex_rgba(opp_hex),
                ply: parse_hex_rgba(ply_hex),
            });
        })
    });
    engine.register_fn("set_icon", |side: &str, char_id: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetIcon {
                side: side.to_string(),
                char_id: char_id.to_string(),
            });
        })
    });
    engine.register_fn("icon_scale", |side: &str, scale: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let scale = dyn_to_f64(&scale)? as f32;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetIconScale {
                side: side.to_string(),
                scale,
            });
        })
    });
    engine.register_fn("icon_bop", |side: &str, enabled: bool| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetIconBop {
                side: side.to_string(),
                enabled,
                every: 4,
                angle_deg: 0.0,
            });
        })
    });
    engine.register_fn(
        "icon_bop",
        |side: &str, enabled: bool, every: Dynamic, angle: Dynamic| -> Result<(), Box<EvalAltResult>> {
            let every = dyn_to_i64(&every)? as i32;
            let angle_deg = dyn_to_f64(&angle)? as f32;
            with_ctx(|c| {
                c.commands.push(ScriptCommand::SetIconBop {
                    side: side.to_string(),
                    enabled,
                    every,
                    angle_deg,
                });
            })
        },
    );
    engine.register_fn("popup", |judgement: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SpawnPopup {
                judgement: judgement.to_string(),
                combo: None,
            });
        })
    });
    engine.register_fn("popup", |judgement: &str, combo: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let cmb = dyn_to_i64(&combo)?;
        let combo = (cmb >= 0).then_some(cmb as u32);
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SpawnPopup {
                judgement: judgement.to_string(),
                combo,
            });
        })
    });
    engine.register_fn("popups_vis", |visible: bool| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetPopupsConfig {
                visible: Some(visible),
                offset: None,
            });
        })
    });
    engine.register_fn("popups_off", |x: Dynamic, y: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let x = dyn_to_f64(&x)? as f32;
        let y = dyn_to_f64(&y)? as f32;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetPopupsConfig {
                visible: None,
                offset: Some([x, y]),
            });
        })
    });
}
