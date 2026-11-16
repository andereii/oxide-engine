use rhai::{Dynamic, Engine, EvalAltResult};

use crate::scripting::api::{ScriptCommand, parse_hex_rgba};
use crate::scripting::backend::context::{dyn_to_f64, dyn_to_usize, with_ctx};

pub fn register_character_api(engine: &mut Engine) {
    engine.register_fn("char_id", |role: &str| {
        with_ctx(|c| match role.trim().to_ascii_lowercase().as_str() {
            "bf" | "boyfriend" | "player" => c.snapshot.bf_id.clone(),
            "dad" | "opponent" => c.snapshot.dad_id.clone(),
            "gf" | "girlfriend" | "spectator" => c.snapshot.gf_id.clone(),
            _ => String::new(),
        })
    });
    engine.register_fn("char_exists", |target: &str| {
        with_ctx(|c| c.snapshot.char_positions.contains_key(target))
    });
    engine.register_fn("anim", |target: &str, name: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::PlayAnim {
                target: target.to_string(),
                anim: name.to_string(),
                force: true,
            });
        })
    });
    engine.register_fn("play_anim", |target: &str, name: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::PlayAnim {
                target: target.to_string(),
                anim: name.to_string(),
                force: true,
            });
        })
    });
    engine.register_fn("play_anim_frame", |target: &str, name: &str, frame: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let frame = dyn_to_usize(&frame)?;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::PlayAnimFrame {
                target: target.to_string(),
                anim: name.to_string(),
                force: true,
                frame,
            });
        })
    });
    engine.register_fn("play_sing", |target: &str, dir: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let dir = dyn_to_usize(&dir)?;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::PlaySing {
                target: target.to_string(),
                dir,
                miss: false,
                suffix: String::new(),
            });
        })
    });
    engine.register_fn(
        "play_sing",
        |target: &str, dir: Dynamic, miss: bool, suffix: &str| -> Result<(), Box<EvalAltResult>> {
            let dir = dyn_to_usize(&dir)?;
            with_ctx(|c| {
                c.commands.push(ScriptCommand::PlaySing {
                    target: target.to_string(),
                    dir,
                    miss,
                    suffix: suffix.to_string(),
                });
            })
        },
    );
    engine.register_fn("dance", |target: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::CharDance {
                target: target.to_string(),
                force: false,
            });
        })
    });
    engine.register_fn("dance", |target: &str, force: bool| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::CharDance {
                target: target.to_string(),
                force,
            });
        })
    });
    engine.register_fn("char_anim", |target: &str| {
        with_ctx(|c| c.snapshot.char_anims.get(target).cloned().unwrap_or_default())
    });
    for name in ["char_anim_finished", "char_anim_done"] {
        engine.register_fn(name, |target: &str| {
            with_ctx(|c| {
                c.snapshot
                    .char_anim_finished
                    .get(target)
                    .copied()
                    .unwrap_or(false)
            })
        });
    }
    engine.register_fn("bf_anim", |name: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::PlayAnim {
                target: "bf".to_string(),
                anim: name.to_string(),
                force: true,
            });
        })
    });
    engine.register_fn("dad_anim", |name: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::PlayAnim {
                target: "dad".to_string(),
                anim: name.to_string(),
                force: true,
            });
        })
    });
    engine.register_fn("gf_anim", |name: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::PlayAnim {
                target: "gf".to_string(),
                anim: name.to_string(),
                force: true,
            });
        })
    });
    engine.register_fn("char_pos", |target: &str| {
        with_ctx(|c| {
            let p = c.snapshot.char_positions.get(target).copied().unwrap_or([0.0, 0.0]);
            vec![Dynamic::from(f64::from(p[0])), Dynamic::from(f64::from(p[1]))]
        })
    });
    engine.register_fn("char_top_left", |target: &str| {
        with_ctx(|c| {
            let p = c.snapshot.char_top_left.get(target).copied().unwrap_or([0.0, 0.0]);
            vec![Dynamic::from(f64::from(p[0])), Dynamic::from(f64::from(p[1]))]
        })
    });
    engine.register_fn("char_focus", |target: &str| {
        with_ctx(|c| {
            let p = c.snapshot.char_focus.get(target).copied().unwrap_or([0.0, 0.0]);
            vec![Dynamic::from(p[0]), Dynamic::from(p[1])]
        })
    });
    engine.register_fn("char_size", |target: &str| {
        with_ctx(|c| {
            let p = c.snapshot.char_sizes.get(target).copied().unwrap_or([0.0, 0.0]);
            vec![Dynamic::from(f64::from(p[0])), Dynamic::from(f64::from(p[1]))]
        })
    });
    engine.register_fn("char_z", |target: &str| {
        with_ctx(|c| f64::from(c.snapshot.char_z.get(target).copied().unwrap_or(-500.0)))
    });
    engine.register_fn("set_char_z", |target: &str, z: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let z = dyn_to_f64(&z)? as f32;
        with_ctx(|c| {
            c.snapshot.char_z.insert(target.to_string(), z);
            c.commands.push(ScriptCommand::SetCharZ {
                target: target.to_string(),
                z,
            });
        })
    });
    engine.register_fn("set_char_pos", |target: &str, x: Dynamic, y: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let x = dyn_to_f64(&x)? as f32;
        let y = dyn_to_f64(&y)? as f32;
        with_ctx(|c| {
            c.snapshot.char_positions.insert(target.to_string(), [x, y]);
            c.commands.push(ScriptCommand::SetCharacterPos {
                target: target.to_string(),
                x,
                y,
            });
        })
    });
    engine.register_fn("get_char_scale", |target: &str| {
        with_ctx(|c| {
            let s = c.snapshot.char_scales.get(target).copied().unwrap_or([1.0, 1.0]);
            vec![Dynamic::from(f64::from(s[0])), Dynamic::from(f64::from(s[1]))]
        })
    });
    engine.register_fn("char_scale", |target: &str, sx: Dynamic, sy: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let sx = dyn_to_f64(&sx)? as f32;
        let sy = dyn_to_f64(&sy)? as f32;
        with_ctx(|c| {
            c.snapshot.char_scales.insert(target.to_string(), [sx, sy]);
            c.commands.push(ScriptCommand::SetCharacterScale {
                target: target.to_string(),
                sx,
                sy,
            });
        })
    });
    engine.register_fn("char_flip", |target: &str, flip_x: bool, flip_y: bool| {
        with_ctx(|c| {
            c.snapshot.char_flips.insert(target.to_string(), [flip_x, flip_y]);
            c.commands.push(ScriptCommand::SetCharFlip {
                target: target.to_string(),
                flip_x,
                flip_y,
            });
        })
    });
    engine.register_fn("get_char_flip", |target: &str| {
        with_ctx(|c| {
            let f = c.snapshot.char_flips.get(target).copied().unwrap_or([false, false]);
            vec![Dynamic::from(f[0]), Dynamic::from(f[1])]
        })
    });
    engine.register_fn("char_tint", |target: &str, hex: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetCharacterTint {
                target: target.to_string(),
                color: parse_hex_rgba(hex),
            });
        })
    });
    engine.register_fn("get_char_alpha", |target: &str| {
        with_ctx(|c| f64::from(c.snapshot.char_alphas.get(target).copied().unwrap_or(1.0)))
    });
    engine.register_fn("char_alpha", |target: &str, alpha: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let alpha = (dyn_to_f64(&alpha)? as f32).clamp(0.0, 1.0);
        with_ctx(|c| {
            c.snapshot.char_alphas.insert(target.to_string(), alpha);
            c.commands.push(ScriptCommand::SetCharacterAlpha {
                target: target.to_string(),
                alpha,
            });
        })
    });
    for name in ["char_visible", "char_vis"] {
        engine.register_fn(name, |target: &str, visible: bool| {
            with_ctx(|c| {
                c.snapshot.char_vis.insert(target.to_string(), visible);
                c.commands.push(ScriptCommand::SetCharacterVisible {
                    target: target.to_string(),
                    visible,
                });
            })
        });
    }
    engine.register_fn("is_char_vis", |target: &str| {
        with_ctx(|c| c.snapshot.char_vis.get(target).copied().unwrap_or(true))
    });
    engine.register_fn("set_dance_every", |target: &str, beats: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let beats = dyn_to_f64(&beats)? as f32;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetCharacterDanceEvery {
                target: target.to_string(),
                beats: beats.max(0.0),
            });
        })
    });
    engine.register_fn("swap_char", |role: &str, char_id: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SwapChar {
                role: role.to_string(),
                char_id: char_id.to_string(),
            });
        })
    });
    engine.register_fn(
        "char_trail",
        |target: &str,
         length: Dynamic,
         delay_s: Dynamic,
         alpha: Dynamic,
         diff: Dynamic|
         -> Result<(), Box<EvalAltResult>> {
            let length = dyn_to_usize(&length)?.clamp(1, 32);
            let delay_s = dyn_to_f64(&delay_s)?.max(0.0);
            let alpha = (dyn_to_f64(&alpha)? as f32).clamp(0.0, 1.0);
            let diff = (dyn_to_f64(&diff)? as f32).clamp(0.0, 1.0);
            with_ctx(|c| {
                c.commands.push(ScriptCommand::SetCharTrail {
                    target: target.to_string(),
                    length,
                    delay_s,
                    alpha,
                    diff,
                    z_layer: None,
                });
            })
        },
    );
    engine.register_fn(
        "char_trail",
        |target: &str,
         length: Dynamic,
         delay_s: Dynamic,
         alpha: Dynamic,
         diff: Dynamic,
         z_layer: Dynamic|
         -> Result<(), Box<EvalAltResult>> {
            let length = dyn_to_usize(&length)?.clamp(1, 32);
            let delay_s = dyn_to_f64(&delay_s)?.max(0.0);
            let alpha = (dyn_to_f64(&alpha)? as f32).clamp(0.0, 1.0);
            let diff = (dyn_to_f64(&diff)? as f32).clamp(0.0, 1.0);
            let z_layer = dyn_to_f64(&z_layer)?.round() as i32;
            with_ctx(|c| {
                c.commands.push(ScriptCommand::SetCharTrail {
                    target: target.to_string(),
                    length,
                    delay_s,
                    alpha,
                    diff,
                    z_layer: Some(z_layer),
                });
            })
        },
    );
    engine.register_fn("clear_char_trail", |target: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::ClearCharTrail {
                target: target.to_string(),
            });
        })
    });
    engine.register_fn("char_underlay", |target: &str, char_id: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetCharUnderlay {
                target: target.to_string(),
                char_id: char_id.to_string(),
                offset: [0.0, 0.0],
                z_offset: -1,
            });
        })
    });
    engine.register_fn(
        "char_underlay",
        |target: &str, char_id: &str, dx: Dynamic, dy: Dynamic| -> Result<(), Box<EvalAltResult>> {
            let offset = [dyn_to_f64(&dx)? as f32, dyn_to_f64(&dy)? as f32];
            with_ctx(|c| {
                c.commands.push(ScriptCommand::SetCharUnderlay {
                    target: target.to_string(),
                    char_id: char_id.to_string(),
                    offset,
                    z_offset: -1,
                });
            })
        },
    );
    engine.register_fn(
        "char_underlay",
        |target: &str,
         char_id: &str,
         dx: Dynamic,
         dy: Dynamic,
         z_offset: Dynamic|
         -> Result<(), Box<EvalAltResult>> {
            let offset = [dyn_to_f64(&dx)? as f32, dyn_to_f64(&dy)? as f32];
            let z_offset = dyn_to_f64(&z_offset)?.round() as i32;
            with_ctx(|c| {
                c.commands.push(ScriptCommand::SetCharUnderlay {
                    target: target.to_string(),
                    char_id: char_id.to_string(),
                    offset,
                    z_offset,
                });
            })
        },
    );
    engine.register_fn("clear_char_underlay", |target: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::ClearCharUnderlay {
                target: target.to_string(),
            });
        })
    });
}
