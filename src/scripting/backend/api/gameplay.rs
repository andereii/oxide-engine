use rhai::{Dynamic, Engine, EvalAltResult};

use crate::scripting::api::{ScriptCommand, TweenProp, TweenTarget, parse_hex_rgba};
use crate::scripting::backend::context::{dyn_to_f64, dyn_to_usize, with_ctx};
use crate::scripting::value::ScriptValue;

pub fn register_gameplay_api(engine: &mut Engine) {
    engine.register_fn("hp", || with_ctx(|c| c.mutable.health));
    engine.register_fn("set_hp", |v: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let num = dyn_to_f64(&v)?;
        with_ctx(|c| {
            c.mutable.health = num;
            c.mutable.dirty_health = true;
        })
    });
    engine.register_fn("add_hp", |d: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let delta = dyn_to_f64(&d)?;
        with_ctx(|c| {
            c.mutable.health += delta;
            c.mutable.dirty_health = true;
        })
    });

    engine.register_fn("score", || with_ctx(|c| c.mutable.score));
    engine.register_fn("set_score", |v: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let num = dyn_to_f64(&v)?;
        with_ctx(|c| {
            c.mutable.score = num;
            c.mutable.dirty_score = true;
        })
    });
    engine.register_fn("add_score", |d: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let delta = dyn_to_f64(&d)?;
        with_ctx(|c| {
            c.mutable.score += delta;
            c.mutable.dirty_score = true;
        })
    });

    engine.register_fn("combo", || with_ctx(|c| i64::from(c.snapshot.combo)));
    engine.register_fn("misses", || with_ctx(|c| i64::from(c.snapshot.misses)));
    engine.register_fn("note_count", || with_ctx(|c| c.snapshot.note_count as i64));

    engine.register_fn("botplay", || with_ctx(|c| c.mutable.botplay));
    engine.register_fn("set_botplay", |enabled: bool| {
        with_ctx(|c| {
            c.mutable.botplay = enabled;
            c.mutable.dirty_botplay = true;
        })
    });

    engine.register_fn("scroll", || with_ctx(|c| c.mutable.scroll_speed));
    engine.register_fn("set_scroll", |v: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let speed = dyn_to_f64(&v)?;
        with_ctx(|c| {
            c.mutable.scroll_speed = speed;
            c.mutable.dirty_scroll_speed = true;
        })
    });
    engine.register_fn("tween_scroll", |v: Dynamic, secs: Dynamic, ease: &str| -> Result<(), Box<EvalAltResult>> {
        let target = dyn_to_f64(&v)?;
        let dur = dyn_to_f64(&secs)?;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::TweenScrollSpeed {
                target,
                duration_s: dur.max(0.0),
                ease: ease.to_string(),
            });
        })
    });

    engine.register_fn("rate", || with_ctx(|c| c.mutable.playback_rate));
    engine.register_fn("set_rate", |v: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let rate = dyn_to_f64(&v)?.max(0.05);
        with_ctx(|c| {
            c.mutable.playback_rate = rate;
            c.mutable.dirty_playback_rate = true;
        })
    });

    engine.register_fn("downscroll", || with_ctx(|c| c.snapshot.downscroll));
    engine.register_fn("middlescroll", || with_ctx(|c| c.mutable.middlescroll));
    engine.register_fn("set_middlescroll", |enabled: bool| {
        with_ctx(|c| {
            c.mutable.middlescroll = enabled;
            c.mutable.dirty_middlescroll = true;
        })
    });
    engine.register_fn("swap_strums", |enabled: bool| {
        with_ctx(|c| {
            c.mutable.strums_swapped = enabled;
            c.mutable.dirty_strums_swapped = true;
        })
    });

    engine.register_fn("status", |text: &str| {
        with_ctx(|c| c.commands.push(ScriptCommand::SetStatus(text.to_string())))
    });
    engine.register_fn("set_score_text", |text: &str| {
        with_ctx(|c| {
            let val = if text.is_empty() { None } else { Some(text.to_string()) };
            c.commands.push(ScriptCommand::SetScoreText(val));
        })
    });
    engine.register_fn("clear_score_text", || {
        with_ctx(|c| c.commands.push(ScriptCommand::SetScoreText(None)))
    });
    engine.register_fn("hud_visible", |visible: bool| {
        with_ctx(|c| c.commands.push(ScriptCommand::SetHudVisible(visible)))
    });

    engine.register_fn("inst_vol", |v: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let vol = dyn_to_f64(&v)? as f32;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetAudioVolumes {
                inst: Some(vol),
                vocals: None,
                player_vocal: None,
                opponent_vocal: None,
            });
        })
    });
    engine.register_fn("vocals_vol", |v: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let vol = dyn_to_f64(&v)? as f32;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetAudioVolumes {
                inst: None,
                vocals: Some(vol),
                player_vocal: None,
                opponent_vocal: None,
            });
        })
    });
    engine.register_fn("p_vocal_vol", |v: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let vol = dyn_to_f64(&v)? as f32;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetAudioVolumes {
                inst: None,
                vocals: None,
                player_vocal: Some(vol),
                opponent_vocal: None,
            });
        })
    });
    engine.register_fn("o_vocal_vol", |v: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let vol = dyn_to_f64(&v)? as f32;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetAudioVolumes {
                inst: None,
                vocals: None,
                player_vocal: None,
                opponent_vocal: Some(vol),
            });
        })
    });

    engine.register_fn("add_note", |time_ms: Dynamic, dir: Dynamic, len_ms: Dynamic, kind: &str| -> Result<(), Box<EvalAltResult>> {
        let time_ms = dyn_to_f64(&time_ms)?;
        let dir = dyn_to_usize(&dir)?;
        let length_ms = dyn_to_f64(&len_ms)?.max(0.0);
        with_ctx(|c| {
            c.commands.push(ScriptCommand::AddNote {
                time_ms,
                dir,
                length_ms,
                kind: kind.to_string(),
            });
        })
    });
    engine.register_fn("trigger_event", |name: &str, val: Dynamic| {
        let json = ScriptValue::from_rhai(&val).to_json();
        with_ctx(|c| {
            c.commands.push(ScriptCommand::TriggerSongEvent {
                name: name.to_string(),
                value: json,
            });
        })
    });
    engine.register_fn("sfx", |path: &str, vol: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let volume = dyn_to_f64(&vol)? as f32;
        with_ctx(|c| {
            c.commands.push(ScriptCommand::PlaySfx {
                path: path.to_string(),
                volume,
            });
        })
    });
    engine.register_fn("sfx", |path: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::PlaySfx {
                path: path.to_string(),
                volume: 1.0,
            });
        })
    });
    engine.register_fn("end_song", || with_ctx(|c| c.commands.push(ScriptCommand::EndSong)));
    engine.register_fn("restart_song", || with_ctx(|c| c.commands.push(ScriptCommand::RestartSong)));
    engine.register_fn("pause_song", || with_ctx(|c| c.commands.push(ScriptCommand::PauseSong)));
}

pub fn register_camera_api(engine: &mut Engine) {
    engine.register_fn("cam_zoom", || with_ctx(|c| c.mutable.cam_zoom));
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
}

