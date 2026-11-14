use bevy::audio::{AudioSink, AudioSinkPlayback, Volume};
use bevy::prelude::Query;

use super::sound::{TrackHandle, TrackRole};

#[derive(Debug, Clone)]
pub struct VoicesGroup {
    pub needs_voices: bool,
    pub multi_vocal: bool,
    pub player_volume: f32,
    pub opponent_volume: f32,
    pub spectator_volume: f32,
}

impl Default for VoicesGroup {
    fn default() -> Self {
        Self {
            needs_voices: true,
            multi_vocal: false,
            player_volume: 1.0,
            opponent_volume: 1.0,
            spectator_volume: 1.0,
        }
    }
}

impl VoicesGroup {
    pub fn role_volume(&self, role: TrackRole, base_volume: f32) -> f32 {
        let group_vol = match role {
            TrackRole::PlayerVocal => self.player_volume,
            TrackRole::OpponentVocal => self.opponent_volume,
            TrackRole::SpectatorVocal => self.spectator_volume,
            TrackRole::SharedVocal => self.player_volume.max(self.opponent_volume),
            TrackRole::Instrumental => 1.0,
        };
        (base_volume * group_vol).clamp(0.0, 1.0)
    }

    pub fn update_pending(
        &self,
        tracks: &mut [TrackHandle],
        master_time_ms: f64,
        master_playing: bool,
        playback_rate: f32,
        sinks: &mut Query<&mut AudioSink>,
    ) {
        if !master_playing || !self.needs_voices {
            return;
        }
        for track in tracks.iter_mut() {
            if track.role == TrackRole::Instrumental || !track.pending {
                continue;
            }
            let target_ms = master_time_ms - track.offset_ms;
            if target_ms >= 0.0 {
                if let Some(entity) = track.entity {
                    if let Ok(mut sink) = sinks.get_mut(entity) {
                        if target_ms > 20.0 {
                            let _ = sink.try_seek(std::time::Duration::from_secs_f64(target_ms / 1000.0));
                        }
                        sink.set_speed(playback_rate);
                        sink.set_volume(Volume::Linear(self.role_volume(track.role, track.base_volume)));
                        sink.play();
                        track.pending = false;
                        track.started = true;
                    }
                }
            }
        }
    }
}

