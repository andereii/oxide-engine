use bevy::audio::{AudioPlayer, PlaybackMode, PlaybackSettings, Volume};
use bevy::prelude::*;
use bevy::sprite::Anchor;

use crate::animations::AnimationController;
use crate::audio::{PcmSfxAudio, PcmSfxCache};
use crate::play::stage::DynamicScriptProp;
use crate::ui::SoundTrayState;
use crate::utils::constants::{
    HEALTH_BAR_WIDTH, HEALTH_BAR_Y_RATIO_DOWNSCROLL, HEALTH_BAR_Y_RATIO_UPSCROLL,
    SCORE_TEXT_X_OFFSET, SCORE_TEXT_Y_OFFSET, SCORE_TEXT_Z, SCREEN_HEIGHT,
};
use crate::utils::ecs::set_atlas_index;

use super::super::scene::PlayState;
use super::HudTextVisual;

#[derive(Component, Clone)]
pub(crate) struct TaggedScriptSound {
    pub(crate) tag: String,
    pub(crate) base_volume: f32,
}

pub(super) fn prewarm_sfx_handles(
    handles: &[Handle<AudioSource>],
    audio_sources: &Assets<AudioSource>,
    pcm_sfx: &mut Assets<PcmSfxAudio>,
    sfx_cache: &mut PcmSfxCache,
) {
    for handle in handles {
        let _ = sfx_cache.get_or_insert(handle, audio_sources, pcm_sfx);
    }
}

pub(super) fn spawn_one_shot(
    commands: &mut Commands,
    handle: Handle<AudioSource>,
    volume: f32,
    audio_sources: &Assets<AudioSource>,
    pcm_sfx: &mut Assets<PcmSfxAudio>,
    sfx_cache: &mut PcmSfxCache,
) {
    let clamped = Volume::Linear(volume.clamp(0.0, 2.0));
    if let Some(pcm_handle) = sfx_cache.get_or_insert(&handle, audio_sources, pcm_sfx) {
        commands.spawn((
            AudioPlayer(pcm_handle),
            PlaybackSettings {
                mode: PlaybackMode::Despawn,
                volume: clamped,
                ..default()
            },
        ));
        return;
    }
    commands.spawn((
        AudioPlayer(handle),
        PlaybackSettings {
            mode: PlaybackMode::Despawn,
            volume: clamped,
            ..default()
        },
    ));
}

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_tagged_sound(
    commands: &mut Commands,
    tag: &str,
    handle: Handle<AudioSource>,
    volume: f32,
    looped: bool,
    pitch: f32,
    audio_sources: &Assets<AudioSource>,
    pcm_sfx: &mut Assets<PcmSfxAudio>,
    sfx_cache: &mut PcmSfxCache,
) {
    let clamped_vol = volume.clamp(0.0, 2.0);
    let settings = PlaybackSettings {
        mode: if looped { PlaybackMode::Loop } else { PlaybackMode::Despawn },
        volume: Volume::Linear(clamped_vol),
        speed: pitch.max(0.05),
        ..default()
    };
    let comp = TaggedScriptSound {
        tag: tag.to_string(),
        base_volume: clamped_vol,
    };
    if let Some(pcm_handle) = sfx_cache.get_or_insert(&handle, audio_sources, pcm_sfx) {
        commands.spawn((AudioPlayer(pcm_handle), settings, comp));
        return;
    }
    commands.spawn((AudioPlayer(handle), settings, comp));
}

#[derive(Default)]
pub(super) struct HudTextSyncCache {
    score_text: String,
    hud_alpha_bits: u32,
    botplay_layout: bool,
    downscroll: bool,
    initialized: bool,
}

fn compute_hud_text_layout(state: &PlayState) -> (String, Anchor, Vec3) {
    let bg_y_flx = if state.player_strumline.is_downscroll {
        SCREEN_HEIGHT * HEALTH_BAR_Y_RATIO_DOWNSCROLL
    } else {
        SCREEN_HEIGHT * HEALTH_BAR_Y_RATIO_UPSCROLL
    };
    let bx = (HEALTH_BAR_WIDTH * 0.5 - SCORE_TEXT_X_OFFSET).round();
    let by = (SCREEN_HEIGHT * 0.5 - (bg_y_flx + SCORE_TEXT_Y_OFFSET)).round();
    let text = state.score_text.clone();
    (text, Anchor::TOP_LEFT, Vec3::new(bx, by, SCORE_TEXT_Z))
}

#[allow(clippy::type_complexity)]
pub(super) fn sync_hud_text(
    state: Res<PlayState>,
    mut cache: Local<HudTextSyncCache>,
    mut hud_text: Query<
        (
            &mut Text2d,
            &mut TextColor,
            Option<&mut bevy::sprite::Text2dShadow>,
            &mut Anchor,
            &mut Transform,
            &mut Visibility,
        ),
        With<HudTextVisual>,
    >,
) {
    let botplay_layout = state.is_bot_play_mode && state.custom_score_text.is_none();
    let downscroll = state.player_strumline.is_downscroll;
    let hud_alpha = state.hud_alpha.clamp(0.0, 1.0);
    let hud_alpha_bits = hud_alpha.to_bits();
    if cache.initialized
        && cache.score_text == state.score_text
        && cache.hud_alpha_bits == hud_alpha_bits
        && cache.botplay_layout == botplay_layout
        && cache.downscroll == downscroll
    {
        return;
    }
    cache.initialized = true;
    cache.score_text.clone_from(&state.score_text);
    cache.hud_alpha_bits = hud_alpha_bits;
    cache.botplay_layout = botplay_layout;
    cache.downscroll = downscroll;

    let (display_text, target_anchor, target_pos) = compute_hud_text_layout(&state);
    let vis = if hud_alpha > 0.001 {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for (mut text, mut text_color, shadow_opt, mut anchor, mut transform, mut visibility) in
        &mut hud_text
    {
        **text = display_text.clone();
        text_color.0 = text_color.0.with_alpha(hud_alpha);
        if let Some(mut shadow) = shadow_opt {
            shadow.color = shadow.color.with_alpha(hud_alpha);
        }
        if *anchor != target_anchor {
            *anchor = target_anchor;
        }
        if transform.translation != target_pos {
            transform.translation = target_pos;
        }
        if *visibility != vis {
            *visibility = vis;
        }
    }
}

pub(super) fn sync_abot_visualizer_audio(
    sound_tray: Option<Res<SoundTrayState>>,
    mut last_vol: Local<Option<f32>>,
    mut viz_props: Query<(
        &DynamicScriptProp,
        &mut AnimationController,
        &mut Sprite,
        &mut Visibility,
    )>,
) {
    let vol = sound_tray
        .as_deref()
        .map_or(1.0, SoundTrayState::effective_volume);
    let prev = last_vol.replace(vol).unwrap_or(vol);

    if vol <= 0.0001 {
        for (prop, _, _, mut vis) in &mut viz_props {
            if prop.id.starts_with("abot_viz_") && *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
        }
        return;
    }

    if (vol - prev).abs() > 1e-4 && prev > 0.0001 {
        let ratio = vol / prev;
        for (prop, mut anim, mut sprite, mut vis) in &mut viz_props {
            if !prop.id.starts_with("abot_viz_") || *vis == Visibility::Hidden {
                continue;
            }
            let cur_level = 6_i32.saturating_sub(anim.frame_cursor as i32).clamp(1, 6);
            let scaled = ((cur_level as f32) * ratio).round() as i32;
            if scaled <= 0 {
                *vis = Visibility::Hidden;
            } else {
                let target_frame = (6 - scaled.clamp(1, 6)) as usize;
                anim.play("idle", true, false, target_frame);
                if let Some(idx) = anim.current_atlas_index() {
                    set_atlas_index(&mut sprite, idx);
                }
            }
        }
    }
}
