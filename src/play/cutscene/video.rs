use std::collections::VecDeque;
use std::io::Read;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver};
use std::thread;

use bevy::asset::RenderAssetUsages;
use bevy::audio::{AudioPlayer, AudioSink, PlaybackMode, PlaybackSettings, Volume};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite::Anchor;

use crate::input::{Action, Controls};
use crate::mods::ModRegistry;
use crate::play::camera::hud::hud_render_layer;
use crate::play::hud::subtitles::parse_srt;
use crate::play::playlist::PlayStatePlaylist;
use crate::play::scene::PlayState;
use crate::play::types::PlayPhase;
use crate::ui::SoundTrayState;
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};

const VIDEO_BACKDROP_Z: f32 = 85.0;
const VIDEO_FRAME_Z: f32 = 86.0;
const VIDEO_SKIP_TEXT_Z: f32 = 87.0;
const VIDEO_WIDTH: u32 = 1280;
const VIDEO_HEIGHT: u32 = 720;
const VIDEO_FPS: f64 = 30.0;

#[derive(Debug, Clone)]
pub enum VideoCutsceneCmd {
    Play { path: String, is_ending: bool },
    Skip,
    Restart,
    Cleanup,
}

#[derive(Component)]
pub struct VideoBackdropVisual;

#[derive(Component)]
pub struct VideoFrameVisual;

#[derive(Component)]
pub struct VideoSkipTextVisual;

#[derive(Component)]
pub struct VideoAudioPlayer;

#[derive(Resource, Default)]
pub struct VideoCutsceneState {
    pub pending_cmds: VecDeque<VideoCutsceneCmd>,
    pub active: bool,
    pub path: String,
    pub is_ending: bool,
    pub elapsed_s: f64,
    pub frame_accum_s: f64,
    pub can_skip: bool,
    pub skip_prompt_shown: bool,
    pub stream_ended: bool,
    pub child: Option<Child>,
    pub frame_rx: Option<Mutex<Receiver<Vec<u8>>>>,
    pub texture_handle: Option<Handle<Image>>,
    pub backdrop_entity: Option<Entity>,
    pub frame_entity: Option<Entity>,
    pub skip_text_entity: Option<Entity>,
    pub audio_entity: Option<Entity>,
}

impl VideoCutsceneState {
    pub fn queue_play(&mut self, path: String, is_ending: bool) {
        self.pending_cmds
            .push_back(VideoCutsceneCmd::Play { path, is_ending });
    }

    pub fn queue_skip(&mut self) {
        self.pending_cmds.push_back(VideoCutsceneCmd::Skip);
    }

    pub fn queue_restart(&mut self) {
        self.pending_cmds.push_back(VideoCutsceneCmd::Restart);
    }

    pub fn queue_cleanup(&mut self) {
        self.pending_cmds.push_back(VideoCutsceneCmd::Cleanup);
    }
}

fn find_ffmpeg() -> Option<String> {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let local = if cfg!(windows) {
            dir.join("ffmpeg.exe")
        } else {
            dir.join("ffmpeg")
        };
        if local.is_file() {
            return Some(local.to_string_lossy().into_owned());
        }
    }
    Some("ffmpeg".to_string())
}

fn decode_video_audio_wav(ffmpeg_bin: &str, video_path: &Path) -> Option<Vec<u8>> {
    let mut cmd = Command::new(ffmpeg_bin);
    cmd.args([
        "-v",
        "error",
        "-i",
        &video_path.to_string_lossy(),
        "-vn",
        "-ac",
        "2",
        "-ar",
        "44100",
        "-f",
        "wav",
        "pipe:1",
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let out = cmd.output().ok()?;
    if out.status.success() && !out.stdout.is_empty() {
        Some(out.stdout)
    } else {
        None
    }
}

fn spawn_video_frame_stream(
    ffmpeg_bin: &str,
    video_path: &Path,
) -> Option<(Child, Receiver<Vec<u8>>)> {
    let filter = format!(
        "fps={VIDEO_FPS},scale={VIDEO_WIDTH}:{VIDEO_HEIGHT}:force_original_aspect_ratio=decrease,pad={VIDEO_WIDTH}:{VIDEO_HEIGHT}:(ow-iw)/2:(oh-ih)/2"
    );
    let mut cmd = Command::new(ffmpeg_bin);
    cmd.args([
        "-v",
        "error",
        "-i",
        &video_path.to_string_lossy(),
        "-an",
        "-vf",
        &filter,
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgba",
        "pipe:1",
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let mut child = cmd.spawn().ok()?;

    let mut stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(4);
    let frame_bytes = (VIDEO_WIDTH * VIDEO_HEIGHT * 4) as usize;
    thread::spawn(move || {
        loop {
            let mut buf = vec![0_u8; frame_bytes];
            let mut read_total = 0;
            while read_total < frame_bytes {
                match stdout.read(&mut buf[read_total..]) {
                    Ok(0) => return,
                    Ok(n) => read_total += n,
                    Err(_) => return,
                }
            }
            if tx.send(buf).is_err() {
                return;
            }
        }
    });
    Some((child, rx))
}

fn cleanup_video(commands: &mut Commands, vid: &mut VideoCutsceneState) {
    if let Some(mut child) = vid.child.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    vid.frame_rx = None;
    for slot in [
        &mut vid.backdrop_entity,
        &mut vid.frame_entity,
        &mut vid.skip_text_entity,
        &mut vid.audio_entity,
    ] {
        if let Some(ent) = slot.take() {
            commands.entity(ent).despawn();
        }
    }
    vid.texture_handle = None;
    vid.active = false;
    vid.elapsed_s = 0.0;
    vid.frame_accum_s = 0.0;
    vid.can_skip = false;
    vid.skip_prompt_shown = false;
    vid.stream_ended = false;
}

fn finish_video(
    commands: &mut Commands,
    state: &mut PlayState,
    playlist: &mut PlayStatePlaylist,
    vid: &mut VideoCutsceneState,
) {
    let is_ending = vid.is_ending;
    cleanup_video(commands, vid);
    state.clear_subtitles();
    state.is_in_cutscene = false;
    if is_ending {
        state.end_song(playlist);
    } else if state.is_in_countdown && state.countdown_paused {
        state.set_hud_visible(true, false);
        state.start_countdown();
    }
}

#[allow(clippy::too_many_arguments)]
pub fn tick_video_cutscene_system(
    time: Res<Time>,
    mut commands: Commands,
    mods: Res<ModRegistry>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    keys: Res<ButtonInput<KeyCode>>,
    controls: Res<Controls>,
    sound_tray: Option<Res<SoundTrayState>>,
    mut state: ResMut<PlayState>,
    mut playlist: ResMut<PlayStatePlaylist>,
    mut vid: ResMut<VideoCutsceneState>,
    mut skip_texts: Query<(&mut TextColor, &mut Visibility), With<VideoSkipTextVisual>>,
    mut sinks: Query<&mut AudioSink, With<VideoAudioPlayer>>,
) {
    let dt = f64::from(time.delta_secs()).min(0.1);
    let paused = matches!(
        state.phase,
        PlayPhase::Loading | PlayPhase::Paused(_) | PlayPhase::GameOver | PlayPhase::Ended
    );
    let master_vol = sound_tray
        .as_deref()
        .map_or(1.0_f32, |st| st.effective_volume());

    let cmds: Vec<VideoCutsceneCmd> = vid.pending_cmds.drain(..).collect();
    for cmd in cmds {
        match cmd {
            VideoCutsceneCmd::Cleanup => {
                cleanup_video(&mut commands, &mut vid);
            }
            VideoCutsceneCmd::Skip => {
                if vid.active {
                    finish_video(&mut commands, &mut state, &mut playlist, &mut vid);
                }
            }
            VideoCutsceneCmd::Restart => {
                if vid.active {
                    let path = vid.path.clone();
                    let is_ending = vid.is_ending;
                    vid.pending_cmds
                        .push_back(VideoCutsceneCmd::Play { path, is_ending });
                }
            }
            VideoCutsceneCmd::Play { path, is_ending } => {
                cleanup_video(&mut commands, &mut vid);
                vid.path = path.clone();
                vid.is_ending = is_ending;
                state.is_in_cutscene = true;
                state.set_hud_visible(false, true);

                let resolved = mods
                    .resolve_file(&path)
                    .or_else(|| mods.resolve_file(&format!("{path}.mp4")))
                    .or_else(|| mods.resolve_file(&format!("{path}.mkv")));
                let Some(video_file) = resolved else {
                    warn!("play_video('{path}'): video file not found, finishing cutscene");
                    finish_video(&mut commands, &mut state, &mut playlist, &mut vid);
                    continue;
                };

                let Some(ffmpeg_bin) = find_ffmpeg() else {
                    warn!("play_video('{path}'): ffmpeg not found, skipping video playback");
                    finish_video(&mut commands, &mut state, &mut playlist, &mut vid);
                    continue;
                };

                let Some((child, rx)) = spawn_video_frame_stream(&ffmpeg_bin, &video_file) else {
                    warn!(
                        "play_video('{path}'): failed to start ffmpeg, skipping video playback"
                    );
                    finish_video(&mut commands, &mut state, &mut playlist, &mut vid);
                    continue;
                };

                vid.active = true;
                vid.child = Some(child);
                vid.frame_rx = Some(Mutex::new(rx));
                vid.elapsed_s = 0.0;
                vid.frame_accum_s = 0.0;
                vid.can_skip = false;
                vid.skip_prompt_shown = false;
                vid.stream_ended = false;

                // Backdrop
                let bd_ent = commands
                    .spawn((
                        Sprite::from_color(
                            Color::BLACK,
                            Vec2::new(SCREEN_WIDTH * 2.0, SCREEN_HEIGHT * 2.0),
                        ),
                        Transform::from_xyz(0.0, 0.0, VIDEO_BACKDROP_Z),
                        Visibility::Visible,
                        hud_render_layer(),
                        VideoBackdropVisual,
                    ))
                    .id();
                vid.backdrop_entity = Some(bd_ent);

                // Video frame texture
                let blank = vec![0_u8; (VIDEO_WIDTH * VIDEO_HEIGHT * 4) as usize];
                let image = Image::new(
                    Extent3d {
                        width: VIDEO_WIDTH,
                        height: VIDEO_HEIGHT,
                        depth_or_array_layers: 1,
                    },
                    TextureDimension::D2,
                    blank,
                    TextureFormat::Rgba8UnormSrgb,
                    RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
                );
                let tex_handle = images.add(image);
                let frame_ent = commands
                    .spawn((
                        Sprite::from_image(tex_handle.clone()),
                        Transform::from_xyz(0.0, 0.0, VIDEO_FRAME_Z),
                        Visibility::Visible,
                        hud_render_layer(),
                        VideoFrameVisual,
                    ))
                    .id();
                vid.texture_handle = Some(tex_handle);
                vid.frame_entity = Some(frame_ent);

                // Skip text
                let mut skip_font = TextFont {
                    font_size: FontSize::Px(28.0),
                    ..default()
                };
                if let Some(font_srv) = mods.resolve_asset_server_path("ui/fonts/VCR OSD Mono.ttf") {
                    let h: Handle<Font> = asset_server.load(font_srv);
                    skip_font.font = h.into();
                }
                let skip_ent = commands
                    .spawn((
                        Text2d::new("Skip [Enter]"),
                        skip_font,
                        TextColor(Color::srgba(1.0, 1.0, 1.0, 0.0)),
                        Anchor::BOTTOM_RIGHT,
                        Transform::from_xyz(
                            SCREEN_WIDTH * 0.5 - 32.0,
                            -SCREEN_HEIGHT * 0.5 + 24.0,
                            VIDEO_SKIP_TEXT_Z,
                        ),
                        Visibility::Hidden,
                        hud_render_layer(),
                        VideoSkipTextVisual,
                    ))
                    .id();
                vid.skip_text_entity = Some(skip_ent);

                // Audio
                if let Some(wav_bytes) = decode_video_audio_wav(&ffmpeg_bin, &video_file) {
                    let audio_handle = audio_sources.add(AudioSource {
                        bytes: wav_bytes.into(),
                    });
                    let audio_ent = commands
                        .spawn((
                            AudioPlayer(audio_handle),
                            PlaybackSettings {
                                mode: PlaybackMode::Despawn,
                                volume: Volume::Linear(master_vol),
                                ..default()
                            },
                            VideoAudioPlayer,
                        ))
                        .id();
                    vid.audio_entity = Some(audio_ent);
                }

                // Optional matching .srt subtitles
                let srt_rel = Path::new(&path)
                    .with_extension("srt")
                    .to_string_lossy()
                    .replace('\\', "/");
                if let Ok(raw_srt) = mods.read_to_string_merged(&srt_rel) {
                    state.load_subtitles(parse_srt(&raw_srt));
                }
            }
        }
    }

    if !vid.active {
        return;
    }

    for mut sink in &mut sinks {
        if paused {
            sink.pause();
        } else {
            sink.play();
            sink.set_volume(Volume::Linear(master_vol));
        }
    }

    if paused {
        return;
    }

    vid.elapsed_s += dt;
    vid.frame_accum_s += dt;
    let frame_period = 1.0 / VIDEO_FPS;
    while vid.frame_accum_s >= frame_period {
        vid.frame_accum_s -= frame_period;
        let mut ended = false;
        if let Some(rx_lock) = vid.frame_rx.as_ref()
            && let Ok(rx) = rx_lock.lock()
        {
            match rx.try_recv() {
                Ok(bytes) => {
                    if let Some(tex_h) = vid.texture_handle.as_ref()
                        && let Some(mut img) = images.get_mut(tex_h)
                    {
                        img.data = Some(bytes);
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    ended = true;
                }
                Err(mpsc::TryRecvError::Empty) => break,
            }
        }
        if ended {
            vid.stream_ended = true;
            break;
        }
    }

    // Skip input: first press reveals prompt, second press skips
    let accept_pressed = controls.just_pressed(&keys, Action::Accept)
        || keys.just_pressed(KeyCode::Space)
        || keys.just_pressed(KeyCode::Enter);
    if accept_pressed {
        if !vid.skip_prompt_shown {
            vid.skip_prompt_shown = true;
            vid.can_skip = true;
            for (mut tc, mut vis) in &mut skip_texts {
                tc.0 = Color::srgba(1.0, 1.0, 1.0, 1.0);
                *vis = Visibility::Visible;
            }
        } else if vid.can_skip {
            finish_video(&mut commands, &mut state, &mut playlist, &mut vid);
            return;
        }
    }

    if vid.stream_ended {
        finish_video(&mut commands, &mut state, &mut playlist, &mut vid);
    }
}
