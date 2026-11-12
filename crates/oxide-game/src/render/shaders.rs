//! Gameplay shader families and their stage/character attachments
//! (`docs/M6-CONTRACT.md` §15).
//!
//! One WGSL fragment covers the three frozen per-sprite families
//! (`AdjustColor`, `DropShadow`, note `HSV`). The scene attaches a
//! [`LayerShader`] to each layer material; [`LayerShaderMaterial`] carries the
//! uniform union and the optional alternate-mask page.
//!
//! The HSBC matrices and the contrast curve are derived in `f64` (the scene's
//! numeric convention) and cast to `f32` exactly once when the uniforms are
//! built, matching the existing vertex-cast rule.

use std::marker::PhantomData;

use bevy::asset::uuid::Uuid;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::{Shader, ShaderRef};
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin};

use super::geometry::{GAME_HEIGHT, GAME_WIDTH, TICK_SECONDS};
use super::load::Role;

/// Stable asset id for the embedded material shader.
///
/// A fixed UUID keeps `Material2d::fragment_shader()` synchronous and
/// app-independent: the shader is inserted into `Assets<Shader>` before the
/// first renderer update, so no `AssetServer` load task can race the first
/// frame. `PipelineCache::extract_shaders` copies the same asset id.
const LAYER_SHADER_UUID: Uuid = Uuid::from_u128(0x0_1de7_6a11_5e11_0000_6f78_6964_6501);

/// The WGSL source embedded into the binary.
const LAYER_SHADER_WGSL: &str = include_str!("layer_shader.wgsl");

/// One HSBC parameter set: `hue` degrees, `saturation`/`contrast` percent and
/// `brightness` on the native 0..255 scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Hsbc {
    pub(crate) brightness: f64,
    pub(crate) hue: f64,
    pub(crate) contrast: f64,
    pub(crate) saturation: f64,
}

impl Hsbc {
    pub(crate) const fn new(brightness: f64, hue: f64, contrast: f64, saturation: f64) -> Self {
        Self {
            brightness,
            hue,
            contrast,
            saturation,
        }
    }
}

/// `DropShadowShader` settings for one character layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DropShadow {
    pub(crate) hsbc: Hsbc,
    /// Rim color normalized by 255 (already divided in the table below).
    pub(crate) color: [f64; 3],
    /// Degrees; native semantics 0 = right, 90 = up.
    pub(crate) angle: f64,
    /// Current animation-frame rotation (no loaded frame carries one yet, so
    /// the loader keeps this at 0.0).
    pub(crate) angle_offset: f64,
    /// Offset in texture-space pixels.
    pub(crate) distance: f64,
    pub(crate) strength: f64,
    pub(crate) threshold: f64,
    pub(crate) mask_threshold: f64,
    pub(crate) use_alt_mask: bool,
    /// Loaded alternate-mask page index.
    pub(crate) mask_page: Option<usize>,
    /// Current animation-frame UV bounds `[left, top, right, bottom]`.
    pub(crate) frame_bounds: [f64; 4],
}

impl Default for DropShadow {
    fn default() -> Self {
        Self {
            hsbc: Hsbc::new(0.0, 0.0, 0.0, 0.0),
            color: [0.0, 0.0, 0.0],
            angle: 0.0,
            angle_offset: 0.0,
            distance: 15.0,
            strength: 1.0,
            threshold: 0.1,
            mask_threshold: 1.0,
            use_alt_mask: false,
            mask_page: None,
            frame_bounds: [0.0, 0.0, 1.0, 1.0],
        }
    }
}

/// Note `HSVShader` multiplicative factors (defaults 1/1/1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Hsv {
    pub(crate) hue: f64,
    pub(crate) sat: f64,
    pub(crate) val: f64,
}

impl Hsv {
    pub(crate) const fn new(hue: f64, sat: f64, val: f64) -> Self {
        Self { hue, sat, val }
    }
}

/// `SserafimShader` settings (`docs/M6-CONTRACT.md` §15.1, inline native
/// GLSL). The scene's 1 ms tick tween/beat state is copied into every
/// attached character and stage-prop layer exactly once per tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Sserafim {
    pub(crate) hsbc: Hsbc,
    /// `darkAmt`, `0..=1`.
    pub(crate) darken_amount: f64,
    /// Normalized `lightColor` (the native default is white).
    pub(crate) pulse_light_color: [f64; 3],
    /// `pulseStrength`.
    pub(crate) pulse_light_strength: f64,
    /// `truckStrength`.
    pub(crate) truck_light_strength: f64,
    /// `isChar`: characters use the 5/3 dark/light divisors, stage props
    /// the 1/2 divisors.
    pub(crate) is_char: bool,
}

impl Sserafim {
    pub(crate) const fn new(is_char: bool) -> Self {
        Self {
            hsbc: Hsbc::new(0.0, 0.0, 0.0, 0.0),
            darken_amount: 0.0,
            pulse_light_color: [1.0, 1.0, 1.0],
            pulse_light_strength: 0.0,
            truck_light_strength: 0.0,
            is_char,
        }
    }
}

impl Default for Sserafim {
    fn default() -> Self {
        Self::new(false)
    }
}

/// Native `WiggleEffectRuntime` effect enum (`wiggle.frag`).
///
/// All five observed effect types are frozen in the WGSL; the shipped stages
/// construct only `Dreamy`, so the remaining variants are kept for the shader
/// contract rather than removed.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WiggleEffect {
    Dreamy = 0,
    Wavy = 1,
    HeatWaveHorizontal = 2,
    HeatWaveVertical = 3,
    Flag = 4,
}

/// `WiggleEffectRuntime` state. `time` starts at 0 and advances by
/// `TICK_SECONDS` on every scene tick (M6-CONTRACT §15).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Wiggle {
    pub(crate) effect: WiggleEffect,
    pub(crate) speed: f64,
    pub(crate) frequency: f64,
    /// UV units.
    pub(crate) amplitude: f64,
    /// Seconds.
    pub(crate) time: f64,
}

impl Wiggle {
    const fn new(effect: WiggleEffect, speed: f64, frequency: f64, amplitude: f64) -> Self {
        Self {
            effect,
            speed,
            frequency,
            amplitude,
            time: 0.0,
        }
    }
}

/// Default `uRainColor` (`0xFF6680CC`) normalized by 255.
pub(crate) const RAIN_COLOR_DEFAULT: [f64; 3] = [
    0x66 as f64 / 255.0,
    0x80 as f64 / 255.0,
    0xCC as f64 / 255.0,
];
/// `phillyStreetsErect` `rainColor = 0xFFa8adb5`.
pub(crate) const RAIN_COLOR_ERECT: [f64; 3] = [
    0xA8 as f64 / 255.0,
    0xAD as f64 / 255.0,
    0xB5 as f64 / 255.0,
];

/// `phillyBlazin` `rainTimeScale` smooth-lerp constants
/// (`MathUtil.smoothLerpPrecision(rainTimeScale, 0.02, elapsed, 1.535, 0.01)`).
pub(crate) const BLAZIN_LERP_TARGET: f64 = 0.02;
pub(crate) const BLAZIN_LERP_SECONDS: f64 = 1.535;
pub(crate) const BLAZIN_LERP_END: f64 = 0.01;
/// `phillyBlazin` `rainTimeScale += 0.7` per note hit.
pub(crate) const BLAZIN_HIT_BOOST: f64 = 0.7;

/// `RuntimeRainShader` state (`rain.frag`, screen and sprite modes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Rain {
    /// Seconds; the native ctor starts at `1.0`.
    pub(crate) time: f64,
    /// Drop-size scale (`FlxG.height / 200` on the shipped stages).
    pub(crate) scale: f64,
    pub(crate) intensity: f64,
    /// Normalized `uRainColor`.
    pub(crate) color: [f64; 3],
    /// `uSpriteMode` (`spookyMansionErect` `bgTrees`).
    pub(crate) sprite_mode: bool,
    /// `uScreenResolution` (the frozen 1280x720 game surface).
    pub(crate) resolution: [f64; 2],
    /// `uCameraBounds = [left, top, right, bottom]`.
    pub(crate) camera_bounds: [f64; 4],
    /// `uFrameBounds = [left, top, right, bottom]` in page UV units.
    pub(crate) frame_bounds: [f64; 4],
}

impl Rain {
    fn new(scale: f64, intensity: f64, color: [f64; 3], sprite_mode: bool) -> Self {
        Self {
            time: 1.0,
            scale,
            intensity,
            color,
            sprite_mode,
            resolution: [GAME_WIDTH, GAME_HEIGHT],
            camera_bounds: [0.0, 0.0, GAME_WIDTH, GAME_HEIGHT],
            frame_bounds: [0.0, 0.0, 1.0, 1.0],
        }
    }
}

/// How a screen-mode rain layer advances across the song.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum RainDriver {
    /// `phillyStreets`/`phillyStreetsErect`: per-tick
    /// `remapToRange(songPosition, 0, music.length, start, end)`.
    Remap { start: f64, end: f64 },
    /// `phillyBlazin`: fixed `0.5` intensity with the note-hit time scale.
    Blazin,
}

/// One screen-mode rain post layer plus its per-stage driver.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ScreenRain {
    pub(crate) rain: Rain,
    pub(crate) driver: RainDriver,
}

/// The shipped screen-mode `FlxG.camera.filters` rain stages
/// (`phillyStreets`, `phillyStreetsErect`, `phillyBlazin`).
pub(crate) fn screen_rain(stage_id: &str, song_id: &str) -> Option<ScreenRain> {
    let scale = GAME_HEIGHT / 200.0;
    let (start, end) = match stage_id {
        "phillyStreets" => match song_id {
            "darnell" => (0.0, 0.1),
            "lit-up" => (0.1, 0.2),
            "2hot" => (0.2, 0.4),
            _ => (0.1, 0.2),
        },
        "phillyStreetsErect" => match song_id {
            "darnell" => (0.0, 0.01),
            "lit-up" => (0.01, 0.02),
            "2hot" => (0.02, 0.04),
            _ => (0.01, 0.02),
        },
        "phillyBlazin" => {
            return Some(ScreenRain {
                rain: Rain::new(scale, 0.5, RAIN_COLOR_DEFAULT, false),
                driver: RainDriver::Blazin,
            });
        }
        _ => return None,
    };
    let color = if stage_id == "phillyStreetsErect" {
        RAIN_COLOR_ERECT
    } else {
        RAIN_COLOR_DEFAULT
    };
    Some(ScreenRain {
        rain: Rain::new(scale, start, color, false),
        driver: RainDriver::Remap { start, end },
    })
}

/// `FlxMath.remapToRange(songPosition, 0, duration, start, end)` clamped to
/// the range. A note-less stream keeps the native no-music start value.
pub(crate) fn rain_intensity(sim_ms: f64, duration_ms: Option<f64>, start: f64, end: f64) -> f64 {
    let Some(duration_ms) = duration_ms.filter(|duration| *duration > 0.0) else {
        return start;
    };
    let progress = (sim_ms / duration_ms).clamp(0.0, 1.0);
    start + (end - start) * progress
}

/// One `phillyBlazin` tick of the note-hit time multiplier:
/// `0.02 + (scale - 0.02) * 0.01^(elapsed / 1.535)`.
pub(crate) fn blazin_time_scale_decay(scale: f64) -> f64 {
    BLAZIN_LERP_TARGET
        + (scale - BLAZIN_LERP_TARGET) * BLAZIN_LERP_END.powf(TICK_SECONDS / BLAZIN_LERP_SECONDS)
}

/// Sprite-level `WiggleEffectRuntime` attachments for the two shipped
/// schoolEvil stages (M6-CONTRACT §15.1).
fn wiggle_for(stage_id: &str, prop_name: &str) -> Option<Wiggle> {
    use WiggleEffect::Dreamy;
    match (stage_id, prop_name) {
        ("schoolEvil", "backTrees") => Some(Wiggle::new(Dreamy, 2.0 * 0.8, 4.0 * 0.4, 0.011)),
        ("schoolEvil", "school") => Some(Wiggle::new(Dreamy, 2.0, 4.0, 0.017)),
        ("schoolEvil", "street") => Some(Wiggle::new(Dreamy, 2.0, 4.0, 0.007)),
        ("schoolEvil", "trees") => Some(Wiggle::new(Dreamy, 2.0, 4.0, 0.007)),
        ("schoolEvilErect", "school") => Some(Wiggle::new(Dreamy, 2.0, 4.0, 0.017)),
        ("schoolEvilErect", "evilstreet") => Some(Wiggle::new(Dreamy, 2.0, 4.0, 0.007)),
        ("schoolEvilErect", "backspikes") => Some(Wiggle::new(Dreamy, 2.0 * 0.8, 4.0 * 0.4, 0.011)),
        ("schoolEvilErect", "backspike") => Some(Wiggle::new(Dreamy, 2.0, 4.0, 0.01)),
        _ => None,
    }
}

/// The per-layer shader attachment. `None` keeps the frozen `ColorMaterial`
/// path byte-for-byte.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum LayerShader {
    None,
    AdjustColor(Hsbc),
    DropShadow(DropShadow),
    Hsv(Hsv),
    Wiggle(Wiggle),
    Rain(Rain),
    Sserafim(Sserafim),
}

impl LayerShader {
    pub(crate) fn is_shader(self) -> bool {
        !matches!(self, Self::None)
    }

    /// True for the full-screen `phillyStreets`-family rain post layer (the
    /// harness binds the offscreen game target as its base texture).
    pub(crate) fn is_screen_rain(self) -> bool {
        matches!(self, Self::Rain(rain) if !rain.sprite_mode)
    }

    /// Builds the material uniform block. `tint` is the `LayerSpec` sRGB color
    /// (the same input `ColorMaterial` receives).
    pub(crate) fn uniforms(self, tint: [f32; 4]) -> Option<LayerUniforms> {
        let kind = match self {
            Self::None => return None,
            Self::AdjustColor(_) => 0.0,
            Self::DropShadow(_) => 1.0,
            Self::Hsv(_) => 2.0,
            Self::Wiggle(_) => 3.0,
            Self::Rain(_) => 4.0,
            Self::Sserafim(_) => 5.0,
        };
        let linear = Color::srgba(tint[0], tint[1], tint[2], tint[3]).to_linear();
        let mut uniforms = LayerUniforms {
            tint: Vec4::new(linear.red, linear.green, linear.blue, linear.alpha),
            flags: Vec4::new(kind, 0.0, 0.0, 0.0),
            hsbc: Vec4::ZERO,
            hue0: Vec4::ZERO,
            hue1: Vec4::ZERO,
            hue2: Vec4::ZERO,
            sat0: Vec4::ZERO,
            sat1: Vec4::ZERO,
            sat2: Vec4::ZERO,
            shadow0: Vec4::ZERO,
            shadow1: Vec4::ZERO,
            frame_bounds: Vec4::new(0.0, 0.0, 1.0, 1.0),
            hsv: Vec4::new(1.0, 1.0, 1.0, 0.0),
            drop_color: Vec4::ZERO,
            effect0: Vec4::ZERO,
            effect1: Vec4::ZERO,
            rain0: Vec4::ZERO,
            rain1: Vec4::ZERO,
            rain2: Vec4::ZERO,
            camera_bounds: Vec4::new(0.0, 0.0, GAME_WIDTH as f32, GAME_HEIGHT as f32),
            sserafim0: Vec4::ZERO,
            sserafim1: Vec4::ZERO,
        };
        match self {
            Self::AdjustColor(hsbc) => fill_hsbc(&mut uniforms, hsbc),
            Self::DropShadow(shadow) => {
                fill_hsbc(&mut uniforms, shadow.hsbc);
                let radians = (shadow.angle + shadow.angle_offset).to_radians();
                let (sin, cos) = radians.sin_cos();
                uniforms.flags.y =
                    f32::from(u8::from(shadow.use_alt_mask && shadow.mask_page.is_some()));
                uniforms.shadow0 = Vec4::new(
                    cos as f32,
                    sin as f32,
                    shadow.distance as f32,
                    shadow.strength as f32,
                );
                uniforms.shadow1 = Vec4::new(
                    shadow.threshold as f32,
                    shadow.mask_threshold as f32,
                    0.0,
                    0.0,
                );
                uniforms.frame_bounds = Vec4::new(
                    shadow.frame_bounds[0] as f32,
                    shadow.frame_bounds[1] as f32,
                    shadow.frame_bounds[2] as f32,
                    shadow.frame_bounds[3] as f32,
                );
                uniforms.drop_color = Vec4::new(
                    shadow.color[0] as f32,
                    shadow.color[1] as f32,
                    shadow.color[2] as f32,
                    1.0,
                );
            }
            Self::Hsv(hsv) => {
                uniforms.hsv = Vec4::new(hsv.hue as f32, hsv.sat as f32, hsv.val as f32, 0.0);
            }
            Self::Wiggle(wiggle) => {
                uniforms.effect0 = Vec4::new(
                    wiggle.effect as i32 as f32,
                    wiggle.speed as f32,
                    wiggle.frequency as f32,
                    wiggle.amplitude as f32,
                );
                uniforms.effect1 = Vec4::new(wiggle.time as f32, 0.0, 0.0, 0.0);
            }
            Self::Rain(rain) => {
                uniforms.rain0 = Vec4::new(
                    rain.time as f32,
                    rain.scale as f32,
                    rain.intensity as f32,
                    if rain.sprite_mode { 1.0 } else { 0.0 },
                );
                uniforms.rain1 = Vec4::new(
                    rain.color[0] as f32,
                    rain.color[1] as f32,
                    rain.color[2] as f32,
                    0.0,
                );
                uniforms.rain2 = Vec4::new(
                    rain.resolution[0] as f32,
                    rain.resolution[1] as f32,
                    0.0,
                    0.0,
                );
                uniforms.camera_bounds = Vec4::new(
                    rain.camera_bounds[0] as f32,
                    rain.camera_bounds[1] as f32,
                    rain.camera_bounds[2] as f32,
                    rain.camera_bounds[3] as f32,
                );
                uniforms.frame_bounds = Vec4::new(
                    rain.frame_bounds[0] as f32,
                    rain.frame_bounds[1] as f32,
                    rain.frame_bounds[2] as f32,
                    rain.frame_bounds[3] as f32,
                );
            }
            Self::Sserafim(sserafim) => {
                fill_hsbc_sserafim(&mut uniforms, sserafim.hsbc);
                uniforms.sserafim0 = Vec4::new(
                    sserafim.darken_amount as f32,
                    sserafim.pulse_light_strength as f32,
                    sserafim.truck_light_strength as f32,
                    if sserafim.is_char { 1.0 } else { 0.0 },
                );
                uniforms.sserafim1 = Vec4::new(
                    sserafim.pulse_light_color[0] as f32,
                    sserafim.pulse_light_color[1] as f32,
                    sserafim.pulse_light_color[2] as f32,
                    0.0,
                );
            }
            Self::None => {}
        }
        Some(uniforms)
    }

    /// The alternate-mask page this shader samples, if any.
    pub(crate) fn mask_page(self) -> Option<usize> {
        match self {
            Self::DropShadow(shadow) => shadow.mask_page,
            Self::None
            | Self::AdjustColor(_)
            | Self::Hsv(_)
            | Self::Wiggle(_)
            | Self::Rain(_)
            | Self::Sserafim(_) => None,
        }
    }
}

fn fill_hsbc(uniforms: &mut LayerUniforms, hsbc: Hsbc) {
    let hue = hue_columns(hsbc.hue);
    let saturation = saturation_columns(hsbc.saturation);
    fill_hsbc_matrices(uniforms, hsbc, hue, saturation);
}

/// `SserafimShader`'s inline HSBC uses its own hue-rotation/saturation
/// weights (not the `AdjustColorShader` W3C/Rec.709 pair) but shares the
/// brightness/255 and positive-contrast curve uploads.
fn fill_hsbc_sserafim(uniforms: &mut LayerUniforms, hsbc: Hsbc) {
    let hue = sserafim_hue_rows(hsbc.hue);
    let saturation = sserafim_saturation_rows(hsbc.saturation);
    fill_hsbc_matrices(uniforms, hsbc, hue, saturation);
}

fn fill_hsbc_matrices(
    uniforms: &mut LayerUniforms,
    hsbc: Hsbc,
    hue: [[f64; 3]; 3],
    saturation: [[f64; 3]; 3],
) {
    uniforms.hsbc = Vec4::new(
        (hsbc.brightness / 255.0) as f32,
        contrast_scale(hsbc.contrast) as f32,
        0.0,
        0.0,
    );
    uniforms.hue0 = Vec4::new(hue[0][0] as f32, hue[0][1] as f32, hue[0][2] as f32, 0.0);
    uniforms.hue1 = Vec4::new(hue[1][0] as f32, hue[1][1] as f32, hue[1][2] as f32, 0.0);
    uniforms.hue2 = Vec4::new(hue[2][0] as f32, hue[2][1] as f32, hue[2][2] as f32, 0.0);
    uniforms.sat0 = Vec4::new(
        saturation[0][0] as f32,
        saturation[0][1] as f32,
        saturation[0][2] as f32,
        0.0,
    );
    uniforms.sat1 = Vec4::new(
        saturation[1][0] as f32,
        saturation[1][1] as f32,
        saturation[1][2] as f32,
        0.0,
    );
    uniforms.sat2 = Vec4::new(
        saturation[2][0] as f32,
        saturation[2][1] as f32,
        saturation[2][2] as f32,
        0.0,
    );
}

/// `SserafimShader` inline GLSL hue rotation rows:
/// `m1 + cos(angle) * m2 + sin(angle) * m3` with the frozen 0.213/0.715/
/// 0.072 weights. The GLSL `mat3(...)` constructors are column-major; the
/// constants below are the resulting matrix rows because the shared
/// `hue0..2` uniform slots are consumed with `dot(row, color)`. At 0 degrees
/// the three matrices sum to identity.
fn sserafim_hue_rows(hue: f64) -> [[f64; 3]; 3] {
    const M1: [[f64; 3]; 3] = [
        [0.213, 0.715, 0.072],
        [0.213, 0.715, 0.072],
        [0.213, 0.715, 0.072],
    ];
    const M2: [[f64; 3]; 3] = [
        [0.787, -0.715, -0.072],
        [-0.213, 0.285, -0.072],
        [-0.213, -0.715, 0.928],
    ];
    const M3: [[f64; 3]; 3] = [
        [-0.213, -0.715, 0.928],
        [0.143, 0.140, -0.283],
        [-0.787, 0.715, 0.072],
    ];
    let radians = hue.to_radians();
    let (sin, cos) = radians.sin_cos();
    let mut rows = [[0.0; 3]; 3];
    for (row, values) in rows.iter_mut().enumerate() {
        for (column, value) in values.iter_mut().enumerate() {
            *value = M1[row][column] + cos * M2[row][column] + sin * M3[row][column];
        }
    }
    rows
}

/// `SserafimShader` inline GLSL saturation rows:
/// `mix(vec3(dot(color, LREC709)), color, 1 + adjusted / 100)` where positive
/// saturation is tripled first (`SserafimShader.hx:182`), with the native
/// 0.30980392/0.60784314/0.08235294 grayscale weights, expanded into the
/// shared row-major `sat0..2` slots.
fn sserafim_saturation_rows(saturation: f64) -> [[f64; 3]; 3] {
    const LUMA: [f64; 3] = [
        0.309_803_921_568_627_5,
        0.607_843_137_254_902,
        0.082_352_941_176_470_6,
    ];
    // `SserafimShader.hx:182`: positive saturation is tripled before the
    // percent conversion (the same native rule as `AdjustColorShader`).
    let adjusted = if saturation > 0.0 {
        saturation * 3.0
    } else {
        saturation
    };
    let value = 1.0 + adjusted / 100.0;
    let inverse = 1.0 - value;
    [
        [
            inverse * LUMA[0] + value,
            inverse * LUMA[1],
            inverse * LUMA[2],
        ],
        [
            inverse * LUMA[0],
            inverse * LUMA[1] + value,
            inverse * LUMA[2],
        ],
        [
            inverse * LUMA[0],
            inverse * LUMA[1],
            inverse * LUMA[2] + value,
        ],
    ]
}

/// W3C-feColorMatrix hue rotation columns (`AdjustColorShader.hx` L108-122).
fn hue_columns(hue: f64) -> [[f64; 3]; 3] {
    const WR: f64 = 0.299;
    const WG: f64 = 0.587;
    const WB: f64 = 0.114;
    let radians = hue.to_radians();
    let (sin, cos) = radians.sin_cos();
    [
        [
            WR + (1.0 - WR) * cos - WR * sin,
            WG - WG * cos - WG * sin,
            WB - WB * cos + (1.0 - WB) * sin,
        ],
        [
            WR - WR * cos + 0.143 * sin,
            WG + (1.0 - WG) * cos + 0.140 * sin,
            WB - WB * cos - 0.283 * sin,
        ],
        [
            WR - WR * cos - (1.0 - WR) * sin,
            WG - WG * cos + WG * sin,
            WB + (1.0 - WB) * cos + WB * sin,
        ],
    ]
}

/// Saturation matrix columns (`AdjustColorShader.hx` L124-137). Positive
/// saturation is tripled before the percent conversion.
fn saturation_columns(saturation: f64) -> [[f64; 3]; 3] {
    const LR: f64 = 0.2126;
    const LG: f64 = 0.7152;
    const LB: f64 = 0.0722;
    let adjusted = if saturation > 0.0 {
        saturation * 3.0
    } else {
        saturation
    };
    let value = 1.0 + adjusted / 100.0;
    let inverse = 1.0 - value;
    [
        [LR * inverse + value, LG * inverse, LB * inverse],
        [LR * inverse, LG * inverse + value, LB * inverse],
        [LR * inverse, LG * inverse, LB * inverse + value],
    ]
}

/// `contrast` percent scale with the positive-only exponential curve
/// (`AdjustColorShader.hx` L72-77).
fn contrast_scale(contrast: f64) -> f64 {
    const E: f64 = std::f64::consts::E;
    let mut value = 1.0 + contrast / 100.0;
    if value > 1.0 {
        value =
            (((0.00852259 * E.powf(4.76454 * (value - 1.0))) * 1.01) - 0.0086078159) * 10.0 + 1.0;
    }
    value
}

/// CPU mirror of the WGSL HSBC composition (tests only). Kept next to the
/// uniform builder so the order cannot drift from the shader transcription.
#[cfg(test)]
fn apply_hsbc(hsbc: Hsbc, color: [f64; 3]) -> [f64; 3] {
    let brightness = hsbc.brightness / 255.0;
    let bright = [
        color[0] + brightness,
        color[1] + brightness,
        color[2] + brightness,
    ];
    let hue = hue_columns(hsbc.hue);
    let bh = [
        bright[0] * hue[0][0] + bright[1] * hue[0][1] + bright[2] * hue[0][2],
        bright[0] * hue[1][0] + bright[1] * hue[1][1] + bright[2] * hue[1][2],
        bright[0] * hue[2][0] + bright[1] * hue[2][1] + bright[2] * hue[2][2],
    ];
    let contrast = contrast_scale(hsbc.contrast);
    let contrasted = [
        (bh[0] - 0.25) * contrast + 0.25,
        (bh[1] - 0.25) * contrast + 0.25,
        (bh[2] - 0.25) * contrast + 0.25,
    ];
    let saturation = saturation_columns(hsbc.saturation);
    [
        contrasted[0] * saturation[0][0]
            + contrasted[1] * saturation[0][1]
            + contrasted[2] * saturation[0][2],
        contrasted[0] * saturation[1][0]
            + contrasted[1] * saturation[1][1]
            + contrasted[2] * saturation[1][2],
        contrasted[0] * saturation[2][0]
            + contrasted[1] * saturation[2][1]
            + contrasted[2] * saturation[2][2],
    ]
}

/// Loaded alternate-mask pages for the two frozen DropShadow stages.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ShaderMaskPages {
    pub(crate) tankman_gf: Option<usize>,
    pub(crate) tankman_nene: Option<usize>,
    pub(crate) tankman_captain_bloody: Option<usize>,
    pub(crate) school_bf: Option<usize>,
    pub(crate) school_pico: Option<usize>,
    pub(crate) school_gf: Option<usize>,
    pub(crate) school_nene: Option<usize>,
    pub(crate) school_senpai: Option<usize>,
}

/// Per-character shader attachment for the frozen stage tables.
///
/// A stage DropShadow fully overrides `AdjustColor` on that layer; the
/// `otis-speaker`/`nene-tankmen` character-level color only applies when no
/// stage DropShadow was attached.
pub(crate) fn character_shader(
    stage_id: &str,
    role: Role,
    character_id: &str,
    masks: &ShaderMaskPages,
) -> LayerShader {
    // `sserafim.hxc:346`: every added character gets `new SserafimShader(true)`.
    if stage_id == "sserafim" {
        return LayerShader::Sserafim(Sserafim::new(true));
    }
    if let Some(shadow) = drop_shadow_for(stage_id, role, character_id, masks) {
        return LayerShader::DropShadow(shadow);
    }
    if let Some(adjust) = stage_adjust_color(stage_id, role) {
        return LayerShader::AdjustColor(adjust);
    }
    match character_id {
        "otis-speaker" | "nene-tankmen" => {
            LayerShader::AdjustColor(Hsbc::new(-40.0, -40.0, -25.0, -20.0))
        }
        _ => LayerShader::None,
    }
}

/// Stage-wide character `AdjustColor` (`setAdjustColor(b, h, c, s)` order).
fn stage_adjust_color(stage_id: &str, role: Role) -> Option<Hsbc> {
    match stage_id {
        "mainStageErect" => Some(match role {
            Role::Player => Hsbc::new(-23.0, 12.0, 7.0, 0.0),
            Role::Opponent => Hsbc::new(-33.0, -32.0, -23.0, 0.0),
            Role::Girlfriend => Hsbc::new(-30.0, -9.0, -4.0, 0.0),
        }),
        "mallXmasErect" => Some(Hsbc::new(0.0, 5.0, 0.0, 20.0)),
        "limoRideErect" => Some(Hsbc::new(-30.0, -30.0, 0.0, -20.0)),
        "phillyTrainErect" => Some(Hsbc::new(-5.0, -26.0, 0.0, -16.0)),
        "phillyStreetsErect" => Some(Hsbc::new(-20.0, -5.0, -25.0, -40.0)),
        _ => None,
    }
}

/// Stage-prop shader attachments: the schoolEvil `WiggleEffectRuntime`
/// sprites, the spookyMansionErect sprite-mode rain and the shipped
/// `AdjustColor` props (M6-CONTRACT §15.1).
/// The `sserafim` stage props that keep plain `ColorMaterial` attachment
/// (`sserafim.hxc:383-411`): the two truck lights, the two backlight
/// surfaces, the test light and the full-screen solid cover.
fn sserafim_plain_prop(prop_name: &str) -> bool {
    matches!(
        prop_name,
        "truckLight1"
            | "truckLight2"
            | "backLightColor"
            | "backLightWhite"
            | "truckTest"
            | "solidCover"
    )
}

pub(crate) fn prop_shader(stage_id: &str, prop_name: &str) -> LayerShader {
    // `sserafim.hxc:383-411`: every non-light prop gets
    // `new SserafimShader()` (isChar false); the light/cover props stay plain.
    if stage_id == "sserafim" {
        return if sserafim_plain_prop(prop_name) {
            LayerShader::None
        } else {
            LayerShader::Sserafim(Sserafim::new(false))
        };
    }
    if let Some(wiggle) = wiggle_for(stage_id, prop_name) {
        return LayerShader::Wiggle(wiggle);
    }
    if stage_id == "spookyMansionErect" && prop_name == "bgTrees" {
        // `spriteMode = true` on the `bgTrees` prop: scale
        // `FlxG.height / 200 * 2`, intensity 0.4, default rain color.
        return LayerShader::Rain(Rain::new(
            GAME_HEIGHT / 200.0 * 2.0,
            0.4,
            RAIN_COLOR_DEFAULT,
            true,
        ));
    }
    let hsbc = match stage_id {
        "mallXmasErect" if prop_name == "santa" => Some(Hsbc::new(0.0, 5.0, 0.0, 20.0)),
        "mallXmasErect" if prop_name == "bottomBoppers" => Some(Hsbc::new(20.0, 15.0, 0.0, 0.0)),
        "limoRideErect"
            if matches!(
                prop_name,
                "limoDancer1"
                    | "limoDancer2"
                    | "limoDancer3"
                    | "limoDancer4"
                    | "limoDancer5"
                    | "fastCar"
            ) =>
        {
            Some(Hsbc::new(-30.0, -30.0, 0.0, -20.0))
        }
        "phillyTrainErect" if prop_name == "train" => Some(Hsbc::new(-5.0, -26.0, 0.0, -16.0)),
        _ => None,
    };
    hsbc.map_or(LayerShader::None, LayerShader::AdjustColor)
}

/// `DropShadowShader` attachment for the two shipped stages (15.1 table).
fn drop_shadow_for(
    stage_id: &str,
    role: Role,
    character_id: &str,
    masks: &ShaderMaskPages,
) -> Option<DropShadow> {
    match stage_id {
        "tankmanBattlefieldErect" => {
            let mut shadow = DropShadow {
                hsbc: Hsbc::new(-46.0, -38.0, -25.0, -20.0),
                color: [
                    0xDF as f64 / 255.0,
                    0xEF as f64 / 255.0,
                    0x3C as f64 / 255.0,
                ],
                ..Default::default()
            };
            match role {
                Role::Player | Role::Girlfriend => {
                    shadow.angle = 90.0;
                    shadow.threshold = 0.1;
                }
                Role::Opponent => {
                    shadow.angle = 25.0;
                    shadow.threshold = 0.3;
                }
            }
            match (role, character_id) {
                (Role::Girlfriend, "gf-tankmen") => {
                    shadow.mask_page = masks.tankman_gf;
                    shadow.mask_threshold = 0.4;
                    shadow.use_alt_mask = true;
                }
                (Role::Girlfriend, "nene-tankmen") => {
                    shadow.mask_page = masks.tankman_nene;
                    shadow.mask_threshold = 0.4;
                    shadow.use_alt_mask = true;
                }
                (Role::Opponent, "tankman-bloody") => {
                    shadow.mask_page = masks.tankman_captain_bloody;
                    shadow.mask_threshold = 1.0;
                    // The native script loads the mask but keeps useAltMask
                    // false until the EnableMask event toggles it.
                    shadow.use_alt_mask = false;
                }
                _ => {}
            }
            Some(shadow)
        }
        "schoolErect" => {
            let mut shadow = DropShadow {
                hsbc: Hsbc::new(-66.0, -10.0, 24.0, -23.0),
                color: [
                    0x52 as f64 / 255.0,
                    0x35 as f64 / 255.0,
                    0x1D as f64 / 255.0,
                ],
                distance: 5.0,
                angle: 90.0,
                threshold: 0.1,
                ..Default::default()
            };
            match role {
                Role::Player => {
                    shadow.mask_threshold = 1.0;
                    shadow.use_alt_mask = true;
                    shadow.mask_page = if character_id == "pico-pixel" {
                        masks.school_pico
                    } else {
                        masks.school_bf
                    };
                }
                Role::Girlfriend => {
                    shadow.hsbc = Hsbc::new(-42.0, -10.0, 5.0, -25.0);
                    shadow.distance = 3.0;
                    shadow.threshold = 0.3;
                    shadow.mask_threshold = 1.0;
                    shadow.use_alt_mask = true;
                    shadow.mask_page = if character_id == "nene-pixel" {
                        masks.school_nene
                    } else {
                        masks.school_gf
                    };
                }
                Role::Opponent => {
                    shadow.mask_threshold = 1.0;
                    shadow.use_alt_mask = true;
                    shadow.mask_page = masks.school_senpai;
                }
            }
            Some(shadow)
        }
        _ => None,
    }
}

/// GPU uniform block shared by every shader family. All members are `vec4`
/// so the WGSL layout has no alignment surprises.
#[derive(Clone, Debug, Default, ShaderType)]
pub(crate) struct LayerUniforms {
    pub(crate) tint: Vec4,
    pub(crate) flags: Vec4,
    pub(crate) hsbc: Vec4,
    pub(crate) hue0: Vec4,
    pub(crate) hue1: Vec4,
    pub(crate) hue2: Vec4,
    pub(crate) sat0: Vec4,
    pub(crate) sat1: Vec4,
    pub(crate) sat2: Vec4,
    pub(crate) shadow0: Vec4,
    pub(crate) shadow1: Vec4,
    pub(crate) frame_bounds: Vec4,
    pub(crate) hsv: Vec4,
    pub(crate) drop_color: Vec4,
    /// Wiggle: x = effect enum, y = speed, z = frequency, w = amplitude (UV).
    pub(crate) effect0: Vec4,
    /// Wiggle: x = time (seconds).
    pub(crate) effect1: Vec4,
    /// Rain: x = time (s), y = scale, z = intensity, w = sprite mode (0/1).
    pub(crate) rain0: Vec4,
    /// Rain: rgb = normalized rain color.
    pub(crate) rain1: Vec4,
    /// Rain: xy = screen resolution.
    pub(crate) rain2: Vec4,
    /// Rain: [left, top, right, bottom] camera view bounds.
    pub(crate) camera_bounds: Vec4,
    /// Sserafim: x = darken amount, y = pulse strength, z = truck strength,
    /// w = isChar (0/1).
    pub(crate) sserafim0: Vec4,
    /// Sserafim: rgb = normalized pulse light color.
    pub(crate) sserafim1: Vec4,
}

/// Bevy `Material2d` implementation for every non-plain layer.
#[derive(Asset, AsBindGroup, Clone, Debug, TypePath)]
pub(crate) struct LayerShaderMaterial {
    #[uniform(0)]
    pub(crate) uniforms: LayerUniforms,
    #[texture(1)]
    #[sampler(2)]
    pub(crate) texture: Option<Handle<Image>>,
    #[texture(3)]
    #[sampler(4)]
    pub(crate) mask_texture: Option<Handle<Image>>,
}

impl Material2d for LayerShaderMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(Handle::Uuid(LAYER_SHADER_UUID, PhantomData))
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

/// Registers the material plugin and inserts the embedded WGSL synchronously.
pub(crate) fn register(app: &mut App) -> Result<(), super::RenderError> {
    app.add_plugins(Material2dPlugin::<LayerShaderMaterial>::default());
    app.world_mut()
        .resource_mut::<Assets<Shader>>()
        .insert(
            Handle::<Shader>::Uuid(LAYER_SHADER_UUID, PhantomData).id(),
            Shader::from_wgsl(LAYER_SHADER_WGSL, "oxide_game/render/layer_shader.wgsl"),
        )
        .map_err(|error| super::RenderError::Renderer {
            detail: format!("could not insert the embedded layer shader: {error}"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected {expected}, got {actual}"
        );
    }

    fn close_f32(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-6,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn zero_hue_and_saturation_matrices_are_identity() {
        assert_eq!(
            hue_columns(0.0),
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        );
        assert_eq!(
            saturation_columns(0.0),
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        );
    }

    #[test]
    fn hue_columns_match_the_w3c_rotation() {
        let columns = hue_columns(90.0);
        // cos 90 = 0, sin 90 = 1.
        close(columns[0][0], 0.299 + (1.0 - 0.299) * 0.0 - 0.299);
        close(columns[0][1], 0.587 - 0.587 * 0.0 - 0.587);
        close(columns[0][2], 0.114 - 0.114 * 0.0 + (1.0 - 0.114));
        close(columns[1][0], 0.299 - 0.299 * 0.0 + 0.143);
        close(columns[1][1], 0.587 + (1.0 - 0.587) * 0.0 + 0.140);
        close(columns[1][2], 0.114 - 0.114 * 0.0 - 0.283);
        close(columns[2][0], 0.299 - 0.299 * 0.0 - (1.0 - 0.299));
        close(columns[2][1], 0.587 - 0.587 * 0.0 + 0.587);
        close(columns[2][2], 0.114 + (1.0 - 0.114) * 0.0 + 0.114);
    }

    #[test]
    fn positive_saturation_is_tripled_and_negative_is_not() {
        // +20 -> 60 -> sat = 1.6, inverse = -0.6.
        let positive = saturation_columns(20.0);
        close(positive[0][0], 0.2126 * -0.6 + 1.6);
        close(positive[0][1], 0.7152 * -0.6);
        close(positive[0][2], 0.0722 * -0.6);

        // -20 -> -20 -> sat = 0.8, inverse = 0.2.
        let negative = saturation_columns(-20.0);
        close(negative[0][0], 0.2126 * 0.2 + 0.8);
        close(negative[0][1], 0.7152 * 0.2);
        close(negative[0][2], 0.0722 * 0.2);
    }

    #[test]
    fn contrast_curve_only_applies_above_one() {
        close(contrast_scale(-4.0), 0.96);
        close(contrast_scale(0.0), 1.0);
        // Known reference values of the frozen curve.
        close(contrast_scale(7.0), 1.034_075_751_576_137);
        close(contrast_scale(24.0), 1.184_009_159_644_73);
        close(contrast_scale(-23.0), 0.77);
    }

    #[test]
    fn hsbc_composition_is_brightness_hue_contrast_saturation() {
        let hsbc = Hsbc::new(-23.0, 12.0, 7.0, 20.0);
        let color = [0.4, 0.5, 0.6];
        let brightness = -23.0 / 255.0;
        let bright = [
            color[0] + brightness,
            color[1] + brightness,
            color[2] + brightness,
        ];
        let hue = hue_columns(12.0);
        let bh = [
            bright[0] * hue[0][0] + bright[1] * hue[0][1] + bright[2] * hue[0][2],
            bright[0] * hue[1][0] + bright[1] * hue[1][1] + bright[2] * hue[1][2],
            bright[0] * hue[2][0] + bright[1] * hue[2][1] + bright[2] * hue[2][2],
        ];
        let contrast = contrast_scale(7.0);
        let contrasted = [
            (bh[0] - 0.25) * contrast + 0.25,
            (bh[1] - 0.25) * contrast + 0.25,
            (bh[2] - 0.25) * contrast + 0.25,
        ];
        let saturation = saturation_columns(20.0);
        let expected = [
            contrasted[0] * saturation[0][0]
                + contrasted[1] * saturation[0][1]
                + contrasted[2] * saturation[0][2],
            contrasted[0] * saturation[1][0]
                + contrasted[1] * saturation[1][1]
                + contrasted[2] * saturation[1][2],
            contrasted[0] * saturation[2][0]
                + contrasted[1] * saturation[2][1]
                + contrasted[2] * saturation[2][2],
        ];
        let actual = apply_hsbc(hsbc, color);
        for (actual, expected) in actual.iter().zip(expected) {
            close(*actual, expected);
        }
        // The composition is not a no-op on the sample.
        assert!((actual[0] - color[0]).abs() > 1e-3);
    }

    #[test]
    fn uniform_builder_casts_hsbc_once_and_keeps_tint_linear() {
        let shader = LayerShader::AdjustColor(Hsbc::new(-23.0, 12.0, 7.0, 0.0));
        let uniforms = shader.uniforms([1.0, 1.0, 1.0, 1.0]).expect("uniforms");
        close_f32(uniforms.hsbc.x, (-23.0f64 / 255.0) as f32);
        close_f32(uniforms.hsbc.y, contrast_scale(7.0) as f32);
        close_f32(uniforms.hue0.x, hue_columns(12.0)[0][0] as f32);
        close_f32(uniforms.sat0.x, 1.0);
        assert_eq!(uniforms.flags.x, 0.0);
        // White sRGB tint stays white in linear space.
        close_f32(uniforms.tint.x, 1.0);
        close_f32(uniforms.tint.w, 1.0);
    }

    #[test]
    fn tankman_drop_shadow_table_matches_the_frozen_values() {
        let masks = ShaderMaskPages {
            tankman_gf: Some(3),
            tankman_nene: Some(4),
            tankman_captain_bloody: Some(5),
            ..Default::default()
        };

        let bf = match character_shader("tankmanBattlefieldErect", Role::Player, "bf", &masks) {
            LayerShader::DropShadow(shadow) => shadow,
            other => panic!("expected DropShadow, got {other:?}"),
        };
        assert_eq!(bf.hsbc, Hsbc::new(-46.0, -38.0, -25.0, -20.0));
        close(bf.color[0], 0xDF as f64 / 255.0);
        close(bf.color[1], 0xEF as f64 / 255.0);
        close(bf.color[2], 0x3C as f64 / 255.0);
        close(bf.angle, 90.0);
        close(bf.threshold, 0.1);
        close(bf.distance, 15.0);
        close(bf.strength, 1.0);
        assert!(!bf.use_alt_mask);
        assert_eq!(bf.mask_page, None);

        let gf = match character_shader(
            "tankmanBattlefieldErect",
            Role::Girlfriend,
            "gf-tankmen",
            &masks,
        ) {
            LayerShader::DropShadow(shadow) => shadow,
            other => panic!("expected DropShadow, got {other:?}"),
        };
        close(gf.angle, 90.0);
        close(gf.mask_threshold, 0.4);
        assert!(gf.use_alt_mask);
        assert_eq!(gf.mask_page, Some(3));

        let nene = match character_shader(
            "tankmanBattlefieldErect",
            Role::Girlfriend,
            "nene-tankmen",
            &masks,
        ) {
            LayerShader::DropShadow(shadow) => shadow,
            other => panic!("expected DropShadow, got {other:?}"),
        };
        assert_eq!(nene.mask_page, Some(4));

        let dad = match character_shader(
            "tankmanBattlefieldErect",
            Role::Opponent,
            "tankman-bloody",
            &masks,
        ) {
            LayerShader::DropShadow(shadow) => shadow,
            other => panic!("expected DropShadow, got {other:?}"),
        };
        close(dad.angle, 25.0);
        close(dad.threshold, 0.3);
        close(dad.mask_threshold, 1.0);
        assert!(!dad.use_alt_mask, "the EnableMask event turns it on");
        assert_eq!(dad.mask_page, Some(5));
    }

    #[test]
    fn school_drop_shadow_table_matches_the_frozen_values() {
        let masks = ShaderMaskPages {
            school_bf: Some(10),
            school_pico: Some(11),
            school_gf: Some(12),
            school_nene: Some(13),
            school_senpai: Some(14),
            ..Default::default()
        };

        let bf = match character_shader("schoolErect", Role::Player, "bf-pixel", &masks) {
            LayerShader::DropShadow(shadow) => shadow,
            other => panic!("expected DropShadow, got {other:?}"),
        };
        assert_eq!(bf.hsbc, Hsbc::new(-66.0, -10.0, 24.0, -23.0));
        close(bf.color[0], 0x52 as f64 / 255.0);
        close(bf.color[1], 0x35 as f64 / 255.0);
        close(bf.color[2], 0x1D as f64 / 255.0);
        close(bf.distance, 5.0);
        close(bf.angle, 90.0);
        close(bf.threshold, 0.1);
        close(bf.mask_threshold, 1.0);
        assert!(bf.use_alt_mask);
        assert_eq!(bf.mask_page, Some(10));

        let pico = match character_shader("schoolErect", Role::Player, "pico-pixel", &masks) {
            LayerShader::DropShadow(shadow) => shadow,
            other => panic!("expected DropShadow, got {other:?}"),
        };
        assert_eq!(pico.mask_page, Some(11));

        let gf = match character_shader("schoolErect", Role::Girlfriend, "gf-pixel", &masks) {
            LayerShader::DropShadow(shadow) => shadow,
            other => panic!("expected DropShadow, got {other:?}"),
        };
        assert_eq!(gf.hsbc, Hsbc::new(-42.0, -10.0, 5.0, -25.0));
        close(gf.distance, 3.0);
        close(gf.threshold, 0.3);
        assert_eq!(gf.mask_page, Some(12));

        let nene = match character_shader("schoolErect", Role::Girlfriend, "nene-pixel", &masks) {
            LayerShader::DropShadow(shadow) => shadow,
            other => panic!("expected DropShadow, got {other:?}"),
        };
        assert_eq!(nene.mask_page, Some(13));

        let dad = match character_shader("schoolErect", Role::Opponent, "senpai", &masks) {
            LayerShader::DropShadow(shadow) => shadow,
            other => panic!("expected DropShadow, got {other:?}"),
        };
        assert_eq!(dad.mask_page, Some(14));
        close(dad.threshold, 0.1);
    }

    #[test]
    fn stage_adjust_color_tables_match_the_frozen_values() {
        assert_eq!(
            character_shader(
                "mainStageErect",
                Role::Player,
                "bf",
                &ShaderMaskPages::default()
            ),
            LayerShader::AdjustColor(Hsbc::new(-23.0, 12.0, 7.0, 0.0))
        );
        assert_eq!(
            character_shader(
                "mainStageErect",
                Role::Girlfriend,
                "gf",
                &ShaderMaskPages::default()
            ),
            LayerShader::AdjustColor(Hsbc::new(-30.0, -9.0, -4.0, 0.0))
        );
        assert_eq!(
            character_shader(
                "mainStageErect",
                Role::Opponent,
                "dad",
                &ShaderMaskPages::default()
            ),
            LayerShader::AdjustColor(Hsbc::new(-33.0, -32.0, -23.0, 0.0))
        );
        assert_eq!(
            character_shader(
                "mallXmasErect",
                Role::Opponent,
                "dad",
                &ShaderMaskPages::default()
            ),
            LayerShader::AdjustColor(Hsbc::new(0.0, 5.0, 0.0, 20.0))
        );
        assert_eq!(
            character_shader(
                "limoRideErect",
                Role::Player,
                "bf",
                &ShaderMaskPages::default()
            ),
            LayerShader::AdjustColor(Hsbc::new(-30.0, -30.0, 0.0, -20.0))
        );
        assert_eq!(
            character_shader(
                "phillyTrainErect",
                Role::Player,
                "bf",
                &ShaderMaskPages::default()
            ),
            LayerShader::AdjustColor(Hsbc::new(-5.0, -26.0, 0.0, -16.0))
        );
        assert_eq!(
            character_shader(
                "phillyStreetsErect",
                Role::Player,
                "bf",
                &ShaderMaskPages::default()
            ),
            LayerShader::AdjustColor(Hsbc::new(-20.0, -5.0, -25.0, -40.0))
        );
        assert_eq!(
            character_shader(
                "mainStage",
                Role::Player,
                "otis-speaker",
                &ShaderMaskPages::default()
            ),
            LayerShader::AdjustColor(Hsbc::new(-40.0, -40.0, -25.0, -20.0))
        );
        assert_eq!(
            character_shader("mainStage", Role::Player, "bf", &ShaderMaskPages::default()),
            LayerShader::None
        );
    }

    #[test]
    fn prop_adjust_color_table_matches_the_frozen_values() {
        assert_eq!(
            prop_shader("mallXmasErect", "santa"),
            LayerShader::AdjustColor(Hsbc::new(0.0, 5.0, 0.0, 20.0))
        );
        assert_eq!(
            prop_shader("mallXmasErect", "bottomBoppers"),
            LayerShader::AdjustColor(Hsbc::new(20.0, 15.0, 0.0, 0.0))
        );
        assert_eq!(
            prop_shader("limoRideErect", "limoDancer3"),
            LayerShader::AdjustColor(Hsbc::new(-30.0, -30.0, 0.0, -20.0))
        );
        assert_eq!(
            prop_shader("limoRideErect", "fastCar"),
            LayerShader::AdjustColor(Hsbc::new(-30.0, -30.0, 0.0, -20.0))
        );
        assert_eq!(
            prop_shader("phillyTrainErect", "train"),
            LayerShader::AdjustColor(Hsbc::new(-5.0, -26.0, 0.0, -16.0))
        );
        assert_eq!(prop_shader("phillyTrainErect", "street"), LayerShader::None);
    }

    #[test]
    fn hsv_uniforms_carry_the_multiplicative_factors() {
        let uniforms = LayerShader::Hsv(Hsv::new(1.0, 0.2, 1.0))
            .uniforms([1.0, 1.0, 1.0, 0.5])
            .expect("hsv uniforms");
        assert_eq!(uniforms.flags.x, 2.0);
        assert_eq!(uniforms.hsv, Vec4::new(1.0, 0.2, 1.0, 0.0));
        close_f32(uniforms.tint.w, 0.5);
    }

    #[test]
    fn drop_shadow_uniforms_pack_angles_masks_and_frame_bounds() {
        let shadow = DropShadow {
            hsbc: Hsbc::new(-46.0, -38.0, -25.0, -20.0),
            color: [
                0xDF as f64 / 255.0,
                0xEF as f64 / 255.0,
                0x3C as f64 / 255.0,
            ],
            angle: 90.0,
            angle_offset: 0.0,
            distance: 15.0,
            strength: 1.0,
            threshold: 0.3,
            mask_threshold: 0.4,
            use_alt_mask: true,
            mask_page: Some(7),
            frame_bounds: [0.25, 0.5, 0.75, 1.0],
        };
        let uniforms = LayerShader::DropShadow(shadow)
            .uniforms([1.0, 1.0, 1.0, 1.0])
            .expect("drop shadow uniforms");
        assert_eq!(uniforms.flags.x, 1.0);
        assert_eq!(uniforms.flags.y, 1.0);
        close_f32(uniforms.shadow0.x, 0.0); // cos 90
        close_f32(uniforms.shadow0.y, 1.0); // sin 90
        close_f32(uniforms.shadow0.z, 15.0);
        close_f32(uniforms.shadow1.x, 0.3);
        close_f32(uniforms.shadow1.y, 0.4);
        close_f32(uniforms.frame_bounds.x, 0.25);
        close_f32(uniforms.frame_bounds.w, 1.0);
        assert_eq!(shadow.mask_page, Some(7));
    }

    // --- 6G sub-slice 2: WiggleEffectRuntime + RuntimeRainShader -----------

    fn wiggle_of(stage_id: &str, prop: &str) -> Wiggle {
        match prop_shader(stage_id, prop) {
            LayerShader::Wiggle(wiggle) => wiggle,
            other => panic!("expected a Wiggle attachment for {stage_id}/{prop}, got {other:?}"),
        }
    }

    #[test]
    fn school_evil_wiggle_tables_match_the_frozen_values() {
        let back_trees = wiggle_of("schoolEvil", "backTrees");
        assert_eq!(back_trees.effect, WiggleEffect::Dreamy);
        close(back_trees.speed, 1.6);
        close(back_trees.frequency, 1.6);
        close(back_trees.amplitude, 0.011);
        close(back_trees.time, 0.0);

        for (prop, amplitude) in [("school", 0.017), ("street", 0.007), ("trees", 0.007)] {
            let wiggle = wiggle_of("schoolEvil", prop);
            assert_eq!(wiggle.effect, WiggleEffect::Dreamy);
            close(wiggle.speed, 2.0);
            close(wiggle.frequency, 4.0);
            close(wiggle.amplitude, amplitude);
            close(wiggle.time, 0.0);
        }

        let school = wiggle_of("schoolEvilErect", "school");
        close(school.amplitude, 0.017);
        let evilstreet = wiggle_of("schoolEvilErect", "evilstreet");
        close(evilstreet.speed, 2.0);
        close(evilstreet.frequency, 4.0);
        close(evilstreet.amplitude, 0.007);
        let backspikes = wiggle_of("schoolEvilErect", "backspikes");
        close(backspikes.speed, 1.6);
        close(backspikes.frequency, 1.6);
        close(backspikes.amplitude, 0.011);
        let backspike = wiggle_of("schoolEvilErect", "backspike");
        close(backspike.amplitude, 0.01);

        assert_eq!(prop_shader("schoolEvil", "solid"), LayerShader::None);
        assert_eq!(prop_shader("schoolEvilErect", "other"), LayerShader::None);
    }

    #[test]
    fn screen_rain_tables_match_the_frozen_values() {
        let streets = screen_rain("phillyStreets", "darnell").expect("phillyStreets rain");
        assert!(!streets.rain.sprite_mode);
        close(streets.rain.scale, 3.6);
        close(streets.rain.time, 1.0);
        close(streets.rain.intensity, 0.0);
        assert_eq!(streets.rain.color, RAIN_COLOR_DEFAULT);
        assert_eq!(
            streets.driver,
            RainDriver::Remap {
                start: 0.0,
                end: 0.1
            }
        );
        assert_eq!(
            screen_rain("phillyStreets", "lit-up")
                .expect("lit-up rain")
                .driver,
            RainDriver::Remap {
                start: 0.1,
                end: 0.2
            }
        );
        assert_eq!(
            screen_rain("phillyStreets", "2hot")
                .expect("2hot rain")
                .driver,
            RainDriver::Remap {
                start: 0.2,
                end: 0.4
            }
        );
        assert_eq!(
            screen_rain("phillyStreets", "unknown")
                .expect("fallback rain")
                .driver,
            RainDriver::Remap {
                start: 0.1,
                end: 0.2
            }
        );

        let erect = screen_rain("phillyStreetsErect", "darnell").expect("erect rain");
        assert_eq!(erect.rain.color, RAIN_COLOR_ERECT);
        close(erect.rain.scale, 3.6);
        assert_eq!(
            erect.driver,
            RainDriver::Remap {
                start: 0.0,
                end: 0.01
            }
        );
        assert_eq!(
            screen_rain("phillyStreetsErect", "2hot")
                .expect("erect 2hot")
                .driver,
            RainDriver::Remap {
                start: 0.02,
                end: 0.04
            }
        );

        let blazin = screen_rain("phillyBlazin", "blazin").expect("blazin rain");
        assert_eq!(blazin.driver, RainDriver::Blazin);
        close(blazin.rain.intensity, 0.5);
        close(blazin.rain.scale, 3.6);
        assert_eq!(blazin.rain.color, RAIN_COLOR_DEFAULT);

        assert!(screen_rain("phillyStreets", "darnell").is_some());
        assert!(screen_rain("spookyMansionErect", "spookez").is_none());
    }

    #[test]
    fn spooky_mansion_sprite_rain_matches_the_frozen_values() {
        let rain = match prop_shader("spookyMansionErect", "bgTrees") {
            LayerShader::Rain(rain) => rain,
            other => panic!("expected sprite-mode rain, got {other:?}"),
        };
        assert!(rain.sprite_mode);
        close(rain.scale, 7.2);
        close(rain.intensity, 0.4);
        close(rain.time, 1.0);
        assert_eq!(rain.color, RAIN_COLOR_DEFAULT);
        assert_eq!(rain.frame_bounds, [0.0, 0.0, 1.0, 1.0]);
        assert!(!LayerShader::Rain(rain).is_screen_rain());
    }

    #[test]
    fn rain_intensity_remaps_clamps_and_resets_without_music() {
        close(rain_intensity(0.0, Some(10_000.0), 0.0, 0.1), 0.0);
        close(rain_intensity(5_000.0, Some(10_000.0), 0.0, 0.1), 0.05);
        close(rain_intensity(10_000.0, Some(10_000.0), 0.0, 0.1), 0.1);
        // The native remap is clamped to the configured range.
        close(rain_intensity(20_000.0, Some(10_000.0), 0.0, 0.1), 0.1);
        close(rain_intensity(-5.0, Some(10_000.0), 0.1, 0.2), 0.1);
        // No music / empty chart keeps the start intensity.
        close(rain_intensity(5_000.0, None, 0.1, 0.2), 0.1);
        close(rain_intensity(5_000.0, Some(0.0), 0.1, 0.2), 0.1);
        // The erect family uses the same remap with its own range.
        close(rain_intensity(5_000.0, Some(10_000.0), 0.0, 0.01), 0.005);
    }

    #[test]
    fn blazin_time_scale_boost_and_decay_match_the_smooth_lerp() {
        let hit = 1.0 + BLAZIN_HIT_BOOST;
        close(hit, 1.7);
        let decayed = blazin_time_scale_decay(hit);
        let expected = 0.02 + (1.7 - 0.02) * 0.01_f64.powf(TICK_SECONDS / 1.535);
        close(decayed, expected);
        assert!(decayed < hit);
        // Repeated decay approaches the 0.02 target without crossing it.
        let mut scale = decayed;
        for _ in 0..10_000 {
            scale = blazin_time_scale_decay(scale);
        }
        assert!(scale > 0.02);
        assert!(scale < 0.021);
    }

    #[test]
    fn wiggle_uniforms_pack_effect_type_speed_frequency_amplitude_and_time() {
        let wiggle = Wiggle {
            effect: WiggleEffect::Flag,
            speed: 2.0,
            frequency: 4.0,
            amplitude: 0.017,
            time: 0.125,
        };
        let uniforms = LayerShader::Wiggle(wiggle)
            .uniforms([1.0, 1.0, 1.0, 1.0])
            .expect("wiggle uniforms");
        assert_eq!(uniforms.flags.x, 3.0);
        assert_eq!(uniforms.effect0.x, 4.0);
        close_f32(uniforms.effect0.y, 2.0);
        close_f32(uniforms.effect0.z, 4.0);
        close_f32(uniforms.effect0.w, 0.017);
        close_f32(uniforms.effect1.x, 0.125);
    }

    // --- 6G sub-slice 3: SserafimShader -----------------------------------

    #[test]
    fn sserafim_default_hsbc_matrices_are_identity() {
        let hue = sserafim_hue_rows(0.0);
        let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        for (row, expected) in hue.iter().zip(identity) {
            for (actual, expected) in row.iter().zip(expected) {
                close(*actual, expected);
            }
        }
        let saturation = sserafim_saturation_rows(0.0);
        for (row, expected) in saturation.iter().zip(identity) {
            for (actual, expected) in row.iter().zip(expected) {
                close(*actual, expected);
            }
        }
    }

    #[test]
    fn sserafim_hsbc_rows_match_the_inline_glsl_matrices() {
        // hue = 90: cos = 0, sin = 1 -> m1 + m3, evaluated in ROW space (the
        // GLSL constructors above are column-major).
        let hue = sserafim_hue_rows(90.0);
        close(hue[0][0], 0.0);
        close(hue[0][1], 0.0);
        close(hue[0][2], 1.0);
        close(hue[1][0], 0.356);
        close(hue[1][1], 0.855);
        close(hue[1][2], -0.211);
        close(hue[2][0], -0.574);
        close(hue[2][1], 1.430);
        close(hue[2][2], 0.144);

        // saturation = -100 -> value 0 -> every row is the grayscale luma row.
        let saturation = sserafim_saturation_rows(-100.0);
        for row in saturation {
            close(row[0], 0.309_803_921_568_627_5);
            close(row[1], 0.607_843_137_254_902);
            close(row[2], 0.082_352_941_176_470_6);
        }
    }

    #[test]
    fn sserafim_positive_saturation_triples_the_percentage() {
        // `SserafimShader.hx:182`: 10% positive saturation becomes 30% before
        // the `1 + adjusted / 100` conversion, i.e. value 1.3 (not 1.1).
        let rows = sserafim_saturation_rows(10.0);
        let value = 1.3;
        let inverse = 1.0 - value;
        close(rows[0][0], inverse * 0.309_803_921_568_627_5 + value);
        close(rows[0][1], inverse * 0.607_843_137_254_902);
        close(rows[0][2], inverse * 0.082_352_941_176_470_6);
        close(rows[1][1], inverse * 0.607_843_137_254_902 + value);
        close(rows[2][2], inverse * 0.082_352_941_176_470_6 + value);
        // Negative saturation keeps its own percentage: -10 -> value 0.9.
        let negative = sserafim_saturation_rows(-10.0);
        close(negative[0][0], 0.1 * 0.309_803_921_568_627_5 + 0.9);
        close(negative[1][1], 0.1 * 0.607_843_137_254_902 + 0.9);
    }

    #[test]
    fn sserafim_attachments_match_the_stage_table() {
        for role in [Role::Player, Role::Opponent, Role::Girlfriend] {
            assert_eq!(
                character_shader(
                    "sserafim",
                    role,
                    "any-character",
                    &ShaderMaskPages::default()
                ),
                LayerShader::Sserafim(Sserafim::new(true))
            );
        }
        // Every stage prop except the frozen light/cover set gets the
        // non-character divisor pair.
        assert_eq!(
            prop_shader("sserafim", "floor"),
            LayerShader::Sserafim(Sserafim::new(false))
        );
        assert_eq!(
            prop_shader("sserafim", "stageFront"),
            LayerShader::Sserafim(Sserafim::new(false))
        );
        for prop in [
            "truckLight1",
            "truckLight2",
            "backLightColor",
            "backLightWhite",
            "truckTest",
            "solidCover",
        ] {
            assert_eq!(
                prop_shader("sserafim", prop),
                LayerShader::None,
                "{prop} stays on the plain ColorMaterial path"
            );
        }
        // Other stages keep their existing attachments.
        assert_eq!(prop_shader("mainStage", "floor"), LayerShader::None);
        assert_eq!(
            character_shader("mainStage", Role::Player, "bf", &ShaderMaskPages::default()),
            LayerShader::None
        );
    }

    #[test]
    fn sserafim_uniforms_pack_the_event_state() {
        let shader = LayerShader::Sserafim(Sserafim {
            hsbc: Hsbc::new(0.0, 0.0, 0.0, 0.0),
            darken_amount: 0.8,
            pulse_light_color: [0.25, 0.5, 0.75],
            pulse_light_strength: 0.4,
            truck_light_strength: 0.6,
            is_char: true,
        });
        let uniforms = shader.uniforms([1.0, 1.0, 1.0, 0.5]).expect("uniforms");
        assert_eq!(uniforms.flags.x, 5.0);
        close_f32(uniforms.sserafim0.x, 0.8);
        close_f32(uniforms.sserafim0.y, 0.4);
        close_f32(uniforms.sserafim0.z, 0.6);
        assert_eq!(uniforms.sserafim0.w, 1.0, "isChar");
        close_f32(uniforms.sserafim1.x, 0.25);
        close_f32(uniforms.sserafim1.y, 0.5);
        close_f32(uniforms.sserafim1.z, 0.75);
        close_f32(uniforms.tint.w, 0.5);
        // Default HSBC = (0, 0, 0, 0): brightness 0, contrast scale 1 and
        // identity matrices (the native identity on [0, 1] RGB).
        assert_eq!(uniforms.hsbc.x, 0.0);
        close_f32(uniforms.hsbc.y, 1.0);
        close_f32(uniforms.hue0.x, 1.0);
        close_f32(uniforms.hue0.y, 0.0);
        close_f32(uniforms.hue1.y, 1.0);
        close_f32(uniforms.hue2.z, 1.0);
        close_f32(uniforms.sat0.x, 1.0);
        close_f32(uniforms.sat1.y, 1.0);
        close_f32(uniforms.sat2.z, 1.0);

        let prop = LayerShader::Sserafim(Sserafim::new(false));
        let uniforms = prop.uniforms([1.0, 1.0, 1.0, 1.0]).expect("uniforms");
        assert_eq!(uniforms.sserafim0.w, 0.0, "stage-prop isChar");
        assert_eq!(uniforms.sserafim0.x, 0.0);
        assert_eq!(uniforms.sserafim1, Vec4::new(1.0, 1.0, 1.0, 0.0));
    }

    #[test]
    fn rain_uniforms_pack_sprite_mode_camera_and_frame_bounds() {
        let rain = Rain {
            time: 1.25,
            scale: 7.2,
            intensity: 0.4,
            color: RAIN_COLOR_ERECT,
            sprite_mode: true,
            resolution: [1280.0, 720.0],
            camera_bounds: [-10.0, 20.0, 1300.0, 760.0],
            frame_bounds: [0.25, 0.5, 0.75, 1.0],
        };
        assert!(!LayerShader::Rain(rain).is_screen_rain());
        let uniforms = LayerShader::Rain(rain)
            .uniforms([1.0, 1.0, 1.0, 0.5])
            .expect("rain uniforms");
        assert_eq!(uniforms.flags.x, 4.0);
        close_f32(uniforms.rain0.x, 1.25);
        close_f32(uniforms.rain0.y, 7.2);
        close_f32(uniforms.rain0.z, 0.4);
        assert_eq!(uniforms.rain0.w, 1.0, "sprite mode flag");
        close_f32(uniforms.rain1.x, RAIN_COLOR_ERECT[0] as f32);
        close_f32(uniforms.rain2.x, 1280.0);
        close_f32(uniforms.camera_bounds.z, 1300.0);
        close_f32(uniforms.frame_bounds.y, 0.5);
        close_f32(uniforms.tint.w, 0.5);

        let screen = Rain {
            sprite_mode: false,
            ..rain
        };
        assert!(LayerShader::Rain(screen).is_screen_rain());
        let uniforms = LayerShader::Rain(screen)
            .uniforms([1.0, 1.0, 1.0, 1.0])
            .expect("screen rain uniforms");
        assert_eq!(uniforms.rain0.w, 0.0);
    }
}
