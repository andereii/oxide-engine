use std::f32::consts::FRAC_PI_2;
use std::path::Path;

use bevy::prelude::*;
use oxin_app::resolve_assets_dir;

use crate::animations::{AnimationController, SparrowAtlas};
use crate::play::characters::schema::{CharacterRole, HealthIconData};
use crate::play::characters::loader::CharactersResource;
use crate::utils::constants::{
    DEFAULT_HEALTH_ICON, HEALTH_ICON_ANGLE_HALF_LIFE, HEALTH_ICON_BOP_SCALE,
    HEALTH_ICON_LOSING_THRESHOLD, HEALTH_ICON_POSITION_OFFSET, HEALTH_ICON_SIZE,
    HEALTH_ICON_WINNING_THRESHOLD, HEALTH_MAX, PIXEL_ICON_SIZE, SCREEN_HEIGHT, SCREEN_WIDTH,
    STEPS_PER_BEAT,
};
use crate::utils::ecs::{set_atlas_index, set_sprite_flip, set_transform_full};

use crate::play::camera::hud::hud_render_layer;

#[derive(Component)]
pub struct HealthIcon {
    pub player_id: u8,
    pub character_id: String,
    pub auto_update: bool,
    pub is_pixel: bool,
    pub is_legacy_style: bool,
    pub should_bop: bool,
    pub bop_every: i32,
    pub bop_angle_deg: f32,
    pub angle_deg: f32,
    pub base_scale: [f32; 2],
    pub icon_offset: [f32; 2],
    pub flip_x: bool,
    pub base_frame_size: [f32; 2],
    pub current_width: f32,
    pub current_height: f32,
    pub legacy_image: Option<Handle<Image>>,
    pub legacy_layout_ready: bool,
    bop_active: bool,
    bop_elapsed: f32,
    bop_duration: f32,
    bop_start_w: f32,
    bop_target_w: f32,
}

impl HealthIcon {
    pub fn snap_to_target_size(&mut self) {
        self.bop_active = false;
        let fw = self.base_frame_size[0].max(1.0);
        let fh = self.base_frame_size[1].max(1.0);
        if fw > fh {
            self.current_width = HEALTH_ICON_SIZE * self.base_scale[0];
            self.current_height = self.current_width * (fh / fw);
        } else {
            self.current_height = HEALTH_ICON_SIZE * self.base_scale[1];
            self.current_width = self.current_height * (fw / fh);
        }
    }

    pub fn on_step_hit(&mut self, cur_step: i32, step_length_ms: f64) {
        if self.bop_every != 0 && cur_step % self.bop_every == 0 && self.should_bop {
            let target_w = HEALTH_ICON_SIZE * self.base_scale[0];
            self.bop_start_w = self.current_width + target_w * HEALTH_ICON_BOP_SCALE;
            self.bop_target_w = target_w;
            self.bop_duration = (step_length_ms * 0.002).min(0.175) as f32;
            self.bop_elapsed = 0.0;
            self.bop_active = true;
            let sign = if self.player_id == 0 { 1.0 } else { -1.0 };
            self.angle_deg += self.bop_angle_deg * sign;
        }
    }

    pub fn update_bop(&mut self, dt: f32) {
        if self.bop_every != 0 {
            let decay = 0.5_f32.powf(dt / HEALTH_ICON_ANGLE_HALF_LIFE as f32);
            self.angle_deg *= decay;
        }
        if !self.bop_active {
            return;
        }
        self.bop_elapsed += dt;
        let t = if self.bop_duration > 0.0 {
            (self.bop_elapsed / self.bop_duration).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let w = self.bop_start_w + (self.bop_target_w - self.bop_start_w) * t;
        let fw = self.base_frame_size[0].max(1.0);
        let fh = self.base_frame_size[1].max(1.0);
        self.current_width = w;
        self.current_height = w * (fh / fw);
        if t >= 1.0 {
            self.bop_active = false;
        }
    }
}

pub fn spawn_health_icons(
    commands: &mut Commands,
    chars: Option<&CharactersResource>,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) {
    let assets_dir = resolve_assets_dir();
    let p2_char = chars.and_then(|c| c.characters.iter().find(|ch| ch.role == CharacterRole::Opponent));
    let p1_char = chars.and_then(|c| c.characters.iter().find(|ch| ch.role == CharacterRole::Player));

    spawn_single_icon(commands, &assets_dir, 1, p2_char.map(|c| c.id.as_str()), p2_char.and_then(|c| c.data.health_icon.as_ref()), asset_server, layouts);
    spawn_single_icon(commands, &assets_dir, 0, p1_char.map(|c| c.id.as_str()), p1_char.and_then(|c| c.data.health_icon.as_ref()), asset_server, layouts);
}

fn spawn_single_icon(
    commands: &mut Commands,
    assets_dir: &Path,
    player_id: u8,
    char_id: Option<&str>,
    data: Option<&HealthIconData>,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) {
    let requested_id = data.and_then(|d| d.id.as_deref()).or(char_id).unwrap_or(DEFAULT_HEALTH_ICON);
    let icon_id = if icon_png_exists(assets_dir, requested_id) { requested_id } else { DEFAULT_HEALTH_ICON };
    let is_pixel = data.and_then(|d| d.is_pixel).unwrap_or(false);
    let should_bop = true;
    let scale = data.and_then(|d| d.scale).unwrap_or(1.0);
    let icon_offset = data.and_then(|d| d.offsets).unwrap_or([0.0, 0.0]);
    let mut flip_x = data.and_then(|d| d.flip_x).unwrap_or(false);
    if player_id == 0 {
        flip_x = !flip_x;
    }

    let xml_rel = format!("gameplay/characters/{icon_id}/icon-{icon_id}.xml");
    let png_rel = format!("gameplay/characters/{icon_id}/icon-{icon_id}.png");
    let texture = asset_server.load(png_rel);
    let mut anim = AnimationController::default();

    let (layout_handle, is_legacy_style, base_frame_size, legacy_image) =
        if let Ok(sparrow) = SparrowAtlas::from_file(assets_dir.join(&xml_rel)) {
            let (layout_data, map) = sparrow.build_layout();
            for name in ["idle", "winning", "losing", "toWinning", "fromWinning", "toLosing", "fromLosing"] {
                let looped = matches!(name, "idle" | "winning" | "losing");
                let (indices, offsets, rotated) = sparrow.find_prefix_frames(&map, name);
                if !indices.is_empty() {
                    anim.add_anim_frames(name, indices, offsets, rotated, 24.0, looped);
                }
            }
            let fw = sparrow.frames.first().map(|f| f.frame_width.max(1) as f32).unwrap_or(HEALTH_ICON_SIZE);
            let fh = sparrow.frames.first().map(|f| f.frame_height.max(1) as f32).unwrap_or(HEALTH_ICON_SIZE);
            (layouts.add(layout_data), false, [fw, fh], None)
        } else {
            let frame_sz = if is_pixel { PIXEL_ICON_SIZE } else { HEALTH_ICON_SIZE };
            let grid = TextureAtlasLayout::from_grid(UVec2::splat(frame_sz as u32), 2, 1, None, None);
            anim.add_anim("idle", vec![0], 0.0, false);
            anim.add_anim("losing", vec![1], 0.0, false);
            (layouts.add(grid), true, [frame_sz, frame_sz], Some(texture.clone()))
        };

    anim.play("idle", true, false, 0);
    let init_idx = anim.current_atlas_index().unwrap_or(0);
    let mut sprite = Sprite::from_atlas_image(texture, TextureAtlas { layout: layout_handle, index: init_idx });
    sprite.flip_x = flip_x;

    let mut icon = HealthIcon {
        player_id,
        character_id: icon_id.to_string(),
        auto_update: true,
        is_pixel,
        is_legacy_style,
        should_bop,
        bop_every: STEPS_PER_BEAT,
        bop_angle_deg: 0.0,
        angle_deg: 0.0,
        base_scale: [scale, scale],
        icon_offset,
        flip_x,
        base_frame_size,
        current_width: HEALTH_ICON_SIZE * scale,
        current_height: HEALTH_ICON_SIZE * scale,
        legacy_image,
        legacy_layout_ready: false,
        bop_active: false,
        bop_elapsed: 0.0,
        bop_duration: 0.0,
        bop_start_w: 0.0,
        bop_target_w: 0.0,
    };
    icon.snap_to_target_size();
    let z = if player_id == 0 { 10.6 } else { 10.5 };
    commands.spawn((sprite, Transform::from_xyz(0.0, 0.0, z), anim, hud_render_layer(), icon));
}

fn icon_png_exists(assets_dir: &Path, char_id: &str) -> bool {
    assets_dir.join(format!("gameplay/characters/{char_id}/icon-{char_id}.png")).is_file()
}

pub fn update_icon_state_and_transform(
    icon: &mut HealthIcon,
    anim: &mut AnimationController,
    sprite: &mut Mut<'_, Sprite>,
    tf: &mut Mut<'_, Transform>,
    images: &Assets<Image>,
    layouts: &mut Assets<TextureAtlasLayout>,
    health: f64,
    split_x_flx: f32,
    bar_y_flx: f32,
) {
    if icon.is_legacy_style && !icon.legacy_layout_ready
        && let Some(img_handle) = &icon.legacy_image
        && let Some(img) = images.get(img_handle)
    {
        let frame_sz = (if icon.is_pixel { PIXEL_ICON_SIZE } else { HEALTH_ICON_SIZE }) as u32;
        let cols = (img.width() / frame_sz.max(1)).max(1);
        let grid = TextureAtlasLayout::from_grid(UVec2::splat(frame_sz.max(1)), cols, 1, None, None);
        if let Some(atlas) = sprite.texture_atlas.as_mut() {
            atlas.layout = layouts.add(grid);
        }
        if cols >= 3 {
            anim.add_anim("winning", vec![2], 0.0, false);
        }
        icon.legacy_layout_ready = true;
    }

    if icon.auto_update {
        let icon_health = if icon.player_id == 0 { health } else { HEALTH_MAX - health };
        step_icon_state_machine(anim, icon_health);
        if let Some(idx) = anim.current_atlas_index() {
            set_atlas_index(sprite, idx);
        }
    }

    let left_flx = if icon.player_id == 0 {
        split_x_flx - HEALTH_ICON_POSITION_OFFSET
    } else {
        split_x_flx - (icon.current_width - HEALTH_ICON_POSITION_OFFSET)
    };
    let top_flx = bar_y_flx - icon.current_height * 0.5;
    let f_off = anim.current_frame_offset();
    let bx = (left_flx + icon.current_width * 0.5) - SCREEN_WIDTH * 0.5 - icon.icon_offset[0] + f_off[0];
    let by = SCREEN_HEIGHT * 0.5 - (top_flx + icon.current_height * 0.5) + icon.icon_offset[1] + f_off[1];
    let sx = icon.current_width / icon.base_frame_size[0].max(1.0);
    let sy = icon.current_height / icon.base_frame_size[1].max(1.0);
    let rot_z = -icon.angle_deg.to_radians()
        + if anim.current_frame_rotated() {
            if icon.flip_x { -FRAC_PI_2 } else { FRAC_PI_2 }
        } else {
            0.0
        };
    set_sprite_flip(sprite, icon.flip_x, false);
    let z = if icon.player_id == 0 { 10.6 } else { 10.5 };
    set_transform_full(tf, Vec3::new(bx, by, z), Vec3::new(sx, sy, 1.0), Quat::from_rotation_z(rot_z));
}

fn step_icon_state_machine(anim: &mut AnimationController, health: f64) {
    let cur = anim.current.as_deref().unwrap_or("");
    match cur {
        "idle" => {
            if health < HEALTH_ICON_LOSING_THRESHOLD {
                play_with_fallback(anim, "toLosing", Some("losing"));
            } else if health > HEALTH_ICON_WINNING_THRESHOLD {
                play_with_fallback(anim, "toWinning", Some("winning"));
            } else {
                play_with_fallback(anim, "idle", None);
            }
        }
        "winning" => {
            if health < HEALTH_ICON_WINNING_THRESHOLD {
                play_with_fallback(anim, "fromWinning", Some("idle"));
            } else {
                play_with_fallback(anim, "winning", Some("idle"));
            }
        }
        "losing" => {
            if health > HEALTH_ICON_LOSING_THRESHOLD {
                play_with_fallback(anim, "fromLosing", Some("idle"));
            } else {
                play_with_fallback(anim, "losing", Some("idle"));
            }
        }
        "toLosing" if anim.finished => play_with_fallback(anim, "losing", Some("idle")),
        "toWinning" if anim.finished => play_with_fallback(anim, "winning", Some("idle")),
        "fromLosing" | "fromWinning" if anim.finished => play_with_fallback(anim, "idle", None),
        "toLosing" | "toWinning" | "fromLosing" | "fromWinning" => {}
        _ => play_with_fallback(anim, "idle", None),
    }
}

fn play_with_fallback(anim: &mut AnimationController, name: &str, fallback: Option<&str>) {
    if anim.has_animation(name) {
        anim.play(name, false, false, 0);
    } else if let Some(fb) = fallback && anim.has_animation(fb) {
        anim.play(fb, false, false, 0);
    }
}

