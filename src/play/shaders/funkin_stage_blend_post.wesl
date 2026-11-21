import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;
import bevy_render::color_operations::{linear_to_srgb, srgb_to_linear};

struct StageOverlaySlot {
    // (top_left_x, top_left_y, world_w, world_h)
    rect: vec4<f32>,
    // (scroll_x, scroll_y, alpha, mode_f32): 0=Overlay, 1=HardLight, 2=SrgbAdd, <0=disabled
    params: vec4<f32>,
    // (tint_r, tint_g, tint_b, flip_bits_f32): bit0=flip_x, bit1=flip_y
    tint_and_flip: vec4<f32>,
};

struct StageBlendPostUniform {
    // (cam_follow_x, cam_follow_y, zoom, active_count_f32)
    camera: vec4<f32>,
    // (screen_w, screen_h, reserved, reserved)
    screen: vec4<f32>,
    slots: array<StageOverlaySlot, 4>,
};

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var<uniform> post: StageBlendPostUniform;
@group(0) @binding(3) var ov_tex0: texture_2d<f32>;
@group(0) @binding(4) var ov_tex1: texture_2d<f32>;
@group(0) @binding(5) var ov_tex2: texture_2d<f32>;
@group(0) @binding(6) var ov_tex3: texture_2d<f32>;
@group(0) @binding(7) var ov_sampler: sampler;

fn sample_overlay_slot(slot_idx: i32, uv: vec2<f32>) -> vec4<f32> {
    if slot_idx == 0 {
        return textureSampleLevel(ov_tex0, ov_sampler, uv, 0.0);
    } else if slot_idx == 1 {
        return textureSampleLevel(ov_tex1, ov_sampler, uv, 0.0);
    } else if slot_idx == 2 {
        return textureSampleLevel(ov_tex2, ov_sampler, uv, 0.0);
    }
    return textureSampleLevel(ov_tex3, ov_sampler, uv, 0.0);
}

fn apply_openfl_overlay(bg: vec3<f32>, src: vec3<f32>) -> vec3<f32> {
    let dark = 2.0 * bg * src;
    let light = vec3<f32>(1.0) - 2.0 * (vec3<f32>(1.0) - bg) * (vec3<f32>(1.0) - src);
    return select(light, dark, bg <= vec3<f32>(0.5));
}

fn apply_openfl_hardlight(bg: vec3<f32>, src: vec3<f32>) -> vec3<f32> {
    let dark = 2.0 * bg * src;
    let light = vec3<f32>(1.0) - 2.0 * (vec3<f32>(1.0) - bg) * (vec3<f32>(1.0) - src);
    return select(light, dark, src <= vec3<f32>(0.5));
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let sampled = textureSampleLevel(screen_texture, screen_sampler, in.uv, 0.0);
    var bg_srgb = clamp(linear_to_srgb(max(sampled.rgb, vec3<f32>(0.0))), vec3<f32>(0.0), vec3<f32>(1.0));

    let zoom = max(post.camera.z, 0.05);
    let screen_sz = max(post.screen.xy, vec2<f32>(1.0));
    let flx_screen = screen_sz * 0.5 + (in.uv - vec2<f32>(0.5)) * (screen_sz / zoom);
    let cam_scroll = post.camera.xy - screen_sz * 0.5;
    let count = clamp(i32(round(post.camera.w)), 0, 4);

    for (var i = 0; i < 4; i = i + 1) {
        if i >= count {
            break;
        }
        let slot = post.slots[i];
        let alpha = slot.params.z;
        let mode = slot.params.w;
        if mode < -0.5 || alpha <= 0.0001 || slot.rect.z <= 0.0 || slot.rect.w <= 0.0 {
            continue;
        }

        let world_pos = flx_screen + cam_scroll * slot.params.xy;
        var ov_uv = (world_pos - slot.rect.xy) / slot.rect.zw;
        if ov_uv.x < 0.0 || ov_uv.x > 1.0 || ov_uv.y < 0.0 || ov_uv.y > 1.0 {
            continue;
        }

        let flip_bits = u32(round(slot.tint_and_flip.w));
        if (flip_bits & 1u) != 0u {
            ov_uv.x = 1.0 - ov_uv.x;
        }
        if (flip_bits & 2u) != 0u {
            ov_uv.y = 1.0 - ov_uv.y;
        }

        let tex_sample = sample_overlay_slot(i, ov_uv);
        let eff_alpha = clamp(tex_sample.a * alpha, 0.0, 1.0);
        if eff_alpha <= 0.0001 {
            continue;
        }
        let src_srgb = clamp(
            linear_to_srgb(max(tex_sample.rgb, vec3<f32>(0.0))) * slot.tint_and_flip.rgb,
            vec3<f32>(0.0),
            vec3<f32>(1.0),
        );

        if mode < 0.5 {
            let res = apply_openfl_overlay(bg_srgb, src_srgb);
            bg_srgb = clamp(mix(bg_srgb, res, eff_alpha), vec3<f32>(0.0), vec3<f32>(1.0));
        } else if mode < 1.5 {
            let res = apply_openfl_hardlight(bg_srgb, src_srgb);
            bg_srgb = clamp(mix(bg_srgb, res, eff_alpha), vec3<f32>(0.0), vec3<f32>(1.0));
        } else {
            bg_srgb = clamp(bg_srgb + src_srgb * eff_alpha, vec3<f32>(0.0), vec3<f32>(1.0));
        }
    }

    return vec4<f32>(srgb_to_linear(max(bg_srgb, vec3<f32>(0.0))), sampled.a);
}
