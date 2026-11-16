pub const DEFAULT_DIFFICULTY: &str = "hard";
pub const DEFAULT_VARIATION: &str = "default";
pub const STAGES_ASSET_PATH: &str = "gameplay/stages";
pub const DEFAULT_CHARACTER_DANCE_EVERY: f32 = 1.0;
pub const DEFAULT_BPM: f64 = 100.0;
pub const STEPS_PER_BEAT: i32 = 4;
pub const SECS_PER_MIN: f64 = 60.0;
pub const MS_PER_SEC: f64 = 1000.0;

pub const DEFAULT_BOP_INTENSITY: f64 = 1.015;
pub const DEFAULT_ZOOM_RATE: f64 = 4.0;
pub const DEFAULT_ZOOM_OFFSET: f64 = 0.0;
pub const DEFAULT_CAMERA_ZOOM: f64 = 1.0;

pub const HEALTH_MIN: f64 = 0.0;
pub const HEALTH_MAX: f64 = 2.0;
pub const HEALTH_STARTING: f64 = 1.0;
pub const HEALTH_LERP_FACTOR: f64 = 0.15;

pub const COLOR_HEALTH_BAR_RED: [u8; 3] = [0xFF, 0x00, 0x00];
pub const COLOR_HEALTH_BAR_GREEN: [u8; 3] = [0x66, 0xFF, 0x33];
pub const DEFAULT_HEALTH_ICON: &str = "face";
pub const HEALTH_ICON_SIZE: f32 = 150.0;
pub const PIXEL_ICON_SIZE: f32 = 32.0;
pub const HEALTH_ICON_BOP_SCALE: f32 = 0.2;
pub const HEALTH_ICON_POSITION_OFFSET: f32 = 26.0;
pub const HEALTH_ICON_WINNING_THRESHOLD: f64 = 0.8 * HEALTH_MAX;
pub const HEALTH_ICON_LOSING_THRESHOLD: f64 = 0.2 * HEALTH_MAX;
pub const HEALTH_ICON_ANGLE_HALF_LIFE: f64 = 0.512;
pub const HEALTH_BAR_Y_RATIO_UPSCROLL: f32 = 0.9;
pub const HEALTH_BAR_Y_RATIO_DOWNSCROLL: f32 = 0.1;
pub const HEALTH_BAR_BORDER_PX: f32 = 4.0;

pub const HEALTH_KILLER_BONUS: f64 = 2.0 / 100.0 * HEALTH_MAX;
pub const HEALTH_SICK_BONUS: f64 = 1.5 / 100.0 * HEALTH_MAX;
pub const HEALTH_GOOD_BONUS: f64 = 0.75 / 100.0 * HEALTH_MAX;
pub const HEALTH_BAD_BONUS: f64 = 0.0 / 100.0 * HEALTH_MAX;
pub const HEALTH_SHIT_BONUS: f64 = -1.0 / 100.0 * HEALTH_MAX;
pub const HEALTH_MISS_BONUS: f64 = -4.0 / 100.0 * HEALTH_MAX;
pub const HEALTH_GHOST_MISS_PENALTY: f64 = -4.0 / 100.0 * HEALTH_MAX;
pub const HEALTH_HOLD_BONUS_PER_SECOND: f64 = 6.0 / 100.0 * HEALTH_MAX;
pub const HEALTH_HOLD_DROP_PENALTY_PER_SECOND: f64 = 0.0;
pub const HEALTH_HOLD_DROP_PENALTY_MAX: f64 = 0.0;

pub const SCORE_GHOST_MISS_PENALTY: f64 = -10.0;
pub const SCORE_HOLD_BONUS_PER_SECOND: f64 = 250.0;
pub const SCORE_HOLD_DROP_PENALTY_PER_SECOND: f64 = -125.0;

pub const JUDGEMENT_SICK_COMBO_BREAK: bool = false;
pub const JUDGEMENT_GOOD_COMBO_BREAK: bool = false;
pub const JUDGEMENT_BAD_COMBO_BREAK: bool = true;
pub const JUDGEMENT_SHIT_COMBO_BREAK: bool = true;
pub const COMBO_POPUP_THRESHOLD: u32 = 10;

pub const HIT_WINDOW_MS: f64 = 160.0;
pub const HOLD_DROP_PENALTY_THRESHOLD_MS: f64 = 160.0;

pub const PBOT1_MAX_SCORE: i32 = 500;
pub const PBOT1_MISS_SCORE: i32 = -100;
pub const PBOT1_SCORING_OFFSET: f64 = 54.99;
pub const PBOT1_SCORING_SLOPE: f64 = 0.080;
pub const PBOT1_MIN_SCORE: f64 = 9.0;
pub const PBOT1_PERFECT_THRESHOLD: f64 = 5.0;
pub const PBOT1_SICK_THRESHOLD: f64 = 45.0;
pub const PBOT1_GOOD_THRESHOLD: f64 = 90.0;
pub const PBOT1_BAD_THRESHOLD: f64 = 135.0;
pub const PBOT1_SHIT_THRESHOLD: f64 = 160.0;

pub const NS_PER_MS: f64 = 1_000_000.0;
pub const TURBO_DEFAULT_DELAY: f64 = 0.5;
pub const TURBO_DEFAULT_INTERVAL: f64 = 0.05;

pub const PIXELS_PER_MS: f64 = 0.45;
pub const KEY_COUNT: usize = 4;
pub const NOTE_SPLASH_CAP: usize = 6;
pub const STRUMLINE_SIZE: f32 = 104.0;
pub const STRUMLINE_INITIAL_OFFSET: f32 = -0.275 * STRUMLINE_SIZE;
pub const NOTE_NUDGE: f32 = 2.0;
pub const NOTE_SPACING: f32 = STRUMLINE_SIZE + 8.0;
pub const STRUMLINE_X_OFFSET: f32 = 48.0;
pub const STRUMLINE_Y_OFFSET: f32 = 24.0;
pub const CONFIRM_HOLD_TIME_S: f64 = 0.15;
pub const GHOST_TAP_DELAY_S: f64 = 3.0 / 8.0;
pub const COUNTDOWN_VOLUME: f32 = 0.6;
pub const SUSTAIN_MISSED_ALPHA: f32 = 0.6;
pub const SUSTAIN_MIN_REMAINING_MS: f64 = 10.0;
pub const VWOOSH_DURATION_S: f64 = 0.5;
pub const SCREEN_WIDTH: f32 = 1280.0;
pub const SCREEN_HEIGHT: f32 = 720.0;
pub const CONDUCTOR_DRIFT_THRESHOLD_MS: f64 = 65.0;
#[allow(dead_code)]
pub const MUSIC_EASE_RATIO: f64 = 42.0;
pub const AUDIO_CORRECTION_EASE_RATIO: f64 = 8.0;
pub const MAX_RELATIVE_CAM_ZOOM: f64 = 1.35;
pub const CAMERA_DECAY_RATE: f64 = 0.95;
pub const DEFAULT_CAMERA_ANGLE_DEG: f32 = 0.0;

pub const MOD_ASSET_SOURCE_ID: &str = "mods";
pub const MOD_MANIFEST_TOML: &str = "mod.toml";
pub const MOD_MANIFEST_JSON: &str = "mod.json";
pub const POLYMOD_META_JSON: &str = "_polymod_meta.json";
pub const DEFAULT_MOD_PRIORITY: i32 = 1000;
pub const DEFAULT_MOD_VERSION: &str = "1.0.0";
pub const DEFAULT_MOD_API_VERSION: &str = "0.1.0";
pub const DEFAULT_MOD_DESCRIPTION: &str = "";
pub const SCRIPT_EXT_RHAI: &str = "rhai";
pub const SCRIPT_EXT_LUA: &str = "lua";
pub const SCRIPT_EXT_WASM: &str = "wasm";
pub const SCRIPT_EXT_DISABLED: &str = "disabled";
pub const SCRIPT_DIRS_GLOBAL: &[&str] = &["scripts/modules", "scripts", "modules"];
pub const SCRIPT_DIR_SONGS: &str = "scripts/songs";
pub const SCRIPT_DIR_STAGES: &str = "scripts/stages";
pub const SCRIPT_DIR_CHARACTERS: &str = "scripts/characters";
pub const SCRIPT_DIR_NOTEKINDS: &str = "scripts/notekinds";
pub const SCRIPT_DIR_EVENTS: &str = "scripts/events";
pub const RHAI_MAX_OPERATIONS: u64 = 1_000_000;
pub const RHAI_MAX_CALL_LEVELS: usize = 64;
pub const RHAI_MAX_ARRAY_SIZE: usize = 65_536;
pub const RHAI_MAX_MAP_SIZE: usize = 32_768;
pub const RHAI_MAX_STRING_SIZE: usize = 1_048_576;

