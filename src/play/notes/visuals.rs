use bevy::prelude::*;

use crate::animations::{AnimationController, SparrowAtlas};
use crate::play::scene::PlayState;

use super::noteskin::NoteStyle;
use super::sustain_trail::compute_sustain_geometry;

#[derive(Component)]
pub struct ReceptorVisual {
    pub lane: usize,
    pub is_player: bool,
    pub static_idx: usize,
    pub press_idx: usize,
    pub confirm_idx: usize,
}

#[derive(Component)]
pub struct NoteVisual {
    pub note_id: usize,
    pub is_player: bool,
    pub lane: usize,
}

#[derive(Component)]
pub struct SustainBodyVisual {
    pub note_id: usize,
    pub is_player: bool,
    pub lane: usize,
    pub raw_width_px: f32,
    pub end_raw_h: f32,
}

#[derive(Component)]
pub struct SustainEndVisual {
    pub note_id: usize,
    pub is_player: bool,
    pub lane: usize,
    pub end_raw_h: f32,
}

#[derive(Resource, Clone)]
pub struct NoteskinAssets {
    pub style: NoteStyle,
    pub strum_texture: Handle<Image>,
    pub strum_layout: Handle<TextureAtlasLayout>,
    pub strum_frames: [(Vec<usize>, Vec<usize>, Vec<usize>); 4],
    pub notes_texture: Handle<Image>,
    pub notes_layout: Handle<TextureAtlasLayout>,
    pub note_indices: [usize; 4],
    pub sustain_texture: Option<Handle<Image>>,
    pub sustain_layout: Option<Handle<TextureAtlasLayout>>,
    pub sustain_indices: [(usize, usize, f32, f32); 4],
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

    let mut strum_frames: [(Vec<usize>, Vec<usize>, Vec<usize>); 4] = Default::default();
    for i in 0..4 {
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
        strum_frames[i] = (st, pr, cf);
    }

    let notes_xml = SparrowAtlas::from_file(style.disk_xml(&style.notes_path))?;
    let (notes_layout_data, notes_map) = notes_xml.build_layout();
    let notes_layout = layouts.add(notes_layout_data);
    let notes_texture = asset_server.load(style.asset_path(&style.notes_path));

    let mut note_indices = [0usize; 4];
    for i in 0..4 {
        note_indices[i] = notes_xml
            .find_prefix_indices(&notes_map, &style.note_anims[i])
            .first()
            .copied()
            .unwrap_or(0);
    }

    let sustain_xml_path = style.disk_xml(&style.sustains_path);
    let (sustain_texture, sustain_layout, sustain_indices) =
        if let Ok(sustain_xml) = SparrowAtlas::from_file(sustain_xml_path) {
            let (s_layout_data, s_map) = sustain_xml.build_layout();
            let s_layout = layouts.add(s_layout_data);
            let s_tex = asset_server.load(style.asset_path(&style.sustains_path));
            let mut s_idx = [(0usize, 0usize, 52.0f32, 44.0f32); 4];
            for i in 0..4 {
                let anim = &style.sustain_anims[i];
                let body = sustain_xml
                    .find_prefix_indices(&s_map, &anim.body)
                    .first()
                    .copied()
                    .unwrap_or(0);
                let end = sustain_xml
                    .find_prefix_indices(&s_map, &anim.end)
                    .first()
                    .copied()
                    .unwrap_or(body);
                let body_w = sustain_xml
                    .frames
                    .get(body)
                    .map(|f| f.width as f32)
                    .unwrap_or(52.0);
                let end_h = sustain_xml
                    .frames
                    .get(end)
                    .map(|f| f.height as f32)
                    .unwrap_or(44.0);
                s_idx[i] = (body, end, body_w, end_h);
            }
            (Some(s_tex), Some(s_layout), s_idx)
        } else {
            (None, None, [(0, 0, 52.0, 44.0); 4])
        };

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
    })
}

pub fn spawn_strumline_and_notes(
    commands: &mut Commands,
    state: &PlayState,
    assets: &NoteskinAssets,
) {
    let style = &assets.style;

    for (is_player, strum) in [
        (false, &state.opponent_strumline),
        (true, &state.player_strumline),
    ] {
        let rec_y = strum.receptor_y();
        for lane in 0..4 {
            let (st, pr, cf) = &assets.strum_frames[lane];
            let static_idx = st.first().copied().unwrap_or(0);
            let press_idx = pr.first().copied().unwrap_or(static_idx);
            let confirm_idx = cf.first().copied().unwrap_or(static_idx);
            let mut anim = AnimationController::default();
            anim.add_anim("static", st.clone(), 24.0, true);
            anim.add_anim("press", pr.clone(), 24.0, false);
            anim.add_anim("confirm", cf.clone(), 24.0, false);
            anim.play("static", true, false, 0);

            let mut sprite = Sprite::from_atlas_image(
                assets.strum_texture.clone(),
                TextureAtlas {
                    layout: assets.strum_layout.clone(),
                    index: static_idx,
                },
            );
            sprite.color = Color::srgba(1.0, 1.0, 1.0, style.strum_alpha);
            commands.spawn((
                sprite,
                Transform::from_xyz(strum.lane_x(lane), rec_y, 0.0)
                    .with_scale(Vec3::splat(style.strum_scale)),
                anim,
                ReceptorVisual {
                    lane,
                    is_player,
                    static_idx,
                    press_idx,
                    confirm_idx,
                },
            ));
        }

        for note in &strum.notes {
            let lane = note.lane.index();
            let note_x = strum.lane_x(lane) + style.notes_offset[0];
            let mut sprite = Sprite::from_atlas_image(
                assets.notes_texture.clone(),
                TextureAtlas {
                    layout: assets.notes_layout.clone(),
                    index: assets.note_indices[lane],
                },
            );
            sprite.color = Color::srgba(1.0, 1.0, 1.0, style.notes_alpha);
            commands.spawn((
                sprite,
                Transform::from_xyz(note_x, -900.0, 2.0)
                    .with_scale(Vec3::splat(style.notes_scale)),
                Visibility::Hidden,
                NoteVisual {
                    note_id: note.data.id,
                    is_player,
                    lane,
                },
            ));

            if note.data.sustain_length_ms <= 0.0 {
                continue;
            }
            let (Some(s_tex), Some(s_layout)) = (&assets.sustain_texture, &assets.sustain_layout)
            else {
                continue;
            };
            let (body_idx, end_idx, body_raw_w, end_raw_h) = assets.sustain_indices[lane];
            let init_geom = compute_sustain_geometry(note, strum, style, -2000.0, end_raw_h);
            let sus_x = strum.lane_x(lane) + style.sustains_offset[0];

            let mut body_sprite = Sprite::from_atlas_image(
                s_tex.clone(),
                TextureAtlas {
                    layout: s_layout.clone(),
                    index: body_idx,
                },
            );
            body_sprite.color = Color::srgba(1.0, 1.0, 1.0, style.sustains_alpha);
            body_sprite.custom_size = Some(Vec2::new(
                body_raw_w,
                (init_geom.body_height_px / style.sustains_scale).max(1.0),
            ));
            commands.spawn((
                body_sprite,
                Transform::from_xyz(sus_x, -900.0, 1.0)
                    .with_scale(Vec3::splat(style.sustains_scale)),
                Visibility::Hidden,
                SustainBodyVisual {
                    note_id: note.data.id,
                    is_player,
                    lane,
                    raw_width_px: body_raw_w,
                    end_raw_h,
                },
            ));

            let mut end_sprite = Sprite::from_atlas_image(
                s_tex.clone(),
                TextureAtlas {
                    layout: s_layout.clone(),
                    index: end_idx,
                },
            );
            end_sprite.color = Color::srgba(1.0, 1.0, 1.0, style.sustains_alpha);
            commands.spawn((
                end_sprite,
                Transform::from_xyz(sus_x + style.sustains_end_offset[0], -900.0, 1.0)
                    .with_scale(Vec3::splat(style.sustains_scale)),
                Visibility::Hidden,
                SustainEndVisual {
                    note_id: note.data.id,
                    is_player,
                    lane,
                    end_raw_h,
                },
            ));
        }
    }
}
