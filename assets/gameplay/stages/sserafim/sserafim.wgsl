// Parameterized custom WGSL fragment shader for the `sserafim` stage.
// Loaded via `Shader::from_wgsl` as a `MaterialExtension2d` fragment override on
// `ExtendedMaterial2d<SpriteMeshMaterial, FunkinSpriteMaterial>`.
//
// Uniform mapping (via `shader_vec` / `shader_val`):
//   fx.custom_0 (`vec0` / u0..u3): (hue_deg, saturation, brightness, contrast)
//   fx.custom_1 (`vec1` / u4..u7): (darken_amount, pulse_strength, truck_strength, is_char)
//   fx.custom_2 (`vec2` / u8..u10): (light_r, light_g, light_b, reserved)

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(5) @interpolate(flat) instance_index: u32,
};

struct SpriteMaterial {
    color: vec4<f32>,
    flags: u32,
    alpha_cutoff: f32,
    vertex_scale: vec2<f32>,
    vertex_offset: vec2<f32>,
    uv_transform: mat3x3<f32>,
    tile_stretch_value: vec2<f32>,
    scale: vec2<f32>,
    min_inset: vec2<f32>,
    max_inset: vec2<f32>,
    side_stretch_value: vec2<f32>,
    center_stretch_value: vec2<f32>,
};

struct FunkinSpriteUniform {
    flags: vec4<u32>,
    time_and_extra: vec4<f32>,
    wiggle_and_rain: vec4<f32>,
    hue_col0: vec4<f32>,
    hue_col1: vec4<f32>,
    hue_col2: vec4<f32>,
    sat_col0: vec4<f32>,
    sat_col1: vec4<f32>,
    sat_col2: vec4<f32>,
    drop_color_and_mask: vec4<f32>,
    drop_angle_and_rain: vec4<f32>,
    custom_0: vec4<f32>,
    custom_1: vec4<f32>,
    custom_2: vec4<f32>,
    custom_3: vec4<f32>,
};

@group(2) @binding(0) var<uniform> material: SpriteMaterial;
@group(2) @binding(1) var sprite_texture: texture_2d<f32>;
@group(2) @binding(2) var sprite_sampler: sampler;
@group(2) @binding(20) var<uniform> fx: FunkinSpriteUniform;

const SPRITE_FLAG_ALPHA_RESERVED: u32 = 3221225472u; // (0b11u32 << 30)
const SPRITE_FLAG_ALPHA_OPAQUE: u32 = 0u;
const SPRITE_FLAG_FLIP_X: u32 = 1u;
const SPRITE_FLAG_FLIP_Y: u32 = 2u;
const SPRITE_FLAG_TILE_X: u32 = 4u;
const SPRITE_FLAG_TILE_Y: u32 = 8u;

const GRAYSCALE_WEIGHTS: vec3<f32> = vec3<f32>(0.30980392, 0.60784314, 0.08235294);
const DEG_TO_RAD: f32 = 0.017453292519943295;
const E_CONST: f32 = 2.718281828459045;

fn linear_to_srgb_ch(v: f32) -> f32 {
    if v <= 0.0 {
        return v;
    }
    if v <= 0.0031308 {
        return v * 12.92;
    }
    return 1.055 * pow(v, 1.0 / 2.4) - 0.055;
}

fn srgb_to_linear_ch(v: f32) -> f32 {
    if v <= 0.0 {
        return v;
    }
    if v <= 0.04045 {
        return v / 12.92;
    }
    return pow((v + 0.055) / 1.055, 2.4);
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        linear_to_srgb_ch(c.r),
        linear_to_srgb_ch(c.g),
        linear_to_srgb_ch(c.b),
    );
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        srgb_to_linear_ch(c.r),
        srgb_to_linear_ch(c.g),
        srgb_to_linear_ch(c.b),
    );
}

fn get_final_uv(uv: vec2<f32>) -> vec2<f32> {
    var out = uv;
    if (material.flags & SPRITE_FLAG_FLIP_X) != 0u {
        out.x = 1.0 - out.x;
    }
    if (material.flags & SPRITE_FLAG_FLIP_Y) != 0u {
        out.y = 1.0 - out.y;
    }
    if (material.flags & SPRITE_FLAG_TILE_X) != 0u && material.tile_stretch_value.x != 0.0 {
        out.x = (out.x - material.tile_stretch_value.x * floor(out.x / material.tile_stretch_value.x))
            / material.tile_stretch_value.x;
    }
    if (material.flags & SPRITE_FLAG_TILE_Y) != 0u && material.tile_stretch_value.y != 0.0 {
        out.y = (out.y - material.tile_stretch_value.y * floor(out.y / material.tile_stretch_value.y))
            / material.tile_stretch_value.y;
    }
    return (material.uv_transform * vec3<f32>(out, 1.0)).xy;
}

fn apply_hue_rotate(a_color: vec3<f32>, a_hue: f32) -> vec3<f32> {
    let angle = a_hue * DEG_TO_RAD;
    let m1 = mat3x3<f32>(
        vec3<f32>(0.213, 0.213, 0.213),
        vec3<f32>(0.715, 0.715, 0.715),
        vec3<f32>(0.072, 0.072, 0.072),
    );
    let m2 = mat3x3<f32>(
        vec3<f32>(0.787, -0.213, -0.213),
        vec3<f32>(-0.715, 0.285, -0.715),
        vec3<f32>(-0.072, -0.072, 0.928),
    );
    let m3 = mat3x3<f32>(
        vec3<f32>(-0.213, 0.143, -0.787),
        vec3<f32>(-0.715, 0.140, 0.715),
        vec3<f32>(0.928, -0.283, 0.072),
    );
    let m = m1 + cos(angle) * m2 + sin(angle) * m3;
    return m * a_color;
}

fn apply_contrast(a_color: vec3<f32>, value_in: f32) -> vec3<f32> {
    var value = 1.0 + (value_in / 100.0);
    if value > 1.0 {
        value = (((0.00852259 * pow(E_CONST, 4.76454 * (value - 1.0))) * 1.01) - 0.0086078159) * 10.0 + 1.0;
    }
    return clamp((a_color - vec3<f32>(0.25)) * value + vec3<f32>(0.25), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn apply_saturation(a_color: vec3<f32>, value_in: f32) -> vec3<f32> {
    var val = value_in;
    if val > 0.0 {
        val = val * 3.0;
    }
    val = 1.0 + (val / 100.0);
    let grayscale = vec3<f32>(dot(a_color, GRAYSCALE_WEIGHTS));
    return clamp(mix(grayscale, a_color, val), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn apply_hsbc(color_in: vec3<f32>, hsbc: vec4<f32>) -> vec3<f32> {
    var color = color_in + vec3<f32>(hsbc.z / 255.0);
    color = apply_hue_rotate(color, hsbc.x);
    color = apply_contrast(color, hsbc.w);
    color = apply_saturation(color, hsbc.y);
    return color;
}

fn hue2rgb(hue_in: f32) -> vec3<f32> {
    let hue = fract(hue_in);
    return clamp(
        vec3<f32>(
            abs(hue * 6.0 - 3.0) - 1.0,
            2.0 - abs(hue * 6.0 - 2.0),
            2.0 - abs(hue * 6.0 - 4.0),
        ),
        vec3<f32>(0.0),
        vec3<f32>(1.0),
    );
}

fn rgb2hsl(c: vec3<f32>) -> vec3<f32> {
    let c_min = min(min(c.r, c.g), c.b);
    let c_max = max(max(c.r, c.g), c.b);
    let delta = c_max - c_min;
    var hsl = vec3<f32>(0.0, 0.0, (c_max + c_min) * 0.5);
    if delta != 0.0 {
        if hsl.z < 0.5 {
            hsl.y = delta / (c_max + c_min);
        } else {
            hsl.y = delta / (2.0 - c_max - c_min);
        }
        let delta_r = (((c_max - c.r) / 6.0) + (delta * 0.5)) / delta;
        let delta_g = (((c_max - c.g) / 6.0) + (delta * 0.5)) / delta;
        let delta_b = (((c_max - c.b) / 6.0) + (delta * 0.5)) / delta;
        if c.r == c_max {
            hsl.x = delta_b - delta_g;
        } else if c.g == c_max {
            hsl.x = (1.0 / 3.0) + delta_r - delta_b;
        } else {
            hsl.x = (2.0 / 3.0) + delta_g - delta_r;
        }
        hsl.x = fract(hsl.x);
    }
    return hsl;
}

fn hsl2rgb(hsl: vec3<f32>) -> vec3<f32> {
    if hsl.y == 0.0 {
        return vec3<f32>(hsl.z);
    }
    var b: f32;
    if hsl.z < 0.5 {
        b = hsl.z * (1.0 + hsl.y);
    } else {
        b = hsl.z + hsl.y - hsl.y * hsl.z;
    }
    let a = 2.0 * hsl.z - b;
    return a + hue2rgb(hsl.x) * (b - a);
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let sample_uv = get_final_uv(mesh.uv);
    let lin_sample = textureSampleLevel(sprite_texture, sprite_sampler, sample_uv, 0.0);
    let srgb_in = linear_to_srgb(max(lin_sample.rgb, vec3<f32>(0.0)));

    var out_col = apply_hsbc(srgb_in, fx.custom_0);

    let dark_amt = fx.custom_1.x;
    let pulse_str = fx.custom_1.y;
    let truck_str = fx.custom_1.z;
    let is_char = fx.custom_1.w > 0.5;

    let combined_alpha = truck_str + pulse_str;
    let light_mult = 0.07;
    var dark_factor = combined_alpha * light_mult + dark_amt;
    if dark_amt > 0.65 {
        dark_factor = combined_alpha * (-light_mult) + dark_amt;
    }

    var light_hsl = rgb2hsl(fx.custom_2.xyz);
    light_hsl.z = light_hsl.z * pulse_str;
    let light_col = hsl2rgb(light_hsl);

    var multiply_col: vec3<f32>;
    if is_char {
        multiply_col = mix(vec3<f32>(1.0), vec3<f32>(0.0), dark_factor / 5.0);
        multiply_col = mix(multiply_col, light_col, dark_factor / 3.0);
    } else {
        multiply_col = mix(vec3<f32>(1.0), vec3<f32>(0.0), dark_factor);
        multiply_col = mix(multiply_col, light_col, dark_factor / 2.0);
    }

    out_col = clamp(out_col * multiply_col, vec3<f32>(0.0), vec3<f32>(1.0));
    var lin_col = vec4<f32>(srgb_to_linear(out_col), lin_sample.a) * material.color;

    if (material.flags & SPRITE_FLAG_ALPHA_RESERVED) == SPRITE_FLAG_ALPHA_OPAQUE {
        lin_col.a = 1.0;
    }
    if fx.flags.z == 2u || fx.flags.z == 3u {
        lin_col = vec4<f32>(lin_col.rgb * lin_col.a, lin_col.a);
    }

    return lin_col;
}
