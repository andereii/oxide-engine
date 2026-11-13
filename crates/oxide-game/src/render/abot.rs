//! `ABotSpeaker` speaker assembly for `nene*` and `otis-speaker` characters.
//!
//! Matches native `nene.hxc`, `nene-christmas.hxc`, `nene-tankmen.hxc`,
//! `nene-dark.hxc`, `otis-speaker.hxc`, `ABotAtlasSprite.hxc`,
//! `ABotVisualizer.hxc`, and `ABotVis.hx` (`docs/M6-CONTRACT.md` §18).

use oxide_vfs::Vfs;

use super::RenderError;
use super::geometry::{Camera, DANCE_STEPS_PER_BEAT, Vec2};
use super::load::{
    AtlasSheet, LoadedCharacter, PageStore, SpriteFrame, load_atlas_sheet, load_sparrow_sheet,
};
use super::scene::{
    CharacterRig, DrawnQuad, camera_projection_matrix, clipped_world_aabb, concat,
    prop_world_matrix, quad_from_matrix, sprite_frame_matrix,
};
use super::shaders::{Hsbc, LayerShader, ShaderMaskPages, character_shader};

const EYE_WHITES_SIZE: [f64; 2] = [160.0, 60.0];
const EYE_WHITES_OFFSET: Vec2 = [40.0, 250.0];
const STEREO_BG_OFFSET: Vec2 = [150.0, 30.0];
const PUPIL_OFFSET: Vec2 = [50.0, 238.0];
const VIZ_ORIGIN_OFFSET: Vec2 = [207.0, 84.0];

/// Native cumulative bar offsets in `ABotVis.hx:30-49` (`draw_x = [0, 59, 56, 66, 54, 52, 51]`,
/// `draw_y = [0, -8, -3.5, -0.4, 0.5, 4.7, 7]`).
const VIZ_BAR_OFFSETS: [Vec2; 7] = [
    [0.0, 0.0],
    [59.0, -8.0],
    [115.0, -11.5],
    [181.0, -11.9],
    [235.0, -11.4],
    [287.0, -6.7],
    [338.0, 0.3],
];

const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const PUPIL_LEFT_END_FRAME: usize = 16;
const PUPIL_RIGHT_START_FRAME: usize = 17;
const PUPIL_RIGHT_END_FRAME: usize = 30;
const UNIT_UV: [[f64; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

/// Supported Animate-Atlas A-Bot speaker character variants (`M6-CONTRACT` §18).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ABotVariant {
    Nene,
    NeneChristmas,
    NeneTankmen,
    NeneDark,
    OtisSpeaker,
}

impl ABotVariant {
    pub(crate) fn from_character_id(id: &str) -> Option<Self> {
        match id {
            "nene" => Some(Self::Nene),
            "nene-christmas" => Some(Self::NeneChristmas),
            "nene-tankmen" => Some(Self::NeneTankmen),
            "nene-dark" => Some(Self::NeneDark),
            "otis-speaker" => Some(Self::OtisSpeaker),
            _ => None,
        }
    }

    fn root_offset(self) -> Vec2 {
        match self {
            Self::Nene | Self::NeneChristmas | Self::NeneTankmen | Self::NeneDark => [-95.0, 384.0],
            Self::OtisSpeaker => [170.0, 455.0],
        }
    }

    fn body_asset_path(self) -> &'static str {
        match self {
            Self::NeneDark => "characters/abot/dark/abotSystem",
            _ => "characters/abot/abotSystem",
        }
    }

    fn uses_note_kind_pupils(self) -> bool {
        !matches!(self, Self::OtisSpeaker)
    }

    fn starts_viz_visible(self) -> bool {
        matches!(self, Self::OtisSpeaker)
    }

    fn part_style(
        self,
        part: ABotPart,
        parent_shader: LayerShader,
        stage_id: &str,
    ) -> ([f32; 4], LayerShader) {
        match self {
            Self::Nene | Self::NeneChristmas => match part {
                ABotPart::Pupil => (WHITE, LayerShader::None),
                ABotPart::Body if stage_id == "phillyBlazin" => {
                    (srgb_u8(0x88, 0x88, 0x88), parent_shader)
                }
                _ => (WHITE, parent_shader),
            },
            Self::OtisSpeaker | Self::NeneTankmen => match part {
                ABotPart::EyeWhites => (WHITE, LayerShader::None),
                ABotPart::StereoBg | ABotPart::Pupil | ABotPart::Body => (
                    WHITE,
                    LayerShader::AdjustColor(Hsbc::new(-40.0, -40.0, -25.0, -20.0)),
                ),
                ABotPart::Viz => (
                    WHITE,
                    LayerShader::AdjustColor(Hsbc::new(-12.0, -30.0, 0.0, -10.0)),
                ),
            },
            Self::NeneDark => match part {
                ABotPart::EyeWhites => (srgb_u8(0x6F, 0x96, 0xCE), LayerShader::None),
                ABotPart::StereoBg => (srgb_u8(0x61, 0x67, 0x85), LayerShader::None),
                ABotPart::Pupil | ABotPart::Body => (WHITE, LayerShader::None),
                ABotPart::Viz => (
                    WHITE,
                    LayerShader::AdjustColor(Hsbc::new(-12.0, -26.0, 0.0, -45.0)),
                ),
            },
        }
    }
}

/// Ordered z-layers forming one `ABotSpeaker` unit (`char_z - 20 .. char_z - 10`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ABotPart {
    EyeWhites,
    StereoBg,
    Pupil,
    Viz,
    Body,
}

impl ABotPart {
    pub(crate) const ALL: [Self; 5] = [
        Self::EyeWhites,
        Self::StereoBg,
        Self::Pupil,
        Self::Viz,
        Self::Body,
    ];

    fn z_offset(self) -> f64 {
        match self {
            Self::EyeWhites => -20.0,
            Self::StereoBg => -18.0,
            Self::Pupil => -15.0,
            Self::Viz => -11.0,
            Self::Body => -10.0,
        }
    }
}

/// Loaded textures, sheets, and variant metadata for one speaker's A-Bot.
#[derive(Debug, Clone)]
pub(crate) struct LoadedABot {
    pub(crate) speaker_index: usize,
    pub(crate) variant: ABotVariant,
    pub(crate) eye_whites_page: usize,
    pub(crate) stereo_page: usize,
    pub(crate) stereo_size: [f64; 2],
    pub(crate) pupil: AtlasSheet,
    pub(crate) body: AtlasSheet,
    pub(crate) viz_page: usize,
    pub(crate) viz_page_size: [f64; 2],
    pub(crate) viz_bars: [Vec<SpriteFrame>; 7],
}

#[derive(Debug, Clone, Copy)]
struct TimelinePlayback {
    start_frame: usize,
    end_frame: usize,
    elapsed_seconds: f64,
    playing: bool,
}

impl TimelinePlayback {
    const fn paused_at(frame: usize) -> Self {
        Self {
            start_frame: frame,
            end_frame: frame,
            elapsed_seconds: 0.0,
            playing: false,
        }
    }

    fn play_range(&mut self, start_frame: usize, end_frame: usize) {
        self.start_frame = start_frame;
        self.end_frame = end_frame.max(start_frame);
        self.elapsed_seconds = 0.0;
        self.playing = self.start_frame < self.end_frame;
    }

    fn advance(&mut self, dt_seconds: f64, fps: f64) {
        if !self.playing {
            return;
        }
        self.elapsed_seconds += dt_seconds;
        if self.raw_frame(fps) >= self.end_frame {
            self.playing = false;
        }
    }

    fn raw_frame(&self, fps: f64) -> usize {
        self.start_frame + ((self.elapsed_seconds * fps).floor().max(0.0)) as usize
    }

    fn frame(&self, fps: f64) -> usize {
        if !self.playing {
            return self.end_frame;
        }
        self.raw_frame(fps).min(self.end_frame)
    }
}

/// Runtime A-Bot speaker unit driven on `Scene` ticks and events.
pub(crate) struct ABotRuntime {
    variant: ABotVariant,
    stage_id: String,
    speaker_dance_every: f64,
    parent_shader: LayerShader,
    speaker_z: f64,
    root: Vec2,
    scroll: Vec2,
    assets: LoadedABot,
    body_fps: f64,
    pupil_fps: f64,
    body_anim: TimelinePlayback,
    pupil_anim: TimelinePlayback,
    viz_levels: [f64; 7],
    song_started: bool,
}

impl ABotRuntime {
    pub(crate) fn new(
        assets: LoadedABot,
        speaker: &CharacterRig,
        stage_id: &str,
        masks: &ShaderMaskPages,
    ) -> Self {
        let variant = assets.variant;
        let offset = variant.root_offset();
        let anchor = speaker.anchor();
        let root = [
            anchor[0] + speaker.data_offsets.0 * speaker.scale + offset[0],
            anchor[1] + speaker.data_offsets.1 * speaker.scale + offset[1],
        ];
        let parent_shader = if matches!(variant, ABotVariant::Nene | ABotVariant::NeneChristmas) {
            character_shader(stage_id, speaker.role, &speaker.id, masks)
        } else {
            LayerShader::None
        };
        let body_fps = assets.body.stage.frame_rate().max(1) as f64;
        let pupil_fps = assets.pupil.stage.frame_rate().max(1) as f64;
        Self {
            variant,
            stage_id: stage_id.to_string(),
            speaker_dance_every: speaker.dance_every(),
            parent_shader,
            speaker_z: speaker.z_index,
            root,
            // Native A-Bot `FunkinSprite` children keep the default `(1.0, 1.0)` scroll factor.
            scroll: [1.0, 1.0],
            song_started: variant.starts_viz_visible(),
            assets,
            body_fps,
            pupil_fps,
            body_anim: TimelinePlayback::paused_at(0),
            pupil_anim: TimelinePlayback::paused_at(0),
            viz_levels: [0.0; 7],
        }
    }

    pub(crate) fn z_index(&self, part: ABotPart) -> f64 {
        self.speaker_z + part.z_offset()
    }

    pub(crate) fn layer_setup(
        &self,
        part: ABotPart,
    ) -> (Vec<Option<usize>>, [f32; 4], LayerShader) {
        let page = match part {
            ABotPart::EyeWhites => self.assets.eye_whites_page,
            ABotPart::StereoBg => self.assets.stereo_page,
            ABotPart::Pupil => self.assets.pupil.page,
            ABotPart::Viz => self.assets.viz_page,
            ABotPart::Body => self.assets.body.page,
        };
        let (color, shader) = self
            .variant
            .part_style(part, self.parent_shader, &self.stage_id);
        (vec![Some(page)], color, shader)
    }

    pub(crate) fn on_step(&mut self, step: i32) {
        if !self.song_started {
            self.song_started = true;
        }
        if self.speaker_dance_every > 0.0
            && (step as f64 % (self.speaker_dance_every * DANCE_STEPS_PER_BEAT)) == 0.0
        {
            let last_body = self.assets.body.stage.frame_count().saturating_sub(1);
            if last_body >= 1 {
                self.body_anim.play_range(1, last_body);
            }
        }
    }

    pub(crate) fn on_camera_focus(&mut self, target_char: i64) {
        match target_char {
            0 => self.move_pupils_right(),
            1 => self.move_pupils_left(),
            _ => {}
        }
    }

    pub(crate) fn on_note_kind(&mut self, kind: &str) {
        if !self.variant.uses_note_kind_pupils() {
            return;
        }
        match kind {
            "weekend-1-lightcan" => self.move_pupils_left(),
            "weekend-1-cockgun" => self.move_pupils_right(),
            _ => {}
        }
    }

    pub(crate) fn update(&mut self, dt_seconds: f64) {
        self.body_anim.advance(dt_seconds, self.body_fps);
        self.pupil_anim.advance(dt_seconds, self.pupil_fps);
    }

    fn move_pupils_left(&mut self) {
        self.pupil_anim.play_range(0, PUPIL_LEFT_END_FRAME);
    }

    fn move_pupils_right(&mut self) {
        self.pupil_anim
            .play_range(PUPIL_RIGHT_START_FRAME, PUPIL_RIGHT_END_FRAME);
    }

    #[cfg(test)]
    pub(crate) fn body_frame(&self) -> usize {
        self.body_anim.frame(self.body_fps)
    }

    #[cfg(test)]
    pub(crate) fn pupil_frame(&self) -> usize {
        self.pupil_anim.frame(self.pupil_fps)
    }

    #[cfg(test)]
    pub(crate) fn root(&self) -> Vec2 {
        self.root
    }

    #[cfg(test)]
    pub(crate) fn set_viz_levels(&mut self, levels: [f64; 7]) {
        self.viz_levels = levels;
    }

    pub(crate) fn draw_part(
        &self,
        camera: &Camera,
        part: ABotPart,
    ) -> Result<Vec<DrawnQuad>, RenderError> {
        let projection = camera_projection_matrix(camera, self.scroll);
        match part {
            ABotPart::EyeWhites => {
                let pos = [
                    self.root[0] + EYE_WHITES_OFFSET[0],
                    self.root[1] + EYE_WHITES_OFFSET[1],
                ];
                let world = [1.0, 0.0, 0.0, 1.0, pos[0], pos[1]];
                Ok(vec![DrawnQuad {
                    page: Some(self.assets.eye_whites_page),
                    quad: quad_from_matrix(
                        concat(projection, world),
                        EYE_WHITES_SIZE[0],
                        EYE_WHITES_SIZE[1],
                        UNIT_UV,
                    ),
                    clip: None,
                }])
            }
            ABotPart::StereoBg => {
                let pos = [
                    self.root[0] + STEREO_BG_OFFSET[0],
                    self.root[1] + STEREO_BG_OFFSET[1],
                ];
                let world = [1.0, 0.0, 0.0, 1.0, pos[0], pos[1]];
                Ok(vec![DrawnQuad {
                    page: Some(self.assets.stereo_page),
                    quad: quad_from_matrix(
                        concat(projection, world),
                        self.assets.stereo_size[0],
                        self.assets.stereo_size[1],
                        UNIT_UV,
                    ),
                    clip: None,
                }])
            }
            ABotPart::Pupil => {
                let pos = [
                    self.root[0] + PUPIL_OFFSET[0],
                    self.root[1] + PUPIL_OFFSET[1],
                ];
                draw_atlas_frame(
                    &self.assets.pupil,
                    self.pupil_anim.frame(self.pupil_fps),
                    pos,
                    projection,
                    "abot:systemEyes",
                )
            }
            ABotPart::Viz => Ok(self.draw_viz(projection)),
            ABotPart::Body => draw_atlas_frame(
                &self.assets.body,
                self.body_anim.frame(self.body_fps),
                self.root,
                projection,
                "abot:abotSystem",
            ),
        }
    }

    fn draw_viz(&self, projection: [f64; 6]) -> Vec<DrawnQuad> {
        if !self.song_started {
            return Vec::new();
        }
        let origin = [
            self.root[0] + VIZ_ORIGIN_OFFSET[0],
            self.root[1] + VIZ_ORIGIN_OFFSET[1],
        ];
        let mut out = Vec::with_capacity(7);
        for (index, frames) in self.assets.viz_bars.iter().enumerate() {
            // `ABotVis.hx:145-162`: `animFrame == 0` hides the bar (`visible = animFrame > 0`).
            let level = self.viz_levels[index].clamp(0.0, 1.0);
            let anim_frame = (level * 6.0).round() as usize;
            if anim_frame == 0 {
                continue;
            }
            let frame_index = 5usize.saturating_sub(anim_frame.saturating_sub(1).min(5));
            let Some(frame) = frames.get(frame_index).or_else(|| frames.last()) else {
                continue;
            };
            let hitbox = frames.first().map_or([0, 0], |base| base.dest_size);
            let pos = [
                origin[0] + VIZ_BAR_OFFSETS[index][0],
                origin[1] + VIZ_BAR_OFFSETS[index][1],
            ];
            let (matrix, uv) = sprite_frame_matrix(
                pos,
                frame,
                [1.0, 1.0],
                hitbox,
                false,
                false,
                0.0,
                self.assets.viz_page_size,
            );
            out.push(DrawnQuad {
                page: Some(self.assets.viz_page),
                quad: quad_from_matrix(
                    concat(projection, matrix),
                    frame.rect[2] as f64,
                    frame.rect[3] as f64,
                    uv,
                ),
                clip: None,
            });
        }
        out
    }
}

fn draw_atlas_frame(
    sheet: &AtlasSheet,
    timeline_frame: usize,
    top_left: Vec2,
    projection: [f64; 6],
    label: &str,
) -> Result<Vec<DrawnQuad>, RenderError> {
    let quads = sheet
        .stage
        .resolve_main_frame_with_clips(timeline_frame)
        .map_err(|source| RenderError::AtlasStage {
            path: label.to_string(),
            source,
        })?;
    let world = prop_world_matrix(sheet.bounds, [1.0, 1.0], false, false, top_left, [0.0, 0.0]);
    let clip_space = concat(projection, world);
    let mut out = Vec::with_capacity(quads.len());
    for resolved in &quads {
        let quad = &resolved.quad;
        let [_, _, width, height] = quad.atlas_rect;
        let drawn = quad_from_matrix(
            concat(clip_space, quad.matrix),
            width as f64,
            height as f64,
            super::scene::quad_uv(quad.atlas_rect, sheet.page_size),
        );
        let clip = match resolved.clip {
            None => None,
            Some(rect) => match clipped_world_aabb(clip_space, rect, &drawn) {
                Some(clip) => Some(clip),
                None => continue,
            },
        };
        out.push(DrawnQuad {
            page: Some(sheet.page),
            quad: drawn,
            clip,
        });
    }
    Ok(out)
}

/// Loads `ABotSpeaker` assets when any character in `characters` is a matching
/// Animate-Atlas speaker variant (`M6-CONTRACT` §18).
pub(crate) fn load_abot_speaker(
    vfs: &Vfs,
    pages: &mut PageStore,
    characters: &[LoadedCharacter],
    level_dir: Option<&str>,
) -> Result<Option<LoadedABot>, RenderError> {
    let Some((speaker_index, variant)) =
        characters
            .iter()
            .enumerate()
            .find_map(|(index, character)| {
                ABotVariant::from_character_id(&character.id).map(|variant| (index, variant))
            })
    else {
        return Ok(None);
    };

    let eye_whites_page = pages.add_solid_white()?;
    let (stereo_page, stereo_size) =
        pages.load_image_key(vfs, "characters/abot/stereoBG", level_dir, false)?;
    let pupil = load_atlas_sheet(vfs, pages, "characters/abot/systemEyes", level_dir, false)?;
    if pupil.stage.frame_count() <= PUPIL_RIGHT_END_FRAME {
        return Err(RenderError::Unsupported {
            detail: format!(
                "characters/abot/systemEyes requires at least {} frames, got {}",
                PUPIL_RIGHT_END_FRAME + 1,
                pupil.stage.frame_count()
            ),
        });
    }

    let viz_sheet = load_sparrow_sheet(vfs, pages, "characters/abot/aBotViz", level_dir, false)?;
    let viz_page = viz_sheet.page;
    let viz_page_size = viz_sheet.page_size;
    let viz_bars = extract_viz_bars(&viz_sheet)?;

    let body = load_atlas_sheet(vfs, pages, variant.body_asset_path(), level_dir, false)?;
    if body.stage.frame_count() < 2 {
        return Err(RenderError::Unsupported {
            detail: format!(
                "{} requires at least 2 frames, got {}",
                variant.body_asset_path(),
                body.stage.frame_count()
            ),
        });
    }

    Ok(Some(LoadedABot {
        speaker_index,
        variant,
        eye_whites_page,
        stereo_page,
        stereo_size,
        pupil,
        body,
        viz_page,
        viz_page_size,
        viz_bars,
    }))
}

fn extract_viz_bars(
    sheet: &super::load::SpriteSheet,
) -> Result<[Vec<SpriteFrame>; 7], RenderError> {
    let mut bars: [Vec<SpriteFrame>; 7] = Default::default();
    for (bar_idx, slot) in bars.iter_mut().enumerate() {
        let prefix = format!("viz{}0", bar_idx + 1);
        let mut matched: Vec<(&str, SpriteFrame)> = sheet
            .prefix_indices(&prefix)
            .into_iter()
            .filter_map(|index| {
                let (name, frame) = sheet.frames.get(index)?;
                Some((name.as_str(), *frame))
            })
            .collect();
        matched.sort_by(|left, right| left.0.cmp(right.0));
        if matched.is_empty() {
            return Err(RenderError::MissingFrame {
                path: "characters/abot/aBotViz".to_string(),
                prefix,
            });
        }
        *slot = matched.into_iter().map(|(_, frame)| frame).collect();
    }
    Ok(bars)
}

fn srgb_u8(r: u8, g: u8, b: u8) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeMap;

    use oxide_atlas::AnimateStage;

    use super::*;
    use crate::render::hud::IconConfig;
    use crate::render::load::{LoadedCharacter, LoadedSlot, Role, Sheet};

    const SPRITEMAP_JSON: &str = r#"{
        "ATLAS": {
            "SPRITES": [
                {"SPRITE": {"name": "0000", "x": 0, "y": 0, "w": 100, "h": 40, "rotated": false}},
                {"SPRITE": {"name": "0001", "x": 0, "y": 40, "w": 80, "h": 24, "rotated": false}}
            ]
        },
        "meta": {"size": {"w": 128, "h": 128}}
    }"#;

    pub(crate) fn verbose_stage(frame_count: usize, dx_per_frame: f64) -> AtlasSheet {
        let mut frames_json = String::new();
        for i in 0..frame_count {
            if i > 0 {
                frames_json.push(',');
            }
            let sprite = if i >= 16 { "0001" } else { "0000" };
            let tx = (i as f64) * dx_per_frame;
            frames_json.push_str(&format!(
                r#"{{"index":{i},"duration":1,"elements":[{{"ATLAS_SPRITE_instance":{{"name":"{sprite}","Matrix3D":{{"m00":1.0,"m01":0.0,"m02":0.0,"m03":0.0,"m10":0.0,"m11":1.0,"m12":0.0,"m13":0.0,"m20":0.0,"m21":0.0,"m22":1.0,"m23":0.0,"m30":{tx},"m31":0.0,"m32":0.0,"m33":1.0}}}}}}]}}"#
            ));
        }
        let anim_json = format!(
            r#"{{"ANIMATION":{{"TIMELINE":{{"LAYERS":[{{"Layer_name":"L","Frames":[{frames_json}]}}]}}}},"metadata":{{"framerate":24.0}}}}"#
        );
        let stage =
            AnimateStage::parse(&anim_json, SPRITEMAP_JSON).expect("parse verbose test stage");
        let bounds = stage.bounds();
        AtlasSheet {
            stage,
            bounds,
            symbol_bounds: BTreeMap::new(),
            page: 9,
            page_size: [128.0, 128.0],
        }
    }

    pub(crate) fn test_loaded_abot(variant: ABotVariant) -> LoadedABot {
        let mut viz_bars: [Vec<SpriteFrame>; 7] = Default::default();
        for (i, slot) in viz_bars.iter_mut().enumerate() {
            for frame_no in 0..6 {
                slot.push(SpriteFrame {
                    page: 4,
                    rect: [i as i32 * 10, frame_no * 20, 50, 180 - frame_no * 20],
                    dest_offset: [0, frame_no * 20],
                    dest_size: [50, 180],
                    rotated: false,
                });
            }
        }
        LoadedABot {
            speaker_index: 2,
            variant,
            eye_whites_page: 1,
            stereo_page: 2,
            stereo_size: [480.0, 300.0],
            pupil: verbose_stage(32, 1.0),
            body: verbose_stage(16, 0.5),
            viz_page: 4,
            viz_page_size: [512.0, 512.0],
            viz_bars,
        }
    }

    fn test_speaker_rig(
        id: &str,
        pos: (f64, f64),
        offsets: (f64, f64),
        z_index: f64,
    ) -> CharacterRig {
        let sheet = verbose_stage(16, 0.0);
        let loaded = LoadedCharacter {
            role: Role::Girlfriend,
            id: id.to_string(),
            sheets: vec![Sheet::Atlas(Box::new(sheet))],
            idle_size: [200.0, 400.0],
            sprite_hitbox: None,
            animations: BTreeMap::new(),
            data_offsets: offsets,
            camera_offsets: (0.0, 0.0),
            sing_time_steps: 4.0,
            dance_every: 1.0,
            scale: 1.0,
            flip_x: false,
            starting_animation: None,
            slot: LoadedSlot {
                z_index,
                position: pos,
                scale: 1.0,
                camera_offsets: (0.0, 0.0),
                scroll: (1.0, 1.0),
            },
            health_icon: IconConfig {
                id: id.to_string(),
                is_pixel: false,
                should_bop: false,
                scale: 1.0,
                flip_x: false,
                offsets: [0.0, 0.0],
            },
        };
        CharacterRig::from_loaded(loaded)
    }

    #[test]
    fn sublayer_z_indices_sort_behind_speaker_and_behind_tank_bricks() {
        let rig = test_speaker_rig("otis-speaker", (800.5, 775.0), (-13.0, -132.0), 100.0);
        let runtime = ABotRuntime::new(
            test_loaded_abot(ABotVariant::OtisSpeaker),
            &rig,
            "tankmanBattlefieldErect",
            &ShaderMaskPages::default(),
        );
        let zs: Vec<f64> = ABotPart::ALL
            .into_iter()
            .map(|part| runtime.z_index(part))
            .collect();
        assert_eq!(zs, vec![80.0, 82.0, 85.0, 89.0, 90.0]);
        assert!(zs.last().copied().unwrap_or(0.0) < rig.z_index);
        assert!(rig.z_index < 101.0); // tankBricks zIndex=101
    }

    #[test]
    fn anchor_math_places_abot_under_nene_and_otis() {
        let nene = test_speaker_rig("nene", (1520.0, 1080.0), (4.0, -89.0), 300.0);
        let nene_anchor = nene.anchor();
        let nene_bot = ABotRuntime::new(
            test_loaded_abot(ABotVariant::Nene),
            &nene,
            "phillyStreetsErect",
            &ShaderMaskPages::default(),
        );
        assert_eq!(
            nene_bot.root(),
            [nene_anchor[0] + 4.0 - 95.0, nene_anchor[1] - 89.0 + 384.0]
        );

        let otis = test_speaker_rig("otis-speaker", (800.5, 775.0), (-13.0, -132.0), 100.0);
        let otis_anchor = otis.anchor();
        let otis_bot = ABotRuntime::new(
            test_loaded_abot(ABotVariant::OtisSpeaker),
            &otis,
            "tankmanBattlefieldErect",
            &ShaderMaskPages::default(),
        );
        assert_eq!(
            otis_bot.root(),
            [
                otis_anchor[0] - 13.0 + 170.0,
                otis_anchor[1] - 132.0 + 455.0
            ]
        );
    }

    #[test]
    fn body_dances_on_beats_and_pupils_track_focus_and_note_kinds() {
        let rig = test_speaker_rig("nene", (1520.0, 1080.0), (4.0, -89.0), 300.0);
        let mut nene = ABotRuntime::new(
            test_loaded_abot(ABotVariant::Nene),
            &rig,
            "phillyStreetsErect",
            &ShaderMaskPages::default(),
        );
        assert_eq!(nene.body_frame(), 0);
        assert_eq!(nene.pupil_frame(), 0);

        nene.on_step(0);
        assert_eq!(nene.body_frame(), 1);
        for _ in 0..700 {
            nene.update(0.001);
        }
        assert_eq!(nene.body_frame(), 15);
        nene.on_step(1);
        assert_eq!(nene.body_frame(), 15);
        nene.on_step(4);
        assert_eq!(nene.body_frame(), 1);

        // FocusCamera char 1 -> plays 0..=16, pauses at 16
        nene.on_camera_focus(1);
        assert_eq!(nene.pupil_frame(), 0);
        for _ in 0..750 {
            nene.update(0.001);
        }
        assert_eq!(nene.pupil_frame(), 16);

        // Unmapped char leaves pupils unchanged
        nene.on_camera_focus(2);
        assert_eq!(nene.pupil_frame(), 16);

        // Note kinds on nene*
        nene.on_note_kind("weekend-1-cockgun");
        assert_eq!(nene.pupil_frame(), 17);
        for _ in 0..600 {
            nene.update(0.001);
        }
        assert_eq!(nene.pupil_frame(), 30);

        nene.on_note_kind("weekend-1-lightcan");
        assert_eq!(nene.pupil_frame(), 0);

        // otis-speaker ignores weekend-1 note kinds
        let otis_rig = test_speaker_rig("otis-speaker", (800.5, 775.0), (-13.0, -132.0), 100.0);
        let mut otis = ABotRuntime::new(
            test_loaded_abot(ABotVariant::OtisSpeaker),
            &otis_rig,
            "tankmanBattlefieldErect",
            &ShaderMaskPages::default(),
        );
        otis.on_note_kind("weekend-1-lightcan");
        assert_eq!(otis.pupil_frame(), 0);
        otis.on_camera_focus(1);
        for _ in 0..750 {
            otis.update(0.001);
        }
        assert_eq!(otis.pupil_frame(), 16);
    }

    #[test]
    fn variant_shaders_and_tints_match_native_scripts() {
        let masks = ShaderMaskPages::default();

        let nene_rig = test_speaker_rig("nene", (0.0, 0.0), (0.0, 0.0), 300.0);
        let nene = ABotRuntime::new(
            test_loaded_abot(ABotVariant::Nene),
            &nene_rig,
            "phillyStreetsErect",
            &masks,
        );
        let expected_nene = LayerShader::AdjustColor(Hsbc::new(-20.0, -5.0, -25.0, -40.0));
        assert_eq!(nene.layer_setup(ABotPart::EyeWhites).2, expected_nene);
        assert_eq!(nene.layer_setup(ABotPart::StereoBg).2, expected_nene);
        assert_eq!(nene.layer_setup(ABotPart::Viz).2, expected_nene);
        assert_eq!(nene.layer_setup(ABotPart::Body).2, expected_nene);
        assert_eq!(nene.layer_setup(ABotPart::Pupil).2, LayerShader::None);

        let blazin = ABotRuntime::new(
            test_loaded_abot(ABotVariant::Nene),
            &nene_rig,
            "phillyBlazin",
            &masks,
        );
        assert_eq!(
            blazin.layer_setup(ABotPart::Body).1,
            srgb_u8(0x88, 0x88, 0x88)
        );

        let otis_rig = test_speaker_rig("otis-speaker", (0.0, 0.0), (0.0, 0.0), 100.0);
        let otis = ABotRuntime::new(
            test_loaded_abot(ABotVariant::OtisSpeaker),
            &otis_rig,
            "tankmanBattlefieldErect",
            &masks,
        );
        let expected_otis = LayerShader::AdjustColor(Hsbc::new(-40.0, -40.0, -25.0, -20.0));
        let expected_otis_viz = LayerShader::AdjustColor(Hsbc::new(-12.0, -30.0, 0.0, -10.0));
        assert_eq!(otis.layer_setup(ABotPart::EyeWhites).2, LayerShader::None);
        assert_eq!(otis.layer_setup(ABotPart::StereoBg).2, expected_otis);
        assert_eq!(otis.layer_setup(ABotPart::Pupil).2, expected_otis);
        assert_eq!(otis.layer_setup(ABotPart::Body).2, expected_otis);
        assert_eq!(otis.layer_setup(ABotPart::Viz).2, expected_otis_viz);

        let dark_rig = test_speaker_rig("nene-dark", (0.0, 0.0), (0.0, 0.0), 300.0);
        let dark = ABotRuntime::new(
            test_loaded_abot(ABotVariant::NeneDark),
            &dark_rig,
            "spookyMansionErect",
            &masks,
        );
        assert_eq!(
            dark.layer_setup(ABotPart::EyeWhites).1,
            srgb_u8(0x6F, 0x96, 0xCE)
        );
        assert_eq!(
            dark.layer_setup(ABotPart::StereoBg).1,
            srgb_u8(0x61, 0x67, 0x85)
        );
        assert_eq!(dark.layer_setup(ABotPart::Body).2, LayerShader::None);
        assert_eq!(
            dark.layer_setup(ABotPart::Viz).2,
            LayerShader::AdjustColor(Hsbc::new(-12.0, -26.0, 0.0, -45.0))
        );
        assert_eq!(
            ABotVariant::NeneDark.body_asset_path(),
            "characters/abot/dark/abotSystem"
        );
        assert!(BTreeMap::<String, ()>::new().is_empty());
    }

    #[test]
    fn headless_viz_hides_zero_level_bars_and_positions_active_bars() {
        let rig = test_speaker_rig("nene", (1520.0, 1080.0), (4.0, -89.0), 300.0);
        let mut nene = ABotRuntime::new(
            test_loaded_abot(ABotVariant::Nene),
            &rig,
            "phillyStreetsErect",
            &ShaderMaskPages::default(),
        );
        let camera = Camera::new(1.0);
        assert!(
            nene.draw_part(&camera, ABotPart::Viz)
                .expect("draw viz")
                .is_empty()
        );
        nene.on_step(0);
        assert!(
            nene.draw_part(&camera, ABotPart::Viz)
                .expect("draw zero-level viz after start")
                .is_empty()
        );

        nene.set_viz_levels([1.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0]);
        let quads = nene
            .draw_part(&camera, ABotPart::Viz)
            .expect("draw active viz");
        assert_eq!(quads.len(), 2);
    }
}
