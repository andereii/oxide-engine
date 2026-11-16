use std::collections::HashSet;

use bevy::asset::{LoadState, UntypedAssetId, UntypedHandle};
use bevy::prelude::*;

use crate::audio::SongAudioPlayer;
use crate::play::characters::{CharactersResource, LoadedCharacter};
use crate::play::hud::countdown::CountdownAssets;
use crate::play::hud::popups::PopupAssets;
use crate::play::notes::render::NoteskinAssets;
use crate::play::stage::StageResource;
use crate::play::stage::loader::{ResolvedStageProp, StagePropKind};

/// Snapshot of asynchronous preload completion reported for HUD/loading-screen consumers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PreloadProgress {
    pub loaded_assets: usize,
    pub total_assets: usize,
    pub audio_ready: bool,
}

impl PreloadProgress {
    /// Returns normalized preload progress in `[0.0, 1.0]`, counting decoded audio readiness
    /// as the final completion step.
    pub fn fraction(&self) -> f32 {
        let total_steps = self.total_assets + 1;
        let done_steps = self.loaded_assets.min(self.total_assets) + usize::from(self.audio_ready);
        (done_steps as f32 / total_steps as f32).clamp(0.0, 1.0)
    }

    pub fn is_complete(&self) -> bool {
        self.loaded_assets >= self.total_assets && self.audio_ready
    }
}

/// Decoupled asset preload barrier shared between scene setup, audio mix readiness, and any
/// future loading screen UI.
#[derive(Resource, Default, Debug)]
pub struct PlayPreloadTracker {
    handles: Vec<UntypedHandle>,
    seen_ids: HashSet<UntypedAssetId>,
    progress: PreloadProgress,
}

impl PlayPreloadTracker {
    pub fn clear(&mut self) {
        self.handles.clear();
        self.seen_ids.clear();
        self.progress = PreloadProgress::default();
    }

    pub fn register_handle(&mut self, handle: impl Into<UntypedHandle>) {
        let untyped = handle.into();
        let id = untyped.id();
        if self.seen_ids.insert(id) {
            self.handles.push(untyped);
            self.progress.total_assets = self.handles.len();
        }
    }

    pub fn register_stage_prop(&mut self, prop: &ResolvedStageProp) {
        match &prop.visual {
            StagePropKind::StaticImage { texture, .. }
            | StagePropKind::AnimatedAtlas { texture, .. } => {
                self.register_handle(texture.clone());
            }
            StagePropKind::CompositeAtlas { textures, .. } => {
                for (tex, _) in textures {
                    self.register_handle(tex.clone());
                }
            }
            StagePropKind::SolidColor { .. } => {}
        }
    }

    #[allow(dead_code)]
    pub fn register_stage(&mut self, stage: &StageResource) {
        for prop in &stage.props {
            self.register_stage_prop(prop);
        }
    }

    pub fn register_character(&mut self, loaded: &LoadedCharacter) {
        for src in &loaded.sources {
            self.register_handle(src.texture.clone());
            for (extra_tex, _) in &src.extra_layers {
                self.register_handle(extra_tex.clone());
            }
        }
    }

    #[allow(dead_code)]
    pub fn register_characters(&mut self, chars: &CharactersResource) {
        for loaded in &chars.characters {
            self.register_character(loaded);
        }
    }

    pub fn register_noteskin(&mut self, skin: &NoteskinAssets) {
        self.register_handle(skin.strum_texture.clone());
        self.register_handle(skin.notes_texture.clone());
        if let Some(tex) = &skin.sustain_texture {
            self.register_handle(tex.clone());
        }
        if let Some(tex) = &skin.splash_texture {
            self.register_handle(tex.clone());
        }
        for tex in skin.hold_cover_textures.iter().flatten() {
            self.register_handle(tex.clone());
        }
    }

    pub fn register_countdown(&mut self, countdown: &CountdownAssets) {
        for sound in countdown.sounds.iter().flatten() {
            self.register_handle(sound.0.clone());
        }
        for graphic in countdown.graphics.iter().flatten() {
            self.register_handle(graphic.texture.clone());
        }
    }

    pub fn register_popups(&mut self, popups: &PopupAssets) {
        for rating in popups.ratings.iter().flatten() {
            self.register_handle(rating.0.clone());
        }
        for digit in popups.digits.iter().flatten() {
            self.register_handle(digit.clone());
        }
    }

    pub fn register_song_audio(&mut self, audio: &SongAudioPlayer) {
        for handle in audio.track_handles() {
            self.register_handle(handle.clone());
        }
    }

    pub fn register_sfx(&mut self, handles: &[Handle<AudioSource>]) {
        for handle in handles {
            self.register_handle(handle.clone());
        }
    }

    /// Evaluates current Bevy asset load states alongside audio mix readiness without panicking
    /// on missing optional assets (`Failed` resolves the barrier cleanly so playback is not hung).
    pub fn poll(&mut self, asset_server: &AssetServer, audio_ready: bool) -> PreloadProgress {
        let mut loaded = 0usize;
        for handle in &self.handles {
            if asset_server.is_loaded_with_dependencies(handle.id())
                || matches!(
                    asset_server.get_load_state(handle.id()),
                    Some(LoadState::Loaded | LoadState::Failed(_))
                )
            {
                loaded += 1;
            }
        }
        self.progress = PreloadProgress {
            loaded_assets: loaded,
            total_assets: self.handles.len(),
            audio_ready,
        };
        self.progress
    }

    // Exposed for future loading-screen UI consumers.
    #[allow(dead_code)]
    pub fn progress(&self) -> PreloadProgress {
        self.progress
    }
}
