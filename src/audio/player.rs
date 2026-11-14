use std::time::Duration;

use bevy::audio::{AudioPlayer, AudioSink, AudioSinkPlayback, PlaybackMode, PlaybackSettings, Volume};
use bevy::prelude::{AssetServer, AudioSource, Commands, Query, Resource};

use crate::play::song::song_asset_path;
use crate::play::types::SongChart;

use super::sound::{TrackHandle, TrackRole};
use super::voices::VoicesGroup;

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
    pub fn load_for_chart(
        &mut self,
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
                song_asset_path(&chart.song_id, "Inst.ogg"),
                0.0,
                1.0,
            );
        } else {
            for spec in chart.audio.instrumental.values() {
                self.push_track(
                    commands,
                    asset_server,
                    TrackRole::Instrumental,
                    song_asset_path(&chart.song_id, &spec.file),
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
                        song_asset_path(&chart.song_id, &spec.file),
                        spec.offset_ms,
                        spec.volume,
                    );
                }
                if let Some(spec) = &chart.audio.opponent_vocal {
                    self.push_track(
                        commands,
                        asset_server,
                        TrackRole::OpponentVocal,
                        song_asset_path(&chart.song_id, &spec.file),
                        spec.offset_ms,
                        spec.volume,
                    );
                }
                if let Some(spec) = &chart.audio.spectator_vocal {
                    self.push_track(
                        commands,
                        asset_server,
                        TrackRole::SpectatorVocal,
                        song_asset_path(&chart.song_id, &spec.file),
                        spec.offset_ms,
                        spec.volume,
                    );
                }
            } else {
                self.push_track(
                    commands,
                    asset_server,
                    TrackRole::SharedVocal,
                    song_asset_path(&chart.song_id, &chart.audio.shared_vocal_file),
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
            } else {
                if target_ms > 20.0 {
                    let _ = sink.try_seek(Duration::from_secs_f64(target_ms / 1000.0));
                }
                sink.play();
                track.pending = false;
                track.started = true;
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
        let entity = inst.entity?;
        let sink = sinks.get(entity).ok()?;
        if sink.is_paused() || sink.empty() {
            return None;
        }
        Some(sink.position().as_secs_f64() * 1000.0 + inst.offset_ms)
    }

    pub fn tick(&mut self, master_time_ms: f64, sinks: &mut Query<&mut AudioSink>) {
        if !self.is_playing || self.is_paused || self.all_tracks_ready {
            return;
        }
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
}

