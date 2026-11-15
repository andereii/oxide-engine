import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;
import bevy_render::color_operations::{linear_to_srgb, srgb_to_linear};

struct RainCameraEffect {
    // (cam_left, cam_top, cam_right, cam_bottom)
    bounds: vec4<f32>,
    // (screen_w, screen_h, scale, intensity)
    screen_and_scale: vec4<f32>,
    // (u_time, rain_r, rain_g, rain_b)
    time_and_color: vec4<f32>,
};

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var texture_sampler: sampler;
@group(0) @binding(2) var<uniform> rain: RainCameraEffect;

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

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let u_scale = max(rain.screen_and_scale.z, 0.01);
    let u_intensity = rain.screen_and_scale.w;
    let u_time = rain.time_and_color.x;
    let rain_color = rain.time_and_color.yzw;

    let origin = rain.bounds.xy;
    let span = max(rain.bounds.zw - rain.bounds.xy, vec2<f32>(1.0));
    var wpos = in.uv * span + origin;

    var add = vec3<f32>(0.0);
    var rain_sum = 0.0;
    let scales = array<f32, 4>(1.0, 1.8, 2.6, 4.8);
    if u_intensity > 0.0001 {
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
    }

    let sample_uv = clamp((wpos - origin) / span, vec2<f32>(0.001), vec2<f32>(0.999));
    let sampled = textureSampleLevel(screen_texture, texture_sampler, sample_uv, 0.0);
    var srgb_rgb = linear_to_srgb(max(sampled.rgb, vec3<f32>(0.0)));
    srgb_rgb += add;
    srgb_rgb = mix(srgb_rgb, rain_color, 0.1 * rain_sum);

    return vec4<f32>(srgb_to_linear(max(srgb_rgb, vec3<f32>(0.0))), sampled.a);
}
