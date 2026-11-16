use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use bevy::math::URect;
use bevy::prelude::*;

use crate::animations::SparrowAtlas;
use crate::utils::constants::{KEY_COUNT, MOD_ASSET_SOURCE_ID};

use crate::play::notes::skin::NoteStyle;
pub use super::visual_pools::spawn_strumline_and_pools;

#[derive(Component)]
pub struct ReceptorVisual {
    pub lane: usize,
    pub is_player: bool,
    pub last_anim_gen: u64,
}

#[derive(Debug, Clone, Default)]
pub struct AnimClipFrames {
    pub indices: Vec<usize>,
    pub offsets: Vec<[f32; 2]>,
    pub rotated: Vec<bool>,
}

impl AnimClipFrames {
    fn from_sparrow((indices, offsets, rotated): (Vec<usize>, Vec<[f32; 2]>, Vec<bool>)) -> Self {
        Self { indices, offsets, rotated }
    }

    fn first_only(&self) -> Self {
        Self {
            indices: self.indices.first().copied().into_iter().collect(),
            offsets: self.offsets.first().copied().into_iter().collect(),
            rotated: self.rotated.first().copied().into_iter().collect(),
        }
    }
}

#[derive(Component)]
pub struct NoteVisual {
    pub is_player: bool,
    pub lane: usize,
    pub assigned_idx: Option<usize>,
    pub assigned_id: usize,
}

#[derive(Component)]
pub struct SustainBodyVisual {
    pub is_player: bool,
    pub lane: usize,
    pub assigned_idx: Option<usize>,
    pub assigned_id: usize,
    pub raw_width_px: f32,
    pub end_raw_h: f32,
}

#[derive(Component)]
pub struct SustainEndVisual {
    pub is_player: bool,
    pub lane: usize,
    pub assigned_idx: Option<usize>,
    pub assigned_id: usize,
    pub end_raw_w: f32,
    pub end_raw_h: f32,
}

#[derive(Resource, Clone)]
pub struct NoteskinAssets {
    pub style: NoteStyle,
    pub strum_texture: Handle<Image>,
    pub strum_layout: Handle<TextureAtlasLayout>,
    pub strum_frames: [(AnimClipFrames, AnimClipFrames, AnimClipFrames, AnimClipFrames); 4],
    pub notes_texture: Handle<Image>,
    pub notes_layout: Handle<TextureAtlasLayout>,
    pub note_frames: [(usize, [f32; 2], bool); 4],
    pub sustain_texture: Option<Handle<Image>>,
    pub sustain_layout: Option<Handle<TextureAtlasLayout>>,
    pub sustain_indices: [(usize, usize, f32, f32); 4],
    pub splash_texture: Option<Handle<Image>>,
    pub splash_layout: Option<Handle<TextureAtlasLayout>>,
    pub splash_variants: [Vec<AnimClipFrames>; 4],
    pub hold_cover_textures: [Option<Handle<Image>>; 4],
    pub hold_cover_layouts: [Option<Handle<TextureAtlasLayout>>; 4],
    pub hold_cover_frames: [(AnimClipFrames, AnimClipFrames, AnimClipFrames); 4],
}

pub fn load_noteskin_assets(
    mods: &crate::mods::ModRegistry,
    style: &NoteStyle,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) -> Result<NoteskinAssets, String> {
    let strum_xml = SparrowAtlas::from_file(style.disk_xml_in(mods, &style.strum_path))?;
    let (strum_layout_data, strum_map) = strum_xml.build_layout();
    let strum_layout = layouts.add(strum_layout_data);
    let strum_texture = asset_server.load(style.asset_path_in(mods, &style.strum_path));

    let mut strum_frames: [(AnimClipFrames, AnimClipFrames, AnimClipFrames, AnimClipFrames); 4] = Default::default();
    for i in 0..KEY_COUNT {
        let a = &style.strum_anims[i];
        let mut st = AnimClipFrames::from_sparrow(strum_xml.find_prefix_frames(&strum_map, &a.static_anim));
        if st.indices.is_empty() {
            st.indices.push(0);
            st.offsets.push([0.0, 0.0]);
            st.rotated.push(false);
        }
        let mut pr = AnimClipFrames::from_sparrow(strum_xml.find_prefix_frames(&strum_map, &a.press_anim));
        if pr.indices.is_empty() {
            pr = st.clone();
        }
        let mut cf = AnimClipFrames::from_sparrow(strum_xml.find_prefix_frames(&strum_map, &a.confirm_anim));
        if cf.indices.is_empty() {
            cf = st.clone();
        }
        let mut ch = AnimClipFrames::from_sparrow(strum_xml.find_prefix_frames(&strum_map, &a.confirm_hold_anim));
        if ch.indices.is_empty() {
            ch = cf.clone();
        }
        strum_frames[i] = (st, pr, cf, ch);
    }

    let notes_xml = SparrowAtlas::from_file(style.disk_xml_in(mods, &style.notes_path))?;
    let (notes_layout_data, notes_map) = notes_xml.build_layout();
    let notes_layout = layouts.add(notes_layout_data);
    let notes_texture = asset_server.load(style.asset_path_in(mods, &style.notes_path));

    let mut note_frames = [(0usize, [0.0f32, 0.0f32], false); 4];
    for i in 0..KEY_COUNT {
        let (indices, offsets, rotated) = notes_xml.find_prefix_frames(&notes_map, &style.note_anims[i]);
        note_frames[i] = (
            indices.first().copied().unwrap_or(0),
            offsets.first().copied().unwrap_or([0.0, 0.0]),
            rotated.first().copied().unwrap_or(false),
        );
    }

    let (sustain_texture, sustain_layout, sustain_indices) =
        load_sustains(mods, style, asset_server, layouts);
    let (splash_texture, splash_layout, splash_variants) =
        load_splashes(mods, style, asset_server, layouts);
    let (hold_cover_textures, hold_cover_layouts, hold_cover_frames) =
        load_hold_covers(mods, style, asset_server, layouts);

    Ok(NoteskinAssets {
        style: style.clone(),
        strum_texture,
        strum_layout,
        strum_frames,
        notes_texture,
        notes_layout,
        note_frames,
        sustain_texture,
        sustain_layout,
        sustain_indices,
        splash_texture,
        splash_layout,
        splash_variants,
        hold_cover_textures,
        hold_cover_layouts,
        hold_cover_frames,
    })
}

fn read_png_dimensions(path: &Path) -> Option<(u32, u32)> {
    let mut file = File::open(path).ok()?;
    let mut header = [0u8; 24];
    file.read_exact(&mut header).ok()?;
    if &header[0..8] != b"\x89PNG\r\n\x1a\n" || &header[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes([header[16], header[17], header[18], header[19]]);
    let h = u32::from_be_bytes([header[20], header[21], header[22], header[23]]);
    Some((w, h))
}

fn resolve_skin_png_disk_path(
    mods: &crate::mods::ModRegistry,
    style: &NoteStyle,
    rel: &str,
) -> Option<PathBuf> {
    let asset_server_path = style.asset_path_in(mods, rel);
    let mods_prefix = format!("{MOD_ASSET_SOURCE_ID}://");
    if let Some(rest) = asset_server_path.strip_prefix(&mods_prefix) {
        let cand = mods.mods_dir.join(rest);
        if cand.is_file() {
            return Some(cand);
        }
    }
    if let Some(found) = mods.resolve_file(&asset_server_path) {
        return Some(found);
    }
    let direct = Path::new(&asset_server_path);
    if direct.is_file() {
        return Some(direct.to_path_buf());
    }
    let fallback = oxin_app::resolve_assets_dir().join(&asset_server_path);
    fallback.is_file().then_some(fallback)
}

fn load_sustains_grid(
    mods: &crate::mods::ModRegistry,
    style: &NoteStyle,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) -> Option<(
    Option<Handle<Image>>,
    Option<Handle<TextureAtlasLayout>>,
    [(usize, usize, f32, f32); 4],
)> {
    let png_path = resolve_skin_png_disk_path(mods, style, &style.sustains_path)?;
    let (width, height) = read_png_dimensions(&png_path)?;
    let cols = (KEY_COUNT * 2) as u32;
    let cell_w = width / cols;
    if cell_w == 0 || height == 0 {
        return None;
    }
    let grid = TextureAtlasLayout::from_grid(UVec2::new(cell_w, height), cols, 1, None, None);
    let layout = layouts.add(grid);
    let tex = asset_server.load(style.asset_path_in(mods, &style.sustains_path));
    let s_idx = std::array::from_fn(|i| (i * 2, i * 2 + 1, cell_w as f32, height as f32));
    Some((Some(tex), Some(layout), s_idx))
}

fn load_sustains(
    mods: &crate::mods::ModRegistry,
    style: &NoteStyle,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) -> (
    Option<Handle<Image>>,
    Option<Handle<TextureAtlasLayout>>,
    [(usize, usize, f32, f32); 4],
) {
    let Ok(xml) = SparrowAtlas::from_file(style.disk_xml_in(mods, &style.sustains_path)) else {
        return load_sustains_grid(mods, style, asset_server, layouts)
            .unwrap_or((None, None, [(0, 0, 52.0, 44.0); 4]));
    };
    let (_, map) = xml.build_layout();
    let mut max_x = xml.width.max(1);
    let mut max_y = xml.height.max(1);
    for f in &xml.frames {
        max_x = max_x.max(f.x + f.width);
        max_y = max_y.max(f.y + f.height);
    }
    let mut trimmed_layout = TextureAtlasLayout::new_empty(UVec2::new(max_x, max_y));
    let tex = asset_server.load(style.asset_path_in(mods, &style.sustains_path));
    let mut s_idx = [(0usize, 0usize, 52.0f32, 44.0f32); 4];
    for i in 0..KEY_COUNT {
        let anim = &style.sustain_anims[i];
        let b_raw = xml.find_prefix_indices(&map, &anim.body).first().copied().unwrap_or(0);
        let e_raw = xml.find_prefix_indices(&map, &anim.end).first().copied().unwrap_or(b_raw);
        let bf = &xml.frames[b_raw];
        let ef = &xml.frames[e_raw];
        let b_trim = if style.is_pixel { 0 } else { (bf.frame_y.max(0) as u32 + 2).min(bf.height / 3) };
        let e_top_trim = if style.is_pixel { 0 } else { (ef.frame_y.max(0) as u32 + 1).min(ef.height / 3) };
        let body_idx = trimmed_layout.add_texture(URect::new(
            bf.x,
            bf.y + b_trim,
            bf.x + bf.width,
            bf.y + bf.height - b_trim,
        ));
        let end_idx = trimmed_layout.add_texture(URect::new(
            ef.x,
            ef.y + e_top_trim,
            ef.x + ef.width,
            ef.y + ef.height,
        ));
        s_idx[i] = (body_idx, end_idx, bf.width as f32, (ef.height - e_top_trim) as f32);
    }
    let layout = layouts.add(trimmed_layout);
    (Some(tex), Some(layout), s_idx)
}

fn load_splashes(
    mods: &crate::mods::ModRegistry,
    style: &NoteStyle,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) -> (
    Option<Handle<Image>>,
    Option<Handle<TextureAtlasLayout>>,
    [Vec<AnimClipFrames>; 4],
) {
    if !style.splash_enabled {
        return (None, None, Default::default());
    }
    let Ok(xml) = SparrowAtlas::from_file(style.disk_xml_in(mods, &style.splash_path)) else {
        return (None, None, Default::default());
    };
    let (layout_data, map) = xml.build_layout();
    let layout = layouts.add(layout_data);
    let tex = asset_server.load(style.asset_path_in(mods, &style.splash_path));
    let mut variants: [Vec<AnimClipFrames>; 4] = Default::default();
    for i in 0..KEY_COUNT {
        for prefix in &style.splash_anims[i] {
            let clip = AnimClipFrames::from_sparrow(xml.find_prefix_frames(&map, prefix));
            if !clip.indices.is_empty() {
                variants[i].push(clip);
            }
        }
    }
    (Some(tex), Some(layout), variants)
}

fn load_hold_covers(
    mods: &crate::mods::ModRegistry,
    style: &NoteStyle,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) -> (
    [Option<Handle<Image>>; 4],
    [Option<Handle<TextureAtlasLayout>>; 4],
    [(AnimClipFrames, AnimClipFrames, AnimClipFrames); 4],
) {
    let mut textures: [Option<Handle<Image>>; 4] = Default::default();
    let mut layout_handles: [Option<Handle<TextureAtlasLayout>>; 4] = Default::default();
    let mut frames: [(AnimClipFrames, AnimClipFrames, AnimClipFrames); 4] = Default::default();
    if !style.hold_cover_enabled {
        return (textures, layout_handles, frames);
    }
    for i in 0..KEY_COUNT {
        let dir_cfg = &style.hold_cover_dirs[i];
        let Ok(xml) = SparrowAtlas::from_file(style.disk_xml_in(mods, &dir_cfg.path)) else {
            continue;
        };
        let (layout_data, map) = xml.build_layout();
        layout_handles[i] = Some(layouts.add(layout_data));
        textures[i] = Some(asset_server.load(style.asset_path_in(mods, &dir_cfg.path)));
        let hold = AnimClipFrames::from_sparrow(xml.find_prefix_frames(&map, &dir_cfg.hold_anim));
        let raw_start = AnimClipFrames::from_sparrow(xml.find_prefix_frames(&map, &dir_cfg.start_anim));
        let start = if raw_start.indices.is_empty() { hold.first_only() } else { raw_start };
        let end = AnimClipFrames::from_sparrow(xml.find_prefix_frames(&map, &dir_cfg.end_anim));
        frames[i] = (start, hold, end);
    }
    (textures, layout_handles, frames)
}
