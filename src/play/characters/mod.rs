mod asset_cache;
pub mod behavior;
pub mod loader;
pub mod schema;
pub mod spawn;
pub mod sync;
#[cfg(test)]
mod tests;

pub use behavior::CharacterActor;
pub use loader::load_characters_resource;
pub use schema::CharacterRole;
pub use spawn::{apply_composite_affine_transforms, spawn_characters};
pub use sync::{CharacterSyncState, sync_characters};
