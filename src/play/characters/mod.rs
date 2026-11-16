mod asset_cache;
pub mod behavior;
pub mod loader;
pub mod schema;
pub mod spawn;
pub mod sync;

pub use asset_cache::char_rel_candidates_for_watch;
pub use behavior::CharacterActor;
pub use loader::{
    CharactersResource, append_character_sidecar_scripts, ensure_character_gameover_anims_loaded,
    load_characters_resource, load_single_character,
};
pub use schema::CharacterRole;
pub use spawn::{apply_composite_affine_transforms, spawn_characters, spawn_single_character};
pub use sync::{CharacterSyncState, sync_characters};

#[cfg(test)]
mod tests;
