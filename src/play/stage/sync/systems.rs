use std::f32::consts::FRAC_PI_2;

use bevy::image::ImageSampler;
use bevy::prelude::*;

use crate::animations::{AnimationController, Bopper};
use crate::play::scene::PlayState;
use crate::play::shaders::stage_blend_post::StagePostOverlayProp;
use crate::play::types::PlayPhase;
use crate::utils::conductor::advance_step_range;
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::utils::ecs::{
    set_atlas_index, set_sprite_flip, set_transform_full, set_transform_pos_scale, set_visibility,
};

use super::super::schema::StageCoordMode;
use super::composite::{
    StagePiecesQuery, StageQuadsQuery, composite_frame_info, hide_composite_prop,
    sync_composite_prop_pieces,
};
use super::{
    DynamicScriptProp, DynamicScriptText, StageAtlasFrameSizes, StageCompositeProp,
    StagePropPixelSampler, StagePropVisual, StageSyncState,
};

pub fn sync_stage_props(
    time: Res<Time>,
    state: Res<PlayState>,
    mut sync_state: ResMut<StageSyncState>,
    mut images: ResMut<Assets<Image>>,
    mut animated_props: Query<(
        &mut StagePropVisual,
        &mut Sprite,
        &mut Transform,
        &mut Visibility,
        Option<&mut AnimationController>,
        Option<&mut Bopper>,
        Option<&DynamicScriptProp>,
        Option<&StageCompositeProp>,
        Option<&StageAtlasFrameSizes>,
        Option<&mut StagePropPixelSampler>,
        Option<&StagePostOverlayProp>,
    )>,
    mut pieces: StagePiecesQuery,
    mut quads: StageQuadsQuery,
) {
    let paused = matches!(
        state.phase,
        PlayPhase::Loading | PlayPhase::Paused(_) | PlayPhase::GameOver | PlayPhase::Ended
    );
    let dt = if paused {
        0.0
    } else {
        time.delta_secs().min(0.1)
    };
    let current_step = state.conductor.current_step;
    let step_range = advance_step_range(
        &mut sync_state.last_step,
        current_step,
        paused,
        state.needs_reset,
    );

    let cam_scroll_x = state.camera.follow_point[0] as f32 - SCREEN_WIDTH * 0.5;
    let cam_scroll_y = state.camera.follow_point[1] as f32 - SCREEN_HEIGHT * 0.5;
    let zoom = (state.camera.current_zoom * state.camera.bop_multiplier).max(0.05) as f32;

    for (
        mut vis,
        mut sprite,
        mut transform,
        mut visibility,
        anim_opt,
        bopper_opt,
        dyn_opt,
        composite_opt,
        frame_sizes_opt,
        sampler_opt,
        post_ov_opt,
    ) in &mut animated_props
    {
        apply_pixel_sampler(sampler_opt, composite_opt, &sprite.image, &mut images);
        step_prop_physics(&mut vis, dt);

        let is_hud = dyn_opt.is_some_and(|d| d.hud);
        let is_hidden = !vis.visible || (is_hud && state.hud_alpha <= 0.001);
        let prop_zoom = if is_hud { 1.0 } else { zoom };
        let eff_scroll_x = if is_hud {
            0.0
        } else {
            cam_scroll_x * vis.scroll_factor[0]
        };
        let eff_scroll_y = if is_hud {
            0.0
        } else {
            cam_scroll_y * vis.scroll_factor[1]
        };

        if let Some(composite) = composite_opt {
            set_visibility(&mut visibility, Visibility::Hidden);
            let (anim_off, anim_flip, clip_bounds_min, clip_bounds_size, cur_pieces) =
                match (anim_opt, bopper_opt) {
                    (Some(mut anim), Some(mut bopper)) => {
                        bopper.sync_finished(&anim);
                        if let Some(steps) = step_range.clone() {
                            for step in steps {
                                bopper.on_step_hit(step, &mut anim);
                            }
                        }
                        let (bmin, bsize, frame_pieces) = composite_frame_info(composite, &anim);
                        (
                            anim.current_offset(),
                            anim.current_flip(),
                            bmin,
                            bsize,
                            frame_pieces,
                        )
                    }
                    (Some(anim), None) => {
                        let (bmin, bsize, frame_pieces) = composite_frame_info(composite, &anim);
                        (
                            anim.current_offset(),
                            anim.current_flip(),
                            bmin,
                            bsize,
                            frame_pieces,
                        )
                    }
                    _ => (
                        [0.0, 0.0],
                        [false, false],
                        composite.bounds_min,
                        composite.bounds_size,
                        &[][..],
                    ),
                };

            if is_hidden {
                hide_composite_prop(composite, &mut pieces, &mut quads);
                continue;
            }

            sync_composite_prop_pieces(
                &vis,
                composite,
                cur_pieces,
                clip_bounds_min,
                clip_bounds_size,
                anim_off,
                anim_flip,
                sprite.color,
                eff_scroll_x,
                eff_scroll_y,
                prop_zoom,
                &mut pieces,
                &mut quads,
            );
            continue;
        }

        if is_hidden {
            set_visibility(&mut visibility, Visibility::Hidden);
            continue;
        }
        let (anim_off, frame_off, anim_flip, rotated, atlas_idx) = match (anim_opt, bopper_opt) {
            (Some(mut anim), Some(mut bopper)) => {
                bopper.sync_finished(&anim);
                if let Some(steps) = step_range.clone() {
                    for step in steps {
                        bopper.on_step_hit(step, &mut anim);
                    }
                }
                let idx = anim.current_atlas_index();
                if let Some(idx) = idx {
                    set_atlas_index(&mut sprite, idx);
                }
                (
                    anim.current_offset(),
                    anim.current_frame_offset(),
                    anim.current_flip(),
                    anim.current_frame_rotated(),
                    idx,
                )
            }
            (Some(anim), None) => {
                let idx = anim.current_atlas_index();
                if let Some(idx) = idx {
                    set_atlas_index(&mut sprite, idx);
                }
                (
                    anim.current_offset(),
                    anim.current_frame_offset(),
                    anim.current_flip(),
                    anim.current_frame_rotated(),
                    idx,
                )
            }
            _ => ([0.0, 0.0], [0.0, 0.0], [false, false], false, None),
        };

        let cur_raw_size = frame_sizes_opt
            .and_then(|fs| atlas_idx.and_then(|idx| fs.0.get(&idx).copied()))
            .unwrap_or(vis.raw_size);

        let (mut world_cx, mut world_cy) = match vis.coord_mode {
            StageCoordMode::Center => (vis.base_position[0], vis.base_position[1]),
            StageCoordMode::TopLeft => (
                vis.base_position[0] + cur_raw_size[0] * vis.base_scale[0] * 0.5,
                vis.base_position[1] + cur_raw_size[1] * vis.base_scale[1] * 0.5,
            ),
        };
        world_cx -= anim_off[0] * vis.base_scale[0];
        world_cy -= anim_off[1] * vis.base_scale[1];

        let screen_cx = world_cx - eff_scroll_x;
        let screen_cy = world_cy - eff_scroll_y;

        let base_rot = -vis.base_angle_deg.to_radians();
        let eff_flip_x = vis.base_flip[0] ^ anim_flip[0];
        let eff_flip_y = vis.base_flip[1] ^ anim_flip[1];
        let foff_x = if eff_flip_x {
            -frame_off[0]
        } else {
            frame_off[0]
        };
        let foff_y = if eff_flip_y {
            -frame_off[1]
        } else {
            frame_off[1]
        };

        let bx = (screen_cx - SCREEN_WIDTH * 0.5) * prop_zoom
            + foff_x * vis.base_scale[0] * prop_zoom;
        let by = (SCREEN_HEIGHT * 0.5 - screen_cy) * prop_zoom
            + foff_y * vis.base_scale[1] * prop_zoom;
        let rot_delta = if rotated {
            if eff_flip_x ^ eff_flip_y {
                -FRAC_PI_2
            } else {
                FRAC_PI_2
            }
        } else {
            0.0
        };
        set_transform_full(
            &mut transform,
            Vec3::new(bx, by, vis.z_layer),
            Vec3::new(
                vis.base_scale[0] * prop_zoom,
                vis.base_scale[1] * prop_zoom,
                1.0,
            ),
            Quat::from_rotation_z(base_rot + rot_delta),
        );
        set_sprite_flip(&mut sprite, eff_flip_x, eff_flip_y);
        set_visibility(
            &mut visibility,
            if post_ov_opt.is_some() {
                Visibility::Hidden
            } else {
                Visibility::Visible
            },
        );
    }
}

fn apply_pixel_sampler(
    sampler_opt: Option<Mut<'_, StagePropPixelSampler>>,
    composite_opt: Option<&StageCompositeProp>,
    sprite_image: &Handle<Image>,
    images: &mut Assets<Image>,
) {
    let Some(mut px) = sampler_opt else {
        return;
    };
    if !px.is_pixel || px.sampler_applied {
        return;
    }
    if let Some(composite) = composite_opt {
        let mut all_ready = true;
        for (tex, _) in &composite.textures {
            if let Some(mut img) = images.get_mut(tex) {
                img.sampler = ImageSampler::nearest();
            } else {
                all_ready = false;
            }
        }
        px.sampler_applied = all_ready;
    } else if let Some(mut img) = images.get_mut(sprite_image) {
        img.sampler = ImageSampler::nearest();
        px.sampler_applied = true;
    }
}

fn step_prop_physics(vis: &mut StagePropVisual, dt: f32) {
    if dt > 0.0 && (vis.velocity != [0.0, 0.0] || vis.acceleration != [0.0, 0.0]) {
        vis.velocity[0] += vis.acceleration[0] * dt;
        vis.velocity[1] += vis.acceleration[1] * dt;
        vis.base_position[0] += vis.velocity[0] * dt;
        vis.base_position[1] += vis.velocity[1] * dt;
    }
}

pub fn sync_script_texts(
    state: Res<PlayState>,
    mut texts: Query<(&DynamicScriptText, &mut Transform, &mut Visibility)>,
) {
    let cam_scroll_x = state.camera.follow_point[0] as f32 - SCREEN_WIDTH * 0.5;
    let cam_scroll_y = state.camera.follow_point[1] as f32 - SCREEN_HEIGHT * 0.5;
    let zoom = (state.camera.current_zoom * state.camera.bop_multiplier).max(0.05) as f32;

    for (txt, mut tf, mut vis) in &mut texts {
        let hidden = !txt.visible || (txt.hud && state.hud_alpha <= 0.001);
        if hidden {
            set_visibility(&mut vis, Visibility::Hidden);
            continue;
        }
        let text_zoom = if txt.hud { 1.0 } else { zoom };
        let sx = if txt.hud {
            0.0
        } else {
            cam_scroll_x * txt.scroll_factor[0]
        };
        let sy = if txt.hud {
            0.0
        } else {
            cam_scroll_y * txt.scroll_factor[1]
        };
        let bx = (txt.base_position[0] - sx - SCREEN_WIDTH * 0.5) * text_zoom;
        let by = (SCREEN_HEIGHT * 0.5 - (txt.base_position[1] - sy)) * text_zoom;
        set_transform_pos_scale(&mut tf, Vec3::new(bx, by, txt.z_layer), Vec3::splat(text_zoom));
        set_visibility(&mut vis, Visibility::Visible);
    }
}
