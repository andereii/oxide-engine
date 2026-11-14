use bevy::prelude::*;

use crate::utils::constants::KEY_COUNT;

use super::judgement::Judgement;
use super::notes::NoteStyle;
use super::scene::PlayState;
use super::types::PlayPhase;

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

#[derive(Component)]
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

struct PopupSpawnParams {
    kind: PopupKind,
    texture: Handle<Image>,
    x: f32,
    y: f32,
    z: f32,
    scale: f32,
    alpha: f32,
    vx: f32,
    vy: f32,
    accel_y: f32,
    delay_s: f32,
    is_pixel: bool,
}

pub fn load_popup_assets(style: &NoteStyle, asset_server: &AssetServer) -> PopupAssets {
    let mut ratings: [Option<(Handle<Image>, f32, f32, [f32; 2])>; 4] = Default::default();
    for i in 0..KEY_COUNT {
        let cfg = &style.popup.judgments[i];
        if let Some(rel) = &cfg.path {
            ratings[i] = Some((
                asset_server.load(style.asset_path(rel)),
                cfg.scale,
                cfg.alpha,
                cfg.offset,
            ));
        }
    }
    let digits = std::array::from_fn(|d| {
        style.popup.combo_digits[d]
            .as_ref()
            .map(|rel| asset_server.load(style.asset_path(rel)))
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

pub fn sync_popups(
    mut commands: Commands,
    time: Res<Time>,
    mut state: ResMut<PlayState>,
    assets_opt: Option<ResMut<PopupAssets>>,
    mut pool: Query<(&mut PopupVisual, &mut Sprite, &mut Transform, &mut Visibility)>,
) {
    if state.is_in_countdown && state.starting_song {
        for (mut vis, _, _, mut visibility) in &mut pool {
            if vis.active {
                vis.active = false;
                *visibility = Visibility::Hidden;
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
        if vis.age_s >= vis.delay_s {
            let raw_t = ((vis.age_s - vis.delay_s) / vis.fade_s).clamp(0.0, 1.0);
            let t = if vis.is_pixel { (raw_t * 2.0).floor() * 0.5 } else { raw_t };
            sprite.color = Color::srgba(1.0, 1.0, 1.0, vis.base_alpha * (1.0 - t));
            if raw_t >= 1.0 {
                vis.active = false;
                *visibility = Visibility::Hidden;
                continue;
            }
        }
        active_any = true;
    }

    let Some(mut assets) = assets_opt else {
        state.pending_popups.clear();
        return;
    };
    if !active_any {
        assets.next_z_step = 0;
    }
    let beat_s = (state.conductor.beat_length_ms() / 1000.0).max(0.1) as f32;
    let events: Vec<PopupEvent> = state.pending_popups.drain(..).collect();

    for ev in events {
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
                    x: -33.28 + offset[0],
                    y: 96.0 - offset[1],
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
            for (i, digit) in split_combo_digits(combo).into_iter().enumerate() {
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
                        x: -31.0 - 36.0 * da_loop + assets.combo_offset[0],
                        y: 18.0 - assets.combo_offset[1],
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

fn next_popup_z(step: &mut u32) -> f32 {
    let z = 4.0 + (*step as f32) * 0.001;
    *step = step.saturating_add(1);
    z
}

fn spawn_or_reuse(
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
    for (mut vis, mut sprite, mut transform, mut visibility) in pool.iter_mut() {
        if !vis.active && vis.kind == p.kind {
            *vis = next_vis;
            sprite.color = color;
            *transform = tf;
            *visibility = Visibility::Visible;
            return;
        }
    }
    let mut sprite = Sprite::from_image(p.texture);
    sprite.color = color;
    commands.spawn((sprite, tf, Visibility::Visible, next_vis));
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

fn split_combo_digits(mut combo: u32) -> Vec<usize> {
    let mut out = Vec::with_capacity(4);
    while combo != 0 {
        out.push((combo % 10) as usize);
        combo /= 10;
    }
    while out.len() < 3 {
        out.push(0);
    }
    out
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

