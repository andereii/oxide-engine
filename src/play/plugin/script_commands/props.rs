use std::collections::HashMap;

use bevy::prelude::*;

use crate::animations::animate_atlas::AnimateAtlas;
use crate::animations::atlas_composite::{
    build_composite_clip, compute_atlas_whole_bounds, sample_default_composite_clip,
};
use crate::animations::{
    AnimationController, AtlasSettings, BaseAnimationData, SparrowAtlas, add_sparrow_animations,
    register_default_sparrow_idle,
};
use crate::mods::ModRegistry;
use crate::play::camera::hud::hud_render_layer;
use crate::play::preload::PlayPreloadTracker;
use crate::play::stage::schema::{StageCoordMode, composite_piece_z};
use crate::play::stage::sync::StageAtlasFrameSizes;
use crate::play::stage::{DynamicScriptProp, StageCompositePiece, StageCompositeProp, StagePropVisual};
use crate::scripting::{ScriptCommand, ScriptPropAnimKind, TweenProp};

use super::PropsQuery;

#[derive(Component, Clone)]
pub(crate) struct ScriptSparrowData {
    pub(crate) atlas: SparrowAtlas,
    pub(crate) indices: HashMap<String, usize>,
}

pub(super) fn build_prop_index_ci(props: &PropsQuery) -> HashMap<String, Vec<Entity>> {
    let mut map: HashMap<String, Vec<Entity>> = HashMap::new();
    for (ent, vis, _, _, _, _) in props.iter() {
        map.entry(vis.name.to_ascii_lowercase()).or_default().push(ent);
    }
    map
}

fn apply_prop_number(
    vis: &mut StagePropVisual,
    sprite: &mut Sprite,
    prop: TweenProp,
    value: f32,
) {
    match prop {
        TweenProp::X => vis.base_position[0] = value,
        TweenProp::Y => vis.base_position[1] = value,
        TweenProp::Z => vis.z_layer = value,
        TweenProp::VelX => vis.velocity[0] = value,
        TweenProp::VelY => vis.velocity[1] = value,
        TweenProp::ScaleX => vis.base_scale[0] = value,
        TweenProp::ScaleY => vis.base_scale[1] = value,
        TweenProp::Angle => vis.base_angle_deg = value,
        TweenProp::Alpha => sprite.color = sprite.color.with_alpha(value.clamp(0.0, 1.0)),
        _ => {}
    }
}

fn patch_pending_prop(
    commands: &mut Commands,
    pending_spawned: &HashMap<String, Entity>,
    id: &str,
    patch: impl FnOnce(&mut StagePropVisual, &mut Sprite) + Send + Sync + 'static,
) {
    if let Some(&ent) = pending_spawned.get(id) {
        commands.queue(move |world: &mut World| {
            let mut q = world.query::<(&mut StagePropVisual, &mut Sprite)>();
            if let Ok((mut vis, mut sprite)) = q.get_mut(world, ent) {
                patch(&mut vis, &mut sprite);
            }
        });
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_prop_cmd(
    cmd: &ScriptCommand,
    commands: &mut Commands,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    mods: &ModRegistry,
    mut preload: Option<&mut PlayPreloadTracker>,
    props: &mut PropsQuery,
    sparrows: &Query<&ScriptSparrowData>,
    prop_index: &mut Option<HashMap<String, Vec<Entity>>>,
    pending_spawned: &mut HashMap<String, Entity>,
) {
    match cmd {
        ScriptCommand::SpawnProp { id, image_path, color, size, pos, z, hud } => {
            *prop_index = None;
            despawn_prop_by_id(commands, props, id, true, pending_spawned);
            let mut sprite = match image_path {
                Some(rel) => {
                    let resolved = mods.resolve_asset_server_path(rel).unwrap_or_else(|| rel.clone());
                    let tex: Handle<Image> = asset_server.load(resolved);
                    if let Some(tr) = preload.as_deref_mut() {
                        tr.register_handle(tex.clone());
                    }
                    let mut spr = Sprite::from_image(tex);
                    spr.color = Color::srgba(color[0], color[1], color[2], color[3]);
                    if size[0] > 0.0 && size[1] > 0.0 {
                        spr.custom_size = Some(Vec2::new(size[0], size[1]));
                    }
                    spr
                }
                None => Sprite::from_color(
                    Color::srgba(color[0], color[1], color[2], color[3]),
                    Vec2::new(size[0].max(1.0), size[1].max(1.0)),
                ),
            };
            sprite.color = Color::srgba(color[0], color[1], color[2], color[3]);
            let vis = StagePropVisual {
                name: id.clone(),
                base_position: *pos,
                base_scale: [1.0, 1.0],
                scroll_factor: if *hud { [0.0, 0.0] } else { [1.0, 1.0] },
                base_flip: [false, false],
                base_angle_deg: 0.0,
                z_layer: *z,
                visible: true,
                coord_mode: StageCoordMode::Center,
                raw_size: *size,
                velocity: [0.0, 0.0],
                acceleration: [0.0, 0.0],
            };
            let mut ec = commands.spawn((
                sprite,
                Transform::from_xyz(0.0, 0.0, *z),
                Visibility::Visible,
                vis,
                DynamicScriptProp {
                    id: id.clone(),
                    hud: *hud,
                },
            ));
            if *hud {
                ec.insert(hud_render_layer());
            }
            pending_spawned.insert(id.clone(), ec.id());
        }
        ScriptCommand::SpawnAnimatedProp {
            id,
            path,
            kind,
            anims,
            color,
            size,
            pos,
            z,
            hud,
            top_left,
        } => {
            *prop_index = None;
            despawn_prop_by_id(commands, props, id, true, pending_spawned);
            let coord_mode = if *top_left {
                StageCoordMode::TopLeft
            } else {
                StageCoordMode::Center
            };
            if let Some(ent) = spawn_animated_script_prop(
                commands,
                asset_server,
                layouts,
                mods,
                preload,
                id,
                path,
                *kind,
                anims,
                *color,
                *size,
                *pos,
                *z,
                *hud,
                coord_mode,
            ) {
                pending_spawned.insert(id.clone(), ent);
            }
        }
        ScriptCommand::DespawnProp(id) => {
            *prop_index = None;
            despawn_prop_by_id(commands, props, id, false, pending_spawned);
            for (ent, txt, _, _, _, _) in texts.iter() {
                if txt.id == *id {
                    commands.entity(ent).despawn();
                }
            }
        }
        ScriptCommand::SetPropNumber { id, prop, value } => {
            let mut matched_prop = false;
            if let Some(index) = prop_index.as_ref() {
                if let Some(entities) = index.get(&id.to_ascii_lowercase()) {
                    for &ent in entities {
                        if let Ok((_, mut vis, mut sprite, _, _, _)) = props.get_mut(ent)
                            && vis.name == *id
                        {
                            apply_prop_number(&mut vis, &mut sprite, *prop, *value);
                            matched_prop = true;
                        }
                    }
                }
            } else {
                for (_, mut vis, mut sprite, _, _, _) in props.iter_mut() {
                    if vis.name == *id {
                        apply_prop_number(&mut vis, &mut sprite, *prop, *value);
                        matched_prop = true;
                    }
                }
            }
            if !matched_prop {
                let p = *prop;
                let v = *value;
                patch_pending_prop(commands, pending_spawned, id, move |vis, sprite| {
                    apply_prop_number(vis, sprite, p, v);
                });
                if !pending_spawned.contains_key(id) {
                    for (_, mut txt, _, _, mut color, _) in texts.iter_mut() {
                        if txt.id == *id {
                            match prop {
                                TweenProp::X => txt.base_position[0] = *value,
                                TweenProp::Y => txt.base_position[1] = *value,
                                TweenProp::Z => txt.z_layer = *value,
                                TweenProp::Alpha => {
                                    color.0 = color.0.with_alpha(value.clamp(0.0, 1.0));
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        ScriptCommand::SetPropTint { id, color } => {
            let mut matched_prop = false;
            for (_, vis, mut sprite, _, _, _) in props.iter_mut() {
                if vis.name == *id {
                    sprite.color = Color::srgba(color[0], color[1], color[2], color[3]);
                    matched_prop = true;
                }
            }
            if !matched_prop {
                let c = *color;
                patch_pending_prop(commands, pending_spawned, id, move |_, sprite| {
                    sprite.color = Color::srgba(c[0], c[1], c[2], c[3]);
                });
            }
            for (_, txt, _, _, mut tc, _) in texts.iter_mut() {
                if txt.id == *id {
                    tc.0 = Color::srgba(color[0], color[1], color[2], color[3]);
                }
            }
        }
        ScriptCommand::SetPropVisible { id, visible } => {
            let mut matched_prop = false;
            for (_, mut vis, _, _, _, _) in props.iter_mut() {
                if vis.name == *id {
                    vis.visible = *visible;
                    matched_prop = true;
                }
            }
            if !matched_prop {
                let v = *visible;
                patch_pending_prop(commands, pending_spawned, id, move |vis, _| {
                    vis.visible = v;
                });
            }
            for (_, mut txt, _, _, _, _) in texts.iter_mut() {
                if txt.id == *id {
                    txt.visible = *visible;
                }
            }
        }
        ScriptCommand::SetPropFlip { id, flip_x, flip_y } => {
            let mut matched_prop = false;
            for (_, mut vis, _, _, _, _) in props.iter_mut() {
                if vis.name == *id {
                    vis.base_flip = [*flip_x, *flip_y];
                    matched_prop = true;
                }
            }
            if !matched_prop {
                let fl = [*flip_x, *flip_y];
                patch_pending_prop(commands, pending_spawned, id, move |vis, _| {
                    vis.base_flip = fl;
                });
            }
        }
        ScriptCommand::SetPropScroll { id, sx, sy } => {
            let mut matched_prop = false;
            for (_, mut vis, _, _, _, _) in props.iter_mut() {
                if vis.name == *id {
                    vis.scroll_factor = [*sx, *sy];
                    matched_prop = true;
                }
            }
            if !matched_prop {
                let sc = [*sx, *sy];
                patch_pending_prop(commands, pending_spawned, id, move |vis, _| {
                    vis.scroll_factor = sc;
                });
            }
            for (_, mut txt, _, _, _, _) in texts.iter_mut() {
                if txt.id == *id {
                    txt.scroll_factor = [*sx, *sy];
                }
            }
        }
        ScriptCommand::SetPropZ { id, z } => {
            let mut matched_prop = false;
            for (_, mut vis, _, _, _, _) in props.iter_mut() {
                if vis.name == *id {
                    vis.z_layer = *z;
                    matched_prop = true;
                }
            }
            if !matched_prop {
                let zv = *z;
                patch_pending_prop(commands, pending_spawned, id, move |vis, _| {
                    vis.z_layer = zv;
                });
            }
            for (_, mut txt, _, _, _, _) in texts.iter_mut() {
                if txt.id == *id {
                    txt.z_layer = *z;
                }
            }
        }
        ScriptCommand::SetPropSize { id, w, h } => {
            let mut matched_prop = false;
            for (_, mut vis, mut sprite, _, _, _) in props.iter_mut() {
                if vis.name == *id {
                    vis.raw_size = [*w, *h];
                    sprite.custom_size = Some(Vec2::new(w.max(1.0), h.max(1.0)));
                    matched_prop = true;
                }
            }
            if !matched_prop {
                let (wv, hv) = (*w, *h);
                patch_pending_prop(commands, pending_spawned, id, move |vis, sprite| {
                    vis.raw_size = [wv, hv];
                    sprite.custom_size = Some(Vec2::new(wv.max(1.0), hv.max(1.0)));
                });
            }
        }
        ScriptCommand::SetPropVelocity { id, vx, vy, ax, ay } => {
            let mut matched_prop = false;
            for (_, mut vis, _, _, _, _) in props.iter_mut() {
                if vis.name == *id {
                    vis.velocity = [*vx, *vy];
                    vis.acceleration = [*ax, *ay];
                    matched_prop = true;
                }
            }
            if !matched_prop {
                let (vel, acc) = ([*vx, *vy], [*ax, *ay]);
                patch_pending_prop(commands, pending_spawned, id, move |vis, _| {
                    vis.velocity = vel;
                    vis.acceleration = acc;
                });
            }
        }
        ScriptCommand::AddPropAnim { id, anim } => {
            for (ent, vis, _, ctrl_opt, _, _) in props.iter_mut() {
                if vis.name == *id
                    && let Some(mut ctrl) = ctrl_opt
                {
                    if !anim.frame_indices.is_empty() {
                        ctrl.add_anim(
                            &anim.name,
                            anim.frame_indices.clone(),
                            anim.frame_rate.max(1.0),
                            anim.looped,
                        );
                        ctrl.set_offset(&anim.name, anim.offsets);
                    } else if let Ok(sparrow) = sparrows.get(ent) {
                        add_sparrow_animations(
                            &sparrow.atlas,
                            &sparrow.indices,
                            &mut ctrl,
                            std::slice::from_ref(anim),
                        );
                    }
                }
            }
        }
        _ => {}
    }
}

pub(crate) fn despawn_prop_by_id(
    commands: &mut Commands,
    props: &PropsQuery,
    id: &str,
    only_dynamic: bool,
    pending_spawned: &mut HashMap<String, Entity>,
) {
    if let Some(pending_ent) = pending_spawned.remove(id) {
        commands.queue(move |world: &mut World| {
            let pieces = world
                .get::<StageCompositeProp>(pending_ent)
                .map(|c| c.piece_entities.clone())
                .unwrap_or_default();
            for p in pieces {
                if world.get_entity(p).is_ok() {
                    world.despawn(p);
                }
            }
            if world.get_entity(pending_ent).is_ok() {
                world.despawn(pending_ent);
            }
        });
    }
    for (entity, vis, _, _, dyn_opt, comp_opt) in props.iter() {
        let matched = if only_dynamic {
            dyn_opt.is_some_and(|d| d.id == id)
        } else {
            dyn_opt.is_some_and(|d| d.id == id) || vis.name == id
        };
        if matched {
            if let Some(comp) = comp_opt {
                for &piece in &comp.piece_entities {
                    commands.entity(piece).despawn();
                }
            }
            commands.entity(entity).despawn();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_animated_script_prop(
    commands: &mut Commands,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    mods: &ModRegistry,
    mut preload: Option<&mut PlayPreloadTracker>,
    id: &str,
    path: &str,
    kind: ScriptPropAnimKind,
    anims: &[BaseAnimationData],
    color: [f32; 4],
    size: [f32; 2],
    pos: [f32; 2],
    z: f32,
    hud: bool,
    coord_mode: StageCoordMode,
) -> Option<Entity> {
    let tint = Color::srgba(color[0], color[1], color[2], color[3]);
    let scroll_factor = if hud { [0.0, 0.0] } else { [1.0, 1.0] };
    let make_vis = |raw_size: [f32; 2]| StagePropVisual {
        name: id.to_string(),
        base_position: pos,
        base_scale: [1.0, 1.0],
        scroll_factor,
        base_flip: [false, false],
        base_angle_deg: 0.0,
        z_layer: z,
        visible: true,
        coord_mode,
        raw_size,
        velocity: [0.0, 0.0],
        acceleration: [0.0, 0.0],
    };

    match kind {
        ScriptPropAnimKind::Static => {
            let resolved_file = mods
                .resolve_file(path)
                .or_else(|| mods.resolve_file(&format!("{path}.png")));
            let raw_size = resolved_file
                .as_deref()
                .and_then(super::super::super::stage::paths::read_image_dimensions)
                .unwrap_or(size);
            let srv = resolved_file
                .as_deref()
                .map(|p| mods.to_asset_server_path(p))
                .unwrap_or_else(|| path.to_string());
            let tex: Handle<Image> = asset_server.load(srv);
            if let Some(tr) = preload.as_deref_mut() {
                tr.register_handle(tex.clone());
            }
            let mut spr = Sprite::from_image(tex);
            spr.color = tint;
            let mut ec = commands.spawn((
                spr,
                Transform::from_xyz(0.0, 0.0, z),
                Visibility::Visible,
                make_vis(raw_size),
                DynamicScriptProp {
                    id: id.to_string(),
                    hud,
                },
            ));
            if hud {
                ec.insert(hud_render_layer());
            }
            Some(ec.id())
        }
        ScriptPropAnimKind::Rect => {
            let spr = Sprite::from_color(tint, Vec2::new(size[0].max(1.0), size[1].max(1.0)));
            let mut ec = commands.spawn((
                spr,
                Transform::from_xyz(0.0, 0.0, z),
                Visibility::Visible,
                make_vis(size),
                DynamicScriptProp {
                    id: id.to_string(),
                    hud,
                },
            ));
            if hud {
                ec.insert(hud_render_layer());
            }
            Some(ec.id())
        }
        ScriptPropAnimKind::Sparrow => {
            let Some(img_path) = mods
                .resolve_file(path)
                .or_else(|| mods.resolve_file(&format!("{path}.png")))
            else {
                warn!("spawn_sparrow({id}): missing image for '{path}'");
                return None;
            };
            let xml_path = img_path.with_extension("xml");
            let Ok(atlas) = SparrowAtlas::from_file(&xml_path) else {
                warn!("spawn_sparrow({id}): failed to parse '{}'", xml_path.display());
                return None;
            };
            let (layout, indices) = atlas.build_layout();
            let mut controller = AnimationController::default();
            if anims.is_empty() {
                register_default_sparrow_idle(&atlas, &indices, &mut controller);
            } else {
                add_sparrow_animations(&atlas, &indices, &mut controller, anims);
            }
            if let Some(first_name) = anims.first().map(|a| a.name.as_str()).or(Some("idle"))
                && controller.has_animation(first_name)
            {
                controller.play(first_name, true, false, 0);
            }
            let frame_sizes: HashMap<usize, [f32; 2]> = atlas
                .frames
                .iter()
                .filter_map(|f| {
                    let (uw, uh) = if f.rotated { (f.height, f.width) } else { (f.width, f.height) };
                    indices
                        .get(&f.name)
                        .copied()
                        .map(|idx| (idx, [f.frame_width.max(uw) as f32, f.frame_height.max(uh) as f32]))
                })
                .collect();
            let frame_size = controller
                .current_atlas_index()
                .and_then(|idx| frame_sizes.get(&idx).copied())
                .unwrap_or([0.0, 0.0]);
            let initial_idx = controller.current_atlas_index().unwrap_or(0);
            let tex: Handle<Image> = asset_server.load(mods.to_asset_server_path(&img_path));
            if let Some(tr) = preload.as_deref_mut() {
                tr.register_handle(tex.clone());
            }
            let mut spr = Sprite::from_atlas_image(
                tex,
                TextureAtlas {
                    layout: layouts.add(layout),
                    index: initial_idx,
                },
            );
            spr.color = tint;
            let mut ec = commands.spawn((
                spr,
                Transform::from_xyz(0.0, 0.0, z),
                Visibility::Visible,
                controller,
                make_vis(frame_size),
                StageAtlasFrameSizes(frame_sizes),
                ScriptSparrowData { atlas, indices },
                DynamicScriptProp {
                    id: id.to_string(),
                    hud,
                },
            ));
            if hud {
                ec.insert(hud_render_layer());
            }
            Some(ec.id())
        }
        ScriptPropAnimKind::Atlas => {
            let Some(dir) = mods.resolve_dir(path) else {
                warn!("spawn_atlas({id}): missing atlas directory '{path}'");
                return None;
            };
            let Ok(atlas) = AnimateAtlas::from_dir(&dir) else {
                warn!("spawn_atlas({id}): failed to parse '{}'", dir.display());
                return None;
            };
            let (atlas_layouts, indices) = atlas.build_layouts();
            let textures: Vec<(Handle<Image>, Handle<TextureAtlasLayout>)> = atlas_layouts
                .into_iter()
                .enumerate()
                .map(|(i, layout)| {
                    let img_rel = mods.to_asset_server_path(&dir.join(format!("spritemap{}.png", i + 1)));
                    let tex: Handle<Image> = asset_server.load(img_rel);
                    if let Some(tr) = preload.as_deref_mut() {
                        tr.register_handle(tex.clone());
                    }
                    (tex, layouts.add(layout))
                })
                .collect();
            let Some((base_tex, base_layout)) = textures.first() else {
                return None;
            };
            let mut controller = AnimationController::default();
            let mut clips = HashMap::new();
            if anims.is_empty() {
                if let Some(clip) = sample_default_composite_clip(&atlas, &indices) {
                    let frame_ids: Vec<usize> = (0..clip.frames.len()).collect();
                    controller.add_anim("idle", frame_ids, clip.fps, clip.looped);
                    clips.insert("idle".to_string(), clip);
                }
            } else {
                for anim in anims {
                    if let Some(clip) = build_composite_clip(&atlas, &indices, anim) {
                        let frame_ids: Vec<usize> = (0..clip.frames.len()).collect();
                        controller.add_anim(&anim.name, frame_ids, clip.fps, clip.looped);
                        controller.set_offset(&anim.name, clip.offset);
                        controller.set_flip(&anim.name, clip.flip);
                        clips.insert(anim.name.clone(), clip);
                    }
                }
            }
            if let Some(first_name) = anims.first().map(|a| a.name.as_str()).or(Some("idle"))
                && controller.has_animation(first_name)
            {
                controller.play(first_name, true, false, 0);
            }
            let max_pieces = clips
                .values()
                .flat_map(|c| c.frames.iter())
                .map(Vec::len)
                .max()
                .unwrap_or(1)
                .max(1);
            let (bounds_min, bounds_size) = clips
                .values()
                .next()
                .map(|c| (c.bounds_min, c.bounds_size))
                .unwrap_or_else(|| compute_atlas_whole_bounds(&atlas, &indices));
            let mut piece_entities = Vec::with_capacity(max_pieces);
            for i in 0..max_pieces {
                let mut pe = commands.spawn((
                    Sprite::from_atlas_image(
                        base_tex.clone(),
                        TextureAtlas {
                            layout: base_layout.clone(),
                            index: 0,
                        },
                    ),
                    Transform::from_xyz(0.0, 0.0, composite_piece_z(z, i)),
                    Visibility::Hidden,
                    StageCompositePiece,
                    super::super::super::characters::spawn::CompositeAffine2d::default(),
                ));
                if hud {
                    pe.insert(hud_render_layer());
                }
                piece_entities.push(pe.id());
            }
            let mut placeholder = Sprite::default();
            placeholder.color = tint;
            let composite = StageCompositeProp {
                textures,
                clips,
                bounds_min,
                bounds_size,
                atlas_settings: AtlasSettings::default(),
                stage_matrix: atlas.stage_matrix,
                piece_entities,
            };
            let mut ec = commands.spawn((
                placeholder,
                Transform::from_xyz(0.0, 0.0, z),
                Visibility::Hidden,
                controller,
                make_vis(bounds_size),
                composite,
                DynamicScriptProp {
                    id: id.to_string(),
                    hud,
                },
            ));
            if hud {
                ec.insert(hud_render_layer());
            }
            Some(ec.id())
        }
    }
}
