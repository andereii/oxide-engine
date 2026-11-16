mod components;
mod systems;

use bevy::prelude::*;

/// Configuration for the reusable black loading screen overlay.
#[derive(Debug, Clone, Copy)]
pub struct LoadingScreenConfig {
    /// Whether the background should be transparent instead of solid black.
    pub transparent: bool,
    /// Delay in seconds before the throbber begins fading in.
    pub delay_secs: f32,
    /// Fade-in duration in seconds for the throbber.
    pub fade_in_secs: f32,
    /// Whether to display the bottom-right spinning throbber.
    pub show_throbber: bool,
}

impl Default for LoadingScreenConfig {
    fn default() -> Self {
        Self {
            transparent: false,
            delay_secs: 1.0,
            fade_in_secs: 0.2,
            show_throbber: true,
        }
    }
}

/// Shared state resource controlling the global loading screen overlay.
/// Any scene or state can show/hide it and report progress without owning UI entities.
#[derive(Resource, Default, Debug)]
pub struct LoadingScreenState {
    active: bool,
    config: LoadingScreenConfig,
    progress: f32,
    generation: u64,
}

impl LoadingScreenState {
    pub fn show(&mut self, config: LoadingScreenConfig) {
        self.active = true;
        self.config = config;
        self.progress = 0.0;
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn hide(&mut self) {
        self.active = false;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn set_progress(&mut self, progress: f32) {
        self.progress = progress.clamp(0.0, 1.0);
    }

    #[allow(dead_code)]
    pub fn progress(&self) -> f32 {
        self.progress
    }

    pub(super) fn config(&self) -> LoadingScreenConfig {
        self.config
    }

    pub(super) fn generation(&self) -> u64 {
        self.generation
    }
}

/// Registers the reusable loading screen overlay systems.
pub struct LoadingScreenPlugin;

impl Plugin for LoadingScreenPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadingScreenState>()
            .init_resource::<components::LoadingScreenRuntime>()
            .add_systems(
                Update,
                (systems::sync_loading_screen, systems::animate_loading_throbber).chain(),
            );
    }
}
