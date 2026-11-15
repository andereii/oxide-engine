use std::time::Duration;

use bevy::audio::{AudioPlayer, AudioSink, AudioSinkPlayback, PlaybackMode, PlaybackSettings, Volume};
use bevy::prelude::{AssetServer, AudioSource, Commands, Query, Resource};

use crate::mods::ModRegistry;
use crate::play::song::song_asset_path_in;
use crate::play::types::SongChart;
use crate::utils::constants::{RESYNC_COOLDOWN_S, RESYNC_THRESHOLD_MS};

use super::sound::{TrackHandle, TrackRole};
use super::voices::VoicesGroup;

/// Only vocal tracks resync periodically (instrumental is the master clock),
/// and never before `resync_cooldown_s` settles an async `rodio` seek.
pub fn should_resync_track(role: TrackRole, cooldown_s: f64) -> bool {
    role != TrackRole::Instrumental && cooldown_s <= 0.0
}

/// Returns `Some(target_ms)` when a started, non-pending audio track has drifted from
/// `master_time_ms` beyond V-Slice's `RESYNC_THRESHOLD_MS` (40ms).
pub fn track_resync_target_ms(
    started: bool,
    pending: bool,
    actual_ms: f64,
    master_time_ms: f64,
    offset_ms: f64,
) -> Option<f64> {
    if !started || pending {
        return None;
    }
    let target_ms = (master_time_ms - offset_ms).max(0.0);
    if (actual_ms - target_ms).abs() > RESYNC_THRESHOLD_MS {
        Some(target_ms)
    } else {
        None
    }
}

#[derive(Resource, Default)]
pub struct SongAudioPlayer {
    pub tracks: Vec<TrackHandle>,
    pub voices: VoicesGroup,
    pub is_playing: bool,
    pub is_paused: bool,
    pub playback_rate: f32,
    pub instrumental_volume: f32,
    last_synced_vols: Option<[f32; 3]>,
    all_tracks_ready: bool,
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
        self.playback_rate = playback_rate as f32;
        self.instrumental_volume = 1.0;
        self.last_synced_vols = None;
        self.all_tracks_ready = false;

        self.voices = VoicesGroup {
            needs_voices: chart.audio.needs_voices,
            multi_vocal: chart.audio.multi_vocal,
            ..Default::default()
        };

        if chart.audio.instrumental.is_empty() {
            self.push_track(
                commands,
                asset_server,
                TrackRole::Instrumental,
                song_asset_path_in(mods, &chart.song_id, "Inst.ogg"),
                0.0,
                1.0,
            );
        } else {
            for spec in chart.audio.instrumental.values() {
                self.push_track(
                    commands,
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
                        commands,
                        asset_server,
                        TrackRole::PlayerVocal,
                        song_asset_path_in(mods, &chart.song_id, &spec.file),
                        spec.offset_ms,
                        spec.volume,
                    );
                }
                if let Some(spec) = &chart.audio.opponent_vocal {
                    self.push_track(
                        commands,
                        asset_server,
                        TrackRole::OpponentVocal,
                        song_asset_path_in(mods, &chart.song_id, &spec.file),
                        spec.offset_ms,
                        spec.volume,
                    );
                }
                if let Some(spec) = &chart.audio.spectator_vocal {
                    self.push_track(
                        commands,
                        asset_server,
                        TrackRole::SpectatorVocal,
                        song_asset_path_in(mods, &chart.song_id, &spec.file),
                        spec.offset_ms,
                        spec.volume,
                    );
                }
            } else {
                self.push_track(
                    commands,
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
        commands: &mut Commands,
        asset_server: &AssetServer,
        role: TrackRole,
        asset_path: String,
        offset_ms: f64,
        base_volume: f32,
    ) {
        let handle: bevy::prelude::Handle<AudioSource> = asset_server.load(&asset_path);
        let entity = commands
            .spawn((
                AudioPlayer(handle.clone()),
                PlaybackSettings {
                    mode: PlaybackMode::Once,
                    volume: Volume::Linear(base_volume),
                    speed: self.playback_rate,
                    paused: true,
                    ..Default::default()
                },
            ))
            .id();

        self.tracks.push(TrackHandle {
            role,
            asset_path,
            offset_ms,
            base_volume,
            handle,
            entity: Some(entity),
            started: false,
            pending: false,
            resync_cooldown_s: 0.0,
        });
    }

    pub fn despawn_all(&mut self, commands: &mut Commands) {
        for track in self.tracks.drain(..) {
            if let Some(entity) = track.entity {
                commands.entity(entity).despawn();
            }
        }
    }

    pub fn play_from(&mut self, start_ms: f64, sinks: &mut Query<&mut AudioSink>) {
        self.is_playing = true;
        self.is_paused = false;
        self.last_synced_vols = None;
        self.all_tracks_ready = false;
        for track in &mut self.tracks {
            let Some(entity) = track.entity else { continue };
            let Ok(mut sink) = sinks.get_mut(entity) else {
                track.pending = true;
                track.started = false;
                continue;
            };
            let target_ms = start_ms - track.offset_ms;
            sink.set_speed(self.playback_rate);
            let vol = match track.role {
                TrackRole::Instrumental => (track.base_volume * self.instrumental_volume).clamp(0.0, 1.0),
                _ => self.voices.role_volume(track.role, track.base_volume),
            };
            sink.set_volume(Volume::Linear(vol));
            if target_ms < 0.0 {
                sink.pause();
                track.pending = true;
                track.started = false;
                track.resync_cooldown_s = 0.0;
            } else {
                if target_ms > 20.0 {
                    let _ = sink.try_seek(Duration::from_secs_f64(target_ms / 1000.0));
                }
                sink.play();
                track.pending = false;
                track.started = true;
                track.resync_cooldown_s = RESYNC_COOLDOWN_S;
            }
        }
    }

    pub fn pause(&mut self, sinks: &mut Query<&mut AudioSink>) {
        self.is_paused = true;
        for track in &self.tracks {
            if let Some(entity) = track.entity {
                if let Ok(sink) = sinks.get(entity) {
                    sink.pause();
                }
            }
        }
    }

    pub fn resume(&mut self, sinks: &mut Query<&mut AudioSink>) {
        if !self.is_playing {
            return;
        }
        self.is_paused = false;
        for track in &self.tracks {
            if track.pending || !track.started {
                continue;
            }
            if let Some(entity) = track.entity {
                if let Ok(sink) = sinks.get(entity) {
                    sink.play();
                }
            }
        }
    }

    pub fn sync_volumes(
        &mut self,
        inst_vol: f32,
        player_vol: f32,
        opponent_vol: f32,
        sinks: &mut Query<&mut AudioSink>,
    ) {
        let next_vols = [inst_vol, player_vol, opponent_vol];
        if self.all_tracks_ready && self.last_synced_vols == Some(next_vols) {
            return;
        }
        self.instrumental_volume = inst_vol;
        self.voices.player_volume = player_vol;
        self.voices.opponent_volume = opponent_vol;

        let mut synced_all = true;
        for track in &self.tracks {
            let Some(entity) = track.entity else { continue };
            if let Ok(mut sink) = sinks.get_mut(entity) {
                let vol = match track.role {
                    TrackRole::Instrumental => (track.base_volume * inst_vol).clamp(0.0, 1.0),
                    _ => self.voices.role_volume(track.role, track.base_volume),
                };
                sink.set_volume(Volume::Linear(vol));
            } else {
                synced_all = false;
            }
        }
        if synced_all && self.all_tracks_ready {
            self.last_synced_vols = Some(next_vols);
        } else {
            self.last_synced_vols = None;
        }
    }

    pub fn master_time_ms(&self, sinks: &Query<&mut AudioSink>) -> Option<f64> {
        if !self.is_playing || self.is_paused {
            return None;
        }
        let inst = self.tracks.iter().find(|t| t.role == TrackRole::Instrumental)?;
        if !inst.started || inst.pending {
            return None;
        }
        let entity = inst.entity?;
        let sink = sinks.get(entity).ok()?;
        if sink.is_paused() || sink.empty() {
            return None;
        }
        let pos = sink.position();
        if inst.resync_cooldown_s > 0.0 && pos.is_zero() {
            return None;
        }
        Some(pos.as_secs_f64() * 1000.0 + inst.offset_ms)
    }

    pub fn tick(&mut self, master_time_ms: f64, elapsed_s: f64, sinks: &mut Query<&mut AudioSink>) {
        if !self.is_playing || self.is_paused {
            return;
        }
        if !self.all_tracks_ready {
            for track in &mut self.tracks {
                if track.role == TrackRole::Instrumental && !track.started {
                    if let Some(entity) = track.entity {
                        if let Ok(mut sink) = sinks.get_mut(entity) {
                            let target_ms = (master_time_ms - track.offset_ms).max(0.0);
                            if target_ms > 20.0 {
                                let _ = sink.try_seek(Duration::from_secs_f64(target_ms / 1000.0));
                            }
                            sink.set_speed(self.playback_rate);
                            sink.set_volume(Volume::Linear(
                                (track.base_volume * self.instrumental_volume).clamp(0.0, 1.0),
                            ));
                            sink.play();
                            track.started = true;
                            track.pending = false;
                            track.resync_cooldown_s = RESYNC_COOLDOWN_S;
                        }
                    }
                }
            }
            self.voices.update_pending(
                &mut self.tracks,
                master_time_ms,
                true,
                self.playback_rate,
                sinks,
            );
            self.all_tracks_ready = self.tracks.iter().all(|t| t.started && !t.pending);
        }

        let sync_reference_ms = self.master_time_ms(sinks).unwrap_or(master_time_ms);

        // Sync vocal tracks against actual instrumental time, with cooldown so async
        // `rodio::Player::try_seek` settles without spamming blocking seeks every frame.
        for track in &mut self.tracks {
            track.resync_cooldown_s = (track.resync_cooldown_s - elapsed_s).max(0.0);
            if !should_resync_track(track.role, track.resync_cooldown_s) {
                continue;
            }
            let Some(entity) = track.entity else {
                continue;
            };
            let Ok(sink) = sinks.get_mut(entity) else {
                continue;
            };
            if sink.empty() || sink.is_paused() {
                continue;
            }
            let actual_ms = sink.position().as_secs_f64() * 1000.0;
            if let Some(target_ms) = track_resync_target_ms(
                track.started,
                track.pending,
                actual_ms,
                sync_reference_ms,
                track.offset_ms,
            ) {
                let _ = sink.try_seek(Duration::from_secs_f64(target_ms / 1000.0));
                track.resync_cooldown_s = RESYNC_COOLDOWN_S;
            }
        }
    }

    /// Returns true when playback has started, is not paused, and every
    /// spawned track (instrumental and any vocal tracks) has finished playing.
    pub fn all_tracks_finished(&self, sinks: &Query<&mut AudioSink>) -> bool {
        if self.tracks.is_empty() {
            return true;
        }
        if !self.is_playing || self.is_paused {
            return false;
        }
        self.tracks.iter().all(|track| {
            let Some(entity) = track.entity else {
                return true;
            };
            match sinks.get(entity) {
                Ok(sink) => track.started && sink.empty(),
                Err(_) => true,
            }
        })
    }
}

