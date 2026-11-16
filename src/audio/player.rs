use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use bevy::audio::{
    AudioPlayer, AudioSink, AudioSinkPlayback, PlaybackMode, PlaybackSettings, SampleRate, Volume,
};
use bevy::prelude::{AssetServer, Assets, AudioSource, Commands, Entity, Handle, Query, Resource};

use crate::mods::ModRegistry;
use crate::play::song::song_asset_path_in;
use crate::play::types::SongChart;

use super::mixer::{MixTrackControl, PcmBuffer, SyncedSongAudio, normalize_pcm};
use super::sound::{TrackHandle, TrackRole};
use super::voices::VoicesGroup;

/// Coordinates multi-track song stems through a single sample-locked [`SyncedSongAudio`] mix.
#[derive(Resource)]
pub struct SongAudioPlayer {
    pub tracks: Vec<TrackHandle>,
    pub voices: VoicesGroup,
    pub is_playing: bool,
    pub is_paused: bool,
    pub playback_rate: f32,
    pub instrumental_volume: f32,
    last_synced_vols: Option<[f32; 3]>,
    pub(super) mix_entity: Option<Entity>,
    pub(super) mix_handle: Option<Handle<SyncedSongAudio>>,
    pub(super) mix_controls: Arc<[MixTrackControl]>,
    pub(super) cursor_frames: Arc<AtomicUsize>,
    pub(super) mix_sample_rate: Option<SampleRate>,
    pub(super) mix_origin_ms: f64,
    mix_empty: bool,
    pub(super) pending_play_from_ms: Option<f64>,
}

impl Default for SongAudioPlayer {
    fn default() -> Self {
        Self {
            tracks: Vec::new(),
            voices: VoicesGroup::default(),
            is_playing: false,
            is_paused: false,
            playback_rate: 1.0,
            instrumental_volume: 1.0,
            last_synced_vols: None,
            mix_entity: None,
            mix_handle: None,
            mix_controls: Arc::from([]),
            cursor_frames: Arc::new(AtomicUsize::new(0)),
            mix_sample_rate: None,
            mix_origin_ms: 0.0,
            mix_empty: false,
            pending_play_from_ms: None,
        }
    }
}

impl SongAudioPlayer {
    pub fn load_for_chart_with_mods(
        &mut self,
        mods: &ModRegistry,
        commands: &mut Commands,
        asset_server: &AssetServer,
        chart: &SongChart,
        playback_rate: f64,
    ) {
        self.despawn_all(commands);
        self.tracks.clear();
        self.is_playing = false;
        self.is_paused = false;
        self.playback_rate = (playback_rate as f32).max(0.01);
        self.instrumental_volume = 1.0;

        self.voices = VoicesGroup::default();

        if chart.audio.instrumental.is_empty() {
            self.push_track(
                asset_server,
                TrackRole::Instrumental,
                song_asset_path_in(mods, &chart.song_id, "Inst.ogg"),
                0.0,
                1.0,
            );
        } else {
            for spec in chart.audio.instrumental.values() {
                self.push_track(
                    asset_server,
                    TrackRole::Instrumental,
                    song_asset_path_in(mods, &chart.song_id, &spec.file),
                    spec.offset_ms,
                    spec.volume,
                );
            }
        }

        if chart.audio.needs_voices {
            if chart.audio.multi_vocal {
                if let Some(spec) = &chart.audio.player_vocal {
                    self.push_track(
                        asset_server,
                        TrackRole::PlayerVocal,
                        song_asset_path_in(mods, &chart.song_id, &spec.file),
                        spec.offset_ms,
                        spec.volume,
                    );
                }
                if let Some(spec) = &chart.audio.opponent_vocal {
                    self.push_track(
                        asset_server,
                        TrackRole::OpponentVocal,
                        song_asset_path_in(mods, &chart.song_id, &spec.file),
                        spec.offset_ms,
                        spec.volume,
                    );
                }
                if let Some(spec) = &chart.audio.spectator_vocal {
                    self.push_track(
                        asset_server,
                        TrackRole::SpectatorVocal,
                        song_asset_path_in(mods, &chart.song_id, &spec.file),
                        spec.offset_ms,
                        spec.volume,
                    );
                }
            } else {
                self.push_track(
                    asset_server,
                    TrackRole::SharedVocal,
                    song_asset_path_in(mods, &chart.song_id, &chart.audio.shared_vocal_file),
                    0.0,
                    1.0,
                );
            }
        }
    }

    fn push_track(
        &mut self,
        asset_server: &AssetServer,
        role: TrackRole,
        asset_path: String,
        offset_ms: f64,
        base_volume: f32,
    ) {
        let handle: Handle<AudioSource> = asset_server.load(&asset_path);
        self.tracks.push(TrackHandle {
            role,
            offset_ms,
            base_volume,
            handle,
        });
    }

    pub fn despawn_all(&mut self, commands: &mut Commands) {
        if let Some(entity) = self.mix_entity.take() {
            commands.entity(entity).despawn();
        }
        self.mix_handle = None;
        self.mix_controls = Arc::from([]);
        self.cursor_frames = Arc::new(AtomicUsize::new(0));
        self.mix_sample_rate = None;
        self.mix_origin_ms = 0.0;
        self.mix_empty = false;
        self.pending_play_from_ms = None;
        self.last_synced_vols = None;
    }

    /// Returns `true` when all song tracks are empty or mixed into a single [`SyncedSongAudio`] entity.
    pub fn is_ready(&self) -> bool {
        self.tracks.is_empty() || self.mix_entity.is_some() || self.mix_empty
    }

    /// Iterates over the raw [`AudioSource`] asset handles queued for the current song.
    pub fn track_handles(&self) -> impl Iterator<Item = &Handle<AudioSource>> {
        self.tracks.iter().map(|track| &track.handle)
    }

    /// Decodes, normalizes, and mixes all loaded stems into a single [`SyncedSongAudio`] entity.
    ///
    /// Returns `false` if any track handle is not yet present in `audio_sources`.
    pub fn prepare_mix_if_ready(
        &mut self,
        commands: &mut Commands,
        audio_sources: &Assets<AudioSource>,
        synced_assets: &mut Assets<SyncedSongAudio>,
    ) -> bool {
        if self.is_ready() {
            return true;
        }

        let mut raw_sources = Vec::with_capacity(self.tracks.len());
        for track in &self.tracks {
            let Some(src) = audio_sources.get(&track.handle) else {
                return false;
            };
            raw_sources.push((track, src));
        }

        let decoded: Vec<(&TrackHandle, Option<PcmBuffer>)> = raw_sources
            .into_iter()
            .map(|(track, src)| (track, PcmBuffer::decode_from_audio_source(src)))
            .collect();

        let ref_format = decoded
            .iter()
            .find_map(|(track, buf)| {
                (track.role == TrackRole::Instrumental)
                    .then_some(buf.as_ref())
                    .flatten()
                    .map(|b| (b.channels, b.sample_rate))
            })
            .or_else(|| {
                decoded
                    .iter()
                    .find_map(|(_, buf)| buf.as_ref().map(|b| (b.channels, b.sample_rate)))
            });

        let Some((target_channels, target_rate)) = ref_format else {
            self.mix_empty = true;
            return true;
        };

        let mix_origin_ms = self.tracks.iter().map(|t| t.offset_ms).fold(0.0_f64, f64::min);
        let rate_f64 = f64::from(target_rate.get());
        let controls: Vec<MixTrackControl> = decoded
            .into_iter()
            .filter_map(|(track, buf_opt)| {
                let buf = buf_opt?;
                let samples = normalize_pcm(&buf, target_channels, target_rate);
                let offset_frames =
                    ((track.offset_ms - mix_origin_ms) / 1000.0 * rate_f64).round() as isize;
                let initial_vol = self.effective_track_volume(track.role, track.base_volume);
                Some(MixTrackControl::new(
                    track.role,
                    track.base_volume,
                    initial_vol,
                    offset_frames,
                    samples,
                ))
            })
            .collect();

        if controls.is_empty() {
            self.mix_empty = true;
            return true;
        }

        let mix_controls: Arc<[MixTrackControl]> = Arc::from(controls);
        let cursor_frames = Arc::new(AtomicUsize::new(0));
        let synced = SyncedSongAudio::new(
            Arc::clone(&mix_controls),
            target_channels,
            target_rate,
            Arc::clone(&cursor_frames),
        );
        let mix_handle = synced_assets.add(synced);
        let mix_entity = commands
            .spawn((
                AudioPlayer(mix_handle.clone()),
                PlaybackSettings {
                    mode: PlaybackMode::Once,
                    volume: Volume::Linear(1.0),
                    speed: self.playback_rate,
                    paused: true,
                    ..Default::default()
                },
            ))
            .id();

        self.mix_entity = Some(mix_entity);
        self.mix_handle = Some(mix_handle);
        self.mix_controls = mix_controls;
        self.cursor_frames = cursor_frames;
        self.mix_sample_rate = Some(target_rate);
        self.mix_origin_ms = mix_origin_ms;
        self.mix_empty = false;
        true
    }

    pub fn play_from(&mut self, start_ms: f64, sinks: &mut Query<&mut AudioSink>) {
        self.is_playing = true;
        self.is_paused = false;
        self.last_synced_vols = None;
        if self.tracks.is_empty() || self.mix_empty {
            self.pending_play_from_ms = None;
            return;
        }
        if self.start_sink(start_ms, sinks) {
            self.pending_play_from_ms = None;
        } else {
            self.pending_play_from_ms = Some(start_ms);
        }
    }

    pub fn pause(&mut self, sinks: &mut Query<&mut AudioSink>) {
        self.is_paused = true;
        if let Some(entity) = self.mix_entity
            && let Ok(sink) = sinks.get(entity)
        {
            sink.pause();
        }
    }

    pub fn resume(&mut self, sinks: &mut Query<&mut AudioSink>) {
        if !self.is_playing {
            return;
        }
        self.is_paused = false;
        if let Some(pending_ms) = self.pending_play_from_ms {
            if self.start_sink(pending_ms, sinks) {
                self.pending_play_from_ms = None;
            }
            return;
        }
        if let Some(entity) = self.mix_entity
            && let Ok(sink) = sinks.get(entity)
        {
            sink.play();
        }
    }

    pub fn sync_volumes(
        &mut self,
        inst_vol: f32,
        player_vol: f32,
        opponent_vol: f32,
        _sinks: &mut Query<&mut AudioSink>,
    ) {
        let next_vols = [inst_vol, player_vol, opponent_vol];
        let ready_to_cache =
            !self.mix_controls.is_empty() || self.tracks.is_empty() || self.mix_empty;
        if ready_to_cache && self.last_synced_vols == Some(next_vols) {
            return;
        }
        self.instrumental_volume = inst_vol;
        self.voices.player_volume = player_vol;
        self.voices.opponent_volume = opponent_vol;

        for control in self.mix_controls.iter() {
            let vol = self.effective_track_volume(control.role, control.base_volume);
            control.set_volume(vol);
        }

        if ready_to_cache {
            self.last_synced_vols = Some(next_vols);
        } else {
            self.last_synced_vols = None;
        }
    }

    pub fn master_time_ms(&self, sinks: &Query<&mut AudioSink>) -> Option<f64> {
        if !self.is_playing || self.is_paused || self.pending_play_from_ms.is_some() {
            return None;
        }
        let entity = self.mix_entity?;
        let sink = sinks.get(entity).ok()?;
        if sink.is_paused() || sink.empty() {
            return None;
        }
        let rate = self.mix_sample_rate?;
        let frames = self.cursor_frames.load(Ordering::Relaxed);
        Some(self.mix_origin_ms + (frames as f64 / f64::from(rate.get())) * 1000.0)
    }

    pub fn tick(&mut self, _master_time_ms: f64, _elapsed_s: f64, sinks: &mut Query<&mut AudioSink>) {
        if !self.is_playing || self.is_paused {
            return;
        }
        if let Some(pending_ms) = self.pending_play_from_ms
            && self.start_sink(pending_ms, sinks)
        {
            self.pending_play_from_ms = None;
        }
    }

    /// Returns true when playback has started, is not paused, and the
    /// mixed song entity has finished playing.
    pub fn all_tracks_finished(&self, sinks: &Query<&mut AudioSink>) -> bool {
        if self.tracks.is_empty() {
            return true;
        }
        if !self.is_playing || self.is_paused {
            return false;
        }
        if self.mix_empty {
            return true;
        }
        let Some(entity) = self.mix_entity else {
            return false;
        };
        match sinks.get(entity) {
            Ok(sink) => self.pending_play_from_ms.is_none() && sink.empty(),
            Err(_) => true,
        }
    }

    fn effective_track_volume(&self, role: TrackRole, base_volume: f32) -> f32 {
        match role {
            TrackRole::Instrumental => (base_volume * self.instrumental_volume).clamp(0.0, 1.0),
            _ => self.voices.role_volume(role, base_volume),
        }
    }

    fn start_sink(&self, start_ms: f64, sinks: &mut Query<&mut AudioSink>) -> bool {
        let Some(entity) = self.mix_entity else {
            return false;
        };
        let Ok(mut sink) = sinks.get_mut(entity) else {
            return false;
        };
        let speed = self.playback_rate.max(0.01);
        sink.set_speed(speed);
        sink.set_volume(Volume::Linear(1.0));
        let target_mix_ms = (start_ms - self.mix_origin_ms).max(0.0);
        if target_mix_ms > 0.0 || self.cursor_frames.load(Ordering::Relaxed) > 0 {
            let seek_secs = (target_mix_ms / 1000.0) / f64::from(speed);
            if seek_secs.is_finite()
                && sink.try_seek(Duration::from_secs_f64(seek_secs.max(0.0))).is_ok()
                && let Some(rate) = self.mix_sample_rate
            {
                let frames =
                    ((target_mix_ms / 1000.0) * f64::from(rate.get())).round().max(0.0) as usize;
                self.cursor_frames.store(frames, Ordering::Relaxed);
            }
        }
        sink.play();
        true
    }
}
