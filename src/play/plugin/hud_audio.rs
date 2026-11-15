use bevy::audio::{AudioPlayer, PlaybackMode, PlaybackSettings, Volume};
use bevy::prelude::*;

use super::super::scene::PlayState;
use super::HudTextVisual;

pub(super) fn spawn_one_shot(commands: &mut Commands, handle: Handle<AudioSource>, volume: f32) {
    commands.spawn((
        AudioPlayer(handle),
        PlaybackSettings {
            mode: PlaybackMode::Despawn,
            volume: Volume::Linear(volume.clamp(0.0, 2.0)),
            ..default()
        },
    ));
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
