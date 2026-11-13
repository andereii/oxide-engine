use bevy::prelude::{AudioSource, Entity, Handle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackRole {
    Instrumental,
    PlayerVocal,
    OpponentVocal,
    SpectatorVocal,
    SharedVocal,
}

#[derive(Debug, Clone)]
pub struct TrackHandle {
    pub role: TrackRole,
    pub asset_path: String,
    pub offset_ms: f64,
    pub base_volume: f32,
    pub handle: Handle<AudioSource>,
    pub entity: Option<Entity>,
    pub started: bool,
    pub pending: bool,
}

