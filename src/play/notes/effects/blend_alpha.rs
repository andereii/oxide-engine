use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;

/// Converts black-keyed screen/additive splash sheets into luminance-derived alpha in-memory
/// so Bevy's HUD `Sprite` pipeline renders them without black fringe boxes and without modifying
/// source image files on disk.
pub(super) fn convert_screen_image_to_alpha(img: &mut Image) {
    let is_srgb = match img.texture_descriptor.format {
        TextureFormat::Rgba8UnormSrgb => true,
        TextureFormat::Rgba8Unorm => false,
        _ => return,
    };
    let Some(data) = img.data.as_mut() else {
        return;
    };
    for pixel in data.chunks_exact_mut(4) {
        screen_pixel_to_alpha(pixel, is_srgb);
    }
}

pub(super) fn screen_pixel_to_alpha(pixel: &mut [u8], is_srgb: bool) {
    if pixel[3] == 0 {
        return;
    }
    if is_srgb {
        let lin = LinearRgba::from(Color::srgba_u8(pixel[0], pixel[1], pixel[2], 255));
        let peak = lin.red.max(lin.green).max(lin.blue).clamp(0.0, 1.0);
        if peak <= 0.0 {
            pixel[3] = 0;
            return;
        }
        let inv = 1.0 / peak;
        let out_bytes = Srgba::from(LinearRgba::new(
            (lin.red * inv).clamp(0.0, 1.0),
            (lin.green * inv).clamp(0.0, 1.0),
            (lin.blue * inv).clamp(0.0, 1.0),
            1.0,
        ))
        .to_u8_array();
        pixel[0] = out_bytes[0];
        pixel[1] = out_bytes[1];
        pixel[2] = out_bytes[2];
        pixel[3] = (f32::from(pixel[3]) * peak).round().clamp(0.0, 255.0) as u8;
    } else {
        let peak = pixel[0].max(pixel[1]).max(pixel[2]);
        if peak == 0 {
            pixel[3] = 0;
            return;
        }
        let inv = 255.0 / f32::from(peak);
        pixel[0] = (f32::from(pixel[0]) * inv).round().clamp(0.0, 255.0) as u8;
        pixel[1] = (f32::from(pixel[1]) * inv).round().clamp(0.0, 255.0) as u8;
        pixel[2] = (f32::from(pixel[2]) * inv).round().clamp(0.0, 255.0) as u8;
        pixel[3] = ((f32::from(pixel[3]) * f32::from(peak)) / 255.0)
            .round()
            .clamp(0.0, 255.0) as u8;
    }
}
