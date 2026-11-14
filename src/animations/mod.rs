pub mod animate_atlas;
mod animate_atlas_raw;
pub mod atlas_composite;
pub mod base;
pub mod bopper;
pub mod controller;
pub mod packer;
pub mod sparrow;
pub mod sprite;

pub use base::{
    BaseAnimationData, add_animate_atlas_animations, add_sparrow_animations,
    register_default_atlas_idle, register_default_sparrow_idle,
};
pub use bopper::Bopper;
pub use controller::AnimationController;
pub use packer::{PackerAtlas, add_packer_animation};
pub use sparrow::SparrowAtlas;
pub use sprite::AnimationsPlugin;
