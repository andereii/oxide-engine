use super::sound::TrackRole;

#[derive(Debug, Clone)]
pub struct VoicesGroup {
    pub player_volume: f32,
    pub opponent_volume: f32,
    pub spectator_volume: f32,
}

impl Default for VoicesGroup {
    fn default() -> Self {
        Self {
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
}

