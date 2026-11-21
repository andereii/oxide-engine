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
    // .x = wiggleAmp, .y = rainScale, .z = rainIntensity, .w = rainColor.b
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
const MODE_TEXTURE_SWAP: u32 = 128u;

const FLAG_USE_ALT_MASK: u32 = 256u;
const FLAG_HAS_AUX_TEX: u32  = 1024u;
const FLAG_PREMULTIPLIED_INPUT: u32 = 2048u;

const EFFECT_TYPE_DREAMY: u32 = 0u;
const EFFECT_TYPE_WAVY: u32 = 1u;
const EFFECT_TYPE_HEAT_WAVE_HORIZONTAL: u32 = 2u;
const EFFECT_TYPE_HEAT_WAVE_VERTICAL: u32 = 3u;
const EFFECT_TYPE_FLAG: u32 = 4u;

const LUMA_VALUE: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);

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
        let rain_color = vec3<f32>(fx.drop_angle_and_rain.z, fx.drop_angle_and_rain.w, fx.wiggle_and_rain.w);

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
        let rwpos = (wpos - orig_wpos) / vec2<f32>(1280.0, 720.0);
        let refracted = sample_tex_srgb(clamp(sample_uv + rwpos, frame_min, frame_max));
        var rgb = refracted.rgb + add;
        rgb = mix(rgb, rain_color, 0.1 * rain_sum);
        srgb_col = vec4<f32>(rgb, refracted.a);
    }

    if (mode_flags & MODE_OVERLAY_BLEND) != 0u && (extra_flags & FLAG_HAS_AUX_TEX) != 0u {
        let aux_sample = sample_aux_srgb(sample_uv + vec2<f32>(0.1, 0.2));
        srgb_col = blend_overlay(srgb_col, aux_sample);
    }

    if (mode_flags & MODE_TEXTURE_SWAP) != 0u && (extra_flags & FLAG_HAS_AUX_TEX) != 0u {
        let aux_sample = sample_aux_srgb(sample_uv);
        let swap_amt = clamp(fx.time_and_extra.y, 0.0, 1.0);
        srgb_col = vec4<f32>(mix(srgb_col.rgb, aux_sample.rgb, swap_amt), srgb_col.a);
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

        var base_rgb = srgb_col.rgb;
        if (extra_flags & FLAG_PREMULTIPLIED_INPUT) != 0u && srgb_col.a > 0.0 {
            base_rgb = clamp(base_rgb / srgb_col.a, vec3<f32>(0.0), vec3<f32>(1.0));
        }
        var color3 = apply_hsbc_effect(base_rgb);
        color3 += fx.drop_color_and_mask.xyz * rim;
        srgb_col = vec4<f32>(clamp(color3, vec3<f32>(0.0), vec3<f32>(1.0)), srgb_col.a);
    } else if (mode_flags & MODE_ADJUST_COLOR) != 0u {
        var base_rgb = srgb_col.rgb;
        if (extra_flags & FLAG_PREMULTIPLIED_INPUT) != 0u && srgb_col.a > 0.0 {
            base_rgb = clamp(base_rgb / srgb_col.a, vec3<f32>(0.0), vec3<f32>(1.0));
        }
        let color3 = apply_hsbc_effect(base_rgb);
        srgb_col = vec4<f32>(clamp(color3, vec3<f32>(0.0), vec3<f32>(1.0)), srgb_col.a);
    }

    var lin_col = vec4<f32>(srgb_to_linear(max(srgb_col.rgb, vec3<f32>(0.0))), srgb_col.a);
    let pre_building_a = lin_col.a;
    lin_col = sprite_functions::get_final_color(lin_col, instance_index);

    // BuildingShader.hx subtracts alphaShit directly from the texture alpha while keeping
    // the sprite's RGB tint; avoid double-attenuating by material.color.a from get_final_color.
    if (mode_flags & MODE_BUILDING) != 0u {
        lin_col.a = select(0.0, max(pre_building_a - fx.time_and_extra.y, 0.0), pre_building_a > 0.0);
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
