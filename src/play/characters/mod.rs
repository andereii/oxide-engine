mod asset_cache;
pub mod behavior;
pub mod loader;
pub mod schema;
pub mod spawn;
pub mod sync;

pub use asset_cache::char_rel_candidates_for_watch;
pub(crate) use asset_cache::load_animate_textures;
pub use behavior::{
    CharacterActor, CharacterTrailConfig, CharacterTrailState, CharacterUnderlayConfig,
    SymbolAtlasAttachment,
};
pub use loader::{
    CharactersResource, LoadedCharacter, append_character_sidecar_scripts,
    ensure_character_gameover_anims_loaded, load_characters_resource, load_single_character,
};
pub use schema::CharacterRole;
pub use spawn::{
    CharacterTrailVisual, apply_composite_affine_transforms, spawn_characters,
    spawn_single_character, spawn_single_character_with_underlay,
};
pub(crate) use spawn::{ensure_actor_piece_capacity, ensure_actor_piece_capacity_world};
pub use sync::{CharacterSyncState, sync_characters};
