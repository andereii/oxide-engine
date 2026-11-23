use rhai::{Array, Dynamic, Engine, EvalAltResult, Map};

use crate::animations::BaseAnimationData;
use crate::scripting::api::{
    ScriptCommand, ScriptPropAnimKind, TweenProp, parse_hex_rgba,
};
use crate::scripting::backend::context::{dyn_to_f64, dyn_to_usize, runtime_err, with_ctx};

fn push_spawn_prop(
    id: &str,
    image_path: Option<String>,
    color: [f32; 4],
    size: [f32; 2],
    x: f32,
    y: f32,
    z: f32,
    hud: bool,
) -> Result<(), Box<EvalAltResult>> {
    with_ctx(|c| {
        c.snapshot.prop_positions.insert(id.to_string(), [x, y]);
        c.snapshot.prop_scales.insert(id.to_string(), [1.0, 1.0]);
        c.snapshot.prop_angles.insert(id.to_string(), 0.0);
        c.snapshot.prop_alphas.insert(id.to_string(), color[3]);
        c.snapshot.prop_z.insert(id.to_string(), z);
        c.snapshot.prop_scrolls.insert(id.to_string(), if hud { [0.0, 0.0] } else { [1.0, 1.0] });
        c.snapshot.prop_flips.insert(id.to_string(), [false, false]);
        c.snapshot.prop_vis.insert(id.to_string(), true);
        c.commands.push(ScriptCommand::SpawnProp {
            id: id.to_string(),
            image_path,
            color,
            size,
            pos: [x, y],
            z,
            hud,
        });
    })
}

fn push_spawn_anim_prop(
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
) -> Result<(), Box<EvalAltResult>> {
    with_ctx(|c| {
        c.snapshot.prop_positions.insert(id.to_string(), pos);
        c.snapshot.prop_scales.insert(id.to_string(), [1.0, 1.0]);
        c.snapshot.prop_angles.insert(id.to_string(), 0.0);
        c.snapshot.prop_alphas.insert(id.to_string(), color[3]);
        c.snapshot.prop_z.insert(id.to_string(), z);
        c.snapshot.prop_scrolls.insert(id.to_string(), if hud { [0.0, 0.0] } else { [1.0, 1.0] });
        c.snapshot.prop_flips.insert(id.to_string(), [false, false]);
        c.snapshot.prop_vis.insert(id.to_string(), true);
        c.commands.push(ScriptCommand::SpawnAnimatedProp {
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
    })
}

pub fn register_props_api(engine: &mut Engine) {
    engine.register_fn("spawn_sprite", |id: &str, path: &str, x: Dynamic, y: Dynamic, z: Dynamic| -> Result<(), Box<EvalAltResult>> {
        push_spawn_prop(id, Some(path.to_string()), [1.0; 4], [0.0, 0.0], dyn_to_f64(&x)? as f32, dyn_to_f64(&y)? as f32, dyn_to_f64(&z)? as f32, false)
    });
    engine.register_fn("spawn_hud_sprite", |id: &str, path: &str, x: Dynamic, y: Dynamic, z: Dynamic| -> Result<(), Box<EvalAltResult>> {
        push_spawn_prop(id, Some(path.to_string()), [1.0; 4], [0.0, 0.0], dyn_to_f64(&x)? as f32, dyn_to_f64(&y)? as f32, dyn_to_f64(&z)? as f32, true)
    });
    engine.register_fn("spawn_rect", |id: &str, w: Dynamic, h: Dynamic, hex: &str, x: Dynamic, y: Dynamic, z: Dynamic| -> Result<(), Box<EvalAltResult>> {
        push_spawn_prop(id, None, parse_hex_rgba(hex), [dyn_to_f64(&w)? as f32, dyn_to_f64(&h)? as f32], dyn_to_f64(&x)? as f32, dyn_to_f64(&y)? as f32, dyn_to_f64(&z)? as f32, false)
    });
    engine.register_fn("spawn_hud_rect", |id: &str, w: Dynamic, h: Dynamic, hex: &str, x: Dynamic, y: Dynamic, z: Dynamic| -> Result<(), Box<EvalAltResult>> {
        push_spawn_prop(id, None, parse_hex_rgba(hex), [dyn_to_f64(&w)? as f32, dyn_to_f64(&h)? as f32], dyn_to_f64(&x)? as f32, dyn_to_f64(&y)? as f32, dyn_to_f64(&z)? as f32, true)
    });
    engine.register_fn("spawn_sprite_tl", |id: &str, path: &str, x: Dynamic, y: Dynamic, z: Dynamic| -> Result<(), Box<EvalAltResult>> {
        push_spawn_anim_prop(id, path.to_string(), ScriptPropAnimKind::Static, Vec::new(), [1.0; 4], [0.0, 0.0], [dyn_to_f64(&x)? as f32, dyn_to_f64(&y)? as f32], dyn_to_f64(&z)? as f32, false, true)
    });
    engine.register_fn("spawn_rect_tl", |id: &str, w: Dynamic, h: Dynamic, hex: &str, x: Dynamic, y: Dynamic, z: Dynamic| -> Result<(), Box<EvalAltResult>> {
        push_spawn_anim_prop(id, String::new(), ScriptPropAnimKind::Rect, Vec::new(), parse_hex_rgba(hex), [dyn_to_f64(&w)? as f32, dyn_to_f64(&h)? as f32], [dyn_to_f64(&x)? as f32, dyn_to_f64(&y)? as f32], dyn_to_f64(&z)? as f32, false, true)
    });
    engine.register_fn("spawn_sparrow", |id: &str, path: &str, x: Dynamic, y: Dynamic, z: Dynamic, anims: Array| -> Result<(), Box<EvalAltResult>> {
        push_spawn_anim_prop(id, path.to_string(), ScriptPropAnimKind::Sparrow, parse_script_anims(anims)?, [1.0; 4], [0.0, 0.0], [dyn_to_f64(&x)? as f32, dyn_to_f64(&y)? as f32], dyn_to_f64(&z)? as f32, false, true)
    });
    engine.register_fn("spawn_sparrow", |id: &str, path: &str, x: Dynamic, y: Dynamic, z: Dynamic, anims: Array, hud: bool| -> Result<(), Box<EvalAltResult>> {
        push_spawn_anim_prop(id, path.to_string(), ScriptPropAnimKind::Sparrow, parse_script_anims(anims)?, [1.0; 4], [0.0, 0.0], [dyn_to_f64(&x)? as f32, dyn_to_f64(&y)? as f32], dyn_to_f64(&z)? as f32, hud, true)
    });
    engine.register_fn("spawn_atlas", |id: &str, path: &str, x: Dynamic, y: Dynamic, z: Dynamic, anims: Array| -> Result<(), Box<EvalAltResult>> {
        push_spawn_anim_prop(id, path.to_string(), ScriptPropAnimKind::Atlas, parse_script_anims(anims)?, [1.0; 4], [0.0, 0.0], [dyn_to_f64(&x)? as f32, dyn_to_f64(&y)? as f32], dyn_to_f64(&z)? as f32, false, true)
    });
    engine.register_fn("spawn_atlas", |id: &str, path: &str, x: Dynamic, y: Dynamic, z: Dynamic, anims: Array, hud: bool| -> Result<(), Box<EvalAltResult>> {
        push_spawn_anim_prop(id, path.to_string(), ScriptPropAnimKind::Atlas, parse_script_anims(anims)?, [1.0; 4], [0.0, 0.0], [dyn_to_f64(&x)? as f32, dyn_to_f64(&y)? as f32], dyn_to_f64(&z)? as f32, hud, true)
    });
    engine.register_fn("despawn", |id: &str| {
        with_ctx(|c| {
            c.snapshot.prop_positions.remove(id);
            c.snapshot.prop_vis.remove(id);
            c.commands.push(ScriptCommand::DespawnProp(id.to_string()));
        })
    });
    engine.register_fn("add_anim", |id: &str, entry: Map| -> Result<(), Box<EvalAltResult>> {
        let anim = parse_single_script_anim(&entry)?;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::AddPropAnim {
                id: id.to_string(),
                anim,
            });
        })
    });

    engine.register_fn("prop_exists", |id: &str| with_ctx(|c| c.snapshot.prop_positions.contains_key(id)));
    engine.register_fn("get_prop", |id: &str, prop: &str| -> Result<f64, Box<EvalAltResult>> {
        let p = TweenProp::parse(prop)
            .ok_or_else(|| runtime_err(format!("unknown prop field '{prop}'")))?;
        with_ctx(|c| match p {
            TweenProp::X => f64::from(c.snapshot.prop_positions.get(id).map_or(0.0, |v| v[0])),
            TweenProp::Y => f64::from(c.snapshot.prop_positions.get(id).map_or(0.0, |v| v[1])),
            TweenProp::Z => f64::from(c.snapshot.prop_z.get(id).copied().unwrap_or(0.0)),
            TweenProp::VelX => f64::from(c.snapshot.prop_velocities.get(id).map_or(0.0, |v| v[0])),
            TweenProp::VelY => f64::from(c.snapshot.prop_velocities.get(id).map_or(0.0, |v| v[1])),
            TweenProp::ScaleX => f64::from(c.snapshot.prop_scales.get(id).map_or(1.0, |v| v[0])),
            TweenProp::ScaleY => f64::from(c.snapshot.prop_scales.get(id).map_or(1.0, |v| v[1])),
            TweenProp::Angle => f64::from(c.snapshot.prop_angles.get(id).copied().unwrap_or(0.0)),
            TweenProp::Alpha => f64::from(c.snapshot.prop_alphas.get(id).copied().unwrap_or(1.0)),
            _ => 0.0,
        })
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
                TweenProp::Z => {
                    c.snapshot.prop_z.insert(id.to_string(), v);
                }
                TweenProp::VelX => {
                    let vy = c.snapshot.prop_velocities.get(id).map_or(0.0, |vel| vel[1]);
                    c.snapshot.prop_velocities.insert(id.to_string(), [v, vy]);
                }
                TweenProp::VelY => {
                    let vx = c.snapshot.prop_velocities.get(id).map_or(0.0, |vel| vel[0]);
                    c.snapshot.prop_velocities.insert(id.to_string(), [vx, v]);
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

    engine.register_fn("get_prop_pos", |id: &str| {
        with_ctx(|c| {
            let p = c.snapshot.prop_positions.get(id).copied().unwrap_or([0.0, 0.0]);
            vec![Dynamic::from(f64::from(p[0])), Dynamic::from(f64::from(p[1]))]
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
    engine.register_fn("get_prop_scale", |id: &str| {
        with_ctx(|c| {
            let s = c.snapshot.prop_scales.get(id).copied().unwrap_or([1.0, 1.0]);
            vec![Dynamic::from(f64::from(s[0])), Dynamic::from(f64::from(s[1]))]
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
    engine.register_fn("prop_size", |id: &str| {
        with_ctx(|c| {
            let s = c.snapshot.prop_sizes.get(id).copied().unwrap_or([0.0, 0.0]);
            vec![Dynamic::from(f64::from(s[0])), Dynamic::from(f64::from(s[1]))]
        })
    });
    engine.register_fn("set_prop_size", |id: &str, w: Dynamic, h: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let w = dyn_to_f64(&w)? as f32;
        let h = dyn_to_f64(&h)? as f32;
        with_ctx(|c| {
            c.snapshot.prop_sizes.insert(id.to_string(), [w, h]);
            c.commands.push(ScriptCommand::SetPropSize {
                id: id.to_string(),
                w,
                h,
            });
        })
    });
    engine.register_fn("get_prop_z", |id: &str| {
        with_ctx(|c| f64::from(c.snapshot.prop_z.get(id).copied().unwrap_or(0.0)))
    });
    engine.register_fn("prop_z", |id: &str, z: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let z = dyn_to_f64(&z)? as f32;
        with_ctx(|c| {
            c.snapshot.prop_z.insert(id.to_string(), z);
            c.commands.push(ScriptCommand::SetPropZ { id: id.to_string(), z });
        })
    });
    engine.register_fn("get_prop_scroll", |id: &str| {
        with_ctx(|c| {
            let s = c.snapshot.prop_scrolls.get(id).copied().unwrap_or([1.0, 1.0]);
            vec![Dynamic::from(f64::from(s[0])), Dynamic::from(f64::from(s[1]))]
        })
    });
    engine.register_fn("prop_scroll", |id: &str, sx: Dynamic, sy: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let sx = dyn_to_f64(&sx)? as f32;
        let sy = dyn_to_f64(&sy)? as f32;
        with_ctx(|c| {
            c.snapshot.prop_scrolls.insert(id.to_string(), [sx, sy]);
            c.commands.push(ScriptCommand::SetPropScroll { id: id.to_string(), sx, sy });
        })
    });
    engine.register_fn("get_prop_flip", |id: &str| {
        with_ctx(|c| {
            let f = c.snapshot.prop_flips.get(id).copied().unwrap_or([false, false]);
            vec![Dynamic::from(f[0]), Dynamic::from(f[1])]
        })
    });
    engine.register_fn("prop_flip", |id: &str, flip_x: bool, flip_y: bool| {
        with_ctx(|c| {
            c.snapshot.prop_flips.insert(id.to_string(), [flip_x, flip_y]);
            c.commands.push(ScriptCommand::SetPropFlip {
                id: id.to_string(),
                flip_x,
                flip_y,
            });
        })
    });
    engine.register_fn("prop_vel", |id: &str, vx: Dynamic, vy: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let vx = dyn_to_f64(&vx)? as f32;
        let vy = dyn_to_f64(&vy)? as f32;
        with_ctx(|c| {
            c.snapshot.prop_velocities.insert(id.to_string(), [vx, vy]);
            c.commands.push(ScriptCommand::SetPropVelocity {
                id: id.to_string(),
                vx,
                vy,
                ax: 0.0,
                ay: 0.0,
            });
        })
    });
    engine.register_fn("prop_vel", |id: &str, vx: Dynamic, vy: Dynamic, ax: Dynamic, ay: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let vx = dyn_to_f64(&vx)? as f32;
        let vy = dyn_to_f64(&vy)? as f32;
        let ax = dyn_to_f64(&ax)? as f32;
        let ay = dyn_to_f64(&ay)? as f32;
        with_ctx(|c| {
            c.snapshot.prop_velocities.insert(id.to_string(), [vx, vy]);
            c.commands.push(ScriptCommand::SetPropVelocity {
                id: id.to_string(),
                vx,
                vy,
                ax,
                ay,
            });
        })
    });
    engine.register_fn("get_prop_angle", |id: &str| {
        with_ctx(|c| f64::from(c.snapshot.prop_angles.get(id).copied().unwrap_or(0.0)))
    });
    engine.register_fn("prop_angle", |id: &str, deg: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let deg = dyn_to_f64(&deg)? as f32;
        with_ctx(|c| {
            c.snapshot.prop_angles.insert(id.to_string(), deg);
            c.commands.push(ScriptCommand::SetPropNumber { id: id.to_string(), prop: TweenProp::Angle, value: deg });
        })
    });
    engine.register_fn("get_prop_alpha", |id: &str| {
        with_ctx(|c| f64::from(c.snapshot.prop_alphas.get(id).copied().unwrap_or(1.0)))
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
    for name in ["prop_visible", "prop_vis"] {
        engine.register_fn(name, |id: &str, visible: bool| {
            with_ctx(|c| {
                c.snapshot.prop_vis.insert(id.to_string(), visible);
                c.commands.push(ScriptCommand::SetPropVisible {
                    id: id.to_string(),
                    visible,
                });
            })
        });
    }
    engine.register_fn("is_prop_vis", |id: &str| {
        with_ctx(|c| c.snapshot.prop_vis.get(id).copied().unwrap_or(true))
    });
    engine.register_fn("prop_anim", |id: &str| {
        with_ctx(|c| c.snapshot.prop_anims.get(id).cloned().unwrap_or_default())
    });
    engine.register_fn("prop_anim_done", |id: &str| {
        with_ctx(|c| c.snapshot.prop_anim_done.get(id).copied().unwrap_or(false))
    });
    engine.register_fn("prop_repeat", |id: &str, rx: bool, ry: bool| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetPropRepeat {
                id: id.to_string(),
                repeat_x: rx,
                repeat_y: ry,
            });
        })
    });
    engine.register_fn(
        "prop_matrix",
        |id: &str,
         a: Dynamic,
         b: Dynamic,
         c_arg: Dynamic,
         d: Dynamic,
         tx: Dynamic,
         ty: Dynamic|
         -> Result<(), Box<EvalAltResult>> {
            let matrix = [
                dyn_to_f64(&a)? as f32,
                dyn_to_f64(&b)? as f32,
                dyn_to_f64(&c_arg)? as f32,
                dyn_to_f64(&d)? as f32,
                dyn_to_f64(&tx)? as f32,
                dyn_to_f64(&ty)? as f32,
            ];
            with_ctx(|ctx| {
                ctx.commands.push(ScriptCommand::SetPropAffine {
                    id: id.to_string(),
                    matrix: Some(matrix),
                });
            })
        },
    );
    engine.register_fn("prop_matrix_clear", |id: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetPropAffine {
                id: id.to_string(),
                matrix: None,
            });
        })
    });
    engine.register_fn("prop_tex", |id: &str, path: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetPropTexture {
                id: id.to_string(),
                path: path.to_string(),
            });
        })
    });
    for name in ["prop_overlay", "text_overlay"] {
        engine.register_fn(name, |id: &str, overlay: bool| {
            with_ctx(|c| {
                c.commands.push(ScriptCommand::SetPropOverlay {
                    id: id.to_string(),
                    overlay,
                });
            })
        });
    }
}

fn parse_single_script_anim(map: &Map) -> Result<BaseAnimationData, Box<EvalAltResult>> {
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
    Ok(data)
}

fn parse_script_anims(anims: Array) -> Result<Vec<BaseAnimationData>, Box<EvalAltResult>> {
    let mut out = Vec::with_capacity(anims.len());
    for item in anims {
        let Some(map) = item.try_cast::<Map>() else {
            return Err(runtime_err("animation entry must be an object map"));
        };
        out.push(parse_single_script_anim(&map)?);
    }
    Ok(out)
}
