use bevy::prelude::Resource;

use crate::utils::constants::DEFAULT_DIFFICULTY;

#[derive(Resource, Debug, Clone)]
pub struct PlayStatePlaylist {
    pub is_story_mode: bool,
    pub playlist_song_ids: Vec<String>,
    pub campaign_score: i32,
    #[allow(dead_code)]
    pub campaign_title: String,
    #[allow(dead_code)]
    pub campaign_id: Option<String>,
    #[allow(dead_code)]
    pub campaign_difficulty: String,
}

impl Default for PlayStatePlaylist {
    fn default() -> Self {
        Self {
            is_story_mode: false,
            playlist_song_ids: Vec::new(),
            campaign_score: 0,
            campaign_title: "UNKNOWN".to_string(),
            campaign_id: None,
            campaign_difficulty: DEFAULT_DIFFICULTY.to_string(),
        }
    }
}

impl PlayStatePlaylist {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

