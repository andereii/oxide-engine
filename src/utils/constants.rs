pub const DEFAULT_DIFFICULTY: &str = "normal";
pub const DEFAULT_VARIATION: &str = "default";
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

pub const HEALTH_SICK_BONUS: f64 = 0.03;
pub const HEALTH_GOOD_BONUS: f64 = 0.015;
pub const HEALTH_BAD_BONUS: f64 = 0.0;
pub const HEALTH_SHIT_BONUS: f64 = -0.02;
pub const HEALTH_MISS_BONUS: f64 = -0.08;
pub const HEALTH_GHOST_MISS_PENALTY: f64 = -0.08;
pub const HEALTH_HOLD_BONUS_PER_SECOND: f64 = 0.12;
pub const HEALTH_HOLD_DROP_PENALTY_PER_SECOND: f64 = 0.0;

pub const SCORE_GHOST_MISS_PENALTY: f64 = -10.0;
pub const SCORE_HOLD_BONUS_PER_SECOND: f64 = 250.0;
pub const SCORE_HOLD_DROP_PENALTY_PER_SECOND: f64 = -125.0;

pub const JUDGEMENT_SICK_COMBO_BREAK: bool = false;
pub const JUDGEMENT_GOOD_COMBO_BREAK: bool = false;
pub const JUDGEMENT_BAD_COMBO_BREAK: bool = true;
pub const JUDGEMENT_SHIT_COMBO_BREAK: bool = true;

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
pub const STRUMLINE_SIZE: f32 = 104.0;
pub const NOTE_SPACING: f32 = STRUMLINE_SIZE + 8.0;
pub const STRUMLINE_X_OFFSET: f32 = 48.0;
pub const STRUMLINE_Y_OFFSET: f32 = 24.0;
pub const CONFIRM_HOLD_TIME_S: f64 = 0.15;
pub const GHOST_TAP_DELAY_S: f64 = 3.0 / 8.0;
pub const COUNTDOWN_VOLUME: f32 = 0.6;
pub const SUSTAIN_MISSED_ALPHA: f32 = 0.6;
