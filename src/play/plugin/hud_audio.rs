use bevy::audio::{AudioPlayer, PlaybackMode, PlaybackSettings, Volume};
use bevy::prelude::*;

use crate::audio::{PcmSfxAudio, PcmSfxCache};

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

pub(super) fn sync_hud_text(
    state: Res<PlayState>,
    mut cache: Local<(String, String, bool)>,
    mut hud_text: Query<(&mut Text2d, &mut Visibility), With<HudTextVisual>>,
) {
    if cache.0 == state.score_text && cache.1 == state.status_message && cache.2 == state.hud_visible {
        return;
    }
    cache.0.clone_from(&state.score_text);
    cache.1.clone_from(&state.status_message);
    cache.2 = state.hud_visible;
    let combined = format!("{}  |  {}", cache.0, cache.1);
    let vis = if state.hud_visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for (mut text, mut visibility) in &mut hud_text {
        **text = combined.clone();
        if *visibility != vis {
            *visibility = vis;
        }
    }
}
