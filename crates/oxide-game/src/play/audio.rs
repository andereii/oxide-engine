//! Audio bridge for the interactive session.
//!
//! The play session reads the shipped Inst/voice OGG bytes through the VFS and
//! hands them to [`oxide_audio::playback::SongPlayback`]. All cache, clock and
//! kira details stay in `oxide-audio`; this module only resolves the slice's
//! observed asset names and maps its typed errors.

use std::path::Path;

use oxide_audio::clock::AudioError;
use oxide_audio::playback::{AudioBytes, SongPlayback, VoiceTrack};
use oxide_vfs::Vfs;

use super::PlayError;

/// The frozen slice song directory. ADR-0021 scopes the playtest to bopeebo.
const SONG_DIR: &str = "songs/bopeebo";
/// Instrumental stem name used by the shipped bopeebo assets.
const INST_FILE: &str = "Inst.ogg";
/// Upper bound for one encoded OGG stem (the shipped stems are ~1-2 MiB).
const AUDIO_SIZE_LIMIT: u64 = 64 * 1024 * 1024;

/// Starts Inst plus every metadata voice track on the playback crate's shared
/// Kira clock.
pub(crate) fn start(
    assets_root: &Path,
    player_vocals: &[String],
    opponent_vocals: &[String],
) -> Result<SongPlayback, PlayError> {
    if player_vocals.is_empty() {
        return Err(PlayError::AudioConfig {
            detail: "metadata playData.characters.playerVocals is empty; the player \
                     vocal mute/restore obligation cannot be scheduled"
                .to_string(),
        });
    }

    let vfs = Vfs::new(assets_root.to_path_buf());
    let instrumental = read_track(&vfs, &format!("{SONG_DIR}/{INST_FILE}"))?;

    let mut voices = Vec::with_capacity(player_vocals.len() + opponent_vocals.len());
    for character in player_vocals.iter().chain(opponent_vocals.iter()) {
        // Keep the exact metadata id: it is also the key the session later
        // passes to `set_voice_volume`, so trimming here would silently make
        // that lookup fail for an id with surrounding whitespace.
        if character.is_empty()
            || character.contains('/')
            || character.contains('\\')
            || character.contains("..")
        {
            return Err(PlayError::AudioConfig {
                detail: format!("invalid voice character id {character:?} in song metadata"),
            });
        }
        let path = format!("{SONG_DIR}/Voices-{character}.ogg");
        voices.push(VoiceTrack {
            character: character.clone(),
            data: read_track(&vfs, &path)?,
        });
    }

    SongPlayback::start(instrumental, voices).map_err(map_error)
}

/// Maps the audio crate's typed error, preserving the unavailable reason.
pub(crate) fn map_error(error: AudioError) -> PlayError {
    match error {
        AudioError::AudioUnavailable(reason) => PlayError::AudioUnavailable { reason },
        other => PlayError::Audio(other),
    }
}

fn read_track(vfs: &Vfs, path: &str) -> Result<AudioBytes, PlayError> {
    let source = vfs
        .resolve(path)
        .ok_or_else(|| PlayError::MissingAudioTrack {
            path: path.to_string(),
        })?;
    let bytes = vfs
        .read(&source, AUDIO_SIZE_LIMIT)
        .map_err(|source| PlayError::AudioRead {
            path: path.to_string(),
            source,
        })?;
    AudioBytes::new(bytes).map_err(map_error)
}
