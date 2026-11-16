import bevy_sprite_render::{
    mesh2d::vertex_output::VertexOutput,
    mesh2d::view_bindings::view,
    sprite_mesh::bindings::{material, texture, texture_sampler},
    sprite_mesh::functions as sprite_functions,
};
import bevy_render::color_operations::{linear_to_srgb, srgb_to_linear};

@if(TONEMAP_IN_SHADER)
import bevy_core_pipeline::tonemapping;
@if(SRGB_OUTPUT)
import bevy_render::color_operations::linear_to_srgb as out_linear_to_srgb;
@if(OKLAB_OUTPUT)
import bevy_render::color_operations::linear_rgb_to_oklab;

struct FunkinSpriteUniform {
    // .x = mode_flags, .y = extra_flags, .z = blend_mode, .w = reserved
    flags: vec4<u32>,
    // .x = uTime, .y = building alphaShit, .z = wiggleSpeed, .w = wiggleFreq
    time_and_extra: vec4<f32>,
    // .x = wiggleAmp, .y = rainScale, .z = rainIntensity, .w = reserved
    wiggle_and_rain: vec4<f32>,
    // .xyz = hueMatrix col 0, .w = brightness / 255.0
    hue_col0: vec4<f32>,
    // .xyz = hueMatrix col 1, .w = contrast
    hue_col1: vec4<f32>,
    // .xyz = hueMatrix col 2, .w = reserved
    hue_col2: vec4<f32>,
    // .xyz = saturationMatrix col 0, .w = drop dist
    sat_col0: vec4<f32>,
    // .xyz = saturationMatrix col 1, .w = drop str
    sat_col1: vec4<f32>,
    // .xyz = saturationMatrix col 2, .w = drop thr
    sat_col2: vec4<f32>,
    // .xyz = dropColor, .w = maskThreshold (thr2)
    drop_color_and_mask: vec4<f32>,
    // .x = angCos, .y = angSin, .z = rainColor.r, .w = rainColor.g
    drop_angle_and_rain: vec4<f32>,
    // .x = rainColor.b, .y = sserafim hue, .z = sserafim sat, .w = sserafim bright
    rain_b_and_ss_hsbc: vec4<f32>,
    // .x = sserafim contrast, .y = darkAmt, .z = pulseStrength, .w = truckStrength
    ss_params: vec4<f32>,
    // .xyz = lightColor, .w = reserved
    ss_light: vec4<f32>,
    // Generic user uniforms (u0..u15 / vec0..vec3)
    custom_0: vec4<f32>,
    custom_1: vec4<f32>,
    custom_2: vec4<f32>,
    custom_3: vec4<f32>,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(20) var<uniform> fx: FunkinSpriteUniform;
@group(constants::MATERIAL_BIND_GROUP) @binding(21) var aux_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(22) var aux_sampler: sampler;

const MODE_ADJUST_COLOR: u32 = 1u;
const MODE_DROP_SHADOW: u32  = 2u;
const MODE_BUILDING: u32     = 4u;
const MODE_WIGGLE: u32       = 8u;
const MODE_OVERLAY_BLEND: u32 = 16u;
const MODE_RAIN_SPRITE: u32  = 32u;
const MODE_SSERAFIM: u32     = 64u;

const FLAG_USE_ALT_MASK: u32 = 256u;
const FLAG_IS_CHAR: u32      = 512u;
const FLAG_HAS_AUX_TEX: u32  = 1024u;

const EFFECT_TYPE_DREAMY: u32 = 0u;
const EFFECT_TYPE_WAVY: u32 = 1u;
const EFFECT_TYPE_HEAT_WAVE_HORIZONTAL: u32 = 2u;
const EFFECT_TYPE_HEAT_WAVE_VERTICAL: u32 = 3u;
const EFFECT_TYPE_FLAG: u32 = 4u;

const LUMA_VALUE: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);
const SSERAFIM_GRAYSCALE: vec3<f32> = vec3<f32>(0.30980392, 0.60784314, 0.08235294);
const DEG_TO_RAD: f32 = 0.017453292519943295;
const E_CONST: f32 = 2.718281828459045;

fn sample_tex_srgb(uv: vec2<f32>) -> vec4<f32> {
    let lin = textureSampleLevel(texture, texture_sampler, uv, 0.0);
    return vec4<f32>(linear_to_srgb(max(lin.rgb, vec3<f32>(0.0))), lin.a);
}

fn sample_aux_srgb(uv: vec2<f32>) -> vec4<f32> {
    let lin = textureSampleLevel(aux_texture, aux_sampler, uv, 0.0);
    return vec4<f32>(linear_to_srgb(max(lin.rgb, vec3<f32>(0.0))), lin.a);
}

fn apply_hsbc_effect(color: vec3<f32>) -> vec3<f32> {
    let brightness = fx.hue_col0.w;
    let contrast = fx.hue_col1.w;
    let added = color + vec3<f32>(brightness);
    let bh = vec3<f32>(
        dot(added, fx.hue_col0.xyz),
        dot(added, fx.hue_col1.xyz),
        dot(added, fx.hue_col2.xyz),
    );
    let c = (bh - vec3<f32>(0.25)) * contrast + vec3<f32>(0.25);
    return vec3<f32>(
        dot(c, fx.sat_col0.xyz),
        dot(c, fx.sat_col1.xyz),
        dot(c, fx.sat_col2.xyz),
    );
}

fn lwidth_manual(center: f32, uv: vec2<f32>, px: vec2<f32>) -> f32 {
    let p2x1 = sample_tex_srgb(uv + vec2<f32>(1.0, 0.0) * px).rgb;
    let p1x2 = sample_tex_srgb(uv + vec2<f32>(0.0, 1.0) * px).rgb;
    let p2x2 = sample_tex_srgb(uv + vec2<f32>(1.0, 1.0) * px).rgb;

    let right = dot(p2x1, LUMA_VALUE);
    let down = dot(p1x2, LUMA_VALUE);
    let diagonal = dot(p2x2, LUMA_VALUE);

    let dx = abs(right - center);
    let dy = abs(down - center);
    let dd = abs(diagonal - center);

    return ((dx + dy + dd * 0.7) / (1.0 + 1.0 + 0.7)) * 2.0;
}

fn apply_wiggle_uv(pt_in: vec2<f32>, tex_size: vec2<f32>) -> vec2<f32> {
    let effect_type = fx.flags.y & 255u;
    let u_time = fx.time_and_extra.x;
    let u_speed = fx.time_and_extra.z;
    let u_freq = fx.time_and_extra.w;
    let u_amp = fx.wiggle_and_rain.x;

    var pt = pt_in;
    var x = 0.0;
    var y = 0.0;

    if effect_type == EFFECT_TYPE_DREAMY {
        let w = 1.0 / tex_size.y;
        let h = 1.0 / tex_size.x;
        pt.x = floor(pt.x / h) * h;
        let offset_x = sin(pt.x * u_freq + u_time * u_speed) * u_amp;
        pt.y += floor(offset_x / w) * w;
        pt.y = floor(pt.y / w) * w;
        let offset_y = sin(pt.y * (u_freq * 0.5) + u_time * (u_speed * 0.5)) * (u_amp * 0.5);
        pt.x += floor(offset_y / h) * h;
    } else if effect_type == EFFECT_TYPE_WAVY {
        pt.y += sin(pt.x * u_freq + u_time * u_speed) * u_amp;
    } else if effect_type == EFFECT_TYPE_HEAT_WAVE_HORIZONTAL {
        x = sin(pt.x * u_freq + u_time * u_speed) * u_amp;
    } else if effect_type == EFFECT_TYPE_HEAT_WAVE_VERTICAL {
        y = sin(pt.y * u_freq + u_time * u_speed) * u_amp;
    } else if effect_type == EFFECT_TYPE_FLAG {
        y = sin(pt.y * u_freq + 10.0 * pt.x + u_time * u_speed) * u_amp;
        x = sin(pt.x * u_freq + 5.0 * pt.y + u_time * u_speed) * u_amp;
    }

    return vec2<f32>(pt.x + x, pt.y + y);
}

fn glsl_mod_vec2(x: vec2<f32>, y: f32) -> vec2<f32> {
    return x - y * floor(x / y);
}

fn glsl_mod_f32(x: f32, y: f32) -> f32 {
    return x - y * floor(x / y);
}

fn rain_rand(a: vec2<f32>) -> f32 {
    let m = glsl_mod_vec2(a, 1000.0);
    return fract(sin(dot(m, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

fn rain_dist(p_in: vec2<f32>, scale: f32, intensity: f32, u_time: f32) -> f32 {
    var p = p_in * 0.1;
    p.x += p.y * 0.1;
    p.y -= u_time * 500.0 / scale;
    p.y *= 0.03;
    let ix = floor(p.x);
    p.y += glsl_mod_f32(ix, 2.0) * 0.5 + (rain_rand(vec2<f32>(ix, ix)) - 0.5) * 0.3;
    let iy = floor(p.y);
    let index = vec2<f32>(ix, iy);
    p -= index;
    p.x += (rain_rand(index.yx) * 2.0 - 1.0) * 0.35;
    let a = abs(p - vec2<f32>(0.5));
    let res = max(a.x * 0.8, a.y * 0.5) - 0.1;
    let empty = rain_rand(index) < mix(1.0, 0.1, intensity);
    if empty {
        return 1.0;
    }
    return res;
}

fn blend_overlay(base: vec4<f32>, blend: vec4<f32>) -> vec4<f32> {
    return mix(
        vec4<f32>(1.0) - 2.0 * (vec4<f32>(1.0) - base) * (vec4<f32>(1.0) - blend),
        2.0 * base * blend,
        step(base, vec4<f32>(0.5)),
    );
}

fn ss_apply_hue_rotate(a_color: vec3<f32>, a_hue: f32) -> vec3<f32> {
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

fn ss_apply_contrast(a_color: vec3<f32>, value_in: f32) -> vec3<f32> {
    var value = 1.0 + (value_in / 100.0);
    if value > 1.0 {
        value = (((0.00852259 * pow(E_CONST, 4.76454 * (value - 1.0))) * 1.01) - 0.0086078159) * 10.0 + 1.0;
    }
    return clamp((a_color - vec3<f32>(0.25)) * value + vec3<f32>(0.25), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn ss_apply_saturation(a_color: vec3<f32>, value_in: f32) -> vec3<f32> {
    var val = value_in;
    if val > 0.0 {
        val = val * 3.0;
    }
    val = 1.0 + (val / 100.0);
    let grayscale = vec3<f32>(dot(a_color, SSERAFIM_GRAYSCALE));
    return clamp(mix(grayscale, a_color, val), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn ss_apply_hsbc(color_in: vec3<f32>) -> vec3<f32> {
    var color = color_in + vec3<f32>(fx.rain_b_and_ss_hsbc.w / 255.0);
    color = ss_apply_hue_rotate(color, fx.rain_b_and_ss_hsbc.y);
    color = ss_apply_contrast(color, fx.ss_params.x);
    color = ss_apply_saturation(color, fx.rain_b_and_ss_hsbc.z);
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
    let mode_flags = fx.flags.x;
    let extra_flags = fx.flags.y;
    let instance_index = mesh.instance_index;

    let base_uv = sprite_functions::get_final_uv(mesh.uv, instance_index);
    let uv_fw = fwidth(base_uv);
    let uv00 = sprite_functions::get_final_uv(vec2<f32>(0.0, 0.0), instance_index);
    let uv11 = sprite_functions::get_final_uv(vec2<f32>(1.0, 1.0), instance_index);
    let frame_min = min(uv00, uv11);
    let frame_max = max(uv00, uv11);
    let frame_span = max(frame_max - frame_min, vec2<f32>(1e-5));
    let tex_size = max(vec2<f32>(textureDimensions(texture)), vec2<f32>(1.0));
    let tex_ratio = 1.0 / tex_size;

    var sample_uv = base_uv;
    if (mode_flags & MODE_WIGGLE) != 0u {
        sample_uv = apply_wiggle_uv(sample_uv, tex_size);
    }

    var srgb_col = sample_tex_srgb(sample_uv);

    if (mode_flags & MODE_RAIN_SPRITE) != 0u {
        let u_scale = max(fx.wiggle_and_rain.y, 0.01);
        let u_intensity = fx.wiggle_and_rain.z;
        let u_time = fx.time_and_extra.x;
        let rain_color = vec3<f32>(fx.drop_angle_and_rain.z, fx.drop_angle_and_rain.w, fx.rain_b_and_ss_hsbc.x);

        let local_uv = clamp((sample_uv - frame_min) / frame_span, vec2<f32>(0.0), vec2<f32>(1.0));
        var wpos = local_uv * vec2<f32>(1280.0, 720.0);
        let orig_wpos = wpos;
        var add = vec3<f32>(0.0);
        var rain_sum = 0.0;
        let scales = array<f32, 4>(1.0, 1.8, 2.6, 4.8);
        for (var i = 0; i < 4; i = i + 1) {
            let sc = scales[i];
            let r = rain_dist(wpos * sc / u_scale + 500.0 * f32(i), sc, u_intensity, u_time);
            if r < 0.0 {
                let v = (1.0 - exp(r * 5.0)) / sc * 2.0;
                wpos.x += v * 10.0 * u_scale;
                wpos.y -= v * 2.0 * u_scale;
                add += vec3<f32>(0.1, 0.15, 0.2) * v;
                rain_sum += (1.0 - rain_sum) * 0.75;
            }
        }
        let rwpos = ((wpos - orig_wpos) / vec2<f32>(1280.0, 720.0)) * frame_span;
        let refracted = sample_tex_srgb(clamp(sample_uv + rwpos, frame_min, frame_max));
        var rgb = refracted.rgb + add;
        rgb = mix(rgb, rain_color, 0.1 * rain_sum);
        srgb_col = vec4<f32>(rgb, refracted.a);
    }

    if (mode_flags & MODE_OVERLAY_BLEND) != 0u && (extra_flags & FLAG_HAS_AUX_TEX) != 0u {
        let aux_sample = sample_aux_srgb(sample_uv + vec2<f32>(0.1, 0.2));
        srgb_col = blend_overlay(srgb_col, aux_sample);
    }

    if (mode_flags & MODE_DROP_SHADOW) != 0u {
        let px = max(tex_ratio, uv_fw);
        let center_luma = dot(srgb_col.rgb, LUMA_VALUE);
        let delta = lwidth_manual(center_luma, sample_uv, px);

        var threshold = fx.sat_col2.w;
        if (extra_flags & FLAG_USE_ALT_MASK) != 0u && (extra_flags & FLAG_HAS_AUX_TEX) != 0u {
            let mask_sample = sample_aux_srgb(sample_uv);
            if mask_sample.b > 0.0 {
                threshold = fx.drop_color_and_mask.w;
            }
        }
        let intensity = smoothstep(threshold - delta, threshold + delta, center_luma);

        let dist = fx.sat_col0.w;
        let str = fx.sat_col1.w;
        let ang_cos = fx.drop_angle_and_rain.x;
        let ang_sin = fx.drop_angle_and_rain.y;
        let checked = vec2<f32>(
            sample_uv.x + dist * ang_cos * tex_ratio.x,
            sample_uv.y - dist * ang_sin * tex_ratio.y,
        );
        let checked_sample = sample_tex_srgb(clamp(checked, frame_min, frame_max));
        var shadow_alpha = 0.0;
        if checked.x > frame_min.x && checked.y > frame_min.y && checked.x < frame_max.x && checked.y < frame_max.y {
            shadow_alpha = checked_sample.a;
        }
        let rim = (1.0 - shadow_alpha * str) * intensity;

        var color3 = apply_hsbc_effect(srgb_col.rgb);
        color3 += fx.drop_color_and_mask.xyz * rim;
        srgb_col = vec4<f32>(clamp(color3, vec3<f32>(0.0), vec3<f32>(1.0)), srgb_col.a);
    } else if (mode_flags & MODE_ADJUST_COLOR) != 0u {
        let color3 = apply_hsbc_effect(srgb_col.rgb);
        srgb_col = vec4<f32>(clamp(color3, vec3<f32>(0.0), vec3<f32>(1.0)), srgb_col.a);
    }

    if (mode_flags & MODE_SSERAFIM) != 0u {
        var out_col = ss_apply_hsbc(srgb_col.rgb);
        let dark_amt = fx.ss_params.y;
        let pulse_str = fx.ss_params.z;
        let truck_str = fx.ss_params.w;
        let combined_alpha = truck_str + pulse_str;
        let light_mult = 0.07;
        var dark_factor = combined_alpha * light_mult + dark_amt;
        if dark_amt > 0.65 {
            dark_factor = combined_alpha * (-light_mult) + dark_amt;
        }
        var bruh_hsl = rgb2hsl(fx.ss_light.xyz);
        bruh_hsl.z = bruh_hsl.z * pulse_str;
        let bruh_col = hsl2rgb(bruh_hsl);

        var multiply_col: vec3<f32>;
        if (extra_flags & FLAG_IS_CHAR) != 0u {
            multiply_col = mix(vec3<f32>(1.0), vec3<f32>(0.0), dark_factor / 5.0);
            multiply_col = mix(multiply_col, bruh_col, dark_factor / 3.0);
        } else {
            multiply_col = mix(vec3<f32>(1.0), vec3<f32>(0.0), dark_factor);
            multiply_col = mix(multiply_col, bruh_col, dark_factor / 2.0);
        }
        out_col *= multiply_col;
        srgb_col = vec4<f32>(clamp(out_col, vec3<f32>(0.0), vec3<f32>(1.0)), srgb_col.a);
    }

    var lin_col = vec4<f32>(srgb_to_linear(max(srgb_col.rgb, vec3<f32>(0.0))), srgb_col.a);
    lin_col = sprite_functions::get_final_color(lin_col, instance_index);

    if (mode_flags & MODE_BUILDING) != 0u {
        if lin_col.a > 0.0 {
            lin_col.a = max(lin_col.a - fx.time_and_extra.y, 0.0);
        }
    }

    // Multiply (2u) and Screen (3u) hardware blend states expect premultiplied source RGB
    // so transparent pixels (alpha == 0) leave the destination unchanged.
    if fx.flags.z == 2u || fx.flags.z == 3u {
        lin_col = vec4<f32>(lin_col.rgb * lin_col.a, lin_col.a);
    }

@if(TONEMAP_IN_SHADER)
    lin_col = tonemapping::tone_mapping(lin_col, view.color_grading);

@if(SRGB_OUTPUT)
    lin_col = vec4<f32>(out_linear_to_srgb(lin_col.rgb), lin_col.a);

@if(OKLAB_OUTPUT)
    lin_col = vec4<f32>(linear_rgb_to_oklab(lin_col.rgb), lin_col.a);

    return lin_col;
}
