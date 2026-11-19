mod components;
mod persistence;
mod systems;

use bevy::prelude::*;

use crate::input::Controls;

pub use components::{SOUNDTRAY_CAMERA_ORDER, SOUNDTRAY_RENDER_LAYER, SoundTrayOverlayCamera};
use components::{BAR_COUNT, TRAY_HIDDEN_Y_FLX, TRAY_HIDE_DELAY_S, TRAY_SHOWN_Y_FLX};
use persistence::{load_sound_preferences, snap_volume};

#[derive(Resource, Debug)]
pub struct SoundTrayState {
    pub volume: f32,
    pub muted: bool,
    pub(super) visible: bool,
    pub(super) timer_s: f32,
    pub(super) current_y: f32,
    pub(super) lerp_y_pos: f32,
    pub(super) current_alpha: f32,
    pub(super) alpha_target: f32,
}

impl Default for SoundTrayState {
    fn default() -> Self {
        let prefs = load_sound_preferences();
        Self {
            volume: prefs.volume,
            muted: prefs.muted,
            visible: false,
            timer_s: 0.0,
            current_y: TRAY_HIDDEN_Y_FLX,
            lerp_y_pos: TRAY_HIDDEN_Y_FLX,
            current_alpha: 0.0,
            alpha_target: 0.0,
        }
    }
}

impl SoundTrayState {
    #[inline]
    pub fn effective_volume(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            self.volume.clamp(0.0, 1.0)
        }
    }

    #[inline]
    pub fn has_audible_volume(&self) -> bool {
        !self.muted && self.volume > 0.0
    }

    #[inline]
    pub fn active_bars(&self) -> usize {
        if !self.has_audible_volume() {
            0
        } else {
            ((self.volume * BAR_COUNT as f32).round() as usize).min(BAR_COUNT)
        }
    }

    pub fn change_volume(&mut self, delta: f32) {
        self.muted = false;
        self.volume = snap_volume(self.volume + delta);
    }

    pub fn toggle_muted(&mut self) {
        self.muted = !self.muted;
    }

    pub fn show_tray(&mut self) {
        self.timer_s = TRAY_HIDE_DELAY_S;
        self.lerp_y_pos = TRAY_SHOWN_Y_FLX;
        self.visible = true;
        self.alpha_target = 1.0;
    }
}

pub struct SoundTrayPlugin;

impl Plugin for SoundTrayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Controls>()
            .init_resource::<SoundTrayState>()
            .add_systems(Startup, systems::setup_soundtray)
            .add_systems(
                Update,
                (systems::handle_soundtray_input, systems::update_soundtray).chain(),
            )
            .add_systems(Last, systems::sync_master_volume);
    }
}
