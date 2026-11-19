use std::collections::HashMap;
use std::path::PathBuf;

use bevy::camera::visibility::RenderLayers;
use bevy::ecs::query::QueryFilter;
use bevy::prelude::*;
use oxin_app::resolve_assets_dir;

use crate::animations::{AnimationController, SparrowAtlas};
use crate::mods::ModRegistry;
use crate::utils::ecs::set_atlas_index;

const SPACE_ADVANCE_PX: f32 = 40.0;
const GLYPH_FPS: f32 = 24.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AtlasFont {
    Bold,
    Default,
    FreeplayClear,
}

impl AtlasFont {
    pub fn asset_stem(self) -> &'static str {
        match self {
            Self::Bold => "ui/fonts/bold",
            Self::Default => "ui/fonts/default",
            Self::FreeplayClear => "ui/fonts/freeplay-clear",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseAllowed {
    Both,
    Upper,
    Lower,
}

#[derive(Debug, Clone)]
pub struct GlyphAnimData {
    pub frames: Vec<usize>,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone)]
pub struct AtlasFontData {
    pub texture: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
    pub max_height: f32,
    pub case_allowed: CaseAllowed,
    pub glyphs: HashMap<String, GlyphAnimData>,
}

impl AtlasFontData {
    pub fn restrict_case(&self, input: &str) -> String {
        match self.case_allowed {
            CaseAllowed::Both => input.to_string(),
            CaseAllowed::Upper => input.to_uppercase(),
            CaseAllowed::Lower => input.to_lowercase(),
        }
    }

    pub fn measure_width(&self, text: &str) -> f32 {
        let cased = self.restrict_case(text);
        let mut max_w: f32 = 0.0;
        let mut x: f32 = 0.0;
        for ch in cased.chars() {
            match ch {
                ' ' => x += SPACE_ADVANCE_PX,
                '\n' => {
                    max_w = max_w.max(x);
                    x = 0.0;
                }
                _ => {
                    let prefix = char_anim_prefix(ch);
                    if let Some(glyph) = self.glyphs.get(prefix) {
                        x += glyph.width;
                    }
                }
            }
        }
        max_w.max(x)
    }
}

#[derive(Resource, Default)]
pub struct AlphabetFontCache {
    fonts: HashMap<AtlasFont, AtlasFontData>,
}

impl AlphabetFontCache {
    pub fn get_or_load(
        &mut self,
        font: AtlasFont,
        mods: Option<&ModRegistry>,
        asset_server: &AssetServer,
        layouts: &mut Assets<TextureAtlasLayout>,
    ) -> Option<&AtlasFontData> {
        if !self.fonts.contains_key(&font) {
            let loaded = load_atlas_font(font, mods, asset_server, layouts)?;
            self.fonts.insert(font, loaded);
        }
        self.fonts.get(&font)
    }
}

#[derive(Component)]
pub struct AlphabetText {
    pub text: String,
    pub font: AtlasFont,
    pub alpha: f32,
    pub char_entities: Vec<Entity>,
}

#[derive(Component)]
pub struct AlphabetChar;

pub struct AlphabetPlugin;

impl Plugin for AlphabetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AlphabetFontCache>()
            .add_systems(Update, step_alphabet_animations);
    }
}

pub fn char_anim_prefix(ch: char) -> &'static str {
    match ch {
        '&' => "-andpersand-",
        '😠' => "-angry faic-",
        '\'' => "-apostraphie-",
        '\\' => "-back slash-",
        ',' => "-comma-",
        '-' => "-dash-",
        '↓' => "-down arrow-",
        '"' | '”' => "-end quote-",
        '!' => "-exclamation point-",
        '/' => "-forward slash-",
        '>' => "-greater than-",
        '♥' | '♡' => "-heart-",
        '←' => "-left arrow-",
        '<' => "-less than-",
        '*' => "-multiply x-",
        '.' => "-period-",
        '?' => "-question mark-",
        '→' => "-right arrow-",
        '“' => "-start quote-",
        '↑' => "-up arrow-",
        _ => {
            // Fallback handled by caller using char buffer when not a mapped symbol.
            ""
        }
    }
}

fn resolve_char_prefix<'a>(ch: char, buf: &'a mut [u8; 4]) -> &'a str {
    let mapped = char_anim_prefix(ch);
    if !mapped.is_empty() {
        return mapped;
    }
    ch.encode_utf8(buf)
}

fn load_atlas_font(
    font: AtlasFont,
    mods: Option<&ModRegistry>,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) -> Option<AtlasFontData> {
    let stem = font.asset_stem();
    let img_rel = format!("{stem}.png");
    let xml_rel = format!("{stem}.xml");

    let img_path = mods
        .and_then(|m| m.resolve_asset_server_path(&img_rel))
        .unwrap_or(img_rel);
    let xml_path: PathBuf = mods
        .and_then(|m| m.resolve_file(&xml_rel))
        .unwrap_or_else(|| resolve_assets_dir().join(&xml_rel));

    let sparrow = match SparrowAtlas::from_file(&xml_path) {
        Ok(atlas) => atlas,
        Err(err) => {
            warn!("Could not load font atlas '{}': {err}", xml_path.display());
            return None;
        }
    };

    let (raw_layout, indices) = sparrow.build_layout();
    let layout = layouts.add(raw_layout);
    let texture: Handle<Image> = asset_server.load(img_path);

    let mut max_height: f32 = 0.0;
    let mut contains_upper = false;
    let mut contains_lower = false;
    let mut grouped: HashMap<String, Vec<(String, usize, f32, f32)>> = HashMap::new();

    for frame in &sparrow.frames {
        let Some(&idx) = indices.get(&frame.name) else {
            continue;
        };
        let (w, h) = if frame.rotated {
            (frame.height as f32, frame.width as f32)
        } else {
            (frame.width as f32, frame.height as f32)
        };
        max_height = max_height.max(h);

        let prefix = if frame.name.len() > 4
            && frame.name.as_bytes()[frame.name.len() - 4..]
                .iter()
                .all(u8::is_ascii_digit)
        {
            &frame.name[..frame.name.len() - 4]
        } else {
            frame.name.as_str()
        };

        let mut prefix_chars = prefix.chars();
        if let (Some(first), None) = (prefix_chars.next(), prefix_chars.next()) {
            if first.is_ascii_uppercase() {
                contains_upper = true;
            } else if first.is_ascii_lowercase() {
                contains_lower = true;
            }
        }

        grouped
            .entry(prefix.to_string())
            .or_default()
            .push((frame.name.clone(), idx, w, h));
    }

    let case_allowed = match (contains_upper, contains_lower) {
        (true, false) => CaseAllowed::Upper,
        (false, true) => CaseAllowed::Lower,
        _ => CaseAllowed::Both,
    };

    let mut glyphs = HashMap::with_capacity(grouped.len());
    for (prefix, mut entries) in grouped {
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let width = entries.first().map_or(0.0, |e| e.2);
        let height = entries.first().map_or(0.0, |e| e.3);
        let frames = entries.into_iter().map(|e| e.1).collect();
        glyphs.insert(
            prefix,
            GlyphAnimData {
                frames,
                width,
                height,
            },
        );
    }

    Some(AtlasFontData {
        texture,
        layout,
        max_height,
        case_allowed,
        glyphs,
    })
}

/// Spawns an animated `AtlasText`-style sprite group positioned by its top-left corner in Bevy world space.
#[allow(clippy::too_many_arguments)]
pub fn spawn_alphabet_text(
    commands: &mut Commands,
    font_kind: AtlasFont,
    font: &AtlasFontData,
    text: &str,
    top_left_world: Vec3,
    scale: f32,
    alpha: f32,
    render_layers: Option<RenderLayers>,
) -> Entity {
    let cased = font.restrict_case(text);
    let alpha = alpha.clamp(0.0, 1.0);

    let mut char_entities = Vec::new();
    let mut root_cmd = commands.spawn((
        Transform::from_translation(top_left_world).with_scale(Vec3::splat(scale)),
        Visibility::Inherited,
    ));
    if let Some(ref layers) = render_layers {
        root_cmd.insert(layers.clone());
    }

    root_cmd.with_children(|parent| {
        let mut x_pos: f32 = 0.0;
        let mut y_pos: f32 = 0.0;
        let mut buf = [0u8; 4];

        for ch in cased.chars() {
            match ch {
                ' ' => x_pos += SPACE_ADVANCE_PX,
                '\n' => {
                    x_pos = 0.0;
                    y_pos += font.max_height;
                }
                _ => {
                    let prefix = resolve_char_prefix(ch, &mut buf);
                    let Some(glyph) = font.glyphs.get(prefix) else {
                        continue;
                    };
                    let Some(&first_idx) = glyph.frames.first() else {
                        continue;
                    };
                    let mut controller = AnimationController::default();
                    controller.add_anim("anim", glyph.frames.clone(), GLYPH_FPS, true);
                    controller.play("anim", true, false, 0);

                    let cx = x_pos + glyph.width * 0.5;
                    let cy = -(y_pos + font.max_height - glyph.height * 0.5);

                    let mut sprite = Sprite::from_atlas_image(
                        font.texture.clone(),
                        TextureAtlas {
                            layout: font.layout.clone(),
                            index: first_idx,
                        },
                    );
                    sprite.color = Color::srgba(1.0, 1.0, 1.0, alpha);

                    let mut child = parent.spawn((
                        sprite,
                        Transform::from_xyz(cx, cy, 0.0),
                        controller,
                        AlphabetChar,
                    ));
                    if let Some(ref layers) = render_layers {
                        child.insert(layers.clone());
                    }
                    char_entities.push(child.id());

                    x_pos += glyph.width;
                }
            }
        }
    });

    root_cmd.insert(AlphabetText {
        text: text.to_string(),
        font: font_kind,
        alpha,
        char_entities,
    });
    root_cmd.id()
}

pub fn set_alphabet_text_alpha<F: QueryFilter>(
    entity: Entity,
    alpha: f32,
    texts: &mut Query<&mut AlphabetText>,
    chars: &mut Query<&mut Sprite, F>,
) {
    let alpha = alpha.clamp(0.0, 1.0);
    if let Ok(mut meta) = texts.get_mut(entity) {
        if (meta.alpha - alpha).abs() <= 1e-4 {
            return;
        }
        meta.alpha = alpha;
        for &child in &meta.char_entities {
            if let Ok(mut sprite) = chars.get_mut(child) {
                sprite.color = Color::srgba(1.0, 1.0, 1.0, alpha);
            }
        }
    }
}

pub fn step_alphabet_animations(
    time: Res<Time>,
    mut query: Query<(&mut AnimationController, &mut Sprite), With<AlphabetChar>>,
) {
    let elapsed_s = time.delta_secs();
    if elapsed_s <= 0.0 {
        return;
    }
    for (mut controller, mut sprite) in &mut query {
        controller.update(elapsed_s, None);
        if let Some(idx) = controller.current_atlas_index() {
            set_atlas_index(&mut sprite, idx);
        }
    }
}
