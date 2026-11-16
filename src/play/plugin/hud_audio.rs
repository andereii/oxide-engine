use bevy::audio::{AudioPlayer, PlaybackMode, PlaybackSettings, Volume};
use bevy::prelude::*;
use bevy::sprite::Anchor;

use crate::audio::{PcmSfxAudio, PcmSfxCache};
use crate::utils::constants::{
    HEALTH_BAR_Y_RATIO_DOWNSCROLL, HEALTH_BAR_Y_RATIO_UPSCROLL, SCREEN_HEIGHT,
};

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
    status_message: String,
    hud_visible: bool,
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
    let by = SCREEN_HEIGHT * 0.5 - (bg_y_flx + 30.0);
    let text = if state.is_bot_play_mode && state.custom_score_text.is_none() {
        state.score_text.clone()
    } else {
        format!("{}  |  {}", state.score_text, state.status_message)
    };
    (text, Anchor::TOP_CENTER, Vec3::new(0.0, by, 10.8))
}

pub(super) fn sync_hud_text(
    state: Res<PlayState>,
    mut cache: Local<HudTextSyncCache>,
    mut hud_text: Query<(&mut Text2d, &mut Anchor, &mut Transform, &mut Visibility), With<HudTextVisual>>,
) {
    let botplay_layout = state.is_bot_play_mode && state.custom_score_text.is_none();
    let downscroll = state.player_strumline.is_downscroll;
    if cache.initialized
        && cache.score_text == state.score_text
        && cache.status_message == state.status_message
        && cache.hud_visible == state.hud_visible
        && cache.botplay_layout == botplay_layout
        && cache.downscroll == downscroll
    {
        return;
    }
    cache.initialized = true;
    cache.score_text.clone_from(&state.score_text);
    cache.status_message.clone_from(&state.status_message);
    cache.hud_visible = state.hud_visible;
    cache.botplay_layout = botplay_layout;
    cache.downscroll = downscroll;

    let (display_text, target_anchor, target_pos) = compute_hud_text_layout(&state);
    let vis = if state.hud_visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for (mut text, mut anchor, mut transform, mut visibility) in &mut hud_text {
        **text = display_text.clone();
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
