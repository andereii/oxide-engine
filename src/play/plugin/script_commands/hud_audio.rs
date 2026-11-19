use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use bevy::audio::Volume;
use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};

use crate::play::hud::health_icon::apply_icon_char_id;
use crate::scripting::ScriptCommand;

use super::super::hud_audio::{spawn_one_shot, spawn_tagged_sound};
use super::super::{AudioTickParams, ScriptHudAndTextParams, ScriptTickParams};

static SCREENSHOT_SEQ: AtomicU64 = AtomicU64::new(1);

fn side_matches_icon(player_id: u8, side: &str) -> bool {
    match side.trim().to_ascii_lowercase().as_str() {
        "p1" | "bf" | "boyfriend" | "player" | "0" => player_id == 0,
        "p2" | "dad" | "opponent" | "1" => player_id == 1,
        "both" | "all" => true,
        _ => false,
    }
}

fn sanitize_screenshot_path(mods_dir: &std::path::Path, name_opt: Option<&str>) -> PathBuf {
    let dir = mods_dir.join(".store").join("screenshots");
    let _ = fs::create_dir_all(&dir);
    let raw = name_opt.unwrap_or("").trim();
    let stem: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    let filename = if stem.is_empty() {
        let seq = SCREENSHOT_SEQ.fetch_add(1, Ordering::Relaxed);
        format!("shot_{seq:04}.png")
    } else if stem.to_ascii_lowercase().ends_with(".png") {
        stem
    } else {
        format!("{stem}.png")
    };
    dir.join(filename)
}

fn trigger_sandboxed_screenshot(
    commands: &mut Commands,
    mods_dir: &std::path::Path,
    name_opt: Option<&str>,
) {
    let out_path = sanitize_screenshot_path(mods_dir, name_opt);
    let mut saver = save_to_disk(out_path.clone());
    commands.spawn(Screenshot::primary_window()).observe(
        move |event: On<ScreenshotCaptured>| {
            saver(event);
            info!("Saved screenshot to {}", out_path.display());
        },
    );
}

fn resolve_audio_asset_path(mods: &crate::mods::ModRegistry, path: &str) -> String {
    if let Some(resolved) = mods.resolve_asset_server_path(path) {
        return resolved;
    }
    let has_ext = path.ends_with(".ogg")
        || path.ends_with(".wav")
        || path.ends_with(".mp3")
        || path.ends_with(".flac");
    if !has_ext {
        for ext in [".ogg", ".wav", ".mp3"] {
            let candidate = format!("{path}{ext}");
            if let Some(resolved) = mods.resolve_asset_server_path(&candidate) {
                return resolved;
            }
        }
    }
    path.to_string()
}

pub(super) fn handle_hud_audio_cmd(
    cmd: &ScriptCommand,
    commands: &mut Commands,
    audio_res: &mut AudioTickParams,
    scripts: &mut ScriptTickParams,
    hud_texts: &mut ScriptHudAndTextParams,
) {
    match cmd {
        ScriptCommand::SetBarColors { opp, ply } => {
            for (fill, mut sprite) in &mut hud_texts.bar_fills {
                let c = if fill.is_player { ply } else { opp };
                sprite.color = Color::srgba(c[0], c[1], c[2], c[3]);
            }
        }
        ScriptCommand::SetIcon { side, char_id } => {
            for (mut icon, mut anim, mut sprite) in &mut hud_texts.icons {
                if side_matches_icon(icon.player_id, side) {
                    apply_icon_char_id(
                        &mut icon,
                        &mut anim,
                        &mut sprite,
                        char_id,
                        &scripts.mods,
                        &scripts.asset_server,
                        &mut scripts.layouts,
                    );
                }
            }
        }
        ScriptCommand::SetIconScale { side, scale } => {
            let s = (*scale).max(0.05);
            for (mut icon, _, _) in &mut hud_texts.icons {
                if side_matches_icon(icon.player_id, side) {
                    icon.base_scale = [s, s];
                    icon.snap_to_target_size();
                }
            }
        }
        ScriptCommand::SetIconBop {
            side,
            enabled,
            every,
            angle_deg,
        } => {
            for (mut icon, _, _) in &mut hud_texts.icons {
                if side_matches_icon(icon.player_id, side) {
                    icon.should_bop = *enabled;
                    icon.bop_every = *every;
                    icon.bop_angle_deg = *angle_deg;
                }
            }
        }
        ScriptCommand::PlaySfx { path, volume } => {
            let resolved = resolve_audio_asset_path(&scripts.mods, path);
            let handle: Handle<AudioSource> = scripts.asset_server.load(resolved);
            spawn_one_shot(
                commands,
                handle,
                *volume,
                &audio_res.audio_sources,
                &mut audio_res.pcm_sfx,
                &mut audio_res.sfx_cache,
            );
        }
        ScriptCommand::PlaySound {
            tag,
            path,
            volume,
            looped,
            pitch,
        } => {
            for (ent, tagged) in &hud_texts.tagged_sounds {
                if tagged.tag == *tag {
                    commands.entity(ent).despawn();
                }
            }
            let resolved = resolve_audio_asset_path(&scripts.mods, path);
            let handle: Handle<AudioSource> = scripts.asset_server.load(resolved);
            spawn_tagged_sound(
                commands,
                tag,
                handle,
                *volume,
                *looped,
                *pitch,
                &audio_res.audio_sources,
                &mut audio_res.pcm_sfx,
                &mut audio_res.sfx_cache,
            );
        }
        ScriptCommand::StopSound(tag) => {
            for (ent, tagged) in &hud_texts.tagged_sounds {
                if tagged.tag == *tag {
                    commands.entity(ent).despawn();
                }
            }
        }
        ScriptCommand::PauseSound { tag, paused } => {
            for (ent, tagged) in &hud_texts.tagged_sounds {
                if tagged.tag == *tag
                    && let Ok(sink) = audio_res.sinks.get(ent)
                {
                    if *paused {
                        sink.pause();
                    } else {
                        sink.play();
                    }
                }
            }
        }
        ScriptCommand::SetSoundParam { tag, volume, pitch } => {
            for (ent, mut tagged) in &mut hud_texts.tagged_sounds {
                if tagged.tag == *tag
                    && let Ok(mut sink) = audio_res.sinks.get_mut(ent)
                {
                    if let Some(v) = volume {
                        let clamped = v.clamp(0.0, 2.0);
                        tagged.base_volume = clamped;
                        sink.set_volume(Volume::Linear(clamped));
                    }
                    if let Some(p) = pitch {
                        sink.set_speed(p.max(0.05));
                    }
                }
            }
        }
        ScriptCommand::SeekSong(ms) => {
            audio_res.audio.play_from(*ms, &mut audio_res.sinks);
            audio_res.sync.was_countdown = false;
            hud_texts.pending_seek.0 = Some(*ms);
        }
        ScriptCommand::TakeScreenshot(name_opt) => {
            trigger_sandboxed_screenshot(commands, &scripts.mods.mods_dir, name_opt.as_deref());
        }
        ScriptCommand::LoadSong {
            song_id,
            difficulty,
            variation,
        } => {
            hud_texts.pending_reload.request = Some((
                song_id.clone(),
                difficulty.clone(),
                variation.clone(),
            ));
        }
        _ => {}
    }
}
