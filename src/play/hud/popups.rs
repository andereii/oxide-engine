use bevy::image::ImageSampler;
use bevy::prelude::*;

use crate::utils::constants::KEY_COUNT;
use crate::utils::ecs::{set_sprite_color, set_visibility};

use super::judgement::Judgement;
use crate::mods::ModRegistry;
use crate::play::camera::hud::hud_render_layer;
use crate::play::notes::NoteStyle;
use crate::play::scene::PlayState;
use crate::play::types::PlayPhase;

pub const PREALLOC_RATINGS_PER_KIND: usize = 6;
pub const PREALLOC_DIGITS_PER_KIND: usize = 12;

#[derive(Debug, Clone, Copy)]
pub struct PopupEvent {
    pub judgement: Option<Judgement>,
    pub combo: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupKind {
    Rating(usize),
    Digit(usize),
}

#[derive(Resource, Default)]
pub struct PopupAssets {
    pub ratings: [Option<(Handle<Image>, f32, f32, [f32; 2])>; 4],
    pub digits: [Option<Handle<Image>>; 10],
    pub combo_scale: f32,
    pub combo_alpha: f32,
    pub combo_offset: [f32; 2],
    pub is_pixel: bool,
    pub rng_state: u32,
    pub next_z_step: u32,
}

#[derive(Default)]
pub struct PopupSamplerState {
    pub ratings: [bool; 4],
    pub digits: [bool; 10],
    pub rating_ids: [Option<AssetId<Image>>; 4],
    pub digit_ids: [Option<AssetId<Image>>; 10],
}

fn apply_handle_nearest(
    handle: &Handle<Image>,
    applied: &mut bool,
    last_id: &mut Option<AssetId<Image>>,
    images: &mut Assets<Image>,
) {
    let id = handle.id();
    if *last_id != Some(id) {
        *last_id = Some(id);
        *applied = false;
    }
    if !*applied
        && let Some(mut img) = images.get_mut(handle)
    {
        img.sampler = ImageSampler::nearest();
        *applied = true;
    }
}

pub(crate) fn apply_pixel_popup_samplers(
    assets: &PopupAssets,
    sampler_state: &mut PopupSamplerState,
    images: &mut Assets<Image>,
) {
    if !assets.is_pixel {
        if sampler_state.ratings.iter().any(|&v| v)
            || sampler_state.digits.iter().any(|&v| v)
        {
            *sampler_state = PopupSamplerState::default();
        }
        return;
    }
    for (i, entry) in assets.ratings.iter().enumerate() {
        let Some((handle, ..)) = entry else {
            sampler_state.ratings[i] = false;
            sampler_state.rating_ids[i] = None;
            continue;
        };
        apply_handle_nearest(
            handle,
            &mut sampler_state.ratings[i],
            &mut sampler_state.rating_ids[i],
            images,
        );
    }
    for (d, entry) in assets.digits.iter().enumerate() {
        let Some(handle) = entry else {
            sampler_state.digits[d] = false;
            sampler_state.digit_ids[d] = None;
            continue;
        };
        apply_handle_nearest(
            handle,
            &mut sampler_state.digits[d],
            &mut sampler_state.digit_ids[d],
            images,
        );
    }
}

#[derive(Component, Clone, Copy)]
pub struct PopupVisual {
    pub kind: PopupKind,
    pub active: bool,
    pub vx: f32,
    pub vy: f32,
    pub accel_y: f32,
    pub age_s: f32,
    pub delay_s: f32,
    pub fade_s: f32,
    pub base_alpha: f32,
    pub is_pixel: bool,
}

pub(super) struct PopupSpawnParams {
    pub(super) kind: PopupKind,
    pub(super) texture: Handle<Image>,
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) z: f32,
    pub(super) scale: f32,
    pub(super) alpha: f32,
    pub(super) vx: f32,
    pub(super) vy: f32,
    pub(super) accel_y: f32,
    pub(super) delay_s: f32,
    pub(super) is_pixel: bool,
}

pub fn load_popup_assets(
    mods: &ModRegistry,
    style: &NoteStyle,
    asset_server: &AssetServer,
) -> PopupAssets {
    let mut ratings: [Option<(Handle<Image>, f32, f32, [f32; 2])>; 4] = Default::default();
    for i in 0..KEY_COUNT {
        let cfg = &style.popup.judgments[i];
        if let Some(rel) = &cfg.path {
            ratings[i] = Some((
                asset_server.load(style.asset_path_in(mods, rel)),
                cfg.scale,
                cfg.alpha,
                cfg.offset,
            ));
        }
    }
    let digits = std::array::from_fn(|d| {
        style.popup.combo_digits[d]
            .as_ref()
            .map(|rel| asset_server.load(style.asset_path_in(mods, rel)))
    });
    PopupAssets {
        ratings,
        digits,
        combo_scale: style.popup.combo_scale,
        combo_alpha: style.popup.combo_alpha,
        combo_offset: style.popup.combo_offset,
        is_pixel: style.is_pixel,
        rng_state: 0xA5A5_5A5A,
        next_z_step: 0,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn sync_popups(
    mut commands: Commands,
    time: Res<Time>,
    mut state: ResMut<PlayState>,
    mut images: ResMut<Assets<Image>>,
    assets_opt: Option<ResMut<PopupAssets>>,
    mut sampler_state: Local<PopupSamplerState>,
    mut pending_buf: Local<Vec<PopupEvent>>,
    mut pool: Query<(&mut PopupVisual, &mut Sprite, &mut Transform, &mut Visibility)>,
) {
    if let Some(ref assets) = assets_opt {
        apply_pixel_popup_samplers(assets, &mut sampler_state, &mut images);
    }

    if state.is_in_countdown && state.starting_song {
        for (mut vis, _, _, mut visibility) in &mut pool {
            if vis.active {
                vis.active = false;
                set_visibility(&mut visibility, Visibility::Hidden);
            }
        }
        state.pending_popups.clear();
        return;
    }
    if matches!(
        state.phase,
        PlayPhase::Paused(_) | PlayPhase::GameOver | PlayPhase::Ended
    ) {
        return;
    }

    let hud_alpha = state.hud_alpha.clamp(0.0, 1.0);
    let dt = time.delta_secs();
    let mut active_any = false;
    for (mut vis, mut sprite, mut transform, mut visibility) in &mut pool {
        if !vis.active {
            continue;
        }
        vis.vy += vis.accel_y * dt;
        transform.translation.x += vis.vx * dt;
        transform.translation.y += vis.vy * dt;
        vis.age_s += dt;
        let alpha = if vis.age_s >= vis.delay_s {
            let raw_t = ((vis.age_s - vis.delay_s) / vis.fade_s).clamp(0.0, 1.0);
            let t = if vis.is_pixel { (raw_t * 2.0).floor() * 0.5 } else { raw_t };
            if raw_t >= 1.0 {
                vis.active = false;
                set_visibility(&mut visibility, Visibility::Hidden);
                continue;
            }
            vis.base_alpha * (1.0 - t)
        } else {
            vis.base_alpha
        };
        if hud_alpha <= 0.001 {
            set_visibility(&mut visibility, Visibility::Hidden);
        } else {
            set_visibility(&mut visibility, Visibility::Visible);
            set_sprite_color(&mut sprite, Color::srgba(1.0, 1.0, 1.0, alpha * hud_alpha));
        }
        active_any = true;
    }

    if state.pending_popups.is_empty() {
        if !active_any && let Some(mut assets) = assets_opt {
            assets.next_z_step = 0;
        }
        return;
    }

    let Some(mut assets) = assets_opt else {
        state.pending_popups.clear();
        return;
    };
    if !active_any {
        assets.next_z_step = 0;
    }
    let beat_s = (state.conductor.beat_length_ms() / 1000.0).max(0.1) as f32;
    pending_buf.clear();
    pending_buf.extend(state.pending_popups.drain(..));

    for ev in pending_buf.iter().copied() {
        if let Some(idx) = ev.judgement.and_then(judgement_index)
            && let Some((tex, scale, alpha, offset)) = assets.ratings[idx].clone()
        {
            let vx = -(rng_u32(&mut assets.rng_state, 0, 10) as f32);
            let vy = rng_u32(&mut assets.rng_state, 140, 175) as f32;
            let z = next_popup_z(&mut assets.next_z_step);
            spawn_or_reuse(
                &mut commands,
                &mut pool,
                PopupSpawnParams {
                    kind: PopupKind::Rating(idx),
                    texture: tex,
                    x: -33.28 + offset[0] + state.popup_offset[0],
                    y: 96.0 - offset[1] - state.popup_offset[1],
                    z,
                    scale,
                    alpha,
                    vx,
                    vy,
                    accel_y: -550.0,
                    delay_s: beat_s,
                    is_pixel: assets.is_pixel,
                },
            );
        }
        if let Some(combo) = ev.combo {
            let (digits, len) = split_combo_digits(combo);
            for (i, &digit) in digits[..len].iter().enumerate() {
                let Some(tex) = assets.digits[digit].clone() else {
                    continue;
                };
                let da_loop = (i + 1) as f32;
                let vx = rng_f32(&mut assets.rng_state, -5.0, 5.0);
                let vy = rng_u32(&mut assets.rng_state, 130, 150) as f32;
                let accel_y = -(rng_u32(&mut assets.rng_state, 250, 300) as f32);
                let z = next_popup_z(&mut assets.next_z_step);
                spawn_or_reuse(
                    &mut commands,
                    &mut pool,
                    PopupSpawnParams {
                        kind: PopupKind::Digit(digit),
                        texture: tex,
                        x: -31.0 - 36.0 * da_loop + assets.combo_offset[0] + state.popup_offset[0],
                        y: 18.0 - assets.combo_offset[1] - state.popup_offset[1],
                        z,
                        scale: assets.combo_scale,
                        alpha: assets.combo_alpha,
                        vx,
                        vy,
                        accel_y,
                        delay_s: beat_s * 2.0,
                        is_pixel: assets.is_pixel,
                    },
                );
            }
        }
    }
}

type PopupSlotMut<'w> = (
    Mut<'w, PopupVisual>,
    Mut<'w, Sprite>,
    Mut<'w, Transform>,
    Mut<'w, Visibility>,
);

fn spawn_popup_entity(
    commands: &mut Commands,
    texture: Handle<Image>,
    tf: Transform,
    color: Color,
    visibility: Visibility,
    vis: PopupVisual,
) {
    let mut sprite = Sprite::from_image(texture);
    sprite.color = color;
    commands.spawn((sprite, tf, visibility, hud_render_layer(), vis));
}

fn spawn_pool_slots(
    commands: &mut Commands,
    kind: PopupKind,
    texture: &Handle<Image>,
    scale: f32,
    alpha: f32,
    is_pixel: bool,
    count: usize,
) {
    let color = Color::srgba(1.0, 1.0, 1.0, alpha);
    let tf = Transform::from_xyz(0.0, -900.0, 11.0).with_scale(Vec3::splat(scale));
    let vis = PopupVisual {
        kind,
        active: false,
        vx: 0.0,
        vy: 0.0,
        accel_y: 0.0,
        age_s: 0.0,
        delay_s: 0.0,
        fade_s: 0.2,
        base_alpha: alpha,
        is_pixel,
    };
    for _ in 0..count {
        spawn_popup_entity(commands, texture.clone(), tf, color, Visibility::Hidden, vis);
    }
}

pub fn spawn_popup_pool(commands: &mut Commands, assets: &PopupAssets) {
    for (idx, entry) in assets.ratings.iter().enumerate() {
        if let Some((tex, scale, alpha, _)) = entry {
            spawn_pool_slots(
                commands,
                PopupKind::Rating(idx),
                tex,
                *scale,
                *alpha,
                assets.is_pixel,
                PREALLOC_RATINGS_PER_KIND,
            );
        }
    }
    for (digit, entry) in assets.digits.iter().enumerate() {
        if let Some(tex) = entry {
            spawn_pool_slots(
                commands,
                PopupKind::Digit(digit),
                tex,
                assets.combo_scale,
                assets.combo_alpha,
                assets.is_pixel,
                PREALLOC_DIGITS_PER_KIND,
            );
        }
    }
}

fn apply_popup_slot(
    slot: PopupSlotMut<'_>,
    texture: Handle<Image>,
    tf: Transform,
    color: Color,
    next_vis: PopupVisual,
) {
    let (mut vis, mut sprite, mut transform, mut visibility) = slot;
    *vis = next_vis;
    if sprite.image.id() != texture.id() {
        sprite.image = texture;
    }
    set_sprite_color(&mut sprite, color);
    *transform = tf;
    set_visibility(&mut visibility, Visibility::Visible);
}

fn next_popup_z(step: &mut u32) -> f32 {
    let z = 11.0 + ((*step % 900) as f32) * 0.001;
    *step = step.saturating_add(1);
    z
}

pub(super) fn spawn_or_reuse(
    commands: &mut Commands,
    pool: &mut Query<(&mut PopupVisual, &mut Sprite, &mut Transform, &mut Visibility)>,
    p: PopupSpawnParams,
) {
    let color = Color::srgba(1.0, 1.0, 1.0, p.alpha);
    let tf = Transform::from_xyz(p.x, p.y, p.z).with_scale(Vec3::splat(p.scale));
    let next_vis = PopupVisual {
        kind: p.kind,
        active: true,
        vx: p.vx,
        vy: p.vy,
        accel_y: p.accel_y,
        age_s: 0.0,
        delay_s: p.delay_s,
        fade_s: 0.2,
        base_alpha: p.alpha,
        is_pixel: p.is_pixel,
    };

    let mut oldest_active: Option<PopupSlotMut<'_>> = None;
    for item in pool.iter_mut() {
        if item.0.kind != p.kind {
            continue;
        }
        if !item.0.active {
            apply_popup_slot(item, p.texture, tf, color, next_vis);
            return;
        }
        let is_older = oldest_active
            .as_ref()
            .is_none_or(|cur| item.0.age_s > cur.0.age_s);
        if is_older {
            oldest_active = Some(item);
        }
    }

    if let Some(slot) = oldest_active {
        apply_popup_slot(slot, p.texture, tf, color, next_vis);
        return;
    }

    spawn_popup_entity(commands, p.texture, tf, color, Visibility::Visible, next_vis);
}

fn judgement_index(j: Judgement) -> Option<usize> {
    match j {
        Judgement::Sick => Some(0),
        Judgement::Good => Some(1),
        Judgement::Bad => Some(2),
        Judgement::Shit => Some(3),
        Judgement::Miss => None,
    }
}

fn split_combo_digits(mut combo: u32) -> ([usize; 10], usize) {
    let mut out = [0usize; 10];
    let mut len = 0;
    while combo != 0 && len < 10 {
        out[len] = (combo % 10) as usize;
        len += 1;
        combo /= 10;
    }
    (out, len.max(3))
}

fn xorshift32(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

fn rng_u32(state: &mut u32, min: u32, max: u32) -> u32 {
    let span = max.saturating_sub(min) + 1;
    min + (xorshift32(state) % span)
}

fn rng_f32(state: &mut u32, min: f32, max: f32) -> f32 {
    let t = (xorshift32(state) & 0xFFFF) as f32 / 65535.0;
    min + (max - min) * t
}

