// Public modding SDK API surface exposed for external consumers and script backends.
#![allow(dead_code)]

pub mod loader;
pub mod manifest;
pub mod store;
pub mod vfs;

// Public modding facade re-exports for external/future consumers.
#[allow(unused_imports)]
pub use loader::{DiscoveredScript, ScriptScope};
#[allow(unused_imports)]
pub use manifest::{ModAuthor, ModDependencyEntry, ModError, ModManifest};
pub use store::ModStore;
#[allow(unused_imports)]
pub use vfs::{LoadedMod, ModRegistry};
