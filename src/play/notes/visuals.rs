use bevy::math::URect;
use bevy::prelude::*;

use crate::animations::SparrowAtlas;
use crate::utils::constants::KEY_COUNT;

use super::noteskin::NoteStyle;
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
    pub strum_frames: [(Vec<usize>, Vec<usize>, Vec<usize>, Vec<usize>); 4],
    pub notes_texture: Handle<Image>,
    pub notes_layout: Handle<TextureAtlasLayout>,
    pub note_indices: [usize; 4],
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
    style: &NoteStyle,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) -> Result<NoteskinAssets, String> {
    let strum_xml = SparrowAtlas::from_file(style.disk_xml(&style.strum_path))?;
    let (strum_layout_data, strum_map) = strum_xml.build_layout();
    let strum_layout = layouts.add(strum_layout_data);
    let strum_texture = asset_server.load(style.asset_path(&style.strum_path));

    let mut strum_frames: [(Vec<usize>, Vec<usize>, Vec<usize>, Vec<usize>); 4] = Default::default();
    for i in 0..KEY_COUNT {
        let a = &style.strum_anims[i];
        let mut st = strum_xml.find_prefix_indices(&strum_map, &a.static_anim);
        if st.is_empty() {
            st.push(0);
        }
        let mut pr = strum_xml.find_prefix_indices(&strum_map, &a.press_anim);
        if pr.is_empty() {
            pr = st.clone();
        }
        let mut cf = strum_xml.find_prefix_indices(&strum_map, &a.confirm_anim);
        if cf.is_empty() {
            cf = st.clone();
        }
        let mut ch = strum_xml.find_prefix_indices(&strum_map, &a.confirm_hold_anim);
        if ch.is_empty() {
            ch = cf.clone();
        }
        strum_frames[i] = (st, pr, cf, ch);
    }

    let notes_xml = SparrowAtlas::from_file(style.disk_xml(&style.notes_path))?;
    let (notes_layout_data, notes_map) = notes_xml.build_layout();
    let notes_layout = layouts.add(notes_layout_data);
    let notes_texture = asset_server.load(style.asset_path(&style.notes_path));

    let mut note_indices = [0usize; 4];
    for i in 0..KEY_COUNT {
        note_indices[i] = notes_xml
            .find_prefix_indices(&notes_map, &style.note_anims[i])
            .first()
            .copied()
            .unwrap_or(0);
    }

    let (sustain_texture, sustain_layout, sustain_indices) =
        load_sustains(style, asset_server, layouts);
    let (splash_texture, splash_layout, splash_variants) =
        load_splashes(style, asset_server, layouts);
    let (hold_cover_textures, hold_cover_layouts, hold_cover_frames) =
        load_hold_covers(style, asset_server, layouts);

    Ok(NoteskinAssets {
        style: style.clone(),
        strum_texture,
        strum_layout,
        strum_frames,
        notes_texture,
        notes_layout,
        note_indices,
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

fn load_sustains(
    style: &NoteStyle,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) -> (
    Option<Handle<Image>>,
    Option<Handle<TextureAtlasLayout>>,
    [(usize, usize, f32, f32); 4],
) {
    let Ok(xml) = SparrowAtlas::from_file(style.disk_xml(&style.sustains_path)) else {
        return (None, None, [(0, 0, 52.0, 44.0); 4]);
    };
    let (_, map) = xml.build_layout();
    let mut max_x = xml.width.max(1);
    let mut max_y = xml.height.max(1);
    for f in &xml.frames {
        max_x = max_x.max(f.x + f.width);
        max_y = max_y.max(f.y + f.height);
    }
    let mut trimmed_layout = TextureAtlasLayout::new_empty(UVec2::new(max_x, max_y));
    let tex = asset_server.load(style.asset_path(&style.sustains_path));
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
    let Ok(xml) = SparrowAtlas::from_file(style.disk_xml(&style.splash_path)) else {
        return (None, None, Default::default());
    };
    let (layout_data, map) = xml.build_layout();
    let layout = layouts.add(layout_data);
    let tex = asset_server.load(style.asset_path(&style.splash_path));
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
        let Ok(xml) = SparrowAtlas::from_file(style.disk_xml(&dir_cfg.path)) else {
            continue;
        };
        let (layout_data, map) = xml.build_layout();
        layout_handles[i] = Some(layouts.add(layout_data));
        textures[i] = Some(asset_server.load(style.asset_path(&dir_cfg.path)));
        let hold = AnimClipFrames::from_sparrow(xml.find_prefix_frames(&map, &dir_cfg.hold_anim));
        let raw_start = AnimClipFrames::from_sparrow(xml.find_prefix_frames(&map, &dir_cfg.start_anim));
        let start = if raw_start.indices.is_empty() { hold.first_only() } else { raw_start };
        let end = AnimClipFrames::from_sparrow(xml.find_prefix_frames(&map, &dir_cfg.end_anim));
        frames[i] = (start, hold, end);
    }
    (textures, layout_handles, frames)
}
