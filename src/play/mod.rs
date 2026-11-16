pub mod camera;
pub mod characters;
pub mod helpers;
pub mod hot_reload;
pub mod hud;
pub mod notes;
pub mod playlist;
pub mod plugin;
pub mod preload;
pub mod scene;
pub mod shaders;
pub mod song;
pub mod stage;
pub mod types;

pub use plugin::PlayPlugin;

#[cfg(test)]
mod tests;
