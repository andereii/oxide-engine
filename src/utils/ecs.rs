use bevy::prelude::*;

/// Updates `Visibility` only when changed so Bevy doesn't rerun visibility culling every frame.
#[inline]
pub fn set_visibility(vis: &mut Mut<'_, Visibility>, target: Visibility) {
    let cur: &Mut<'_, Visibility> = &*vis;
    if **cur != target {
        **vis = target;
    }
}

/// Updates only the texture atlas index when it differs, avoiding `Changed<Sprite>` churn.
#[inline]
pub fn set_atlas_index(sprite: &mut Mut<'_, Sprite>, index: usize) {
    let cur: &Mut<'_, Sprite> = &*sprite;
    if cur.texture_atlas.as_ref().is_some_and(|a| a.index != index)
        && let Some(atlas) = sprite.texture_atlas.as_mut()
    {
        atlas.index = index;
    }
}

/// Updates image handle and atlas layout/index only when any part differs.
#[inline]
pub fn set_sprite_atlas(
    sprite: &mut Mut<'_, Sprite>,
    texture: &Handle<Image>,
    layout: &Handle<TextureAtlasLayout>,
    index: usize,
) {
    let cur: &Mut<'_, Sprite> = &*sprite;
    let same_img = cur.image.id() == texture.id();
    let same_layout = cur
        .texture_atlas
        .as_ref()
        .is_some_and(|a| a.layout.id() == layout.id());
    let same_idx = cur.texture_atlas.as_ref().is_some_and(|a| a.index == index);
    if same_img && same_layout && same_idx {
        return;
    }
    if !same_img {
        sprite.image = texture.clone();
    }
    if same_layout {
        if let Some(atlas) = sprite.texture_atlas.as_mut() {
            atlas.index = index;
        }
    } else {
        sprite.texture_atlas = Some(TextureAtlas {
            layout: layout.clone(),
            index,
        });
    }
}

#[inline]
pub fn set_sprite_color(sprite: &mut Mut<'_, Sprite>, color: Color) {
    let cur: &Mut<'_, Sprite> = &*sprite;
    if cur.color != color {
        sprite.color = color;
    }
}

#[inline]
pub fn set_sprite_flip(sprite: &mut Mut<'_, Sprite>, flip_x: bool, flip_y: bool) {
    let (cur_x, cur_y) = {
        let cur: &Mut<'_, Sprite> = &*sprite;
        (cur.flip_x, cur.flip_y)
    };
    if cur_x == flip_x && cur_y == flip_y {
        return;
    }
    if cur_x != flip_x {
        sprite.flip_x = flip_x;
    }
    if cur_y != flip_y {
        sprite.flip_y = flip_y;
    }
}

#[inline]
pub fn set_sprite_custom_size(sprite: &mut Mut<'_, Sprite>, size: Option<Vec2>) {
    let cur: &Mut<'_, Sprite> = &*sprite;
    if cur.custom_size != size {
        sprite.custom_size = size;
    }
}

#[inline]
pub fn set_transform_pos_scale(tf: &mut Mut<'_, Transform>, translation: Vec3, scale: Vec3) {
    let (cur_t, cur_s) = {
        let cur: &Mut<'_, Transform> = &*tf;
        (cur.translation, cur.scale)
    };
    if cur_t == translation && cur_s == scale {
        return;
    }
    if cur_t != translation {
        tf.translation = translation;
    }
    if cur_s != scale {
        tf.scale = scale;
    }
}

#[inline]
pub fn set_transform_full(
    tf: &mut Mut<'_, Transform>,
    translation: Vec3,
    scale: Vec3,
    rotation: Quat,
) {
    let (cur_t, cur_s, cur_r) = {
        let cur: &Mut<'_, Transform> = &*tf;
        (cur.translation, cur.scale, cur.rotation)
    };
    if cur_t == translation && cur_s == scale && cur_r == rotation {
        return;
    }
    if cur_t != translation {
        tf.translation = translation;
    }
    if cur_s != scale {
        tf.scale = scale;
    }
    if cur_r != rotation {
        tf.rotation = rotation;
    }
}
