use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use bevy::asset::{AssetApp, AssetPlugin};
use bevy::audio::{
    AudioPlayer, AudioPlugin, AudioSource, AudioSink, ChannelCount, Decodable, PlaybackMode,
    PlaybackSettings, SampleRate, Source, Volume,
};
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::{App, AssetServer, Assets, Commands, MinimalPlugins, Res, ResMut, World};

use crate::mods::ModRegistry;
use crate::play::types::{AudioTrackSpec, ResolvedAudioConfig, SongChart};
use crate::utils::constants::MS_PER_SEC;

use super::mixer::{
    MixTrackControl, OxideAudioPlugin, PcmBuffer, PcmSfxAudio, PcmSfxCache, SyncedSongAudio,
    normalize_pcm,
};
use super::player::SongAudioPlayer;
use super::sound::{TrackHandle, TrackRole};

const BEEP_OGG: &[u8] = include_bytes!("../../tests/fixtures/media/mini/sounds/beep.ogg");
const INST_OGG: &[u8] =
    include_bytes!("../../tests/fixtures/media/mini/songs/synthetic/Inst.ogg");
const VOICES_OGG: &[u8] =
    include_bytes!("../../tests/fixtures/media/mini/songs/synthetic/Voices-bf.ogg");

fn nz_ch(ch: u16) -> ChannelCount {
    ChannelCount::new(ch).expect("non-zero channel count")
}

fn nz_rate(rate: u32) -> SampleRate {
    SampleRate::new(rate).expect("non-zero sample rate")
}

fn assert_slice_close(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len());
    for (idx, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
        assert!((a - e).abs() < 1e-5, "sample {idx} mismatch: {a} != {e}");
    }
}

fn test_player_world() -> World {
    let mut world = World::new();
    world.init_resource::<Assets<AudioSource>>();
    world.init_resource::<Assets<SyncedSongAudio>>();
    world.init_resource::<SongAudioPlayer>();
    world
}

fn insert_track(
    player: &mut SongAudioPlayer,
    role: TrackRole,
    offset_ms: f64,
    base_volume: f32,
    handle: bevy::prelude::Handle<AudioSource>,
) {
    player.tracks.push(TrackHandle {
        role,
        offset_ms,
        base_volume,
        handle,
    });
}

#[test]
fn mix_track_control_atomic_volume_updates_lock_free() {
    let initial = 0.8_f32;
    let updated = 0.25_f32;
    let ctrl = MixTrackControl::new(
        TrackRole::PlayerVocal,
        initial,
        initial,
        0,
        Arc::from([0.5, -0.5]),
    );
    assert!((ctrl.volume() - initial).abs() < 1e-6);

    ctrl.set_volume(updated);
    assert!((ctrl.volume() - updated).abs() < 1e-6);

    ctrl.set_volume(-0.4);
    assert_eq!(ctrl.volume().to_bits(), 0.0_f32.to_bits());

    ctrl.set_volume(f32::NAN);
    assert_eq!(ctrl.volume().to_bits(), 0.0_f32.to_bits());
}

#[test]
fn normalize_pcm_identity_returns_shared_arc_without_copy() {
    let rate = nz_rate(44_100);
    let ch = nz_ch(2);
    let samples: Arc<[f32]> = Arc::from([0.1, -0.2, 0.3, -0.4]);
    let buf = PcmBuffer {
        samples: Arc::clone(&samples),
        channels: ch,
        sample_rate: rate,
    };
    let out = normalize_pcm(&buf, ch, rate);
    assert!(Arc::ptr_eq(&samples, &out));
}

#[test]
fn normalize_pcm_converts_mono_stereo_and_arbitrary_channels() {
    let rate = nz_rate(32_000);
    let mono_samples = [0.5_f32, -0.25_f32];
    let mono = PcmBuffer {
        samples: Arc::from(mono_samples),
        channels: nz_ch(1),
        sample_rate: rate,
    };
    let mono_to_stereo = normalize_pcm(&mono, nz_ch(2), rate);
    let expected_mono_stereo: Vec<f32> = mono_samples.iter().flat_map(|&s| [s, s]).collect();
    assert_slice_close(&mono_to_stereo, &expected_mono_stereo);

    let mono_to_quad = normalize_pcm(&mono, nz_ch(4), rate);
    let expected_mono_quad: Vec<f32> = mono_samples.iter().flat_map(|&s| [s, s, s, s]).collect();
    assert_slice_close(&mono_to_quad, &expected_mono_quad);

    let stereo_frames = [[0.8_f32, 0.2_f32], [-0.4_f32, 0.4_f32]];
    let stereo_flat: Vec<f32> = stereo_frames.iter().flatten().copied().collect();
    let stereo = PcmBuffer {
        samples: Arc::from(stereo_flat),
        channels: nz_ch(2),
        sample_rate: rate,
    };
    let stereo_to_mono = normalize_pcm(&stereo, nz_ch(1), rate);
    let expected_stereo_mono: Vec<f32> = stereo_frames
        .iter()
        .map(|&[l, r]| (l + r) * 0.5)
        .collect();
    assert_slice_close(&stereo_to_mono, &expected_stereo_mono);

    let stereo_to_quad = normalize_pcm(&stereo, nz_ch(4), rate);
    let expected_stereo_quad: Vec<f32> = stereo_frames
        .iter()
        .flat_map(|&[l, r]| [l, r, 0.0, 0.0])
        .collect();
    assert_slice_close(&stereo_to_quad, &expected_stereo_quad);

    let quad_frame = [1.0_f32, 0.0_f32, 0.5_f32, 1.0_f32];
    let quad = PcmBuffer {
        samples: Arc::from(quad_frame),
        channels: nz_ch(4),
        sample_rate: rate,
    };
    let quad_to_stereo = normalize_pcm(&quad, nz_ch(2), rate);
    let expected_quad_stereo = [
        (quad_frame[0] + quad_frame[2]) * 0.5,
        (quad_frame[1] + quad_frame[3]) * 0.5,
    ];
    assert_slice_close(&quad_to_stereo, &expected_quad_stereo);
}

#[test]
fn normalize_pcm_resamples_linearly_both_up_and_down() {
    let up_buf = PcmBuffer {
        samples: Arc::from([0.0, 1.0, 0.0]),
        channels: nz_ch(1),
        sample_rate: nz_rate(1_000),
    };
    let upsampled = normalize_pcm(&up_buf, nz_ch(1), nz_rate(2_000));
    assert_slice_close(&upsampled, &[0.0, 0.5, 1.0, 0.5, 0.0, 0.0]);

    let down_buf = PcmBuffer {
        samples: Arc::from([0.0, 0.25, 0.5, 0.75, 1.0, 0.5]),
        channels: nz_ch(1),
        sample_rate: nz_rate(3_000),
    };
    let downsampled = normalize_pcm(&down_buf, nz_ch(1), nz_rate(1_500));
    assert_slice_close(&downsampled, &[0.0, 0.5, 1.0]);

    // Cross-rate stereo conversion derived from input frame count and rate ratio.
    let in_rate = 22_050_u32;
    let out_rate = 24_000_u32;
    let in_frames = 2_205_usize;
    let channels = 2_u16;
    let left_val = 0.2_f32;
    let right_val = -0.2_f32;
    let cross = PcmBuffer {
        samples: Arc::from([left_val, right_val].repeat(in_frames)),
        channels: nz_ch(channels),
        sample_rate: nz_rate(in_rate),
    };
    let cross_out = normalize_pcm(&cross, nz_ch(channels), nz_rate(out_rate));
    let expected_out_frames = (in_frames * out_rate as usize) / in_rate as usize;
    assert_eq!(cross_out.len(), expected_out_frames * usize::from(channels));
    for frame in cross_out.chunks_exact(usize::from(channels)) {
        assert!((frame[0] - left_val).abs() < 1e-5);
        assert!((frame[1] - right_val).abs() < 1e-5);
    }
}

#[test]
fn synced_mix_source_is_sample_locked_with_zero_drift_and_atomic_volumes() {
    let rate_hz = 10_000_u32;
    let rate = nz_rate(rate_hz);
    let ch_count = 2_usize;
    let ch = nz_ch(ch_count as u16);
    let total_inst_frames = 5_000_usize;
    let vocal_offset_frames = 250_isize;
    let total_vocal_frames = 4_800_usize;

    let phase_period = 100_usize;
    let phase_divisor = 200.0_f32;
    let marker_interval = 500_usize;
    let high_marker = 0.4_f32;
    let low_marker = 0.1_f32;
    let right_ratio = 0.5_f32;

    let expected_inst = |f: usize| (f % phase_period) as f32 / phase_divisor;
    let expected_marker = |vf: usize| {
        if vf % marker_interval == 0 {
            high_marker
        } else {
            low_marker
        }
    };

    let mut inst_samples = Vec::with_capacity(total_inst_frames * ch_count);
    for f in 0..total_inst_frames {
        let phase = expected_inst(f);
        inst_samples.push(phase);
        inst_samples.push(-phase);
    }

    let mut vocal_samples = Vec::with_capacity(total_vocal_frames * ch_count);
    for f in 0..total_vocal_frames {
        let marker = expected_marker(f);
        vocal_samples.push(marker);
        vocal_samples.push(marker * right_ratio);
    }

    let tracks: Arc<[MixTrackControl]> = Arc::from([
        MixTrackControl::new(
            TrackRole::Instrumental,
            1.0,
            1.0,
            0,
            Arc::from(inst_samples),
        ),
        MixTrackControl::new(
            TrackRole::PlayerVocal,
            1.0,
            1.0,
            vocal_offset_frames,
            Arc::from(vocal_samples),
        ),
    ]);
    let cursor = Arc::new(AtomicUsize::new(0));
    let song = SyncedSongAudio::new(Arc::clone(&tracks), ch, rate, Arc::clone(&cursor));

    let vocal_offset_u = vocal_offset_frames as usize;
    let expected_total_frames = total_inst_frames.max(vocal_offset_u + total_vocal_frames);
    assert_eq!(song.total_frames(), expected_total_frames);
    assert_eq!(song.channels(), ch);
    assert_eq!(song.sample_rate(), rate);

    let mut src = song.decoder();
    assert_eq!(src.current_span_len(), None);
    assert_eq!(
        src.total_duration(),
        Some(Duration::from_secs_f64(
            expected_total_frames as f64 / f64::from(rate_hz)
        ))
    );

    // Before vocal offset: only instrumental contributes.
    for f in 0..vocal_offset_u {
        let phase = expected_inst(f);
        let l = src.next().expect("left sample");
        assert_eq!(cursor.load(Ordering::Relaxed), f);
        let r = src.next().expect("right sample");
        assert_eq!(cursor.load(Ordering::Relaxed), f + 1);
        assert!((l - phase).abs() < 1e-6);
        assert!((r + phase).abs() < 1e-6);
    }

    // Across active overlap frames: zero relative drift between stems, verified at every frame.
    let overlap_end = 4_250_usize;
    for f in vocal_offset_u..overlap_end {
        let phase = expected_inst(f);
        let marker = expected_marker(f - vocal_offset_u);
        let l = src.next().expect("left sample");
        let r = src.next().expect("right sample");
        assert!((l - (phase + marker)).abs() < 1e-5, "drift at frame {f}");
        assert!(
            (r - (-phase + marker * right_ratio)).abs() < 1e-5,
            "drift at frame {f}"
        );
    }

    // Mute vocal track atomically mid-stream and confirm immediate attenuation.
    tracks[1].set_volume(0.0);
    let muted_end = 4_500_usize;
    for f in overlap_end..muted_end {
        let phase = expected_inst(f);
        let l = src.next().expect("left sample");
        let r = src.next().expect("right sample");
        assert!((l - phase).abs() < 1e-6);
        assert!((r + phase).abs() < 1e-6);
    }

    // Attenuated volume on vocal track.
    let half_vol = 0.5_f32;
    tracks[1].set_volume(half_vol);
    let l = src.next().expect("left sample");
    let r = src.next().expect("right sample");
    let phase = expected_inst(muted_end);
    let marker = expected_marker(muted_end - vocal_offset_u);
    assert!((l - (phase + marker * half_vol)).abs() < 1e-5);
    assert!((r - (-phase + marker * right_ratio * half_vol)).abs() < 1e-5);

    // Drain remainder and verify exact termination and cursor count.
    let consumed_frames = muted_end + 1;
    let remaining: Vec<f32> = src.collect();
    assert_eq!(
        remaining.len(),
        (expected_total_frames - consumed_frames) * ch_count
    );
    assert_eq!(cursor.load(Ordering::Relaxed), expected_total_frames);
}

#[test]
fn synced_mix_source_clamps_mixed_peaks_and_seeks_o1() {
    let rate_hz = 1_000_u32;
    let rate = nz_rate(rate_hz);
    let ch = nz_ch(2);
    let total_frames = 1_000_usize;
    let peak_frame = 10_usize;
    let peak_val = 0.8_f32;
    let ramp_step = 0.0005_f32;

    let mut a_samples = Vec::with_capacity(total_frames * 2);
    let mut b_samples = Vec::with_capacity(total_frames * 2);
    for f in 0..total_frames {
        let val = if f == peak_frame {
            peak_val
        } else {
            f as f32 * ramp_step
        };
        let neg = -val;
        a_samples.push(val);
        a_samples.push(neg);
        b_samples.push(val);
        b_samples.push(neg);
    }
    let tracks: Arc<[MixTrackControl]> = Arc::from([
        MixTrackControl::new(TrackRole::Instrumental, 1.0, 1.0, 0, Arc::from(a_samples)),
        MixTrackControl::new(TrackRole::SharedVocal, 1.0, 1.0, 0, Arc::from(b_samples)),
    ]);
    let cursor = Arc::new(AtomicUsize::new(0));
    let song = SyncedSongAudio::new(tracks, ch, rate, Arc::clone(&cursor));
    let mut src = song.decoder();

    let ms_for_frame = |f: usize| Duration::from_secs_f64(f as f64 / f64::from(rate_hz));

    // Seek directly to peak_frame and verify clamped sum [+1.0, -1.0].
    src.try_seek(ms_for_frame(peak_frame)).expect("seek ok");
    assert_eq!(cursor.load(Ordering::Relaxed), peak_frame);
    assert_eq!(src.local_frame(), peak_frame);
    assert_eq!(src.channel_idx(), 0);
    let l_clamped = src.next().expect("clamped left");
    let r_clamped = src.next().expect("clamped right");
    assert!((l_clamped - 1.0).abs() < 1e-6);
    assert!((r_clamped + 1.0).abs() < 1e-6);
    assert_eq!(cursor.load(Ordering::Relaxed), peak_frame + 1);

    // Seek mid-frame after pulling one channel; channel_idx resets to 0 cleanly.
    let mid_frame = 500_usize;
    let _ = src.next();
    assert_eq!(src.channel_idx(), 1);
    src.try_seek(ms_for_frame(mid_frame)).expect("seek ok");
    assert_eq!(cursor.load(Ordering::Relaxed), mid_frame);
    assert_eq!(src.local_frame(), mid_frame);
    assert_eq!(src.channel_idx(), 0);
    let l = src.next().expect("left at mid_frame");
    let expected_mid = 2.0 * (mid_frame as f32 * ramp_step);
    assert!((l - expected_mid).abs() < 1e-5);

    // Seek backwards.
    let back_frame = 200_usize;
    src.try_seek(ms_for_frame(back_frame))
        .expect("seek backwards ok");
    assert_eq!(cursor.load(Ordering::Relaxed), back_frame);
    let l_back = src.next().expect("left at back_frame");
    let expected_back = 2.0 * (back_frame as f32 * ramp_step);
    assert!((l_back - expected_back).abs() < 1e-5);

    // Seek past end saturates to total_frames and ends iterator.
    src.try_seek(Duration::from_secs(60))
        .expect("seek past end ok");
    assert_eq!(cursor.load(Ordering::Relaxed), total_frames);
    assert_eq!(src.next(), None);
}

#[test]
fn pcm_sfx_decode_seek_and_cache_reuses_handle() {
    let mut audio_sources = Assets::<AudioSource>::default();
    let mut pcm_assets = Assets::<PcmSfxAudio>::default();
    let mut cache = PcmSfxCache::default();

    // Empty bytes safely return None without panicking.
    let empty_handle = audio_sources.add(AudioSource {
        bytes: Arc::from([]),
    });
    assert!(
        cache
            .get_or_insert(&empty_handle, &audio_sources, &mut pcm_assets)
            .is_none()
    );

    // Real Vorbis audio fixture decodes, seeks in O(1), and hits cache on subsequent lookups.
    let beep_handle = audio_sources.add(AudioSource {
        bytes: Arc::from(BEEP_OGG),
    });
    let first = cache
        .get_or_insert(&beep_handle, &audio_sources, &mut pcm_assets)
        .expect("beep.ogg decodes");
    let second = cache
        .get_or_insert(&beep_handle, &audio_sources, &mut pcm_assets)
        .expect("cached hit");
    assert_eq!(first, second);

    let sfx = pcm_assets.get(&first).expect("pcm sfx stored");
    let mut dec = sfx.decoder();
    assert_eq!(dec.current_span_len(), None);
    assert!(dec.total_duration().is_some_and(|d| !d.is_zero()));
    dec.try_seek(Duration::from_millis(5)).expect("sfx seek ok");
    assert!(dec.next().is_some());
    dec.try_seek(Duration::from_secs(600))
        .expect("sfx saturate seek ok");
    assert_eq!(dec.next(), None);
}

#[test]
fn oxide_audio_plugin_registers_assets_and_cache() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        AudioPlugin::default(),
        OxideAudioPlugin,
    ));
    assert!(app.world().contains_resource::<Assets<SyncedSongAudio>>());
    assert!(app.world().contains_resource::<Assets<PcmSfxAudio>>());
    assert!(app.world().contains_resource::<PcmSfxCache>());
}

#[test]
fn load_for_chart_collects_tracks_without_spawning_entities() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<AudioSource>()
        .init_asset::<SyncedSongAudio>()
        .init_resource::<SongAudioPlayer>();

    let mods = ModRegistry::discover(PathBuf::new(), PathBuf::new());
    let mut inst_map = HashMap::new();
    inst_map.insert(
        "inst".to_string(),
        AudioTrackSpec {
            file: "Inst.ogg".to_string(),
            offset_ms: 0.0,
            volume: 0.9,
        },
    );
    let chart = SongChart {
        song_id: "test_song".to_string(),
        audio: ResolvedAudioConfig {
            multi_vocal: true,
            needs_voices: true,
            instrumental: inst_map,
            player_vocal: Some(AudioTrackSpec {
                file: "Voices-bf.ogg".to_string(),
                offset_ms: 120.0,
                volume: 0.8,
            }),
            opponent_vocal: Some(AudioTrackSpec {
                file: "Voices-dad.ogg".to_string(),
                offset_ms: -40.0,
                volume: 1.0,
            }),
            ..Default::default()
        },
        ..Default::default()
    };

    let expected_tracks = chart.audio.instrumental.len()
        + usize::from(chart.audio.player_vocal.is_some())
        + usize::from(chart.audio.opponent_vocal.is_some());
    let target_rate = 1.25_f64;
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  asset_server: Res<AssetServer>,
                  mut player: ResMut<SongAudioPlayer>| {
                player.load_for_chart_with_mods(
                    &mods,
                    &mut commands,
                    &asset_server,
                    &chart,
                    target_rate,
                );
            },
        )
        .expect("system runs");

    assert_eq!(
        app.world_mut()
            .query::<&AudioPlayer<SyncedSongAudio>>()
            .iter(app.world())
            .count(),
        0
    );
    let player = app.world().resource::<SongAudioPlayer>();
    assert_eq!(player.tracks.len(), expected_tracks);
    assert_eq!(player.track_handles().count(), expected_tracks);
    assert!(!player.is_ready());
    assert!((player.playback_rate - target_rate as f32).abs() < 1e-6);
}

#[test]
fn prepare_mix_waits_for_all_sources_then_spawns_one_entity_with_offsets() {
    let mut world = test_player_world();

    let (inst_handle, missing_handle) = {
        let mut audio_sources = world.resource_mut::<Assets<AudioSource>>();
        let inst = audio_sources.add(AudioSource {
            bytes: Arc::from(INST_OGG),
        });
        let missing = audio_sources.reserve_handle();
        (inst, missing)
    };

    let inst_offset_ms = 0.0_f64;
    let vocal_offset_ms = 250.0_f64;
    {
        let mut player = world.resource_mut::<SongAudioPlayer>();
        insert_track(
            &mut player,
            TrackRole::Instrumental,
            inst_offset_ms,
            1.0,
            inst_handle,
        );
        insert_track(
            &mut player,
            TrackRole::PlayerVocal,
            vocal_offset_ms,
            0.8,
            missing_handle.clone(),
        );
    }

    // Returns false until every handle has loaded into Assets<AudioSource>.
    let ready_before = world
        .run_system_once(
            |mut commands: Commands,
             audio_sources: Res<Assets<AudioSource>>,
             mut synced: ResMut<Assets<SyncedSongAudio>>,
             mut player: ResMut<SongAudioPlayer>| {
                player.prepare_mix_if_ready(&mut commands, &audio_sources, &mut synced)
            },
        )
        .expect("system runs");
    assert!(!ready_before);
    assert!(!world.resource::<SongAudioPlayer>().is_ready());

    world
        .resource_mut::<Assets<AudioSource>>()
        .insert(
            missing_handle.id(),
            AudioSource {
                bytes: Arc::from(VOICES_OGG),
            },
        )
        .expect("insert reserved audio source");

    let count_mix_entities = |world: &mut World| {
        world
            .query::<&AudioPlayer<SyncedSongAudio>>()
            .iter(world)
            .count()
    };
    assert_eq!(count_mix_entities(&mut world), 0);

    let ready_after = world
        .run_system_once(
            |mut commands: Commands,
             audio_sources: Res<Assets<AudioSource>>,
             mut synced: ResMut<Assets<SyncedSongAudio>>,
             mut player: ResMut<SongAudioPlayer>| {
                player.prepare_mix_if_ready(&mut commands, &audio_sources, &mut synced)
            },
        )
        .expect("system runs");
    assert!(ready_after);
    assert_eq!(count_mix_entities(&mut world), 1);

    // Calling prepare_mix_if_ready again is idempotent and spawns no extra entities.
    let ready_again = world
        .run_system_once(
            |mut commands: Commands,
             audio_sources: Res<Assets<AudioSource>>,
             mut synced: ResMut<Assets<SyncedSongAudio>>,
             mut player: ResMut<SongAudioPlayer>| {
                player.prepare_mix_if_ready(&mut commands, &audio_sources, &mut synced)
            },
        )
        .expect("system runs");
    assert!(ready_again);
    assert_eq!(count_mix_entities(&mut world), 1);

    let player = world.resource::<SongAudioPlayer>();
    assert!(player.is_ready());
    let expected_origin = inst_offset_ms.min(vocal_offset_ms);
    assert_eq!(player.mix_origin_ms.to_bits(), expected_origin.to_bits());
    assert_eq!(player.mix_controls.len(), 2);
    let rate = f64::from(player.mix_sample_rate.expect("rate").get());
    assert_eq!(
        player.mix_controls[0].offset_frames,
        ((inst_offset_ms - expected_origin) / MS_PER_SEC * rate).round() as isize
    );
    assert_eq!(
        player.mix_controls[1].offset_frames,
        ((vocal_offset_ms - expected_origin) / MS_PER_SEC * rate).round() as isize
    );

    let mix_entity = player.mix_entity.expect("one mix entity");
    let settings = world
        .entity(mix_entity)
        .get::<PlaybackSettings>()
        .expect("playback settings attached");
    assert!(settings.paused);
    assert_eq!(settings.volume, Volume::Linear(1.0));
    assert!(matches!(settings.mode, PlaybackMode::Once));

    // Verify SyncedSongAudio asset exists and cursor advances synchronously with samples.
    let mix_handle = player.mix_handle.as_ref().expect("mix handle").clone();
    let synced_assets = world.resource::<Assets<SyncedSongAudio>>();
    let synced_song = synced_assets.get(&mix_handle).expect("synced asset");
    let ch = usize::from(synced_song.channels().get());
    let test_frames = 100_usize;
    let mut decoder = synced_song.decoder();
    for _ in 0..(test_frames * ch) {
        assert!(decoder.next().is_some());
    }
    assert_eq!(player.cursor_frames.load(Ordering::Relaxed), test_frames);
}

#[test]
fn negative_track_offset_shifts_mix_origin_and_preserves_alignment() {
    let mut world = test_player_world();
    let (inst_handle, opp_handle) = {
        let mut audio_sources = world.resource_mut::<Assets<AudioSource>>();
        let inst = audio_sources.add(AudioSource {
            bytes: Arc::from(INST_OGG),
        });
        let opp = audio_sources.add(AudioSource {
            bytes: Arc::from(VOICES_OGG),
        });
        (inst, opp)
    };

    let inst_offset_ms = 0.0_f64;
    let opp_offset_ms = -150.0_f64;
    {
        let mut player = world.resource_mut::<SongAudioPlayer>();
        insert_track(
            &mut player,
            TrackRole::Instrumental,
            inst_offset_ms,
            1.0,
            inst_handle,
        );
        insert_track(
            &mut player,
            TrackRole::OpponentVocal,
            opp_offset_ms,
            1.0,
            opp_handle,
        );
    }

    let ok = world
        .run_system_once(
            |mut commands: Commands,
             audio_sources: Res<Assets<AudioSource>>,
             mut synced: ResMut<Assets<SyncedSongAudio>>,
             mut player: ResMut<SongAudioPlayer>| {
                player.prepare_mix_if_ready(&mut commands, &audio_sources, &mut synced)
            },
        )
        .expect("system runs");
    assert!(ok);

    let player = world.resource::<SongAudioPlayer>();
    let expected_origin = inst_offset_ms.min(opp_offset_ms);
    assert!((player.mix_origin_ms - expected_origin).abs() < 1e-9);
    let rate = f64::from(player.mix_sample_rate.expect("rate").get());
    assert_eq!(
        player.mix_controls[0].offset_frames,
        ((inst_offset_ms - expected_origin) / MS_PER_SEC * rate).round() as isize
    );
    assert_eq!(
        player.mix_controls[1].offset_frames,
        ((opp_offset_ms - expected_origin) / MS_PER_SEC * rate).round() as isize
    );
}

#[test]
fn sync_volumes_updates_atomic_controls_and_skips_unchanged_frames() {
    let mut world = test_player_world();
    let (inst_handle, player_handle, opp_handle) = {
        let mut audio_sources = world.resource_mut::<Assets<AudioSource>>();
        (
            audio_sources.add(AudioSource {
                bytes: Arc::from(INST_OGG),
            }),
            audio_sources.add(AudioSource {
                bytes: Arc::from(VOICES_OGG),
            }),
            audio_sources.add(AudioSource {
                bytes: Arc::from(VOICES_OGG),
            }),
        )
    };

    let inst_base = 0.8_f32;
    let player_base = 0.5_f32;
    let opp_base = 1.0_f32;
    {
        let mut player = world.resource_mut::<SongAudioPlayer>();
        insert_track(
            &mut player,
            TrackRole::Instrumental,
            0.0,
            inst_base,
            inst_handle,
        );
        insert_track(
            &mut player,
            TrackRole::PlayerVocal,
            0.0,
            player_base,
            player_handle,
        );
        insert_track(
            &mut player,
            TrackRole::OpponentVocal,
            0.0,
            opp_base,
            opp_handle,
        );
    }

    world
        .run_system_once(
            |mut commands: Commands,
             audio_sources: Res<Assets<AudioSource>>,
             mut synced: ResMut<Assets<SyncedSongAudio>>,
             mut player: ResMut<SongAudioPlayer>| {
                assert!(player.prepare_mix_if_ready(&mut commands, &audio_sources, &mut synced));
            },
        )
        .expect("prepare ok");

    let inst_bus = 0.5_f32;
    let player_bus = 0.6_f32;
    let opp_bus = 0.25_f32;
    world
        .run_system_once(
            move |mut player: ResMut<SongAudioPlayer>, mut sinks: bevy::prelude::Query<&mut AudioSink>| {
                player.sync_volumes(inst_bus, player_bus, opp_bus, &mut sinks);
            },
        )
        .expect("sync ok");

    let sentinel_vol = 0.99_f32;
    {
        let player = world.resource::<SongAudioPlayer>();
        assert!((player.mix_controls[0].volume() - inst_base * inst_bus).abs() < 1e-6);
        assert!((player.mix_controls[1].volume() - player_base * player_bus).abs() < 1e-6);
        assert!((player.mix_controls[2].volume() - opp_base * opp_bus).abs() < 1e-6);

        // Sentinel value written directly to confirm unchanged frames skip atomic writes.
        player.mix_controls[0].set_volume(sentinel_vol);
    }

    world
        .run_system_once(
            move |mut player: ResMut<SongAudioPlayer>, mut sinks: bevy::prelude::Query<&mut AudioSink>| {
                player.sync_volumes(inst_bus, player_bus, opp_bus, &mut sinks);
            },
        )
        .expect("cached skip ok");
    assert!(
        (world.resource::<SongAudioPlayer>().mix_controls[0].volume() - sentinel_vol).abs() < 1e-6
    );

    let next_inst_bus = 0.75_f32;
    world
        .run_system_once(
            move |mut player: ResMut<SongAudioPlayer>, mut sinks: bevy::prelude::Query<&mut AudioSink>| {
                player.sync_volumes(next_inst_bus, player_bus, opp_bus, &mut sinks);
            },
        )
        .expect("dirty sync ok");
    assert!(
        (world.resource::<SongAudioPlayer>().mix_controls[0].volume() - inst_base * next_inst_bus)
            .abs()
            < 1e-6
    );
}

#[test]
fn play_pause_and_pending_state_guards_handle_empty_and_unready_tracks() {
    let mut world = test_player_world();

    world
        .run_system_once(
            |player: Res<SongAudioPlayer>, sinks: bevy::prelude::Query<&mut AudioSink>| {
                assert!(player.is_ready());
                assert!(player.all_tracks_finished(&sinks));
                assert!(player.master_time_ms(&sinks).is_none());
            },
        )
        .expect("empty check ok");

    let unready_handle = world
        .resource_mut::<Assets<AudioSource>>()
        .reserve_handle();
    {
        let mut player = world.resource_mut::<SongAudioPlayer>();
        insert_track(
            &mut player,
            TrackRole::Instrumental,
            0.0,
            1.0,
            unready_handle,
        );
    }

    let pending_ms = 1200.0_f64;
    world
        .run_system_once(
            move |mut player: ResMut<SongAudioPlayer>,
                  mut sinks: bevy::prelude::Query<&mut AudioSink>| {
                player.play_from(pending_ms, &mut sinks);
                assert!(
                    player
                        .pending_play_from_ms
                        .is_some_and(|ms| (ms - pending_ms).abs() < 1e-6)
                );
            },
        )
        .expect("pending play ok");

    world
        .run_system_once(
            |player: Res<SongAudioPlayer>, sinks: bevy::prelude::Query<&mut AudioSink>| {
                assert!(!player.all_tracks_finished(&sinks));
                assert!(player.master_time_ms(&sinks).is_none());
            },
        )
        .expect("unfinished while unready");

    world
        .run_system_once(
            |mut commands: Commands, mut player: ResMut<SongAudioPlayer>| {
                player.despawn_all(&mut commands);
            },
        )
        .expect("despawn ok");
    let player = world.resource::<SongAudioPlayer>();
    assert!(player.mix_entity.is_none());
    assert!(player.pending_play_from_ms.is_none());
}
