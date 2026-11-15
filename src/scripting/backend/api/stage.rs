use rhai::{Array, Dynamic, Engine, EvalAltResult, Map};

use crate::animations::BaseAnimationData;
use crate::scripting::api::{
    NewTimerRequest, ScriptCommand, ScriptPropAnimKind, TweenProp, TweenTarget, parse_hex_rgba,
};
use crate::scripting::value::ScriptValue;

use crate::scripting::backend::context::{dyn_to_f64, dyn_to_i64, dyn_to_usize, runtime_err, with_ctx};

pub fn register_character_and_modchart_api(engine: &mut Engine) {
    engine.register_fn("char_id", |role: &str| {
        with_ctx(|c| match role.trim().to_ascii_lowercase().as_str() {
            "bf" | "boyfriend" | "player" => c.snapshot.bf_id.clone(),
            "dad" | "opponent" => c.snapshot.dad_id.clone(),
            "gf" | "girlfriend" | "spectator" => c.snapshot.gf_id.clone(),
            _ => String::new(),
        })
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
    engine.register_fn("char_anim", |target: &str| {
        with_ctx(|c| c.snapshot.char_anims.get(target).cloned().unwrap_or_default())
    });
    engine.register_fn("char_anim_finished", |target: &str| {
        with_ctx(|c| {
            c.snapshot
                .char_anim_finished
                .get(target)
                .copied()
                .unwrap_or(false)
        })
    });
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
    engine.register_fn("char_z", |target: &str| {
        with_ctx(|c| f64::from(c.snapshot.char_z.get(target).copied().unwrap_or(-500.0)))
    });
    engine.register_fn("set_char_pos", |target: &str, x: Dynamic, y: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let x = dyn_to_f64(&x)? as f32;
        let y = dyn_to_f64(&y)? as f32;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetCharacterPos {
                target: target.to_string(),
                x,
                y,
            });
        })
    });
    engine.register_fn("char_scale", |target: &str, sx: Dynamic, sy: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let sx = dyn_to_f64(&sx)? as f32;
        let sy = dyn_to_f64(&sy)? as f32;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetCharacterScale {
                target: target.to_string(),
                sx,
                sy,
            });
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
    engine.register_fn("char_alpha", |target: &str, alpha: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let alpha = dyn_to_f64(&alpha)? as f32;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetCharacterAlpha {
                target: target.to_string(),
                alpha,
            });
        })
    });
    engine.register_fn("char_visible", |target: &str, visible: bool| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetCharacterVisible {
                target: target.to_string(),
                visible,
            });
        })
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

    // Modcharting lane helpers (lanes 0..3 opponent, 4..7 player).
    engine.register_fn("lane_x", |lane: Dynamic, dx: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let l = dyn_to_usize(&lane)?;
        let dx = dyn_to_f64(&dx)? as f32;
        with_ctx(|c| {
            let cur_y = c.snapshot.lane_offsets.get(&l).map_or(0.0, |p| p[1]);
            c.snapshot.lane_offsets.insert(l, [dx, cur_y]);
            c.commands.push(ScriptCommand::SetLaneOffset { lane: l, x: dx, y: cur_y });
        })
    });
    engine.register_fn("lane_y", |lane: Dynamic, dy: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let l = dyn_to_usize(&lane)?;
        let dy = dyn_to_f64(&dy)? as f32;
        with_ctx(|c| {
            let cur_x = c.snapshot.lane_offsets.get(&l).map_or(0.0, |p| p[0]);
            c.snapshot.lane_offsets.insert(l, [cur_x, dy]);
            c.commands.push(ScriptCommand::SetLaneOffset { lane: l, x: cur_x, y: dy });
        })
    });
    engine.register_fn("lane_pos", |lane: Dynamic, dx: Dynamic, dy: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let l = dyn_to_usize(&lane)?;
        let dx = dyn_to_f64(&dx)? as f32;
        let dy = dyn_to_f64(&dy)? as f32;
        with_ctx(|c| {
            c.snapshot.lane_offsets.insert(l, [dx, dy]);
            c.commands.push(ScriptCommand::SetLaneOffset { lane: l, x: dx, y: dy });
        })
    });
    engine.register_fn("lane_angle", |lane: Dynamic, deg: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let l = dyn_to_usize(&lane)?;
        let deg = dyn_to_f64(&deg)? as f32;
        with_ctx(|c| {
            c.snapshot.lane_angles.insert(l, deg);
            c.commands.push(ScriptCommand::SetLaneAngle { lane: l, deg });
        })
    });
    engine.register_fn("lane_scale", |lane: Dynamic, sx: Dynamic, sy: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let l = dyn_to_usize(&lane)?;
        let sx = dyn_to_f64(&sx)? as f32;
        let sy = dyn_to_f64(&sy)? as f32;
        with_ctx(|c| {
            c.snapshot.lane_scales.insert(l, [sx, sy]);
            c.commands.push(ScriptCommand::SetLaneScale { lane: l, sx, sy });
        })
    });
    engine.register_fn("lane_alpha", |lane: Dynamic, alpha: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let l = dyn_to_usize(&lane)?;
        let alpha = (dyn_to_f64(&alpha)? as f32).clamp(0.0, 1.0);
        with_ctx(|c| {
            c.snapshot.lane_alphas.insert(l, alpha);
            c.commands.push(ScriptCommand::SetLaneAlpha { lane: l, alpha });
        })
    });
    engine.register_fn("reset_lanes", || {
        with_ctx(|c| {
            c.snapshot.lane_offsets.clear();
            c.snapshot.lane_angles.clear();
            c.snapshot.lane_scales.clear();
            c.snapshot.lane_alphas.clear();
            c.commands.push(ScriptCommand::ResetLanes);
        })
    });
    engine.register_fn("tween_lane", |lane: Dynamic, prop: &str, to: Dynamic, secs: Dynamic, ease: &str| -> Result<(), Box<EvalAltResult>> {
        let l = dyn_to_usize(&lane)?;
        let target_val = dyn_to_f64(&to)?;
        let dur = dyn_to_f64(&secs)?;
        let p = TweenProp::parse(prop)
            .ok_or_else(|| runtime_err(format!("unknown tween property '{prop}'")))?;
        with_ctx(|c| c.push_tween(TweenTarget::Lane(l), p, target_val, dur, ease))
    });
    engine.register_fn("tween_lane", |lane: Dynamic, prop: &str, to: Dynamic, secs: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let l = dyn_to_usize(&lane)?;
        let target_val = dyn_to_f64(&to)?;
        let dur = dyn_to_f64(&secs)?;
        let p = TweenProp::parse(prop)
            .ok_or_else(|| runtime_err(format!("unknown tween property '{prop}'")))?;
        with_ctx(|c| c.push_tween(TweenTarget::Lane(l), p, target_val, dur, "linear"))
    });
}

pub fn register_props_tweens_timers_and_store_api(engine: &mut Engine) {
    engine.register_fn("spawn_sprite", |id: &str, path: &str, x: Dynamic, y: Dynamic, z: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let x = dyn_to_f64(&x)? as f32;
        let y = dyn_to_f64(&y)? as f32;
        let z = dyn_to_f64(&z)? as f32;
        with_ctx(|c| {
            c.snapshot.prop_positions.insert(id.to_string(), [x, y]);
            c.snapshot.prop_scales.insert(id.to_string(), [1.0, 1.0]);
            c.snapshot.prop_angles.insert(id.to_string(), 0.0);
            c.snapshot.prop_alphas.insert(id.to_string(), 1.0);
            c.commands.push(ScriptCommand::SpawnProp {
                id: id.to_string(),
                image_path: Some(path.to_string()),
                color: [1.0, 1.0, 1.0, 1.0],
                size: [0.0, 0.0],
                pos: [x, y],
                z,
                hud: false,
            });
        })
    });
    engine.register_fn("spawn_hud_sprite", |id: &str, path: &str, x: Dynamic, y: Dynamic, z: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let x = dyn_to_f64(&x)? as f32;
        let y = dyn_to_f64(&y)? as f32;
        let z = dyn_to_f64(&z)? as f32;
        with_ctx(|c| {
            c.snapshot.prop_positions.insert(id.to_string(), [x, y]);
            c.snapshot.prop_scales.insert(id.to_string(), [1.0, 1.0]);
            c.snapshot.prop_angles.insert(id.to_string(), 0.0);
            c.snapshot.prop_alphas.insert(id.to_string(), 1.0);
            c.commands.push(ScriptCommand::SpawnProp {
                id: id.to_string(),
                image_path: Some(path.to_string()),
                color: [1.0, 1.0, 1.0, 1.0],
                size: [0.0, 0.0],
                pos: [x, y],
                z,
                hud: true,
            });
        })
    });
    engine.register_fn("spawn_rect", |id: &str, w: Dynamic, h: Dynamic, hex: &str, x: Dynamic, y: Dynamic, z: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let w = dyn_to_f64(&w)? as f32;
        let h = dyn_to_f64(&h)? as f32;
        let x = dyn_to_f64(&x)? as f32;
        let y = dyn_to_f64(&y)? as f32;
        let z = dyn_to_f64(&z)? as f32;
        with_ctx(|c| {
            c.snapshot.prop_positions.insert(id.to_string(), [x, y]);
            c.snapshot.prop_scales.insert(id.to_string(), [1.0, 1.0]);
            c.snapshot.prop_angles.insert(id.to_string(), 0.0);
            c.snapshot.prop_alphas.insert(id.to_string(), 1.0);
            c.commands.push(ScriptCommand::SpawnProp {
                id: id.to_string(),
                image_path: None,
                color: parse_hex_rgba(hex),
                size: [w, h],
                pos: [x, y],
                z,
                hud: false,
            });
        })
    });
    engine.register_fn("spawn_hud_rect", |id: &str, w: Dynamic, h: Dynamic, hex: &str, x: Dynamic, y: Dynamic, z: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let w = dyn_to_f64(&w)? as f32;
        let h = dyn_to_f64(&h)? as f32;
        let x = dyn_to_f64(&x)? as f32;
        let y = dyn_to_f64(&y)? as f32;
        let z = dyn_to_f64(&z)? as f32;
        with_ctx(|c| {
            c.snapshot.prop_positions.insert(id.to_string(), [x, y]);
            c.snapshot.prop_scales.insert(id.to_string(), [1.0, 1.0]);
            c.snapshot.prop_angles.insert(id.to_string(), 0.0);
            c.snapshot.prop_alphas.insert(id.to_string(), 1.0);
            c.commands.push(ScriptCommand::SpawnProp {
                id: id.to_string(),
                image_path: None,
                color: parse_hex_rgba(hex),
                size: [w, h],
                pos: [x, y],
                z,
                hud: true,
            });
        })
    });
    engine.register_fn("spawn_sprite_tl", |id: &str, path: &str, x: Dynamic, y: Dynamic, z: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let x = dyn_to_f64(&x)? as f32;
        let y = dyn_to_f64(&y)? as f32;
        let z = dyn_to_f64(&z)? as f32;
        with_ctx(|c| {
            c.snapshot.prop_positions.insert(id.to_string(), [x, y]);
            c.snapshot.prop_scales.insert(id.to_string(), [1.0, 1.0]);
            c.snapshot.prop_angles.insert(id.to_string(), 0.0);
            c.snapshot.prop_alphas.insert(id.to_string(), 1.0);
            c.commands.push(ScriptCommand::SpawnAnimatedProp {
                id: id.to_string(),
                path: path.to_string(),
                kind: ScriptPropAnimKind::Static,
                anims: Vec::new(),
                color: [1.0, 1.0, 1.0, 1.0],
                size: [0.0, 0.0],
                pos: [x, y],
                z,
                hud: false,
                top_left: true,
            });
        })
    });
    engine.register_fn("spawn_rect_tl", |id: &str, w: Dynamic, h: Dynamic, hex: &str, x: Dynamic, y: Dynamic, z: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let w = dyn_to_f64(&w)? as f32;
        let h = dyn_to_f64(&h)? as f32;
        let x = dyn_to_f64(&x)? as f32;
        let y = dyn_to_f64(&y)? as f32;
        let z = dyn_to_f64(&z)? as f32;
        with_ctx(|c| {
            c.snapshot.prop_positions.insert(id.to_string(), [x, y]);
            c.snapshot.prop_scales.insert(id.to_string(), [1.0, 1.0]);
            c.snapshot.prop_angles.insert(id.to_string(), 0.0);
            c.snapshot.prop_alphas.insert(id.to_string(), 1.0);
            c.commands.push(ScriptCommand::SpawnAnimatedProp {
                id: id.to_string(),
                path: String::new(),
                kind: ScriptPropAnimKind::Rect,
                anims: Vec::new(),
                color: parse_hex_rgba(hex),
                size: [w, h],
                pos: [x, y],
                z,
                hud: false,
                top_left: true,
            });
        })
    });
    engine.register_fn("spawn_sparrow", |id: &str, path: &str, x: Dynamic, y: Dynamic, z: Dynamic, anims: Array| -> Result<(), Box<EvalAltResult>> {
        let x = dyn_to_f64(&x)? as f32;
        let y = dyn_to_f64(&y)? as f32;
        let z = dyn_to_f64(&z)? as f32;
        let anims = parse_script_anims(anims)?;
        with_ctx(|c| {
            c.snapshot.prop_positions.insert(id.to_string(), [x, y]);
            c.snapshot.prop_scales.insert(id.to_string(), [1.0, 1.0]);
            c.snapshot.prop_angles.insert(id.to_string(), 0.0);
            c.snapshot.prop_alphas.insert(id.to_string(), 1.0);
            c.commands.push(ScriptCommand::SpawnAnimatedProp {
                id: id.to_string(),
                path: path.to_string(),
                kind: ScriptPropAnimKind::Sparrow,
                anims,
                color: [1.0, 1.0, 1.0, 1.0],
                size: [0.0, 0.0],
                pos: [x, y],
                z,
                hud: false,
                top_left: true,
            });
        });
        Ok(())
    });
    engine.register_fn("spawn_atlas", |id: &str, path: &str, x: Dynamic, y: Dynamic, z: Dynamic, anims: Array| -> Result<(), Box<EvalAltResult>> {
        let x = dyn_to_f64(&x)? as f32;
        let y = dyn_to_f64(&y)? as f32;
        let z = dyn_to_f64(&z)? as f32;
        let anims = parse_script_anims(anims)?;
        with_ctx(|c| {
            c.snapshot.prop_positions.insert(id.to_string(), [x, y]);
            c.snapshot.prop_scales.insert(id.to_string(), [1.0, 1.0]);
            c.snapshot.prop_angles.insert(id.to_string(), 0.0);
            c.snapshot.prop_alphas.insert(id.to_string(), 1.0);
            c.commands.push(ScriptCommand::SpawnAnimatedProp {
                id: id.to_string(),
                path: path.to_string(),
                kind: ScriptPropAnimKind::Atlas,
                anims,
                color: [1.0, 1.0, 1.0, 1.0],
                size: [0.0, 0.0],
                pos: [x, y],
                z,
                hud: false,
                top_left: true,
            });
        });
        Ok(())
    });
    engine.register_fn("despawn", |id: &str| {
        with_ctx(|c| c.commands.push(ScriptCommand::DespawnProp(id.to_string())))
    });

    engine.register_fn("set_prop", |id: &str, prop: &str, val: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let v = dyn_to_f64(&val)? as f32;
        let p = TweenProp::parse(prop)
            .ok_or_else(|| runtime_err(format!("unknown prop field '{prop}'")))?;
        with_ctx(|c| {
            match p {
                TweenProp::X => {
                    let y = c.snapshot.prop_positions.get(id).map_or(0.0, |pos| pos[1]);
                    c.snapshot.prop_positions.insert(id.to_string(), [v, y]);
                }
                TweenProp::Y => {
                    let x = c.snapshot.prop_positions.get(id).map_or(0.0, |pos| pos[0]);
                    c.snapshot.prop_positions.insert(id.to_string(), [x, v]);
                }
                TweenProp::ScaleX => {
                    let sy = c.snapshot.prop_scales.get(id).map_or(1.0, |s| s[1]);
                    c.snapshot.prop_scales.insert(id.to_string(), [v, sy]);
                }
                TweenProp::ScaleY => {
                    let sx = c.snapshot.prop_scales.get(id).map_or(1.0, |s| s[0]);
                    c.snapshot.prop_scales.insert(id.to_string(), [sx, v]);
                }
                TweenProp::Angle => {
                    c.snapshot.prop_angles.insert(id.to_string(), v);
                }
                TweenProp::Alpha => {
                    c.snapshot.prop_alphas.insert(id.to_string(), v.clamp(0.0, 1.0));
                }
                _ => {}
            }
            c.commands.push(ScriptCommand::SetPropNumber {
                id: id.to_string(),
                prop: p,
                value: v,
            });
        })
    });
    engine.register_fn("prop_pos", |id: &str, x: Dynamic, y: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let x = dyn_to_f64(&x)? as f32;
        let y = dyn_to_f64(&y)? as f32;
        with_ctx(|c| {
            c.snapshot.prop_positions.insert(id.to_string(), [x, y]);
            c.commands.push(ScriptCommand::SetPropNumber { id: id.to_string(), prop: TweenProp::X, value: x });
            c.commands.push(ScriptCommand::SetPropNumber { id: id.to_string(), prop: TweenProp::Y, value: y });
        })
    });
    engine.register_fn("prop_scale", |id: &str, sx: Dynamic, sy: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let sx = dyn_to_f64(&sx)? as f32;
        let sy = dyn_to_f64(&sy)? as f32;
        with_ctx(|c| {
            c.snapshot.prop_scales.insert(id.to_string(), [sx, sy]);
            c.commands.push(ScriptCommand::SetPropNumber { id: id.to_string(), prop: TweenProp::ScaleX, value: sx });
            c.commands.push(ScriptCommand::SetPropNumber { id: id.to_string(), prop: TweenProp::ScaleY, value: sy });
        })
    });
    engine.register_fn("prop_angle", |id: &str, deg: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let deg = dyn_to_f64(&deg)? as f32;
        with_ctx(|c| {
            c.snapshot.prop_angles.insert(id.to_string(), deg);
            c.commands.push(ScriptCommand::SetPropNumber { id: id.to_string(), prop: TweenProp::Angle, value: deg });
        })
    });
    engine.register_fn("prop_alpha", |id: &str, a: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let a = (dyn_to_f64(&a)? as f32).clamp(0.0, 1.0);
        with_ctx(|c| {
            c.snapshot.prop_alphas.insert(id.to_string(), a);
            c.commands.push(ScriptCommand::SetPropNumber { id: id.to_string(), prop: TweenProp::Alpha, value: a });
        })
    });
    engine.register_fn("prop_tint", |id: &str, hex: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetPropTint {
                id: id.to_string(),
                color: parse_hex_rgba(hex),
            });
        })
    });
    engine.register_fn("prop_visible", |id: &str, visible: bool| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetPropVisible {
                id: id.to_string(),
                visible,
            });
        })
    });

    engine.register_fn("tween", |target: &str, prop: &str, to: Dynamic, secs: Dynamic, ease: &str| -> Result<(), Box<EvalAltResult>> {
        let t = TweenTarget::parse(target);
        let p = TweenProp::parse(prop)
            .ok_or_else(|| runtime_err(format!("unknown tween prop '{prop}'")))?;
        let end = dyn_to_f64(&to)?;
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| c.push_tween(t, p, end, dur, ease))
    });
    engine.register_fn("tween", |target: &str, prop: &str, to: Dynamic, secs: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let t = TweenTarget::parse(target);
        let p = TweenProp::parse(prop)
            .ok_or_else(|| runtime_err(format!("unknown tween prop '{prop}'")))?;
        let end = dyn_to_f64(&to)?;
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| c.push_tween(t, p, end, dur, "linear"))
    });
    engine.register_fn("tween_prop", |id: &str, prop: &str, to: Dynamic, secs: Dynamic, ease: &str| -> Result<(), Box<EvalAltResult>> {
        let p = TweenProp::parse(prop)
            .ok_or_else(|| runtime_err(format!("unknown tween prop '{prop}'")))?;
        let end = dyn_to_f64(&to)?;
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| c.push_tween(TweenTarget::Prop(id.to_string()), p, end, dur, ease))
    });
    engine.register_fn("cancel_tweens", |target: &str| {
        let t = TweenTarget::parse(target);
        with_ctx(|c| c.cancel_tween_targets.push(t))
    });

    engine.register_fn("after", |secs: Dynamic, fn_name: &str| -> Result<(), Box<EvalAltResult>> {
        let dur = dyn_to_f64(&secs)?.max(0.0);
        with_ctx(|c| {
            c.new_timers.push(NewTimerRequest {
                script_id: c.current_script_id.clone(),
                fn_name: fn_name.to_string(),
                interval_s: dur,
                repeats_left: Some(1),
            });
        })
    });
    engine.register_fn("every", |secs: Dynamic, fn_name: &str, count: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let dur = dyn_to_f64(&secs)?.max(0.001);
        let cnt = dyn_to_i64(&count)?;
        let repeats = if cnt <= 0 { None } else { Some(cnt as u32) };
        with_ctx(|c| {
            c.new_timers.push(NewTimerRequest {
                script_id: c.current_script_id.clone(),
                fn_name: fn_name.to_string(),
                interval_s: dur,
                repeats_left: repeats,
            });
        })
    });
    engine.register_fn("cancel_timer", |fn_name: &str| {
        with_ctx(|c| {
            c.cancel_timers_for_fn
                .push((c.current_script_id.clone(), fn_name.to_string()));
        })
    });

    // VFS + ModStore helpers
    engine.register_fn("file_exists", |rel: &str| with_ctx(|c| c.registry.file_exists(rel)));
    engine.register_fn("read_file", |rel: &str| -> Result<Dynamic, Box<EvalAltResult>> {
        with_ctx(|c| match c.registry.read_to_string(rel) {
            Ok(txt) => Dynamic::from(txt),
            Err(_) => Dynamic::UNIT,
        })
    });
    engine.register_fn("read_json", |rel: &str| -> Result<Dynamic, Box<EvalAltResult>> {
        with_ctx(|c| {
            let Ok(txt) = c.registry.read_to_string(rel) else {
                return Dynamic::UNIT;
            };
            let Ok(json) = serde_json::from_str::<serde_json::Value>(&txt) else {
                return Dynamic::UNIT;
            };
            ScriptValue::from_json(&json).into_rhai()
        })
    });

    engine.register_fn("save_has", |key: &str| {
        with_ctx(|c| c.store.has(&c.current_mod_id.clone(), key))
    });
    engine.register_fn("save_get", |key: &str| {
        with_ctx(|c| {
            let ns = c.current_mod_id.clone();
            c.store
                .get(&ns, key)
                .map(|v| ScriptValue::from_json(v).into_rhai())
                .unwrap_or(Dynamic::UNIT)
        })
    });
    engine.register_fn("save_set", |key: &str, val: Dynamic| {
        let json = ScriptValue::from_rhai(&val).to_json();
        with_ctx(|c| {
            let ns = c.current_mod_id.clone();
            c.store.set(&ns, key, json);
        })
    });
    engine.register_fn("save_flush", || -> Result<bool, Box<EvalAltResult>> {
        with_ctx(|c| {
            let ns = c.current_mod_id.clone();
            c.store.save_namespace(&ns).is_ok()
        })
    });
    engine.register_fn("global_get", |key: &str| {
        with_ctx(|c| {
            c.store
                .get("global", key)
                .map(|v| ScriptValue::from_json(v).into_rhai())
                .unwrap_or(Dynamic::UNIT)
        })
    });
    engine.register_fn("global_set", |key: &str, val: Dynamic| {
        let json = ScriptValue::from_rhai(&val).to_json();
        with_ctx(|c| c.store.set("global", key, json))
    });
}

fn parse_script_anims(anims: Array) -> Result<Vec<BaseAnimationData>, Box<EvalAltResult>> {
    let mut out = Vec::with_capacity(anims.len());
    for item in anims {
        let Some(map) = item.try_cast::<Map>() else {
            return Err(runtime_err("animation entry must be an object map"));
        };
        let mut data = BaseAnimationData::default();
        if let Some(v) = map.get("name").and_then(|d| d.clone().into_string().ok()) {
            data.name = v;
        }
        if let Some(v) = map.get("prefix").and_then(|d| d.clone().into_string().ok()) {
            data.prefix = v;
        }
        if let Some(v) = map.get("fps") {
            data.frame_rate = dyn_to_f64(v)? as f32;
        }
        if let Some(v) = map.get("looped").and_then(|d| d.as_bool().ok()) {
            data.looped = v;
        }
        if let Some(v) = map.get("flip_x").and_then(|d| d.as_bool().ok()) {
            data.flip_x = Some(v);
        }
        if let Some(v) = map.get("flip_y").and_then(|d| d.as_bool().ok()) {
            data.flip_y = Some(v);
        }
        if let Some(arr) = map.get("offsets").and_then(|d| d.clone().try_cast::<Array>())
            && arr.len() >= 2
        {
            data.offsets = [dyn_to_f64(&arr[0])? as f32, dyn_to_f64(&arr[1])? as f32];
        }
        if let Some(arr) = map.get("indices").and_then(|d| d.clone().try_cast::<Array>()) {
            for idx in arr {
                data.frame_indices.push(dyn_to_usize(&idx)?);
            }
        } else if let (Some(s), Some(e)) = (map.get("start"), map.get("end")) {
            let start = dyn_to_usize(s)?;
            let end = dyn_to_usize(e)?;
            if start <= end {
                data.frame_indices.extend(start..=end);
            } else {
                data.frame_indices.extend((end..=start).rev());
            }
        }
        out.push(data);
    }
    Ok(out)
}

