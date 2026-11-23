use std::collections::HashMap;
use std::fs;
use std::path::Path;

use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, Image, ImageSampler, ImageType};
use bevy::render::render_resource::TextureFormat;

use super::animate_atlas::{
    AnimateAtlas, AnimateElement, AnimateLayerType, AnimateSpriteEntry, AnimateTimelineFrame,
    AnimateTimelineLayer, BakedSheetImage,
};
use super::atlas_composite::{
    CompositePiece, collect_single_layer_frame_pieces, collect_single_layer_tick_pieces,
    mul_affine, upright_size,
};

const MAX_BAKED_SHEET_DIM: u32 = 4096;
const BAKED_PADDING_PX: u32 = 1;

struct LoadedSheetPixels {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

#[derive(Hash, PartialEq, Eq)]
struct PieceKey {
    sprite_index: usize,
    spritemap_idx: usize,
    rotated: bool,
    matrix_bits: [u32; 6],
    color_bits: [u32; 4],
}

#[derive(Hash, PartialEq, Eq)]
struct BakedMaskKey {
    bbox: [i32; 4],
    masked: Vec<PieceKey>,
    masker: Vec<PieceKey>,
}

struct ShelfPacker {
    sheets: Vec<BakedSheetImage>,
    cur_x: u32,
    cur_y: u32,
    row_h: u32,
    sheet_w: u32,
    sheet_h: u32,
}

impl ShelfPacker {
    fn new() -> Self {
        Self {
            sheets: Vec::new(),
            cur_x: 0,
            cur_y: 0,
            row_h: 0,
            sheet_w: MAX_BAKED_SHEET_DIM,
            sheet_h: MAX_BAKED_SHEET_DIM,
        }
    }

    fn alloc(&mut self, w: u32, h: u32, rgba: &[u8]) -> Option<(usize, u32, u32)> {
        if w == 0 || h == 0 || w > MAX_BAKED_SHEET_DIM || h > MAX_BAKED_SHEET_DIM {
            return None;
        }
        let pw = w + BAKED_PADDING_PX;
        let ph = h + BAKED_PADDING_PX;
        if self.sheets.is_empty() {
            self.push_new_sheet();
        }
        if self.cur_x + pw > self.sheet_w {
            self.cur_x = 0;
            self.cur_y += self.row_h;
            self.row_h = 0;
        }
        if self.cur_y + ph > self.sheet_h {
            self.push_new_sheet();
        }
        let sheet_idx = self.sheets.len() - 1;
        let x = self.cur_x;
        let y = self.cur_y;
        let sheet = &mut self.sheets[sheet_idx];
        for row in 0..h {
            let src_start = (row * w * 4) as usize;
            let src_end = src_start + (w * 4) as usize;
            let dst_start = (((y + row) * sheet.width + x) * 4) as usize;
            let dst_end = dst_start + (w * 4) as usize;
            sheet.rgba[dst_start..dst_end].copy_from_slice(&rgba[src_start..src_end]);
        }
        self.cur_x += pw;
        self.row_h = self.row_h.max(ph);
        Some((sheet_idx, x, y))
    }

    fn push_new_sheet(&mut self) {
        let len = (self.sheet_w * self.sheet_h * 4) as usize;
        self.sheets.push(BakedSheetImage {
            width: self.sheet_w,
            height: self.sheet_h,
            rgba: vec![0u8; len],
        });
        self.cur_x = 0;
        self.cur_y = 0;
        self.row_h = 0;
    }

    fn finish(mut self) -> Vec<BakedSheetImage> {
        if let Some(last) = self.sheets.last_mut() {
            let used_h = (self.cur_y + self.row_h).max(1).min(last.height);
            if used_h < last.height {
                last.height = used_h;
                last.rgba.truncate((last.width * used_h * 4) as usize);
            }
        }
        self.sheets
    }
}

fn any_clipped_layer(layers: &[AnimateTimelineLayer]) -> bool {
    layers
        .iter()
        .any(|l| l.layer_type == AnimateLayerType::Clipped)
}

fn load_spritemap_pixels(dir: &Path, count: usize) -> Option<Vec<LoadedSheetPixels>> {
    let mut out = Vec::with_capacity(count);
    for idx in 1..=count {
        let path = dir.join(format!("spritemap{idx}.png"));
        let bytes = fs::read(&path).ok()?;
        let mut img = Image::from_buffer(
            &bytes,
            ImageType::Extension("png"),
            CompressedImageFormats::NONE,
            true,
            ImageSampler::Default,
            RenderAssetUsages::MAIN_WORLD,
        )
        .ok()?;
        if img.texture_descriptor.format != TextureFormat::Rgba8UnormSrgb {
            img = img.convert(TextureFormat::Rgba8UnormSrgb)?;
        }
        let width = img.width();
        let height = img.height();
        let rgba = img.data?;
        if rgba.len() < (width * height * 4) as usize {
            return None;
        }
        out.push(LoadedSheetPixels {
            width,
            height,
            rgba,
        });
    }
    Some(out)
}

fn pieces_aabb(pieces: &[CompositePiece]) -> Option<[i32; 4]> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for piece in pieces {
        let (w, h) = upright_size(piece);
        let m = piece.matrix;
        for (cx, cy) in [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)] {
            let x = m[0] * cx + m[2] * cy + m[4];
            let y = m[1] * cx + m[3] * cy + m[5];
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    if !min_x.is_finite() || !min_y.is_finite() || !max_x.is_finite() || !max_y.is_finite() {
        return None;
    }
    Some([
        min_x.floor() as i32 - 1,
        min_y.floor() as i32 - 1,
        max_x.ceil() as i32 + 1,
        max_y.ceil() as i32 + 1,
    ])
}

fn piece_keys(pieces: &[CompositePiece]) -> Vec<PieceKey> {
    pieces
        .iter()
        .map(|p| PieceKey {
            sprite_index: p.sprite_index,
            spritemap_idx: p.spritemap_idx,
            rotated: p.rotated,
            matrix_bits: p.matrix.map(f32::to_bits),
            color_bits: p.color.map(f32::to_bits),
        })
        .collect()
}

fn sample_entry_bilinear(
    sheet: &LoadedSheetPixels,
    entry: &AnimateSpriteEntry,
    u: f32,
    v: f32,
) -> [f32; 4] {
    if entry.w == 0 || entry.h == 0 {
        return [0.0; 4];
    }
    let (fx, fy) = if entry.rotated {
        (
            entry.x as f32 + (entry.w as f32 - v) - 0.5,
            entry.y as f32 + u - 0.5,
        )
    } else {
        (entry.x as f32 + u - 0.5, entry.y as f32 + v - 0.5)
    };
    let min_x = entry.x as i32;
    let max_x = (entry.x + entry.w - 1) as i32;
    let min_y = entry.y as i32;
    let max_y = (entry.y + entry.h - 1) as i32;

    let x0 = (fx.floor() as i32).clamp(min_x, max_x);
    let y0 = (fy.floor() as i32).clamp(min_y, max_y);
    let x1 = (x0 + 1).clamp(min_x, max_x);
    let y1 = (y0 + 1).clamp(min_y, max_y);
    let tx = (fx - x0 as f32).clamp(0.0, 1.0);
    let ty = (fy - y0 as f32).clamp(0.0, 1.0);

    let read_premul = |x: i32, y: i32| -> [f32; 4] {
        let ux = (x as u32).min(sheet.width.saturating_sub(1));
        let uy = (y as u32).min(sheet.height.saturating_sub(1));
        let off = ((uy * sheet.width + ux) * 4) as usize;
        let a = f32::from(sheet.rgba[off + 3]) / 255.0;
        [
            (f32::from(sheet.rgba[off]) / 255.0) * a,
            (f32::from(sheet.rgba[off + 1]) / 255.0) * a,
            (f32::from(sheet.rgba[off + 2]) / 255.0) * a,
            a,
        ]
    };

    let p00 = read_premul(x0, y0);
    let p10 = read_premul(x1, y0);
    let p01 = read_premul(x0, y1);
    let p11 = read_premul(x1, y1);

    let w00 = (1.0 - tx) * (1.0 - ty);
    let w10 = tx * (1.0 - ty);
    let w01 = (1.0 - tx) * ty;
    let w11 = tx * ty;

    let pr = p00[0] * w00 + p10[0] * w10 + p01[0] * w01 + p11[0] * w11;
    let pg = p00[1] * w00 + p10[1] * w10 + p01[1] * w01 + p11[1] * w11;
    let pb = p00[2] * w00 + p10[2] * w10 + p01[2] * w01 + p11[2] * w11;
    let pa = (p00[3] * w00 + p10[3] * w10 + p01[3] * w01 + p11[3] * w11).clamp(0.0, 1.0);
    if pa <= 1e-6 {
        return [0.0; 4];
    }
    [
        (pr / pa).clamp(0.0, 1.0),
        (pg / pa).clamp(0.0, 1.0),
        (pb / pa).clamp(0.0, 1.0),
        pa,
    ]
}

fn rasterize_piece_to_region(
    piece: &CompositePiece,
    bbox: [i32; 4],
    atlas: &AnimateAtlas,
    sheets: &[LoadedSheetPixels],
    mut visit: impl FnMut(usize, [f32; 4]),
) {
    let Some(entry) = atlas.sprites.get(piece.sprite_index) else {
        return;
    };
    let Some(sheet) = sheets.get(entry.spritemap_idx) else {
        return;
    };
    let [a, b, c, d, tx, ty] = piece.matrix;
    let det = a * d - b * c;
    if det.abs() < 1e-8 {
        return;
    }
    let inv_det = 1.0 / det;
    let (uw, uh) = upright_size(piece);

    let Some(paabb) = pieces_aabb(std::slice::from_ref(piece)) else {
        return;
    };
    let x_start = paabb[0].max(bbox[0]);
    let y_start = paabb[1].max(bbox[1]);
    let x_end = paabb[2].min(bbox[2]);
    let y_end = paabb[3].min(bbox[3]);
    if x_end <= x_start || y_end <= y_start {
        return;
    }
    let out_w = (bbox[2] - bbox[0]) as usize;

    for wy in y_start..y_end {
        let py = (wy - bbox[1]) as usize;
        let wy_f = wy as f32 + 0.5 - ty;
        for wx in x_start..x_end {
            let wx_f = wx as f32 + 0.5 - tx;
            let u = (d * wx_f - c * wy_f) * inv_det;
            let v = (-b * wx_f + a * wy_f) * inv_det;
            if u >= 0.0 && u < uw && v >= 0.0 && v < uh {
                let sampled = sample_entry_bilinear(sheet, entry, u, v);
                if sampled[3] > 0.0 {
                    let px = (wx - bbox[0]) as usize;
                    visit(
                        py * out_w + px,
                        [
                            sampled[0] * piece.color[0],
                            sampled[1] * piece.color[1],
                            sampled[2] * piece.color[2],
                            (sampled[3] * piece.color[3]).clamp(0.0, 1.0),
                        ],
                    );
                }
            }
        }
    }
}

fn rasterize_masked_bitmap(
    bbox: [i32; 4],
    masked: &[CompositePiece],
    masker: &[CompositePiece],
    atlas: &AnimateAtlas,
    sheets: &[LoadedSheetPixels],
) -> Vec<u8> {
    let w = (bbox[2] - bbox[0]).max(0) as usize;
    let h = (bbox[3] - bbox[1]).max(0) as usize;
    let px_count = w * h;
    let mut color_buf = vec![0.0_f32; px_count * 4];
    for piece in masked {
        rasterize_piece_to_region(piece, bbox, atlas, sheets, |idx, src| {
            let base = idx * 4;
            let sa = src[3];
            let da = color_buf[base + 3];
            let out_a = sa + da * (1.0 - sa);
            if out_a > 1e-6 {
                let inv = 1.0 / out_a;
                let keep = da * (1.0 - sa);
                color_buf[base] = (src[0] * sa + color_buf[base] * keep) * inv;
                color_buf[base + 1] = (src[1] * sa + color_buf[base + 1] * keep) * inv;
                color_buf[base + 2] = (src[2] * sa + color_buf[base + 2] * keep) * inv;
                color_buf[base + 3] = out_a;
            }
        });
    }

    let mut mask_alpha = vec![0.0_f32; px_count];
    for piece in masker {
        rasterize_piece_to_region(piece, bbox, atlas, sheets, |idx, src| {
            let prev = mask_alpha[idx];
            mask_alpha[idx] = prev + src[3] * (1.0 - prev);
        });
    }

    let mut out = vec![0u8; px_count * 4];
    for i in 0..px_count {
        let base = i * 4;
        let final_a = (color_buf[base + 3] * mask_alpha[i]).clamp(0.0, 1.0);
        if final_a <= 0.0 {
            continue;
        }
        out[base] = (color_buf[base] * 255.0).round().clamp(0.0, 255.0) as u8;
        out[base + 1] = (color_buf[base + 1] * 255.0).round().clamp(0.0, 255.0) as u8;
        out[base + 2] = (color_buf[base + 2] * 255.0).round().clamp(0.0, 255.0) as u8;
        out[base + 3] = (final_a * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    out
}

fn find_clipper_layer_idx(layers: &[AnimateTimelineLayer], layer_idx: usize) -> Option<usize> {
    let target = layers[layer_idx].clipped_by.as_deref()?;
    (0..layer_idx).rev().find(|&i| {
        layers[i].layer_type == AnimateLayerType::Clipper && layers[i].name == target
    })
}

fn bake_timeline_list(
    atlas: &mut AnimateAtlas,
    sheets: &[LoadedSheetPixels],
    layers: &mut [AnimateTimelineLayer],
    packer: &mut ShelfPacker,
    dedup: &mut HashMap<BakedMaskKey, (String, [i32; 2])>,
    orig_spritemap_count: usize,
    seq: &mut usize,
) {
    let indices: HashMap<String, usize> = atlas
        .sprites
        .iter()
        .enumerate()
        .map(|(i, s)| (s.name.clone(), i))
        .collect();

    for idx in 0..layers.len() {
        if layers[idx].layer_type != AnimateLayerType::Clipped {
            continue;
        }
        let Some(masker_idx) = find_clipper_layer_idx(layers, idx) else {
            // Matching `flixel-animate` (`Layer.hx`): missing clipper hides the clipped layer.
            layers[idx].visible = false;
            continue;
        };

        let masker_layer = layers[masker_idx].clone();
        let orig_frames = std::mem::take(&mut layers[idx].frames);
        let mut new_frames: Vec<AnimateTimelineFrame> = Vec::with_capacity(orig_frames.len());
        let mut masker_pieces = Vec::new();
        let mut masked_pieces = Vec::new();

        for kf in orig_frames {
            if kf.elements.is_empty() {
                new_frames.push(kf);
                continue;
            }
            let mut per_tick_result: Vec<(bool, Option<(String, [i32; 2])>)> =
                Vec::with_capacity(kf.duration);

            for t in kf.index..kf.index + kf.duration {
                masker_pieces.clear();
                collect_single_layer_tick_pieces(
                    atlas,
                    &indices,
                    &masker_layer,
                    t,
                    [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                    [1.0; 4],
                    0,
                    &mut masker_pieces,
                );
                if masker_pieces.is_empty() {
                    per_tick_result.push((false, None));
                    continue;
                }

                masked_pieces.clear();
                collect_single_layer_frame_pieces(
                    atlas,
                    &indices,
                    &kf,
                    t - kf.index,
                    [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                    [1.0; 4],
                    0,
                    &mut masked_pieces,
                );
                let (Some(masked_bb), Some(masker_bb)) =
                    (pieces_aabb(&masked_pieces), pieces_aabb(&masker_pieces))
                else {
                    per_tick_result.push((true, None));
                    continue;
                };
                let ix0 = masked_bb[0].max(masker_bb[0]);
                let iy0 = masked_bb[1].max(masker_bb[1]);
                let ix1 = masked_bb[2].min(masker_bb[2]);
                let iy1 = masked_bb[3].min(masker_bb[3]);
                if ix1 <= ix0 || iy1 <= iy0 {
                    per_tick_result.push((true, None));
                    continue;
                }
                let ibox = [ix0, iy0, ix1, iy1];
                let key = BakedMaskKey {
                    bbox: ibox,
                    masked: piece_keys(&masked_pieces),
                    masker: piece_keys(&masker_pieces),
                };
                let placement = if let Some(cached) = dedup.get(&key) {
                    cached.clone()
                } else {
                    let rgba = rasterize_masked_bitmap(
                        ibox,
                        &masked_pieces,
                        &masker_pieces,
                        atlas,
                        sheets,
                    );
                    let bw = (ix1 - ix0) as u32;
                    let bh = (iy1 - iy0) as u32;
                    let Some((sheet_rel_idx, ax, ay)) = packer.alloc(bw, bh, &rgba) else {
                        per_tick_result.push((true, None));
                        continue;
                    };
                    let sprite_name = format!("__baked_clip_{}", *seq);
                    *seq += 1;
                    let entry_idx = atlas.sprites.len();
                    atlas.sprite_lookup.insert(sprite_name.clone(), entry_idx);
                    atlas.sprites.push(AnimateSpriteEntry {
                        name: sprite_name.clone(),
                        x: ax,
                        y: ay,
                        w: bw,
                        h: bh,
                        rotated: false,
                        spritemap_idx: orig_spritemap_count + sheet_rel_idx,
                    });
                    let placed = (sprite_name, [ix0, iy0]);
                    dedup.insert(key, placed.clone());
                    placed
                };
                per_tick_result.push((true, Some(placement)));
            }

            // Coalesce consecutive ticks with identical masked/unmasked outputs cleanly.
            let mut offset = 0usize;
            while offset < per_tick_result.len() {
                let first = &per_tick_result[offset];
                let mut run = 1usize;
                while offset + run < per_tick_result.len() && &per_tick_result[offset + run] == first
                {
                    run += 1;
                }
                let frame_name = if offset == 0 { kf.name.clone() } else { None };
                let (was_masked, placement_opt) = first;
                let frame = if !was_masked {
                    AnimateTimelineFrame {
                        index: kf.index + offset,
                        duration: run,
                        name: frame_name,
                        color: kf.color,
                        elements: kf.elements.clone(),
                    }
                } else if let Some((sprite_name, [px, py])) = placement_opt {
                    AnimateTimelineFrame {
                        index: kf.index + offset,
                        duration: run,
                        name: frame_name,
                        color: [1.0; 4],
                        elements: vec![AnimateElement {
                            symbol_name: None,
                            sprite_name: Some(sprite_name.clone()),
                            bitmap_fallback: None,
                            matrix: mul_affine(
                                [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                                [1.0, 0.0, 0.0, 1.0, *px as f32, *py as f32],
                            ),
                            first_frame: 0,
                            last_frame: -1,
                            loop_mode: "LP".to_string(),
                            color: [1.0; 4],
                        }],
                    }
                } else {
                    AnimateTimelineFrame {
                        index: kf.index + offset,
                        duration: run,
                        name: frame_name,
                        color: [1.0; 4],
                        elements: Vec::new(),
                    }
                };
                new_frames.push(frame);
                offset += run;
            }
        }

        layers[idx].frames = new_frames;
        layers[idx].layer_type = AnimateLayerType::Normal;
        layers[idx].clipped_by = None;
    }
}

pub(super) fn bake_clipped_layers(atlas: &mut AnimateAtlas, dir: &Path) {
    let has_any = any_clipped_layer(&atlas.main_layers)
        || atlas.symbols.values().any(|layers| any_clipped_layer(layers));
    if !has_any {
        return;
    }
    let orig_count = atlas.spritemap_count.max(1);
    let Some(sheets) = load_spritemap_pixels(dir, orig_count) else {
        return;
    };

    let mut packer = ShelfPacker::new();
    let mut dedup: HashMap<BakedMaskKey, (String, [i32; 2])> = HashMap::new();
    let mut seq = 0usize;

    let sym_names: Vec<String> = atlas.symbols.keys().cloned().collect();
    // Bounded multi-pass handles clipped symbols nested inside other clipped symbols.
    let max_passes = sym_names.len().clamp(1, 8);
    for _ in 0..max_passes {
        let mut progressed = false;
        for name in &sym_names {
            let Some(mut layers) = atlas.symbols.remove(name) else {
                continue;
            };
            if any_clipped_layer(&layers) {
                bake_timeline_list(
                    atlas,
                    &sheets,
                    &mut layers,
                    &mut packer,
                    &mut dedup,
                    orig_count,
                    &mut seq,
                );
                progressed = true;
            }
            atlas.symbols.insert(name.clone(), layers);
        }
        if !progressed {
            break;
        }
    }

    if any_clipped_layer(&atlas.main_layers) {
        let mut main = std::mem::take(&mut atlas.main_layers);
        bake_timeline_list(
            atlas,
            &sheets,
            &mut main,
            &mut packer,
            &mut dedup,
            orig_count,
            &mut seq,
        );
        atlas.main_layers = main;
    }

    let finished_sheets = packer.finish();
    atlas.baked_sheets = finished_sheets;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipped_layer_masks_out_pixels_outside_clipper_and_skips_empty_masker() {
        let mut sheet = vec![0u8; 8 * 8 * 4];
        // Mask sprite (4x4 white at (0,0), with half transparent right half)
        for y in 0..4 {
            for x in 0..4 {
                let off = ((y * 8 + x) * 4) as usize;
                sheet[off] = 255;
                sheet[off + 1] = 255;
                sheet[off + 2] = 255;
                sheet[off + 3] = if x < 2 { 255 } else { 0 };
            }
        }
        // Source sprite (4x4 red at (4,0))
        for y in 0..4 {
            for x in 4..8 {
                let off = ((y * 8 + x) * 4) as usize;
                sheet[off] = 220;
                sheet[off + 1] = 30;
                sheet[off + 2] = 40;
                sheet[off + 3] = 255;
            }
        }
        let sheets = vec![LoadedSheetPixels {
            width: 8,
            height: 8,
            rgba: sheet,
        }];
        let mut atlas = AnimateAtlas {
            name: "test".to_string(),
            framerate: 24.0,
            main_symbol: "main".to_string(),
            stage_matrix: None,
            spritemap_count: 1,
            sprites: vec![
                AnimateSpriteEntry {
                    name: "mask".to_string(),
                    x: 0,
                    y: 0,
                    w: 4,
                    h: 4,
                    rotated: false,
                    spritemap_idx: 0,
                },
                AnimateSpriteEntry {
                    name: "fg".to_string(),
                    x: 4,
                    y: 0,
                    w: 4,
                    h: 4,
                    rotated: false,
                    spritemap_idx: 0,
                },
            ],
            sprite_lookup: HashMap::from([("mask".to_string(), 0), ("fg".to_string(), 1)]),
            main_layers: vec![
                AnimateTimelineLayer {
                    name: "MaskLayer".to_string(),
                    visible: false,
                    layer_type: AnimateLayerType::Clipper,
                    clipped_by: None,
                    frames: vec![
                        AnimateTimelineFrame {
                            index: 0,
                            duration: 1,
                            name: None,
                            color: [1.0; 4],
                            elements: vec![AnimateElement {
                                symbol_name: None,
                                sprite_name: Some("mask".to_string()),
                                bitmap_fallback: None,
                                matrix: [1.0, 0.0, 0.0, 1.0, 10.0, 10.0],
                                first_frame: 0,
                                last_frame: -1,
                                loop_mode: "LP".to_string(),
                                color: [1.0; 4],
                            }],
                        },
                        AnimateTimelineFrame {
                            index: 1,
                            duration: 1,
                            name: None,
                            color: [1.0; 4],
                            elements: Vec::new(),
                        },
                    ],
                },
                AnimateTimelineLayer {
                    name: "BodyLayer".to_string(),
                    visible: true,
                    layer_type: AnimateLayerType::Clipped,
                    clipped_by: Some("MaskLayer".to_string()),
                    frames: vec![AnimateTimelineFrame {
                        index: 0,
                        duration: 2,
                        name: Some("anim".to_string()),
                        color: [1.0; 4],
                        elements: vec![AnimateElement {
                            symbol_name: None,
                            sprite_name: Some("fg".to_string()),
                            bitmap_fallback: None,
                            matrix: [1.0, 0.0, 0.0, 1.0, 10.0, 10.0],
                            first_frame: 0,
                            last_frame: -1,
                            loop_mode: "LP".to_string(),
                            color: [1.0; 4],
                        }],
                    }],
                },
            ],
            symbols: HashMap::new(),
            baked_sheets: Vec::new(),
        };

        let mut packer = ShelfPacker::new();
        let mut dedup = HashMap::new();
        let mut seq = 0usize;
        let mut main = std::mem::take(&mut atlas.main_layers);
        bake_timeline_list(&mut atlas, &sheets, &mut main, &mut packer, &mut dedup, 1, &mut seq);
        atlas.main_layers = main;
        atlas.baked_sheets = packer.finish();

        // Tick 0 must be baked and masked to left 2px (mask x < 2); tick 1 stays unmasked (`fg`).
        assert_eq!(atlas.main_layers[1].frames.len(), 2);
        assert_eq!(atlas.baked_sheets.len(), 1);
        let baked_name = atlas.main_layers[1].frames[0].elements[0]
            .sprite_name
            .as_deref()
            .expect("baked sprite name");
        let entry = &atlas.sprites[atlas.sprite_lookup[baked_name]];
        let baked = &atlas.baked_sheets[0];
        let ox = entry.x + (10 - (atlas.main_layers[1].frames[0].elements[0].matrix[4] as i32)) as u32;
        let oy = entry.y + (10 - (atlas.main_layers[1].frames[0].elements[0].matrix[5] as i32)) as u32;
        let inside_a = baked.rgba[((oy * baked.width + ox) * 4 + 3) as usize];
        let outside_a = baked.rgba[((oy * baked.width + ox + 3) * 4 + 3) as usize];
        assert!(inside_a > 200);
        assert_eq!(outside_a, 0);
        assert_eq!(
            atlas.main_layers[1].frames[1].elements[0].sprite_name.as_deref(),
            Some("fg")
        );
    }
}
