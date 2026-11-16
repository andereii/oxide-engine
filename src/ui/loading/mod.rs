mod character;
mod components;
mod systems;

use bevy::prelude::*;

/// Configuration for the reusable Funkin loading screen overlay
/// (`mic-stand.png` progress reveal + animated loading character).
#[derive(Debug, Clone, Copy)]
pub struct LoadingScreenConfig {
    /// Whether the background should be transparent instead of solid black.
    pub transparent: bool,
    /// Whether to display the `mic-stand.png` progress reveal bar.
    pub show_progress: bool,
    /// Whether to display a randomly selected loading character (`ui/loading/characters`).
    pub show_character: bool,
}

impl Default for LoadingScreenConfig {
    fn default() -> Self {
        Self {
            transparent: false,
            show_progress: true,
            show_character: true,
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
                (
                    systems::sync_loading_screen,
                    systems::sync_loading_mic_and_bar,
                    systems::sync_loading_character,
                )
                    .chain()
                    .after(crate::animations::sprite::step_animations),
            );
    }
}
