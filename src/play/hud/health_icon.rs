use std::f32::consts::FRAC_PI_2;
use std::fs;

use bevy::image::ImageSampler;
use bevy::prelude::*;

use crate::animations::{AnimationController, SparrowAtlas};
use crate::mods::ModRegistry;
use crate::play::characters::schema::{CharacterData, CharacterRole, HealthIconData};
use crate::play::characters::loader::CharactersResource;
use crate::utils::constants::{
    DEFAULT_HEALTH_ICON, HEALTH_ICON_ANGLE_HALF_LIFE, HEALTH_ICON_BOP_SCALE,
    HEALTH_ICON_LOSING_THRESHOLD, HEALTH_ICON_POSITION_OFFSET, HEALTH_ICON_SIZE,
    HEALTH_ICON_WINNING_THRESHOLD, HEALTH_MAX, PIXEL_ICON_SIZE, SCREEN_HEIGHT, SCREEN_WIDTH,
    STEPS_PER_BEAT,
};
use crate::utils::ecs::{set_atlas_index, set_sprite_flip, set_transform_full};

use crate::play::camera::hud::hud_render_layer;

#[derive(Resource, Default)]
pub struct PreloadedHealthIcons(pub Vec<Handle<Image>>);

#[derive(Clone)]
pub struct PendingIconSwap {
    pub character_id: String,
    pub texture: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
    pub anim: AnimationController,
    pub is_legacy_style: bool,
    pub is_pixel: bool,
    pub base_frame_size: [f32; 2],
    pub base_scale: [f32; 2],
    pub icon_offset: [f32; 2],
    pub flip_x: bool,
    pub legacy_image: Option<Handle<Image>>,
}

#[derive(Component)]
pub struct HealthIcon {
    pub player_id: u8,
    #[allow(dead_code)]
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
    pub sampler_applied: bool,
    pub base_alpha: f32,
    pub visible: bool,
    pub pending_swap: Option<PendingIconSwap>,
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
    mods: &ModRegistry,
    commands: &mut Commands,
    chars: Option<&CharactersResource>,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) {
    let p2_char = chars.and_then(|c| c.characters.iter().find(|ch| ch.role == CharacterRole::Opponent));
    let p1_char = chars.and_then(|c| c.characters.iter().find(|ch| ch.role == CharacterRole::Player));

    spawn_single_icon(
        commands,
        mods,
        1,
        p2_char.map(|c| c.id.as_str()),
        p2_char.and_then(|c| c.data.health_icon.as_ref()),
        p2_char.is_some_and(|c| c.data.is_pixel),
        asset_server,
        layouts,
    );
    spawn_single_icon(
        commands,
        mods,
        0,
        p1_char.map(|c| c.id.as_str()),
        p1_char.and_then(|c| c.data.health_icon.as_ref()),
        p1_char.is_some_and(|c| c.data.is_pixel),
        asset_server,
        layouts,
    );
}

fn load_icon_visual_bundle(
    mods: &ModRegistry,
    icon_id: &str,
    is_pixel: bool,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) -> (
    Handle<Image>,
    Handle<TextureAtlasLayout>,
    AnimationController,
    bool,
    [f32; 2],
    Option<Handle<Image>>,
) {
    let xml_rel = format!("gameplay/characters/{icon_id}/icon-{icon_id}.xml");
    let png_rel = format!("gameplay/characters/{icon_id}/icon-{icon_id}.png");
    let png_asset = mods.resolve_asset_server_path(&png_rel).unwrap_or(png_rel);
    let texture = asset_server.load(png_asset);
    let mut anim = AnimationController::default();

    let xml_disk = mods
        .resolve_file(&xml_rel)
        .unwrap_or_else(|| mods.assets_dir.join(&xml_rel));
    let (layout_handle, is_legacy_style, base_frame_size, legacy_image) =
        if let Ok(sparrow) = SparrowAtlas::from_file(&xml_disk) {
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
    (texture, layout_handle, anim, is_legacy_style, base_frame_size, legacy_image)
}

pub fn resolve_icon_asset_rel(mods: &ModRegistry, char_id: &str) -> String {
    let direct_exists = icon_png_exists(mods, char_id);
    let char_meta = load_char_icon_meta(mods, char_id);
    let hdata = char_meta.as_ref().and_then(|(_, h)| h.as_ref());
    let requested_id = if direct_exists {
        char_id
    } else {
        hdata.and_then(|d| d.id.as_deref()).unwrap_or(char_id)
    };
    let icon_id = if direct_exists || icon_png_exists(mods, requested_id) {
        requested_id
    } else {
        DEFAULT_HEALTH_ICON
    };
    let png_rel = format!("gameplay/characters/{icon_id}/icon-{icon_id}.png");
    mods.resolve_asset_server_path(&png_rel).unwrap_or(png_rel)
}

#[allow(clippy::too_many_arguments)]
pub fn apply_icon_char_id(
    icon: &mut HealthIcon,
    anim: &mut AnimationController,
    sprite: &mut Sprite,
    char_id: &str,
    flip_x_override: Option<bool>,
    mods: &ModRegistry,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    images: Option<&mut Assets<Image>>,
) {
    let direct_exists = icon_png_exists(mods, char_id);
    let char_meta = load_char_icon_meta(mods, char_id);
    let hdata = char_meta.as_ref().and_then(|(_, h)| h.as_ref());
    let requested_id = if direct_exists {
        char_id
    } else {
        hdata.and_then(|d| d.id.as_deref()).unwrap_or(char_id)
    };
    let icon_id = if direct_exists || icon_png_exists(mods, requested_id) {
        requested_id
    } else {
        DEFAULT_HEALTH_ICON
    };
    let fallback_pixel = icon_id == DEFAULT_HEALTH_ICON && requested_id != DEFAULT_HEALTH_ICON;
    let next_is_pixel = if fallback_pixel {
        false
    } else {
        hdata
            .and_then(|h| h.is_pixel)
            .or_else(|| char_meta.as_ref().map(|(is_px, _)| *is_px))
            .unwrap_or(false)
    };
    let (texture, layout_handle, next_anim, is_legacy, base_frame_size, legacy_image) =
        load_icon_visual_bundle(mods, icon_id, next_is_pixel, asset_server, layouts);
    let s = hdata.and_then(|h| h.scale).unwrap_or(1.0);
    let next_flip_x = if let Some(fx) = flip_x_override {
        fx
    } else {
        let mut fx = hdata.and_then(|h| h.flip_x).unwrap_or(false);
        if icon.player_id == 0 {
            fx = !fx;
        }
        fx
    };
    let swap = PendingIconSwap {
        character_id: icon_id.to_string(),
        texture,
        layout: layout_handle,
        anim: next_anim,
        is_legacy_style: is_legacy,
        is_pixel: next_is_pixel,
        base_frame_size,
        base_scale: [s, s],
        icon_offset: [0.0, 0.0],
        flip_x: next_flip_x,
        legacy_image,
    };

    match images {
        Some(imgs) if imgs.contains(&swap.texture) => {
            commit_icon_swap(icon, anim, sprite, swap, Some(imgs), layouts);
        }
        Some(_) => {
            // Keep the current icon visible until the new texture finishes loading to avoid a blank flicker.
            icon.pending_swap = Some(swap);
        }
        None => {
            commit_icon_swap(icon, anim, sprite, swap, None, layouts);
        }
    }
}

fn commit_icon_swap(
    icon: &mut HealthIcon,
    anim: &mut AnimationController,
    sprite: &mut Sprite,
    mut swap: PendingIconSwap,
    mut images: Option<&mut Assets<Image>>,
    layouts: &mut Assets<TextureAtlasLayout>,
) {
    let mut legacy_layout_ready = false;
    let mut sampler_applied = false;
    if let Some(imgs) = images.as_deref_mut() {
        if swap.is_pixel
            && let Some(mut img) = imgs.get_mut(&swap.texture)
        {
            img.sampler = ImageSampler::nearest();
            sampler_applied = true;
        }
        if swap.is_legacy_style
            && let Some(img_handle) = &swap.legacy_image
            && let Some(img) = imgs.get(img_handle)
        {
            let frame_sz = (if swap.is_pixel { PIXEL_ICON_SIZE } else { HEALTH_ICON_SIZE }) as u32;
            let cols = (img.width() / frame_sz.max(1)).max(1);
            let grid = TextureAtlasLayout::from_grid(
                UVec2::splat(frame_sz.max(1)),
                cols,
                1,
                None,
                None,
            );
            swap.layout = layouts.add(grid);
            if cols >= 3 {
                swap.anim.add_anim("winning", vec![2], 0.0, false);
            }
            legacy_layout_ready = true;
        }
    }

    *anim = swap.anim;
    icon.character_id = swap.character_id;
    icon.is_pixel = swap.is_pixel;
    icon.is_legacy_style = swap.is_legacy_style;
    icon.base_frame_size = swap.base_frame_size;
    icon.legacy_image = swap.legacy_image;
    icon.legacy_layout_ready = legacy_layout_ready;
    icon.sampler_applied = sampler_applied;
    icon.base_scale = swap.base_scale;
    icon.icon_offset = swap.icon_offset;
    icon.flip_x = swap.flip_x;
    icon.pending_swap = None;
    icon.snap_to_target_size();

    let init_idx = anim.current_atlas_index().unwrap_or(0);
    sprite.image = swap.texture;
    sprite.flip_x = icon.flip_x;
    sprite.texture_atlas = Some(TextureAtlas {
        layout: swap.layout,
        index: init_idx,
    });
}

fn load_char_icon_meta(mods: &ModRegistry, char_id: &str) -> Option<(bool, Option<HealthIconData>)> {
    let rel = format!("gameplay/characters/{char_id}/{char_id}.json");
    let path = mods.resolve_file(&rel)?;
    let raw = fs::read_to_string(&path).ok()?;
    let data = serde_json::from_str::<CharacterData>(&raw).ok()?;
    Some((data.is_pixel, data.health_icon))
}

#[allow(clippy::too_many_arguments)]
fn spawn_single_icon(
    commands: &mut Commands,
    mods: &ModRegistry,
    player_id: u8,
    char_id: Option<&str>,
    data: Option<&HealthIconData>,
    char_is_pixel: bool,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) {
    let requested_id = data.and_then(|d| d.id.as_deref()).or(char_id).unwrap_or(DEFAULT_HEALTH_ICON);
    let icon_id = if icon_png_exists(mods, requested_id) { requested_id } else { DEFAULT_HEALTH_ICON };
    let is_pixel = if icon_id == DEFAULT_HEALTH_ICON && requested_id != DEFAULT_HEALTH_ICON {
        false
    } else {
        data.and_then(|d| d.is_pixel).unwrap_or(char_is_pixel)
    };
    let should_bop = true;
    let scale = data.and_then(|d| d.scale).unwrap_or(1.0);
    let icon_offset = data.and_then(|d| d.offsets).unwrap_or([0.0, 0.0]);
    let mut flip_x = data.and_then(|d| d.flip_x).unwrap_or(false);
    if player_id == 0 {
        flip_x = !flip_x;
    }

    let (texture, layout_handle, anim, is_legacy_style, base_frame_size, legacy_image) =
        load_icon_visual_bundle(mods, icon_id, is_pixel, asset_server, layouts);

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
        sampler_applied: false,
        base_alpha: 1.0,
        visible: true,
        pending_swap: None,
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

fn icon_png_exists(mods: &ModRegistry, char_id: &str) -> bool {
    mods.file_exists(&format!("gameplay/characters/{char_id}/icon-{char_id}.png"))
}

#[allow(clippy::too_many_arguments)]
pub fn update_icon_state_and_transform(
    icon: &mut HealthIcon,
    anim: &mut AnimationController,
    sprite: &mut Mut<'_, Sprite>,
    tf: &mut Mut<'_, Transform>,
    images: &mut Assets<Image>,
    layouts: &mut Assets<TextureAtlasLayout>,
    health: f64,
    split_x_flx: f32,
    bar_y_flx: f32,
) {
    if let Some(ref pending) = icon.pending_swap
        && images.contains(&pending.texture)
        && let Some(swap) = icon.pending_swap.take()
    {
        commit_icon_swap(icon, anim, sprite, swap, Some(images), layouts);
    }

    if icon.is_pixel
        && !icon.sampler_applied
        && let Some(mut img) = images.get_mut(&sprite.image)
    {
        img.sampler = ImageSampler::nearest();
        icon.sampler_applied = true;
    }

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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temp_mod_root(suffix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!("oxide_icon_test_{suffix}_{nanos}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn apply_icon_char_id_prefers_direct_icon_and_clears_spawn_offsets() {
        let root = temp_mod_root("direct");
        let char_dir = root.join("gameplay/characters/custom-singer");
        fs::create_dir_all(&char_dir).expect("create char dir");
        // Write valid 1x1 PNG header/bytes (only path existence is probed by icon_png_exists)
        fs::write(char_dir.join("icon-custom-singer.png"), b"").expect("write icon");
        fs::write(
            char_dir.join("custom-singer.json"),
            r#"{"assetPath":"gameplay/characters/custom-singer/sheet","healthIcon":{"id":"bf","offsets":[0,5000]}}"#,
        )
        .expect("write json");

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<TextureAtlasLayout>();
        let asset_server = app.world().resource::<AssetServer>().clone();
        let mut layouts = app.world_mut().resource_mut::<Assets<TextureAtlasLayout>>();

        let mut mods = ModRegistry::default();
        mods.assets_dir = root.clone();

        let mut icon = HealthIcon {
            player_id: 1,
            character_id: "dad".to_string(),
            auto_update: true,
            is_pixel: false,
            is_legacy_style: true,
            should_bop: true,
            bop_every: 4,
            bop_angle_deg: 0.0,
            angle_deg: 0.0,
            base_scale: [1.0, 1.0],
            icon_offset: [0.0, 5000.0],
            flip_x: false,
            base_frame_size: [150.0, 150.0],
            current_width: 150.0,
            current_height: 150.0,
            legacy_image: None,
            legacy_layout_ready: false,
            sampler_applied: false,
            base_alpha: 1.0,
            visible: true,
            pending_swap: None,
            bop_active: false,
            bop_elapsed: 0.0,
            bop_duration: 0.0,
            bop_start_w: 0.0,
            bop_target_w: 0.0,
        };
        let mut anim = AnimationController::default();
        let mut sprite = Sprite::default();

        apply_icon_char_id(
            &mut icon,
            &mut anim,
            &mut sprite,
            "custom-singer",
            None,
            &mods,
            &asset_server,
            &mut layouts,
            None,
        );

        assert_eq!(icon.character_id, "custom-singer");
        assert_eq!(icon.icon_offset, [0.0, 0.0]);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn pending_icon_swap_preserves_old_texture_until_new_image_loads() {
        let root = temp_mod_root("pending_swap");
        for id in ["old-singer", "new-singer"] {
            let dir = root.join(format!("gameplay/characters/{id}"));
            fs::create_dir_all(&dir).expect("create char dir");
            fs::write(dir.join(format!("icon-{id}.png")), b"").expect("write icon");
        }

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<TextureAtlasLayout>();
        let asset_server = app.world().resource::<AssetServer>().clone();
        let old_handle = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::default());

        let mut mods = ModRegistry::default();
        mods.assets_dir = root.clone();

        let mut icon = HealthIcon {
            player_id: 1,
            character_id: "old-singer".to_string(),
            auto_update: true,
            is_pixel: false,
            is_legacy_style: true,
            should_bop: true,
            bop_every: 4,
            bop_angle_deg: 0.0,
            angle_deg: 0.0,
            base_scale: [1.0, 1.0],
            icon_offset: [0.0, 0.0],
            flip_x: false,
            base_frame_size: [150.0, 150.0],
            current_width: 150.0,
            current_height: 150.0,
            legacy_image: Some(old_handle.clone()),
            legacy_layout_ready: true,
            sampler_applied: false,
            base_alpha: 1.0,
            visible: true,
            pending_swap: None,
            bop_active: false,
            bop_elapsed: 0.0,
            bop_duration: 0.0,
            bop_start_w: 0.0,
            bop_target_w: 0.0,
        };
        let mut anim = AnimationController::default();
        let mut sprite = Sprite::from_image(old_handle.clone());

        app.world_mut().resource_scope(|world, mut images: Mut<Assets<Image>>| {
            let mut layouts = world.resource_mut::<Assets<TextureAtlasLayout>>();
            apply_icon_char_id(
                &mut icon,
                &mut anim,
                &mut sprite,
                "new-singer",
                None,
                &mods,
                &asset_server,
                &mut layouts,
                Some(&mut images),
            );
        });

        assert_eq!(icon.character_id, "old-singer");
        assert_eq!(sprite.image, old_handle);
        let target_handle = icon
            .pending_swap
            .as_ref()
            .expect("pending swap queued")
            .texture
            .clone();

        app.world_mut()
            .resource_mut::<Assets<Image>>()
            .insert(&target_handle, Image::default())
            .expect("insert loaded image");

        app.world_mut().resource_scope(|world, mut images: Mut<Assets<Image>>| {
            let mut layouts = world.resource_mut::<Assets<TextureAtlasLayout>>();
            if let Some(swap) = icon.pending_swap.take().filter(|p| images.contains(&p.texture)) {
                commit_icon_swap(
                    &mut icon,
                    &mut anim,
                    &mut sprite,
                    swap,
                    Some(&mut images),
                    &mut layouts,
                );
            }
        });

        assert_eq!(icon.character_id, "new-singer");
        assert!(icon.pending_swap.is_none());
        assert_eq!(sprite.image, target_handle);

        let _ = fs::remove_dir_all(&root);
    }
}

