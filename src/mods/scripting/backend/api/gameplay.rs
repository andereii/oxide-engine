use rhai::{Dynamic, Engine, EvalAltResult};

use crate::scripting::api::ScriptCommand;
use crate::scripting::backend::context::{dyn_to_f64, dyn_to_usize, with_ctx};
use crate::scripting::value::ScriptValue;

fn register_tagged_sound_fns(engine: &mut Engine, name: &str) {
    engine.register_fn(name, |tag: &str, path: &str| {
        with_ctx(|c| {
            c.snapshot.sound_volumes.insert(tag.to_string(), 1.0);
            c.commands.push(ScriptCommand::PlaySound {
                tag: tag.to_string(),
                path: path.to_string(),
                volume: 1.0,
                looped: false,
                pitch: 1.0,
            });
        })
    });
    engine.register_fn(name, |tag: &str, path: &str, vol: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let volume = dyn_to_f64(&vol)? as f32;
        with_ctx(|c| {
            c.snapshot.sound_volumes.insert(tag.to_string(), volume);
            c.commands.push(ScriptCommand::PlaySound {
                tag: tag.to_string(),
                path: path.to_string(),
                volume,
                looped: false,
                pitch: 1.0,
            });
        })
    });
    engine.register_fn(name, |tag: &str, path: &str, vol: Dynamic, looped: bool| -> Result<(), Box<EvalAltResult>> {
        let volume = dyn_to_f64(&vol)? as f32;
        with_ctx(|c| {
            c.snapshot.sound_volumes.insert(tag.to_string(), volume);
            c.commands.push(ScriptCommand::PlaySound {
                tag: tag.to_string(),
                path: path.to_string(),
                volume,
                looped,
                pitch: 1.0,
            });
        })
    });
    engine.register_fn(
        name,
        |tag: &str, path: &str, vol: Dynamic, looped: bool, pitch: Dynamic| -> Result<(), Box<EvalAltResult>> {
            let volume = dyn_to_f64(&vol)? as f32;
            let pitch = (dyn_to_f64(&pitch)? as f32).max(0.05);
            with_ctx(|c| {
                c.snapshot.sound_volumes.insert(tag.to_string(), volume);
                c.commands.push(ScriptCommand::PlaySound {
                    tag: tag.to_string(),
                    path: path.to_string(),
                    volume,
                    looped,
                    pitch,
                });
            })
        },
    );
}

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
    engine.register_fn("max_combo", || with_ctx(|c| i64::from(c.snapshot.max_combo)));
    engine.register_fn("misses", || with_ctx(|c| i64::from(c.snapshot.misses)));
    engine.register_fn("sicks", || with_ctx(|c| i64::from(c.snapshot.sick)));
    engine.register_fn("goods", || with_ctx(|c| i64::from(c.snapshot.good)));
    engine.register_fn("bads", || with_ctx(|c| i64::from(c.snapshot.bad)));
    engine.register_fn("shits", || with_ctx(|c| i64::from(c.snapshot.shit)));
    engine.register_fn("hit_count", || with_ctx(|c| i64::from(c.snapshot.hit_notes)));
    engine.register_fn("note_count", || with_ctx(|c| c.snapshot.note_count as i64));
    engine.register_fn("acc", || with_ctx(|c| c.snapshot.accuracy()));
    engine.register_fn("rank", || with_ctx(|c| c.snapshot.rank_name().to_string()));
    engine.register_fn("deaths", || with_ctx(|c| i64::from(c.snapshot.deaths)));

    engine.register_fn("botplay", || with_ctx(|c| c.mutable.botplay));
    engine.register_fn("set_botplay", |enabled: bool| {
        with_ctx(|c| {
            c.mutable.botplay = enabled;
            c.mutable.dirty_botplay = true;
        })
    });
    engine.register_fn("practice", || with_ctx(|c| c.mutable.practice_mode));
    engine.register_fn("set_practice", |enabled: bool| {
        with_ctx(|c| {
            c.mutable.practice_mode = enabled;
            c.mutable.dirty_practice = true;
        })
    });
    engine.register_fn("ghost_tap", || with_ctx(|c| c.mutable.ghost_tap));
    engine.register_fn("set_ghost_tap", |enabled: bool| {
        with_ctx(|c| {
            c.mutable.ghost_tap = enabled;
            c.mutable.dirty_ghost_tap = true;
        })
    });
    engine.register_fn("paused", || with_ctx(|c| c.snapshot.paused));
    engine.register_fn("in_cutscene", || with_ctx(|c| c.mutable.in_cutscene));
    engine.register_fn("set_cutscene", |enabled: bool| {
        with_ctx(|c| {
            c.mutable.in_cutscene = enabled;
            c.mutable.dirty_cutscene = true;
        })
    });
    engine.register_fn("can_pause", || with_ctx(|c| c.mutable.can_pause));
    engine.register_fn("set_can_pause", |enabled: bool| {
        with_ctx(|c| {
            c.mutable.can_pause = enabled;
            c.mutable.dirty_can_pause = true;
        })
    });
    engine.register_fn("lock_keys", |disabled: bool| {
        with_ctx(|c| {
            c.mutable.keys_disabled = disabled;
            c.mutable.dirty_keys_disabled = true;
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

    engine.register_fn("downscroll", || with_ctx(|c| c.mutable.downscroll));
    engine.register_fn("set_downscroll", |enabled: bool| {
        with_ctx(|c| {
            c.mutable.downscroll = enabled;
            c.mutable.dirty_downscroll = true;
        })
    });
    engine.register_fn("middlescroll", || with_ctx(|c| c.mutable.middlescroll));
    engine.register_fn("middlescroll", |enabled: bool| {
        with_ctx(|c| {
            c.mutable.middlescroll = enabled;
            c.mutable.dirty_middlescroll = true;
        })
    });
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

    engine.register_fn("get_inst_vol", || with_ctx(|c| c.snapshot.inst_vol));
    engine.register_fn("sound_volume", || with_ctx(|c| c.snapshot.master_volume));
    engine.register_fn("get_vocals_vol", || {
        with_ctx(|c| (c.snapshot.p_vocal_vol + c.snapshot.o_vocal_vol) * 0.5)
    });
    engine.register_fn("get_p_vocal_vol", || with_ctx(|c| c.snapshot.p_vocal_vol));
    engine.register_fn("get_o_vocal_vol", || with_ctx(|c| c.snapshot.o_vocal_vol));
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

    // Tagged audio channels
    register_tagged_sound_fns(engine, "play_snd");
    register_tagged_sound_fns(engine, "sound");
    for name in ["stop_snd", "stop_sound"] {
        engine.register_fn(name, |tag: &str| {
            with_ctx(|c| {
                c.snapshot.active_sounds.retain(|s| s != tag);
                c.snapshot.sound_volumes.remove(tag);
                c.commands.push(ScriptCommand::StopSound(tag.to_string()));
            })
        });
    }
    engine.register_fn("pause_snd", |tag: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::PauseSound {
                tag: tag.to_string(),
                paused: true,
            });
        })
    });
    engine.register_fn("resume_snd", |tag: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::PauseSound {
                tag: tag.to_string(),
                paused: false,
            });
        })
    });
    engine.register_fn("snd_vol", |tag: &str, v: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let vol = dyn_to_f64(&v)? as f32;
        with_ctx(|c| {
            c.snapshot.sound_volumes.insert(tag.to_string(), vol);
            c.commands.push(ScriptCommand::SetSoundParam {
                tag: tag.to_string(),
                volume: Some(vol),
                pitch: None,
            });
        })
    });
    engine.register_fn("snd_pitch", |tag: &str, p: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let pitch = (dyn_to_f64(&p)? as f32).max(0.05);
        with_ctx(|c| {
            c.commands.push(ScriptCommand::SetSoundParam {
                tag: tag.to_string(),
                volume: None,
                pitch: Some(pitch),
            });
        })
    });
    engine.register_fn("snd_playing", |tag: &str| {
        with_ctx(|c| c.snapshot.active_sounds.iter().any(|s| s == tag))
    });

    // Flow, countdown & playlist
    engine.register_fn("end_song", || with_ctx(|c| c.commands.push(ScriptCommand::EndSong)));
    engine.register_fn("restart_song", || with_ctx(|c| c.commands.push(ScriptCommand::RestartSong)));
    engine.register_fn("pause_song", || with_ctx(|c| c.commands.push(ScriptCommand::PauseSong)));
    engine.register_fn("resume_song", || with_ctx(|c| c.commands.push(ScriptCommand::ResumeSong)));
    engine.register_fn("seek", |ms: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let ms = dyn_to_f64(&ms)?.max(0.0);
        with_ctx(|c| c.commands.push(ScriptCommand::SeekSong(ms)))
    });
    engine.register_fn("countdown", |action: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::ControlCountdown {
                action: action.to_string(),
            });
        })
    });

    engine.register_fn("story_mode", || with_ctx(|c| c.snapshot.story_mode));
    engine.register_fn("set_story_mode", |enabled: bool| {
        with_ctx(|c| {
            c.snapshot.story_mode = enabled;
            c.commands.push(ScriptCommand::SetPlaylist {
                story_mode: Some(enabled),
                songs: None,
                score: None,
            });
        })
    });
    engine.register_fn("playlist", || {
        with_ctx(|c| {
            c.snapshot
                .playlist_songs
                .iter()
                .cloned()
                .map(Dynamic::from)
                .collect::<rhai::Array>()
        })
    });
    engine.register_fn("set_playlist", |songs: rhai::Array| {
        let list: Vec<String> = songs
            .into_iter()
            .filter_map(|v| v.into_string().ok())
            .collect();
        with_ctx(|c| {
            c.snapshot.playlist_songs = list.clone();
            c.commands.push(ScriptCommand::SetPlaylist {
                story_mode: None,
                songs: Some(list),
                score: None,
            });
        })
    });
    engine.register_fn("campaign_score", || with_ctx(|c| i64::from(c.snapshot.campaign_score)));
    engine.register_fn("set_campaign_score", |score: Dynamic| -> Result<(), Box<EvalAltResult>> {
        let score = dyn_to_f64(&score)? as i32;
        with_ctx(|c| {
            c.snapshot.campaign_score = score;
            c.commands.push(ScriptCommand::SetPlaylist {
                story_mode: None,
                songs: None,
                score: Some(score),
            });
        })
    });

    engine.register_fn("load_song", |song_id: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::LoadSong {
                song_id: song_id.to_string(),
                difficulty: None,
                variation: None,
            });
        })
    });
    engine.register_fn("load_song", |song_id: &str, diff: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::LoadSong {
                song_id: song_id.to_string(),
                difficulty: Some(diff.to_string()),
                variation: None,
            });
        })
    });
    engine.register_fn("load_song", |song_id: &str, diff: &str, var: &str| {
        with_ctx(|c| {
            c.commands.push(ScriptCommand::LoadSong {
                song_id: song_id.to_string(),
                difficulty: Some(diff.to_string()),
                variation: Some(var.to_string()),
            });
        })
    });
}
