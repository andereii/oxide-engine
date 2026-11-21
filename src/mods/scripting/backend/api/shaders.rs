use rhai::{Dynamic, Engine, EvalAltResult};

use crate::scripting::api::{ScriptCommand, parse_hex_rgba};
use crate::scripting::backend::context::{dyn_to_f64, dyn_to_i64, with_ctx};

fn parse_rgb_hex3(hex: &str) -> [f32; 3] {
    let [r, g, b, _] = parse_hex_rgba(hex);
    [r, g, b]
}

fn push_adjust_color(
    target: &str,
    hue: Dynamic,
    sat: Dynamic,
    bright: Dynamic,
    contrast: Dynamic,
) -> Result<(), Box<EvalAltResult>> {
    let hue = dyn_to_f64(&hue)? as f32;
    let saturation = dyn_to_f64(&sat)? as f32;
    let brightness = dyn_to_f64(&bright)? as f32;
    let contrast = dyn_to_f64(&contrast)? as f32;
    with_ctx(|c| {
        c.commands.push(ScriptCommand::SetShaderAdjustColor {
            target: target.to_string(),
            hue,
            saturation,
            brightness,
            contrast,
        });
    })
}

fn push_drop_shadow_cmd(
    target: &str,
    color_hex: &str,
    angle: f64,
    dist: f64,
    strength: f64,
    thr: f64,
    hue: f64,
    sat: f64,
    bright: f64,
    contrast: f64,
    alt_mask_path: Option<String>,
    mask_threshold: f64,
) -> Result<(), Box<EvalAltResult>> {
    let use_alt_mask = alt_mask_path.is_some();
    with_ctx(|c| {
        c.commands.push(ScriptCommand::SetShaderDropShadow {
            target: target.to_string(),
            color: parse_rgb_hex3(color_hex),
            angle_deg: angle as f32,
            distance: dist as f32,
            strength: strength as f32,
            threshold: thr as f32,
            hue: hue as f32,
            saturation: sat as f32,
            brightness: bright as f32,
            contrast: contrast as f32,
            use_alt_mask,
            alt_mask_path,
            mask_threshold: mask_threshold as f32,
        });
    })
}

fn register_shader_presets_under(
    engine: &mut Engine,
    adj_name: &str,
    drop_name: &str,
    wig_name: &str,
    rain_name: &str,
    overlay_name: &str,
    rain_sprite_name: &str,
    building_name: &str,
) {
    engine.register_fn(adj_name, |target: &str, hue: Dynamic, sat: Dynamic, bright: Dynamic, contrast: Dynamic| {
        push_adjust_color(target, hue, sat, bright, contrast)
    });
    engine.register_fn(
        drop_name,
        |target: &str, color_hex: &str, angle: Dynamic, dist: Dynamic, strength: Dynamic, thr: Dynamic| -> Result<(), Box<EvalAltResult>> {
            push_drop_shadow_cmd(target, color_hex, dyn_to_f64(&angle)?, dyn_to_f64(&dist)?, dyn_to_f64(&strength)?, dyn_to_f64(&thr)?, 0.0, 0.0, 0.0, 0.0, None, 0.5)
        },
    );
    engine.register_fn(
        drop_name,
        |target: &str, color_hex: &str, angle: Dynamic, dist: Dynamic, strength: Dynamic, thr: Dynamic, hue: Dynamic, sat: Dynamic, bright: Dynamic, contrast: Dynamic| -> Result<(), Box<EvalAltResult>> {
            push_drop_shadow_cmd(target, color_hex, dyn_to_f64(&angle)?, dyn_to_f64(&dist)?, dyn_to_f64(&strength)?, dyn_to_f64(&thr)?, dyn_to_f64(&hue)?, dyn_to_f64(&sat)?, dyn_to_f64(&bright)?, dyn_to_f64(&contrast)?, None, 0.5)
        },
    );
    engine.register_fn(
        drop_name,
        |target: &str, color_hex: &str, angle: Dynamic, dist: Dynamic, strength: Dynamic, thr: Dynamic, hue: Dynamic, sat: Dynamic, bright: Dynamic, contrast: Dynamic, mask_path: &str, mask_thr: Dynamic| -> Result<(), Box<EvalAltResult>> {
            let mask = (!mask_path.trim().is_empty()).then(|| mask_path.to_string());
            push_drop_shadow_cmd(target, color_hex, dyn_to_f64(&angle)?, dyn_to_f64(&dist)?, dyn_to_f64(&strength)?, dyn_to_f64(&thr)?, dyn_to_f64(&hue)?, dyn_to_f64(&sat)?, dyn_to_f64(&bright)?, dyn_to_f64(&contrast)?, mask, dyn_to_f64(&mask_thr)?)
        },
    );
    engine.register_fn(
        wig_name,
        |target: &str, effect: Dynamic, speed: Dynamic, freq: Dynamic, amp: Dynamic| -> Result<(), Box<EvalAltResult>> {
            let effect_str = if let Ok(s) = effect.clone().into_string() {
                s
            } else {
                dyn_to_i64(&effect)?.to_string()
            };
            let speed = dyn_to_f64(&speed)? as f32;
            let frequency = dyn_to_f64(&freq)? as f32;
            let amplitude = dyn_to_f64(&amp)? as f32;
            with_ctx(|c| {
                c.commands.push(ScriptCommand::SetShaderWiggle {
                    target: target.to_string(),
                    effect: effect_str,
                    speed,
                    frequency,
                    amplitude,
                });
            })
        },
    );
    engine.register_fn(
        rain_name,
        |scale: Dynamic, intensity: Dynamic, color_hex: &str| -> Result<(), Box<EvalAltResult>> {
            let scale = dyn_to_f64(&scale)? as f32;
            let intensity = dyn_to_f64(&intensity)? as f32;
            let color = parse_rgb_hex3(color_hex);
            with_ctx(|c| {
                c.commands.push(ScriptCommand::SetCameraRain {
                    scale,
                    start_intensity: intensity,
                    end_intensity: intensity,
                    color,
                    time_scale_decay: false,
                });
            })
        },
    );
    engine.register_fn(
        rain_name,
        |scale: Dynamic, intensity: Dynamic, color_hex: &str, decay: bool| -> Result<(), Box<EvalAltResult>> {
            let scale = dyn_to_f64(&scale)? as f32;
            let intensity = dyn_to_f64(&intensity)? as f32;
            let color = parse_rgb_hex3(color_hex);
            with_ctx(|c| {
                c.commands.push(ScriptCommand::SetCameraRain {
                    scale,
                    start_intensity: intensity,
                    end_intensity: intensity,
                    color,
                    time_scale_decay: decay,
                });
            })
        },
    );
    engine.register_fn(
        rain_name,
        |scale: Dynamic, start_intensity: Dynamic, end_intensity: Dynamic, color_hex: &str, decay: bool| -> Result<(), Box<EvalAltResult>> {
            let scale = dyn_to_f64(&scale)? as f32;
            let start_intensity = dyn_to_f64(&start_intensity)? as f32;
            let end_intensity = dyn_to_f64(&end_intensity)? as f32;
            let color = parse_rgb_hex3(color_hex);
            with_ctx(|c| {
                c.commands.push(ScriptCommand::SetCameraRain {
                    scale,
                    start_intensity,
                    end_intensity,
                    color,
                    time_scale_decay: decay,
                });
            })
        },
    );
    engine.register_fn(overlay_name, |target: &str, aux_path: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetShaderOverlayBlend {
                target: target.to_string(),
                aux_path: aux_path.to_string(),
            });
        })
    });
    engine.register_fn(
        rain_sprite_name,
        |target: &str, scale: Dynamic, intensity: Dynamic, color_hex: &str| -> Result<(), Box<EvalAltResult>> {
            let scale = dyn_to_f64(&scale)? as f32;
            let intensity = dyn_to_f64(&intensity)? as f32;
            let color = parse_rgb_hex3(color_hex);
            with_ctx(|c| {
                c.commands.push(ScriptCommand::SetShaderRainSprite {
                    target: target.to_string(),
                    scale,
                    intensity,
                    color,
                });
            })
        },
    );
    engine.register_fn(building_name, |target: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetShaderBuilding {
                target: target.to_string(),
                alpha_shit: 1.0,
            });
        })
    });
    engine.register_fn(
        building_name,
        |target: &str, alpha_shit: Dynamic| -> Result<(), Box<EvalAltResult>> {
            let alpha_shit = dyn_to_f64(&alpha_shit)? as f32;
            with_ctx(|c| {
                c.commands.push(ScriptCommand::SetShaderBuilding {
                    target: target.to_string(),
                    alpha_shit,
                });
            })
        },
    );
}

pub fn register_shader_api(engine: &mut Engine) {
    register_shader_presets_under(
        engine,
        "set_adjust_color",
        "set_drop_shadow",
        "set_wiggle",
        "set_rain",
        "set_overlay_blend",
        "set_rain_sprite",
        "set_building",
    );
    register_shader_presets_under(
        engine,
        "adjust_color",
        "drop_shadow",
        "wiggle",
        "rain",
        "overlay_blend",
        "rain_sprite",
        "building",
    );

    for name in ["blend", "set_blend"] {
        engine.register_fn(name, |target: &str, mode: &str| {
            with_ctx(|c| {
                c.commands.push(ScriptCommand::SetBlendMode {
                    target: target.to_string(),
                    mode: mode.to_string(),
                });
            })
        });
    }
    for name in ["shader", "set_shader"] {
        engine.register_fn(name, |target: &str, path: &str| {
            with_ctx(|c| {
                c.commands.push(ScriptCommand::SetCustomShader {
                    target: target.to_string(),
                    path: path.to_string(),
                });
            })
        });
    }
    engine.register_fn("shader_val", |target: &str, key: &str, val: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let v = dyn_to_f64(&val)? as f32;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetShaderParam {
                target: target.to_string(),
                key: key.to_string(),
                values: [v, 0.0, 0.0, 0.0],
            });
        })
    });
    engine.register_fn(
        "shader_vec",
        |target: &str, key: &str, x: Dynamic, y: Dynamic, z: Dynamic, w: Dynamic| -> Result<(), Box<EvalAltResult>> {
            let values = [
                dyn_to_f64(&x)? as f32,
                dyn_to_f64(&y)? as f32,
                dyn_to_f64(&z)? as f32,
                dyn_to_f64(&w)? as f32,
            ];
            with_ctx(|c| {
                c.commands.push(ScriptCommand::SetShaderParam {
                    target: target.to_string(),
                    key: key.to_string(),
                    values,
                });
            })
        },
    );
    engine.register_fn("shader_tex", |target: &str, path: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetShaderTex {
                target: target.to_string(),
                path: path.to_string(),
            });
        })
    });

    engine.register_fn("clear_shader", |target: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::ClearShader {
                target: target.to_string(),
            });
        })
    });
}
