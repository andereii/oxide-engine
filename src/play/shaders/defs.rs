use std::f32::consts::PI;

pub const SHADER_MODE_ADJUST_COLOR: u32 = 1 << 0;
pub const SHADER_MODE_DROP_SHADOW: u32 = 1 << 1;
pub const SHADER_MODE_BUILDING: u32 = 1 << 2;
pub const SHADER_MODE_WIGGLE: u32 = 1 << 3;
pub const SHADER_MODE_OVERLAY_BLEND: u32 = 1 << 4;
pub const SHADER_MODE_RAIN_SPRITE: u32 = 1 << 5;
pub const SHADER_MODE_TEXTURE_SWAP: u32 = 1 << 7;

pub const SHADER_FLAG_USE_ALT_MASK: u32 = 1 << 8;
pub const SHADER_FLAG_HAS_AUX_TEX: u32 = 1 << 10;
pub const SHADER_FLAG_PREMULTIPLIED_INPUT: u32 = 1 << 11;

pub const FUNKIN_SCREEN_HEIGHT: f32 = 720.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, bevy::prelude::Reflect)]
#[repr(u32)]
pub enum WiggleEffectType {
    #[default]
    Dreamy = 0,
    Wavy = 1,
    HeatWaveHorizontal = 2,
    HeatWaveVertical = 3,
    Flag = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, bevy::prelude::Reflect)]
#[repr(u32)]
pub enum FunkinBlendMode {
    #[default]
    Normal = 0,
    Add = 1,
    Multiply = 2,
    Screen = 3,
}

impl FunkinBlendMode {
    /// Parses a Funkin/Haxe blend mode identifier (`normal`, `add`, `multiply`, `screen`).
    #[allow(dead_code)] // Used by oxide crate targets; shader_parity_tests imports defs.rs via #[path].
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "add" | "additive" => Self::Add,
            "multiply" | "mult" => Self::Multiply,
            "screen" => Self::Screen,
            _ => Self::Normal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CharShaderRole {
    Player,
    Spectator,
    Opponent,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AdjustColorParams {
    pub hue: f32,
    pub saturation: f32,
    pub brightness: f32,
    pub contrast: f32,
}

impl AdjustColorParams {
    pub const fn new(hue: f32, saturation: f32, brightness: f32, contrast: f32) -> Self {
        Self {
            hue,
            saturation,
            brightness,
            contrast,
        }
    }

    pub fn brightness_norm(self) -> f32 {
        self.brightness / 255.0
    }

    /// Exact Funkin contrast curve from `AdjustColorShader.hx` and `DropShadowShader.hx`.
    pub fn contrast_norm(self) -> f32 {
        let mut val = 1.0 + self.contrast / 100.0;
        if val > 1.0 {
            val = (((0.00852259 * (4.76454 * (val - 1.0)).exp()) * 1.01) - 0.0086078159) * 10.0;
            val += 1.0;
        }
        val
    }

    /// Exact Funkin saturation scaling from `AdjustColorShader.hx` and `DropShadowShader.hx`.
    pub fn saturation_factor(self) -> f32 {
        let mut s = self.saturation;
        if s > 0.0 {
            s *= 3.0;
        }
        1.0 + (s / 100.0)
    }

    /// Column-major 3x3 hue matrix matching OpenFL/GLSL `makeHueMatrix(hue * TO_RAD)`.
    pub fn hue_matrix(self) -> [[f32; 3]; 3] {
        let h = self.hue * (PI / 180.0);
        let c = h.cos();
        let s = h.sin();
        let wr = 0.299;
        let wg = 0.587;
        let wb = 0.114;

        [
            [
                wr + (1.0 - wr) * c - wr * s,
                wg - wg * c - wg * s,
                wb - wb * c + (1.0 - wb) * s,
            ],
            [
                wr - wr * c + 0.143 * s,
                wg + (1.0 - wg) * c + 0.140 * s,
                wb - wb * c - 0.283 * s,
            ],
            [
                wr - wr * c - (1.0 - wr) * s,
                wg - wg * c + wg * s,
                wb + (1.0 - wb) * c + wb * s,
            ],
        ]
    }

    /// Column-major 3x3 saturation matrix matching `makeSaturationMatrix`.
    pub fn saturation_matrix(self) -> [[f32; 3]; 3] {
        let s = self.saturation_factor();
        let lr = 0.2126;
        let lg = 0.7152;
        let lb = 0.0722;
        let inv = 1.0 - s;

        [
            [lr * inv + s, lg * inv, lb * inv],
            [lr * inv, lg * inv + s, lb * inv],
            [lr * inv, lg * inv, lb * inv + s],
        ]
    }

    /// CPU reference implementation of `applyHSBCEffect(color)` in sRGB `[0, 1]`.
    #[allow(dead_code)] // Exercised by standalone integration tests under tests/.
    pub fn apply_cpu_srgb(self, rgb: [f32; 3]) -> [f32; 3] {
        let b = self.brightness_norm();
        let added = [rgb[0] + b, rgb[1] + b, rgb[2] + b];
        let hm = self.hue_matrix();
        let bh = [
            added[0] * hm[0][0] + added[1] * hm[0][1] + added[2] * hm[0][2],
            added[0] * hm[1][0] + added[1] * hm[1][1] + added[2] * hm[1][2],
            added[0] * hm[2][0] + added[1] * hm[2][1] + added[2] * hm[2][2],
        ];
        let cn = self.contrast_norm();
        let c = [
            (bh[0] - 0.25) * cn + 0.25,
            (bh[1] - 0.25) * cn + 0.25,
            (bh[2] - 0.25) * cn + 0.25,
        ];
        let sm = self.saturation_matrix();
        [
            c[0] * sm[0][0] + c[1] * sm[0][1] + c[2] * sm[0][2],
            c[0] * sm[1][0] + c[1] * sm[1][1] + c[2] * sm[1][2],
            c[0] * sm[2][0] + c[1] * sm[2][1] + c[2] * sm[2][2],
        ]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DropShadowParams {
    pub adjust: AdjustColorParams,
    pub color: [f32; 3],
    pub angle_deg: f32,
    pub angle_offset_deg: f32,
    pub distance: f32,
    pub strength: f32,
    pub threshold: f32,
    pub mask_threshold: f32,
    pub use_alt_mask: bool,
    pub alt_mask_path: Option<String>,
}

impl Default for DropShadowParams {
    fn default() -> Self {
        Self {
            adjust: AdjustColorParams::default(),
            color: [1.0, 1.0, 1.0],
            angle_deg: 0.0,
            angle_offset_deg: 0.0,
            distance: 15.0,
            strength: 1.0,
            threshold: 0.1,
            mask_threshold: 0.5,
            use_alt_mask: false,
            alt_mask_path: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WiggleParams {
    pub effect_type: WiggleEffectType,
    pub speed: f32,
    pub frequency: f32,
    pub amplitude: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuildingParams {
    pub alpha_shit: f32,
}

impl Default for BuildingParams {
    fn default() -> Self {
        Self { alpha_shit: 1.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RainSpriteParams {
    pub scale: f32,
    pub intensity: f32,
    pub rain_color: [f32; 3],
}

impl Default for RainSpriteParams {
    fn default() -> Self {
        Self {
            scale: FUNKIN_SCREEN_HEIGHT / 200.0 * 2.0,
            intensity: 0.4,
            rain_color: argb_to_rgb_f32(0xFF6680CC),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SpriteShaderConfig {
    pub mode_flags: u32,
    pub blend_mode: FunkinBlendMode,
    pub adjust: AdjustColorParams,
    pub drop_shadow: DropShadowParams,
    pub wiggle: WiggleParams,
    pub building: BuildingParams,
    pub overlay_path: Option<String>,
    pub rain_sprite: RainSpriteParams,
    pub custom_params: [[f32; 4]; 4],
}

impl SpriteShaderConfig {
    pub fn adjust_color(hue: f32, saturation: f32, brightness: f32, contrast: f32) -> Self {
        Self {
            mode_flags: SHADER_MODE_ADJUST_COLOR,
            adjust: AdjustColorParams::new(hue, saturation, brightness, contrast),
            ..Default::default()
        }
    }

    pub fn blend_only(blend_mode: FunkinBlendMode) -> Self {
        Self {
            blend_mode,
            ..Default::default()
        }
    }

    pub fn drop_shadow(params: DropShadowParams) -> Self {
        Self {
            mode_flags: SHADER_MODE_DROP_SHADOW,
            adjust: params.adjust,
            drop_shadow: params,
            ..Default::default()
        }
    }

    pub fn wiggle(effect_type: WiggleEffectType, speed: f32, frequency: f32, amplitude: f32) -> Self {
        Self {
            mode_flags: SHADER_MODE_WIGGLE,
            wiggle: WiggleParams {
                effect_type,
                speed,
                frequency,
                amplitude,
            },
            ..Default::default()
        }
    }

    pub fn building(alpha_shit: f32) -> Self {
        Self {
            mode_flags: SHADER_MODE_BUILDING,
            building: BuildingParams { alpha_shit },
            ..Default::default()
        }
    }

    pub fn overlay_blend(overlay_path: impl Into<String>) -> Self {
        Self {
            mode_flags: SHADER_MODE_OVERLAY_BLEND,
            overlay_path: Some(overlay_path.into()),
            ..Default::default()
        }
    }

    pub fn rain_sprite(scale: f32, intensity: f32, rain_color: [f32; 3]) -> Self {
        Self {
            mode_flags: SHADER_MODE_RAIN_SPRITE,
            rain_sprite: RainSpriteParams {
                scale,
                intensity,
                rain_color,
            },
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraRainSpec {
    pub scale: f32,
    pub start_intensity: f32,
    pub end_intensity: f32,
    pub rain_color: [f32; 3],
    pub time_scale_decay: bool,
}

pub const fn argb_to_rgb_f32(argb: u32) -> [f32; 3] {
    let r = ((argb >> 16) & 0xFF) as f32 / 255.0;
    let g = ((argb >> 8) & 0xFF) as f32 / 255.0;
    let b = (argb & 0xFF) as f32 / 255.0;
    [r, g, b]
}

#[allow(dead_code)] // Retained as shared ARGB-to-RGBA conversion helper for shader definitions/tests.
pub const fn argb_to_rgba_f32(argb: u32) -> [f32; 4] {
    let a = ((argb >> 24) & 0xFF) as f32 / 255.0;
    let r = ((argb >> 16) & 0xFF) as f32 / 255.0;
    let g = ((argb >> 8) & 0xFF) as f32 / 255.0;
    let b = (argb & 0xFF) as f32 / 255.0;
    [r, g, b, a]
}
