use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use super::super::types::PauseMode;

pub const PAUSE_RENDER_LAYER: usize = 29;
pub const PAUSE_CAMERA_ORDER: isize = 50;

pub(super) const PAUSE_BG_Z: f32 = 0.0;
pub(super) const PAUSE_META_Z: f32 = 10.0;
pub(super) const PAUSE_MENU_Z: f32 = 20.0;

pub(super) const BG_TARGET_ALPHA: f32 = 0.6;
pub(super) const BG_TWEEN_DURATION_S: f32 = 0.8;

pub(super) const MUSIC_FADE_IN_TIME_S: f32 = 5.0;
pub(super) const MUSIC_FINAL_VOLUME: f32 = 0.75;

pub(super) const CHARTER_FADE_DELAY_S: f32 = 15.0;
pub(super) const CHARTER_FADE_DURATION_S: f32 = 0.75;

pub(super) const META_TWEEN_DURATION_S: f32 = 1.8;
pub(super) const META_STAGGER_DELAY_S: f32 = 0.1;

pub(super) const MENU_TWEEN_DURATION_S: f32 = 0.33;
pub(super) const MENU_BASE_X_FLX: f32 = 90.0;
pub(super) const MENU_SCROLL_SOUND_PATH: &str = "ui/main-menu/scroll-menu.ogg";
pub(super) const MENU_SCROLL_SOUND_VOLUME: f32 = 0.4;

pub(super) const OFFSET_MAX_MS: f64 = 1500.0;
pub(super) const OFFSET_MIN_MS: f64 = -1500.0;
pub(super) const OFFSET_FAST_DELAY_S: f32 = 0.5;
pub(super) const OFFSET_FAST_RATE_MS_PER_S: f64 = 30.0;

#[inline]
pub(super) fn pause_render_layer() -> RenderLayers {
    RenderLayers::layer(PAUSE_RENDER_LAYER)
}

#[derive(Component)]
pub struct PauseOverlayCamera;

#[derive(Component)]
pub(super) struct PauseSubSceneEntity;

#[derive(Component)]
pub(super) struct PauseBackgroundVisual;

#[derive(Component)]
pub(super) struct PauseMetadataItem {
    pub(super) start_y_flx: f32,
    pub(super) target_y_flx: f32,
    pub(super) delay_s: f32,
    pub(super) elapsed_s: f32,
    pub(super) is_practice_only: bool,
    pub(super) is_artist_slot: bool,
    pub(super) is_deaths_slot: bool,
    pub(super) is_offset_slot: bool,
}

#[derive(Component)]
pub(super) struct PauseMenuEntryVisual {
    pub(super) index: usize,
    pub(super) action: PauseMenuAction,
    pub(super) start_x_flx: f32,
    pub(super) start_y_flx: f32,
    pub(super) target_x_flx: f32,
    pub(super) target_y_flx: f32,
    pub(super) current_x_flx: f32,
    pub(super) current_y_flx: f32,
    pub(super) tween_elapsed_s: f32,
    pub(super) alpha: f32,
}

#[derive(Component)]
pub(super) struct PauseMusicAudio {
    pub(super) fade_elapsed_s: f32,
    pub(super) random_seek_done: bool,
}

#[derive(Debug, Clone)]
pub(super) enum PauseMenuAction {
    Resume,
    RestartSong,
    SwitchMode(PauseMode),
    ChangeDifficulty(String),
    EnablePracticeMode,
    SkipCutscene,
    RestartCutscene,
    SkipConversation,
    RestartConversation,
    ExitToMenu,
    ReturnToChartEditor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CharterCyclePhase {
    WaitingToFadeOut,
    FadingOut,
    FadingIn,
}

#[derive(Resource)]
pub struct PauseSubSceneState {
    pub(super) active: bool,
    pub(super) mode: PauseMode,
    pub(super) current_entry: usize,
    pub(super) entry_count: usize,
    pub(super) just_opened: bool,
    pub(super) allow_input: bool,
    pub(super) debug_hidden: bool,
    pub(super) bg_elapsed_s: f32,
    pub(super) charter_phase: CharterCyclePhase,
    pub(super) charter_timer_s: f32,
    pub(super) showing_charter: bool,
    pub(super) artist_label: String,
    pub(super) charter_label: String,
    pub(super) fast_offset: bool,
    pub(super) last_offset_press_s: f32,
    pub(super) needs_menu_rebuild: bool,
    pub(super) music_was_focused: bool,
}

impl Default for PauseSubSceneState {
    fn default() -> Self {
        Self {
            active: false,
            mode: PauseMode::Standard,
            current_entry: 0,
            entry_count: 0,
            just_opened: true,
            allow_input: true,
            debug_hidden: false,
            bg_elapsed_s: 0.0,
            charter_phase: CharterCyclePhase::WaitingToFadeOut,
            charter_timer_s: CHARTER_FADE_DELAY_S,
            showing_charter: false,
            artist_label: "Artist: Kawai Sprite".to_string(),
            charter_label: "Charter: Unknown".to_string(),
            fast_offset: false,
            last_offset_press_s: 0.0,
            needs_menu_rebuild: false,
            music_was_focused: true,
        }
    }
}
