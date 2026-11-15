use bevy::audio::{AudioPlayer, PlaybackMode, PlaybackSettings, Volume};
use bevy::image::ImageSampler;
use bevy::prelude::*;

use crate::play::notes::NoteStyle;
use crate::play::scene::PlayState;
use crate::play::types::PlayPhase;

use crate::play::camera::hud::hud_render_layer;

#[derive(Clone)]
pub struct CountdownGraphicSpec {
    pub texture: Handle<Image>,
    pub scale: f32,
    pub alpha: f32,
    pub offset: [f32; 2],
}

#[derive(Resource, Default)]
pub struct CountdownAssets {
    pub sounds: [Option<(Handle<AudioSource>, f32)>; 4],
    pub graphics: [Option<CountdownGraphicSpec>; 4],
    pub is_pixel: bool,
    pub sampler_applied: [bool; 4],
}

#[derive(Component)]
pub struct CountdownGraphicVisual {
    pub age_s: f32,
    pub duration_s: f32,
    pub base_alpha: f32,
}

pub fn load_countdown_assets(style: &NoteStyle, asset_server: &AssetServer) -> CountdownAssets {
    let sounds = std::array::from_fn(|idx| {
        style.countdown_sounds[idx]
            .as_ref()
            .filter(|rel| !rel.trim().is_empty())
            .map(|rel| (asset_server.load(style.asset_path(rel)), style.countdown_volumes[idx]))
    });
    let graphics = std::array::from_fn(|idx| {
        let cfg = &style.countdown_images[idx];
        cfg.path
            .as_ref()
            .filter(|rel| !rel.trim().is_empty())
            .map(|rel| CountdownGraphicSpec {
                texture: asset_server.load(style.asset_path(rel)),
                scale: cfg.scale,
                alpha: cfg.alpha,
                offset: cfg.offset,
            })
    });
    CountdownAssets {
        sounds,
        graphics,
        is_pixel: style.is_pixel,
        sampler_applied: [false; 4],
    }
}

pub fn sync_countdown(
    mut commands: Commands,
    time: Res<Time>,
    mut state: ResMut<PlayState>,
    mut images: ResMut<Assets<Image>>,
    assets_opt: Option<ResMut<CountdownAssets>>,
    mut visuals: Query<(Entity, &mut CountdownGraphicVisual, &mut Sprite)>,
) {
    let Some(mut assets) = assets_opt else {
        state.pending_countdown_step = None;
        return;
    };

    if assets.is_pixel {
        for idx in 0..4 {
            if !assets.sampler_applied[idx]
                && let Some(spec) = &assets.graphics[idx]
                && let Some(mut img) = images.get_mut(&spec.texture)
            {
                img.sampler = ImageSampler::nearest();
                assets.sampler_applied[idx] = true;
            }
        }
    }

    if state.needs_reset {
        for (entity, _, _) in &visuals {
            commands.entity(entity).despawn();
        }
    }

    if let Some(step) = state.pending_countdown_step.take() {
        if let Some((handle, volume)) = assets.sounds.get(step).and_then(|s| s.as_ref()) {
            commands.spawn((
                AudioPlayer(handle.clone()),
                PlaybackSettings {
                    mode: PlaybackMode::Despawn,
                    volume: Volume::Linear(volume.clamp(0.0, 2.0)),
                    ..default()
                },
            ));
        }

        if !state.is_minimal_mode
            && let Some(spec) = assets.graphics.get(step).and_then(|g| g.as_ref())
        {
            let beat_s = (state.conductor.beat_length_ms() / 1000.0) / state.playback_rate.max(0.01);
            let mut sprite = Sprite::from_image(spec.texture.clone());
            sprite.color = Color::srgba(1.0, 1.0, 1.0, spec.alpha);
            commands.spawn((
                sprite,
                Transform::from_xyz(spec.offset[0], -spec.offset[1], 20.0)
                    .with_scale(Vec3::splat(spec.scale)),
                hud_render_layer(),
                CountdownGraphicVisual {
                    age_s: 0.0,
                    duration_s: (beat_s as f32).max(0.05),
                    base_alpha: spec.alpha,
                },
            ));
        }
    }

    if matches!(state.phase, PlayPhase::Paused(_) | PlayPhase::GameOver | PlayPhase::Ended) {
        return;
    }

    let dt = time.delta_secs().min(0.1);
    for (entity, mut vis, mut sprite) in &mut visuals {
        vis.age_s += dt;
        let t = (vis.age_s / vis.duration_s).clamp(0.0, 1.0);
        let alpha = vis.base_alpha * (1.0 - ease_cube_in_out(t));
        sprite.color = Color::srgba(1.0, 1.0, 1.0, alpha);
        if t >= 1.0 {
            commands.entity(entity).despawn();
        }
    }
}

fn ease_cube_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

