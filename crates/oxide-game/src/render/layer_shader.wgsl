// Per-layer gameplay material for the M6-CONTRACT §15 shader families.
//
// The WGSL is a direct transcription of the frozen behavior specification:
// `AdjustColor` (HSBC), `DropShadow` (rim lighting + HSBC + optional alt mask),
// note `HSV` desaturation, `WiggleEffectRuntime` displaced UVs,
// `RuntimeRainShader` (screen and sprite modes) and `SserafimShader`
// (event-tweened dark/pulse/truck light composition). The shared HSBC
// composition order is brightness -> hue -> contrast -> saturation.
//
// Textures are `Rgba8UnormSrgb`, so `textureSample` returns LINEAR RGB with
// straight (non-premultiplied) alpha. The frozen shader math operates on the
// sampled color exactly as specified: brightness is added to the straight RGB,
// and `DropShadow` luma multiplies RGB by the straight alpha so transparent
// texels contribute zero luma. The material writes STRAIGHT RGBA; Bevy's
// `AlphaMode2d::Blend` (`SRC_ALPHA, ONE_MINUS_SRC_ALPHA`) consumes it.

struct LayerUniforms {
    // Straight-alpha tint: RGB is linear, A multiplies the sampled alpha.
    tint: vec4<f32>,
    // x = shader kind (0 adjust, 1 drop shadow, 2 hsv, 3 wiggle, 4 rain,
    // 5 sserafim), y = alternate-mask enabled, z/w reserved.
    flags: vec4<f32>,
    // x = brightness / 255, y = contrast scale (already curved).
    hsbc: vec4<f32>,
    hue0: vec4<f32>,
    hue1: vec4<f32>,
    hue2: vec4<f32>,
    sat0: vec4<f32>,
    sat1: vec4<f32>,
    sat2: vec4<f32>,
    // x = cos(angle), y = sin(angle), z = distance, w = strength.
    shadow0: vec4<f32>,
    // x = threshold, y = mask threshold.
    shadow1: vec4<f32>,
    // Current animation-frame UV bounds [left, top, right, bottom].
    frame_bounds: vec4<f32>,
    // HSV multiplicatives: hue, saturation, value.
    hsv: vec4<f32>,
    // Drop-shadow rim color normalized by 255.
    drop_color: vec4<f32>,
    // Wiggle: x = effect enum, y = speed, z = frequency, w = amplitude (UV).
    effect0: vec4<f32>,
    // Wiggle: x = time (seconds).
    effect1: vec4<f32>,
    // Rain: x = time (s), y = scale, z = intensity, w = sprite mode (0/1).
    rain0: vec4<f32>,
    // Rain: rgb = normalized rain color.
    rain1: vec4<f32>,
    // Rain: xy = screen resolution.
    rain2: vec4<f32>,
    // Rain: [left, top, right, bottom] camera view bounds.
    camera_bounds: vec4<f32>,
    // Sserafim: x = darken amount, y = pulse strength, z = truck strength,
    // w = isChar (0/1).
    sserafim0: vec4<f32>,
    // Sserafim: rgb = normalized pulse light color.
    sserafim1: vec4<f32>,
};

@group(2) @binding(0) var<uniform> u: LayerUniforms;
@group(2) @binding(1) var base_texture: texture_2d<f32>;
@group(2) @binding(2) var base_sampler: sampler;
@group(2) @binding(3) var alt_mask: texture_2d<f32>;
@group(2) @binding(4) var alt_mask_sampler: sampler;

// `SserafimShader` `LIGHT_MULTIPLIER`.
const LIGHT_MULTIPLIER: f32 = 0.07;

struct FragmentInput {
    @location(2) uv: vec2<f32>,
};

fn hsbc_apply(color: vec3<f32>) -> vec3<f32> {
    let bright = vec3<f32>(u.hsbc.x) + color;
    let bh = vec3<f32>(
        dot(bright, u.hue0.xyz),
        dot(bright, u.hue1.xyz),
        dot(bright, u.hue2.xyz),
    );
    let contrasted = (bh - vec3<f32>(0.25)) * u.hsbc.y + vec3<f32>(0.25);
    return vec3<f32>(
        dot(contrasted, u.sat0.xyz),
        dot(contrasted, u.sat1.xyz),
        dot(contrasted, u.sat2.xyz),
    );
}

fn get_luma(rgba: vec4<f32>) -> f32 {
    // Straight-alpha Bevy sample: premultiply before the luma dot so a
    // transparent texel has zero luma (the native `getLumaRGB` input).
    return dot(rgba.rgb * rgba.a, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// Difference-based anti-aliased edge width (native `lwidth_manual`).
fn lwidth_manual(center: f32, uv: vec2<f32>, px: vec2<f32>) -> f32 {
    let right = get_luma(textureSample(base_texture, base_sampler, uv + vec2<f32>(px.x, 0.0)));
    let down = get_luma(textureSample(base_texture, base_sampler, uv + vec2<f32>(0.0, px.y)));
    let diagonal = get_luma(textureSample(base_texture, base_sampler, uv + px));
    let dx = abs(right - center);
    let dy = abs(down - center);
    let dd = abs(diagonal - center);
    return ((dx + dy + dd * 0.7) / 2.7) * 2.0;
}

fn rgb_to_hsv(rgb: vec3<f32>) -> vec3<f32> {
    let maxc = max(rgb.r, max(rgb.g, rgb.b));
    let minc = min(rgb.r, min(rgb.g, rgb.b));
    let delta = maxc - minc;
    var hue = 0.0;
    if delta > 0.0 {
        if maxc == rgb.r {
            hue = (rgb.g - rgb.b) / delta;
            if hue < 0.0 {
                hue = hue + 6.0;
            }
        } else if maxc == rgb.g {
            hue = 2.0 + (rgb.b - rgb.r) / delta;
        } else {
            hue = 4.0 + (rgb.r - rgb.g) / delta;
        }
        hue = hue / 6.0;
    }
    var saturation = 0.0;
    if maxc > 0.0 {
        saturation = delta / maxc;
    }
    return vec3<f32>(hue, saturation, maxc);
}

fn hsv_to_rgb(hsv: vec3<f32>) -> vec3<f32> {
    let hue = fract(hsv.x) * 6.0;
    let saturation = hsv.y;
    let value = hsv.z;
    let sector = floor(hue);
    let fraction = hue - sector;
    let p = value * (1.0 - saturation);
    let q = value * (1.0 - saturation * fraction);
    let t = value * (1.0 - saturation * (1.0 - fraction));
    switch i32(sector) % 6 {
        case 0: { return vec3<f32>(value, t, p); }
        case 1: { return vec3<f32>(q, value, p); }
        case 2: { return vec3<f32>(p, value, t); }
        case 3: { return vec3<f32>(p, q, value); }
        case 4: { return vec3<f32>(t, p, value); }
        default: { return vec3<f32>(value, p, q); }
    }
}

// `SserafimShader` inline GLSL: HSBC with the native 0.213/0.715/0.072 hue
// and 0.3098/0.6078/0.0824 saturation weights. The hue/saturation matrices
// are built in `shaders.rs` and reuse the shared hue0..2/sat0..2 slots.
fn sserafim_hsbc_apply(color: vec3<f32>) -> vec3<f32> {
    let bright = color + vec3<f32>(u.hsbc.x);
    let bh = vec3<f32>(
        dot(bright, u.hue0.xyz),
        dot(bright, u.hue1.xyz),
        dot(bright, u.hue2.xyz),
    );
    let contrasted = clamp(
        (bh - vec3<f32>(0.25)) * u.hsbc.y + vec3<f32>(0.25),
        vec3<f32>(0.0),
        vec3<f32>(1.0),
    );
    return clamp(
        vec3<f32>(
            dot(contrasted, u.sat0.xyz),
            dot(contrasted, u.sat1.xyz),
            dot(contrasted, u.sat2.xyz),
        ),
        vec3<f32>(0.0),
        vec3<f32>(1.0),
    );
}

fn sserafim_rgb2hsl(c: vec3<f32>) -> vec3<f32> {
    let c_min = min(min(c.r, c.g), c.b);
    let c_max = max(max(c.r, c.g), c.b);
    let delta = c_max - c_min;
    var hsl = vec3<f32>(0.0, 0.0, (c_max + c_min) * 0.5);
    if delta != 0.0 {
        hsl.y = select(
            delta / (2.0 - c_max - c_min),
            delta / (c_max + c_min),
            hsl.z < 0.5,
        );
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

fn sserafim_hue2rgb(h: f32) -> vec3<f32> {
    let hf = fract(h);
    return clamp(
        vec3<f32>(
            abs(hf * 6.0 - 3.0) - 1.0,
            2.0 - abs(hf * 6.0 - 2.0),
            2.0 - abs(hf * 6.0 - 4.0),
        ),
        vec3<f32>(0.0),
        vec3<f32>(1.0),
    );
}

fn sserafim_hsl2rgb(hsl: vec3<f32>) -> vec3<f32> {
    if hsl.y == 0.0 {
        return vec3<f32>(hsl.z);
    }
    let b = select(
        hsl.z + hsl.y - hsl.y * hsl.z,
        hsl.z * (1.0 + hsl.y),
        hsl.z < 0.5,
    );
    let a = 2.0 * hsl.z - b;
    return vec3<f32>(a) + sserafim_hue2rgb(hsl.x) * (b - a);
}

// `WiggleEffectRuntime` (`wiggle.frag`): displaced UV per effect type.
fn wiggle_uv(uv: vec2<f32>) -> vec2<f32> {
    let tex_size = vec2<f32>(textureDimensions(base_texture));
    let speed = u.effect0.y;
    let frequency = u.effect0.z;
    let amplitude = u.effect0.w;
    let time = u.effect1.x;
    var pt = uv;
    let effect = i32(u.effect0.x);
    if effect == 0 {
        // DREAMY: pixel-grid quantized two-axis sine.
        let w = 1.0 / tex_size.y;
        let h = 1.0 / tex_size.x;
        pt.x = floor(pt.x / h) * h;
        let offset_x = sin(pt.x * frequency + time * speed) * amplitude;
        pt.y = pt.y + floor(offset_x / w) * w;
        pt.y = floor(pt.y / w) * w;
        let offset_y =
            sin(pt.y * (frequency / 2.0) + time * (speed / 2.0)) * (amplitude / 2.0);
        pt.x = pt.x + floor(offset_y / h) * h;
    } else if effect == 1 {
        pt.y = pt.y + sin(pt.x * frequency + time * speed) * amplitude;
    } else if effect == 2 {
        pt.x = pt.x + sin(pt.x * frequency + time * speed) * amplitude;
    } else if effect == 3 {
        pt.y = pt.y + sin(pt.y * frequency + time * speed) * amplitude;
    } else if effect == 4 {
        let y = sin(pt.y * frequency + 10.0 * pt.x + time * speed) * amplitude;
        let x = sin(pt.x * frequency + 5.0 * pt.y + time * speed) * amplitude;
        pt = vec2<f32>(pt.x + x, pt.y + y);
    }
    return pt;
}

// GLSL `mod(x, y) = x - y * floor(x / y)`; WGSL `%` truncates instead, so
// negative grid coordinates must use this form (`rain.frag` rand/rainDist).
fn glsl_mod2(x: vec2<f32>, y: vec2<f32>) -> vec2<f32> {
    return x - y * floor(x / y);
}

fn glsl_mod1(x: f32, y: f32) -> f32 {
    return x - y * floor(x / y);
}

fn rain_rand(a: vec2<f32>) -> f32 {
    return fract(
        sin(dot(glsl_mod2(a, vec2<f32>(1000.0)), vec2<f32>(12.9898, 78.233)))
            * 43758.5453,
    );
}

fn rain_dist(p_in: vec2<f32>, scale: f32, intensity: f32, time: f32) -> f32 {
    var p = p_in * 0.1;
    p.x = p.x + p.y * 0.1;
    p.y = p.y - time * 500.0 / scale;
    p.y = p.y * 0.03;
    let ix = floor(p.x);
    p.y = p.y + glsl_mod1(ix, 2.0) * 0.5 + (rain_rand(vec2<f32>(ix, ix)) - 0.5) * 0.3;
    let iy = floor(p.y);
    let index = vec2<f32>(ix, iy);
    p = p - index;
    p.x = p.x + (rain_rand(index.yx) * 2.0 - 1.0) * 0.35;
    let a = abs(p - vec2<f32>(0.5, 0.5));
    let res = max(a.x * 0.8, a.y * 0.5) - 0.1;
    let empty = rain_rand(index) < mix(1.0, 0.1, intensity);
    return select(res, 1.0, empty);
}

// `RuntimePostEffectShader` coordinate helpers.
fn screen_to_world(sc: vec2<f32>) -> vec2<f32> {
    let scale = u.camera_bounds.zw - u.camera_bounds.xy;
    return sc * scale + u.camera_bounds.xy;
}

fn world_to_screen(wc: vec2<f32>) -> vec2<f32> {
    let scale = u.camera_bounds.zw - u.camera_bounds.xy;
    return (wc - u.camera_bounds.xy) / scale;
}

fn screen_to_frame(sc: vec2<f32>) -> vec2<f32> {
    let size = max(u.frame_bounds.zw - u.frame_bounds.xy, vec2<f32>(1e-6));
    let clamped = clamp(sc, u.frame_bounds.xy, u.frame_bounds.zw);
    return (clamped - u.frame_bounds.xy) / size;
}

// `RuntimeRainShader` (`rain.frag`), dead puddle/light paths omitted.
fn rain_color_at(uv: vec2<f32>) -> vec4<f32> {
    let scale = u.rain0.y;
    let intensity = u.rain0.z;
    let time = u.rain0.x;
    let sprite_mode = u.rain0.w > 0.5;
    var wpos: vec2<f32>;
    if sprite_mode {
        wpos = screen_to_world(screen_to_frame(uv));
    } else {
        wpos = screen_to_world(uv);
    }
    let orig_wpos = wpos;
    var add = vec3<f32>(0.0, 0.0, 0.0);
    var rain_sum = 0.0;
    let layer_scales = array<f32, 4>(1.0, 1.8, 2.6, 4.8);
    for (var i: i32 = 0; i < 4; i = i + 1) {
        let layer_scale = layer_scales[i];
        let r = rain_dist(
            wpos * layer_scale / scale + vec2<f32>(500.0 * f32(i)),
            layer_scale,
            intensity,
            time,
        );
        if r < 0.0 {
            let v = (1.0 - exp(r * 5.0)) / layer_scale * 2.0;
            wpos.x = wpos.x + v * 10.0 * scale;
            wpos.y = wpos.y - v * 2.0 * scale;
            add = add + vec3<f32>(0.1, 0.15, 0.2) * v;
            rain_sum = rain_sum + (1.0 - rain_sum) * 0.75;
        }
    }
    var sampled: vec4<f32>;
    if sprite_mode {
        let rwpos = (wpos - orig_wpos) / (u.camera_bounds.zw - u.camera_bounds.xy);
        sampled = textureSample(base_texture, base_sampler, uv + rwpos);
    } else {
        let sample_uv = clamp(world_to_screen(wpos), vec2<f32>(0.0), vec2<f32>(1.0));
        sampled = textureSample(base_texture, base_sampler, sample_uv);
    }
    var rgb = sampled.rgb + add;
    rgb = mix(rgb, u.rain1.xyz, 0.1 * rain_sum);
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), sampled.a * u.tint.a);
}

@fragment
fn fragment(input: FragmentInput) -> @location(0) vec4<f32> {
    let kind = i32(u.flags.x);
    if kind == 3 {
        let sampled = textureSample(base_texture, base_sampler, wiggle_uv(input.uv));
        let color4 = sampled * u.tint;
        return vec4<f32>(clamp(color4.rgb, vec3<f32>(0.0), vec3<f32>(1.0)), color4.a);
    }
    if kind == 4 {
        return rain_color_at(input.uv);
    }
    let sampled = textureSample(base_texture, base_sampler, input.uv);
    let color4 = sampled * u.tint;

    if kind == 1 {
        // DropShadowShader (inline native GLSL).
        let ratio = 1.0 / vec2<f32>(textureDimensions(base_texture));
        let px = max(ratio, fwidth(input.uv));
        let luma = get_luma(color4);
        let delta = lwidth_manual(luma, input.uv, px);
        var threshold = u.shadow1.x;
        if u.flags.y > 0.5
            && textureSample(alt_mask, alt_mask_sampler, input.uv).b > 0.0
        {
            threshold = u.shadow1.y;
        }
        let intensity = smoothstep(threshold - delta, threshold + delta, luma);
        var color3 = hsbc_apply(color4.rgb);
        let checked = vec2<f32>(
            input.uv.x + u.shadow0.z * u.shadow0.x * ratio.x,
            input.uv.y - u.shadow0.z * u.shadow0.y * ratio.y,
        );
        var shadow_alpha = 0.0;
        if checked.x > u.frame_bounds.x
            && checked.y > u.frame_bounds.y
            && checked.x < u.frame_bounds.z
            && checked.y < u.frame_bounds.w
        {
            // Bounds-conditioned sample: explicit LOD keeps this valid in
            // non-uniform control flow.
            shadow_alpha = textureSampleLevel(
                base_texture,
                base_sampler,
                checked,
                0.0,
            ).a;
        }
        let rim = (1.0 - shadow_alpha * u.shadow0.w) * intensity;
        color3 = color3 + u.drop_color.rgb * rim;
        return vec4<f32>(clamp(color3, vec3<f32>(0.0), vec3<f32>(1.0)), color4.a);
    }

    if kind == 5 {
        // SserafimShader (inline native GLSL): HSBC then the event-tweened
        // dark/pulse/truck composition. `darkAmt > 0.65` flips the sign of
        // the combined light term.
        let base_color = sserafim_hsbc_apply(color4.rgb);
        let combined_alpha = u.sserafim0.z + u.sserafim0.y;
        var dark_factor = combined_alpha * LIGHT_MULTIPLIER + u.sserafim0.x;
        if u.sserafim0.x > 0.65 {
            dark_factor = combined_alpha * (-LIGHT_MULTIPLIER) + u.sserafim0.x;
        }
        var light_hsl = sserafim_rgb2hsl(u.sserafim1.xyz);
        light_hsl.z = light_hsl.z * u.sserafim0.y;
        let bruh_color = sserafim_hsl2rgb(light_hsl);
        var multiply_color: vec3<f32>;
        if u.sserafim0.w > 0.5 {
            multiply_color = mix(vec3<f32>(1.0), vec3<f32>(0.0), dark_factor / 5.0);
            multiply_color = mix(multiply_color, bruh_color, dark_factor / 3.0);
        } else {
            multiply_color = mix(vec3<f32>(1.0), vec3<f32>(0.0), dark_factor);
            multiply_color = mix(multiply_color, bruh_color, dark_factor / 2.0);
        }
        let out_color = clamp(base_color * multiply_color, vec3<f32>(0.0), vec3<f32>(1.0));
        return vec4<f32>(out_color, sampled.a * u.tint.a);
    }

    if kind == 2 {
        // HSVShader (`hsv.frag`): multiplicative factors plus the value
        // coupling `value *= (hue * 0.5) + 0.5`.
        var hsv = rgb_to_hsv(color4.rgb);
        hsv.x = hsv.x * u.hsv.x;
        hsv.y = hsv.y * u.hsv.y;
        hsv.z = hsv.z * u.hsv.z;
        hsv.z = hsv.z * (u.hsv.x * 0.5 + 0.5);
        let color3 = hsv_to_rgb(hsv);
        return vec4<f32>(clamp(color3, vec3<f32>(0.0), vec3<f32>(1.0)), color4.a);
    }

    // AdjustColorShader (`adjustColor.frag`): fixed brightness -> hue ->
    // contrast -> saturation order, straight-alpha in and out.
    let color3 = hsbc_apply(color4.rgb);
    return vec4<f32>(clamp(color3, vec3<f32>(0.0), vec3<f32>(1.0)), color4.a);
}
