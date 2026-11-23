use crate::scripting::api::ScriptCommand;
use crate::scripting::value::ScriptValue;

use super::bindings::{
    ScriptApiEntry, arg, arg_bool, arg_f32, arg_f64, arg_i64, arg_list, arg_str, arg_usize,
};
use super::context::ScriptContext;

fn api_hp(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(ctx.mutable.health))
}
fn api_set_hp(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.mutable.health = arg_f64(args, 0)?;
    ctx.mutable.dirty_health = true;
    Ok(ScriptValue::Null)
}
fn api_add_hp(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.mutable.health += arg_f64(args, 0)?;
    ctx.mutable.dirty_health = true;
    Ok(ScriptValue::Null)
}

fn api_score(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(ctx.mutable.score))
}
fn api_set_score(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.mutable.score = arg_f64(args, 0)?;
    ctx.mutable.dirty_score = true;
    Ok(ScriptValue::Null)
}
fn api_add_score(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.mutable.score += arg_f64(args, 0)?;
    ctx.mutable.dirty_score = true;
    Ok(ScriptValue::Null)
}

fn api_combo(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Int(i64::from(ctx.snapshot.combo)))
}
fn api_max_combo(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Int(i64::from(ctx.snapshot.max_combo)))
}
fn api_misses(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Int(i64::from(ctx.snapshot.misses)))
}
fn api_sicks(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Int(i64::from(ctx.snapshot.sick)))
}
fn api_goods(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Int(i64::from(ctx.snapshot.good)))
}
fn api_bads(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Int(i64::from(ctx.snapshot.bad)))
}
fn api_shits(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Int(i64::from(ctx.snapshot.shit)))
}
fn api_hit_count(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Int(i64::from(ctx.snapshot.hit_notes)))
}
fn api_note_count(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Int(ctx.snapshot.note_count as i64))
}
fn api_acc(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(ctx.snapshot.accuracy()))
}
fn api_rank(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::String(ctx.snapshot.rank_name().to_string()))
}
fn api_deaths(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Int(i64::from(ctx.snapshot.deaths)))
}

fn api_botplay(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(ctx.mutable.botplay))
}
fn api_set_botplay(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.mutable.botplay = arg_bool(args, 0)?;
    ctx.mutable.dirty_botplay = true;
    Ok(ScriptValue::Null)
}
fn api_practice(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(ctx.mutable.practice_mode))
}
fn api_set_practice(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.mutable.practice_mode = arg_bool(args, 0)?;
    ctx.mutable.dirty_practice = true;
    Ok(ScriptValue::Null)
}
fn api_ghost_tap(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(ctx.mutable.ghost_tap))
}
fn api_set_ghost_tap(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.mutable.ghost_tap = arg_bool(args, 0)?;
    ctx.mutable.dirty_ghost_tap = true;
    Ok(ScriptValue::Null)
}
fn api_paused(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(ctx.snapshot.paused))
}
fn api_in_cutscene(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(ctx.mutable.in_cutscene))
}
fn api_set_cutscene(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.mutable.in_cutscene = arg_bool(args, 0)?;
    ctx.mutable.dirty_cutscene = true;
    Ok(ScriptValue::Null)
}

fn api_start_dialogue(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.snapshot.in_dialogue = true;
    ctx.commands
        .push(ScriptCommand::StartDialogue(arg_str(args, 0)?.to_string()));
    Ok(ScriptValue::Null)
}
fn api_skip_dialogue(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SkipDialogue);
    Ok(ScriptValue::Null)
}
fn api_in_dialogue(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(ctx.snapshot.in_dialogue))
}
fn api_dialogue_speaker(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::String(ctx.snapshot.dialogue_speaker.clone()))
}
fn api_dialogue_box(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::String(ctx.snapshot.dialogue_box.clone()))
}
fn api_dialogue_music(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands
        .push(ScriptCommand::SetDialogueMusic(arg_bool(args, 0)?));
    Ok(ScriptValue::Null)
}
fn api_dialogue_box_anim(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands
        .push(ScriptCommand::PlayDialogueBoxAnim(arg_str(args, 0)?.to_string()));
    Ok(ScriptValue::Null)
}

fn api_play_video(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let path = arg_str(args, 0)?.to_string();
    let is_ending = if args.len() >= 2 {
        let kind = arg_str(args, 1)?.trim();
        kind.eq_ignore_ascii_case("ending") || kind.eq_ignore_ascii_case("end")
    } else {
        false
    };
    ctx.snapshot.in_video = true;
    ctx.commands
        .push(ScriptCommand::PlayVideo { path, is_ending });
    Ok(ScriptValue::Null)
}
fn api_skip_video(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SkipVideo);
    Ok(ScriptValue::Null)
}
fn api_in_video(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(ctx.snapshot.in_video))
}

fn api_can_pause(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(ctx.mutable.can_pause))
}
fn api_set_can_pause(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.mutable.can_pause = arg_bool(args, 0)?;
    ctx.mutable.dirty_can_pause = true;
    Ok(ScriptValue::Null)
}
fn api_lock_keys(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.mutable.keys_disabled = arg_bool(args, 0)?;
    ctx.mutable.dirty_keys_disabled = true;
    Ok(ScriptValue::Null)
}

fn api_scroll(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(ctx.mutable.scroll_speed))
}
fn api_set_scroll(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.mutable.scroll_speed = arg_f64(args, 0)?;
    ctx.mutable.dirty_scroll_speed = true;
    Ok(ScriptValue::Null)
}
fn api_tween_scroll(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::TweenScrollSpeed {
        target: arg_f64(args, 0)?,
        duration_s: arg_f64(args, 1)?.max(0.0),
        ease: arg_str(args, 2)?.to_string(),
    });
    Ok(ScriptValue::Null)
}

fn api_rate(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(ctx.mutable.playback_rate))
}
fn api_set_rate(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.mutable.playback_rate = arg_f64(args, 0)?.max(0.05);
    ctx.mutable.dirty_playback_rate = true;
    Ok(ScriptValue::Null)
}

fn api_downscroll(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(ctx.mutable.downscroll))
}
fn api_set_downscroll(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.mutable.downscroll = arg_bool(args, 0)?;
    ctx.mutable.dirty_downscroll = true;
    Ok(ScriptValue::Null)
}
fn api_middlescroll(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    if args.is_empty() {
        Ok(ScriptValue::Bool(ctx.mutable.middlescroll))
    } else {
        ctx.mutable.middlescroll = arg_bool(args, 0)?;
        ctx.mutable.dirty_middlescroll = true;
        Ok(ScriptValue::Null)
    }
}
fn api_swap_strums(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.mutable.strums_swapped = arg_bool(args, 0)?;
    ctx.mutable.dirty_strums_swapped = true;
    Ok(ScriptValue::Null)
}

fn api_status(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands
        .push(ScriptCommand::SetStatus(arg_str(args, 0)?.to_string()));
    Ok(ScriptValue::Null)
}
fn api_set_score_text(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let text = arg_str(args, 0)?;
    let val = (!text.is_empty()).then(|| text.to_string());
    ctx.commands.push(ScriptCommand::SetScoreText(val));
    Ok(ScriptValue::Null)
}
fn api_clear_score_text(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetScoreText(None));
    Ok(ScriptValue::Null)
}
fn api_hud_visible(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let visible = arg_bool(args, 0)?;
    if args.len() >= 2 {
        ctx.commands.push(ScriptCommand::FadeHud {
            alpha: if visible { 1.0 } else { 0.0 },
            duration_s: arg_f64(args, 1)?.max(0.0),
            ease: "quadOut".to_string(),
        });
    } else {
        ctx.commands.push(ScriptCommand::SetHudVisible(visible));
    }
    Ok(ScriptValue::Null)
}
fn api_hud_fade(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let alpha = arg_f32(args, 0)?.clamp(0.0, 1.0);
    let duration_s = arg_f64(args, 1)?.max(0.0);
    let ease = if args.len() >= 3 { arg_str(args, 2)? } else { "linear" };
    ctx.commands.push(ScriptCommand::FadeHud {
        alpha,
        duration_s,
        ease: ease.to_string(),
    });
    Ok(ScriptValue::Null)
}

fn api_get_inst_vol(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(ctx.snapshot.inst_vol))
}
fn api_sound_volume(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(ctx.snapshot.master_volume))
}
fn api_get_vocals_vol(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(
        (ctx.snapshot.p_vocal_vol + ctx.snapshot.o_vocal_vol) * 0.5,
    ))
}
fn api_get_p_vocal_vol(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(ctx.snapshot.p_vocal_vol))
}
fn api_get_o_vocal_vol(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Float(ctx.snapshot.o_vocal_vol))
}

fn api_inst_vol(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetAudioVolumes {
        inst: Some(arg_f32(args, 0)?),
        vocals: None,
        player_vocal: None,
        opponent_vocal: None,
    });
    Ok(ScriptValue::Null)
}
fn api_vocals_vol(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetAudioVolumes {
        inst: None,
        vocals: Some(arg_f32(args, 0)?),
        player_vocal: None,
        opponent_vocal: None,
    });
    Ok(ScriptValue::Null)
}
fn api_p_vocal_vol(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetAudioVolumes {
        inst: None,
        vocals: None,
        player_vocal: Some(arg_f32(args, 0)?),
        opponent_vocal: None,
    });
    Ok(ScriptValue::Null)
}
fn api_o_vocal_vol(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::SetAudioVolumes {
        inst: None,
        vocals: None,
        player_vocal: None,
        opponent_vocal: Some(arg_f32(args, 0)?),
    });
    Ok(ScriptValue::Null)
}

fn api_add_note(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::AddNote {
        time_ms: arg_f64(args, 0)?,
        dir: arg_usize(args, 1)?,
        length_ms: arg_f64(args, 2)?.max(0.0),
        kind: arg_str(args, 3)?.to_string(),
    });
    Ok(ScriptValue::Null)
}
fn api_trigger_event(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let name = arg_str(args, 0)?.to_string();
    let json = arg(args, 1)?.to_json();
    ctx.commands
        .push(ScriptCommand::TriggerSongEvent { name, value: json });
    Ok(ScriptValue::Null)
}
fn api_sfx(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let volume = if args.len() >= 2 { arg_f32(args, 1)? } else { 1.0 };
    ctx.commands.push(ScriptCommand::PlaySfx {
        path: arg_str(args, 0)?.to_string(),
        volume,
    });
    Ok(ScriptValue::Null)
}
fn api_preload_snd(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::PreloadSound {
        path: arg_str(args, 0)?.to_string(),
    });
    Ok(ScriptValue::Null)
}

fn api_play_snd(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let tag = arg_str(args, 0)?.to_string();
    let path = arg_str(args, 1)?.to_string();
    let volume = if args.len() >= 3 { arg_f32(args, 2)? } else { 1.0 };
    let looped = if args.len() >= 4 { arg_bool(args, 3)? } else { false };
    let pitch = if args.len() >= 5 {
        arg_f32(args, 4)?.max(0.05)
    } else {
        1.0
    };
    ctx.snapshot.sound_volumes.insert(tag.clone(), volume);
    ctx.commands.push(ScriptCommand::PlaySound {
        tag,
        path,
        volume,
        looped,
        pitch,
    });
    Ok(ScriptValue::Null)
}
fn api_stop_snd(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let tag = arg_str(args, 0)?;
    ctx.snapshot.active_sounds.retain(|s| s != tag);
    ctx.snapshot.sound_volumes.remove(tag);
    ctx.commands.push(ScriptCommand::StopSound(tag.to_string()));
    Ok(ScriptValue::Null)
}
fn api_pause_snd(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::PauseSound {
        tag: arg_str(args, 0)?.to_string(),
        paused: true,
    });
    Ok(ScriptValue::Null)
}
fn api_resume_snd(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::PauseSound {
        tag: arg_str(args, 0)?.to_string(),
        paused: false,
    });
    Ok(ScriptValue::Null)
}
fn api_snd_vol(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let tag = arg_str(args, 0)?;
    let vol = arg_f32(args, 1)?;
    ctx.snapshot.sound_volumes.insert(tag.to_string(), vol);
    ctx.commands.push(ScriptCommand::SetSoundParam {
        tag: tag.to_string(),
        volume: Some(vol),
        pitch: None,
    });
    Ok(ScriptValue::Null)
}
fn api_snd_pitch(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let pitch = arg_f32(args, 1)?.max(0.05);
    ctx.commands.push(ScriptCommand::SetSoundParam {
        tag: arg_str(args, 0)?.to_string(),
        volume: None,
        pitch: Some(pitch),
    });
    Ok(ScriptValue::Null)
}
fn api_snd_playing(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let tag = arg_str(args, 0)?;
    Ok(ScriptValue::Bool(
        ctx.snapshot.active_sounds.iter().any(|s| s == tag),
    ))
}

fn api_end_song(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::EndSong);
    Ok(ScriptValue::Null)
}
fn api_restart_song(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::RestartSong);
    Ok(ScriptValue::Null)
}
fn api_pause_song(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::PauseSong);
    Ok(ScriptValue::Null)
}
fn api_resume_song(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::ResumeSong);
    Ok(ScriptValue::Null)
}
fn api_seek(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands
        .push(ScriptCommand::SeekSong(arg_f64(args, 0)?.max(0.0)));
    Ok(ScriptValue::Null)
}
fn api_countdown(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    ctx.commands.push(ScriptCommand::ControlCountdown {
        action: arg_str(args, 0)?.to_string(),
    });
    Ok(ScriptValue::Null)
}

fn api_story_mode(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Bool(ctx.snapshot.story_mode))
}
fn api_set_story_mode(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let enabled = arg_bool(args, 0)?;
    ctx.snapshot.story_mode = enabled;
    ctx.commands.push(ScriptCommand::SetPlaylist {
        story_mode: Some(enabled),
        songs: None,
        score: None,
    });
    Ok(ScriptValue::Null)
}
fn api_playlist(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::List(
        ctx.snapshot
            .playlist_songs
            .iter()
            .cloned()
            .map(ScriptValue::String)
            .collect(),
    ))
}
fn api_set_playlist(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let list: Vec<String> = arg_list(args, 0)?
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    ctx.snapshot.playlist_songs = list.clone();
    ctx.commands.push(ScriptCommand::SetPlaylist {
        story_mode: None,
        songs: Some(list),
        score: None,
    });
    Ok(ScriptValue::Null)
}
fn api_campaign_score(ctx: &mut ScriptContext, _: &[ScriptValue]) -> Result<ScriptValue, String> {
    Ok(ScriptValue::Int(i64::from(ctx.snapshot.campaign_score)))
}
fn api_set_campaign_score(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let score = arg_i64(args, 0)? as i32;
    ctx.snapshot.campaign_score = score;
    ctx.commands.push(ScriptCommand::SetPlaylist {
        story_mode: None,
        songs: None,
        score: Some(score),
    });
    Ok(ScriptValue::Null)
}

fn api_load_song(ctx: &mut ScriptContext, args: &[ScriptValue]) -> Result<ScriptValue, String> {
    let song_id = arg_str(args, 0)?.to_string();
    let difficulty = if args.len() >= 2 {
        Some(arg_str(args, 1)?.to_string())
    } else {
        None
    };
    let variation = if args.len() >= 3 {
        Some(arg_str(args, 2)?.to_string())
    } else {
        None
    };
    ctx.commands.push(ScriptCommand::LoadSong {
        song_id,
        difficulty,
        variation,
    });
    Ok(ScriptValue::Null)
}

pub(crate) fn api_entries() -> Vec<ScriptApiEntry> {
    vec![
        ScriptApiEntry::new(&["hp"], &[0], api_hp),
        ScriptApiEntry::new(&["set_hp"], &[1], api_set_hp),
        ScriptApiEntry::new(&["add_hp"], &[1], api_add_hp),
        ScriptApiEntry::new(&["score"], &[0], api_score),
        ScriptApiEntry::new(&["set_score"], &[1], api_set_score),
        ScriptApiEntry::new(&["add_score"], &[1], api_add_score),
        ScriptApiEntry::new(&["combo"], &[0], api_combo),
        ScriptApiEntry::new(&["max_combo"], &[0], api_max_combo),
        ScriptApiEntry::new(&["misses"], &[0], api_misses),
        ScriptApiEntry::new(&["sicks"], &[0], api_sicks),
        ScriptApiEntry::new(&["goods"], &[0], api_goods),
        ScriptApiEntry::new(&["bads"], &[0], api_bads),
        ScriptApiEntry::new(&["shits"], &[0], api_shits),
        ScriptApiEntry::new(&["hit_count"], &[0], api_hit_count),
        ScriptApiEntry::new(&["note_count"], &[0], api_note_count),
        ScriptApiEntry::new(&["acc"], &[0], api_acc),
        ScriptApiEntry::new(&["rank"], &[0], api_rank),
        ScriptApiEntry::new(&["deaths"], &[0], api_deaths),
        ScriptApiEntry::new(&["botplay"], &[0], api_botplay),
        ScriptApiEntry::new(&["set_botplay"], &[1], api_set_botplay),
        ScriptApiEntry::new(&["practice"], &[0], api_practice),
        ScriptApiEntry::new(&["set_practice"], &[1], api_set_practice),
        ScriptApiEntry::new(&["ghost_tap"], &[0], api_ghost_tap),
        ScriptApiEntry::new(&["set_ghost_tap"], &[1], api_set_ghost_tap),
        ScriptApiEntry::new(&["paused"], &[0], api_paused),
        ScriptApiEntry::new(&["in_cutscene"], &[0], api_in_cutscene),
        ScriptApiEntry::new(&["set_cutscene"], &[1], api_set_cutscene),
        ScriptApiEntry::new(&["start_dialogue", "start_conversation"], &[1], api_start_dialogue),
        ScriptApiEntry::new(&["skip_dialogue", "end_dialogue"], &[0], api_skip_dialogue),
        ScriptApiEntry::new(&["in_dialogue"], &[0], api_in_dialogue),
        ScriptApiEntry::new(&["dialogue_speaker"], &[0], api_dialogue_speaker),
        ScriptApiEntry::new(&["dialogue_box"], &[0], api_dialogue_box),
        ScriptApiEntry::new(&["dialogue_music"], &[1], api_dialogue_music),
        ScriptApiEntry::new(&["dialogue_box_anim"], &[1], api_dialogue_box_anim),
        ScriptApiEntry::new(&["play_video"], &[1, 2], api_play_video),
        ScriptApiEntry::new(&["skip_video"], &[0], api_skip_video),
        ScriptApiEntry::new(&["in_video"], &[0], api_in_video),
        ScriptApiEntry::new(&["can_pause"], &[0], api_can_pause),
        ScriptApiEntry::new(&["set_can_pause"], &[1], api_set_can_pause),
        ScriptApiEntry::new(&["lock_keys"], &[1], api_lock_keys),
        ScriptApiEntry::new(&["scroll"], &[0], api_scroll),
        ScriptApiEntry::new(&["set_scroll"], &[1], api_set_scroll),
        ScriptApiEntry::new(&["tween_scroll"], &[3], api_tween_scroll),
        ScriptApiEntry::new(&["rate"], &[0], api_rate),
        ScriptApiEntry::new(&["set_rate"], &[1], api_set_rate),
        ScriptApiEntry::new(&["downscroll"], &[0], api_downscroll),
        ScriptApiEntry::new(&["set_downscroll"], &[1], api_set_downscroll),
        ScriptApiEntry::new(&["middlescroll"], &[0, 1], api_middlescroll),
        ScriptApiEntry::new(&["set_middlescroll"], &[1], api_middlescroll),
        ScriptApiEntry::new(&["swap_strums"], &[1], api_swap_strums),
        ScriptApiEntry::new(&["status"], &[1], api_status),
        ScriptApiEntry::new(&["set_score_text"], &[1], api_set_score_text),
        ScriptApiEntry::new(&["clear_score_text"], &[0], api_clear_score_text),
        ScriptApiEntry::new(&["hud_visible"], &[1, 2], api_hud_visible),
        ScriptApiEntry::new(&["hud_fade"], &[2, 3], api_hud_fade),
        ScriptApiEntry::new(&["get_inst_vol"], &[0], api_get_inst_vol),
        ScriptApiEntry::new(&["sound_volume"], &[0], api_sound_volume),
        ScriptApiEntry::new(&["get_vocals_vol"], &[0], api_get_vocals_vol),
        ScriptApiEntry::new(&["get_p_vocal_vol"], &[0], api_get_p_vocal_vol),
        ScriptApiEntry::new(&["get_o_vocal_vol"], &[0], api_get_o_vocal_vol),
        ScriptApiEntry::new(&["inst_vol"], &[1], api_inst_vol),
        ScriptApiEntry::new(&["vocals_vol"], &[1], api_vocals_vol),
        ScriptApiEntry::new(&["p_vocal_vol"], &[1], api_p_vocal_vol),
        ScriptApiEntry::new(&["o_vocal_vol"], &[1], api_o_vocal_vol),
        ScriptApiEntry::new(&["add_note"], &[4], api_add_note),
        ScriptApiEntry::new(&["trigger_event"], &[2], api_trigger_event),
        ScriptApiEntry::new(&["sfx"], &[1, 2], api_sfx),
        ScriptApiEntry::new(&["preload_snd", "preload_sfx"], &[1], api_preload_snd),
        ScriptApiEntry::new(&["play_snd", "sound"], &[2, 3, 4, 5], api_play_snd),
        ScriptApiEntry::new(&["stop_snd", "stop_sound"], &[1], api_stop_snd),
        ScriptApiEntry::new(&["pause_snd"], &[1], api_pause_snd),
        ScriptApiEntry::new(&["resume_snd"], &[1], api_resume_snd),
        ScriptApiEntry::new(&["snd_vol"], &[2], api_snd_vol),
        ScriptApiEntry::new(&["snd_pitch"], &[2], api_snd_pitch),
        ScriptApiEntry::new(&["snd_playing"], &[1], api_snd_playing),
        ScriptApiEntry::new(&["end_song"], &[0], api_end_song),
        ScriptApiEntry::new(&["restart_song"], &[0], api_restart_song),
        ScriptApiEntry::new(&["pause_song"], &[0], api_pause_song),
        ScriptApiEntry::new(&["resume_song"], &[0], api_resume_song),
        ScriptApiEntry::new(&["seek"], &[1], api_seek),
        ScriptApiEntry::new(&["countdown"], &[1], api_countdown),
        ScriptApiEntry::new(&["story_mode"], &[0], api_story_mode),
        ScriptApiEntry::new(&["set_story_mode"], &[1], api_set_story_mode),
        ScriptApiEntry::new(&["playlist"], &[0], api_playlist),
        ScriptApiEntry::new(&["set_playlist"], &[1], api_set_playlist),
        ScriptApiEntry::new(&["campaign_score"], &[0], api_campaign_score),
        ScriptApiEntry::new(&["set_campaign_score"], &[1], api_set_campaign_score),
        ScriptApiEntry::new(&["load_song"], &[1, 2, 3], api_load_song),
    ]
}
