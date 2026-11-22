use std::collections::VecDeque;

use bevy::audio::{AudioPlayer, AudioSink, PlaybackMode, PlaybackSettings, Volume};
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use serde::Deserialize;

use crate::animations::{
    AnimationController, BaseAnimationData, SparrowAtlas, add_sparrow_animations,
    register_default_sparrow_idle,
};
use crate::input::{Action, Controls};
use crate::mods::ModRegistry;
use crate::play::camera::hud::hud_render_layer;
use crate::play::playlist::PlayStatePlaylist;
use crate::play::scene::PlayState;
use crate::play::types::PlayPhase;
use crate::scripting::{HookKind, ScriptEvent, parse_hex_rgba};
use crate::ui::SoundTrayState;
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::utils::ecs::{set_atlas_index, set_visibility};

const DIALOGUE_BACKDROP_Z: f32 = 80.0;
const DIALOGUE_SPEAKER_Z: f32 = 81.0;
const DIALOGUE_BOX_Z: f32 = 82.0;
const DIALOGUE_TEXT_Z: f32 = 83.0;
const CHAR_TYPE_INTERVAL_S: f64 = 0.04;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConversationData {
    #[serde(default)]
    pub backdrop: Option<ConversationBackdropData>,
    #[serde(default)]
    pub music: Option<ConversationMusicData>,
    #[serde(default)]
    pub outro: Option<ConversationOutroData>,
    #[serde(default)]
    pub dialogue: Vec<DialogueEntryData>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConversationBackdropData {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub fade_time: f64,
    #[serde(default)]
    pub color: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMusicData {
    #[serde(default)]
    pub asset: String,
    #[serde(default)]
    pub fade_time: f64,
    #[serde(default)]
    pub looped: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConversationOutroData {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub fade_time: f64,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DialogueEntryData {
    #[serde(default)]
    pub speaker: String,
    #[serde(default)]
    pub speaker_animation: String,
    #[serde(rename = "box", default)]
    pub box_id: String,
    #[serde(default)]
    pub box_animation: String,
    #[serde(default)]
    pub text: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DialogueAnimDef {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub offsets: Vec<f32>,
    #[serde(default = "default_fps")]
    pub frame_rate: f64,
    #[serde(default)]
    pub looped: bool,
    #[serde(default)]
    pub frame_indices: Vec<usize>,
}

fn default_fps() -> f64 {
    24.0
}

fn default_scale() -> f32 {
    1.0
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DialogueBoxTextDef {
    #[serde(default)]
    pub offsets: Vec<f32>,
    #[serde(default)]
    pub width: f32,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub font_family: String,
    #[serde(default)]
    pub shadow_color: String,
    #[serde(default)]
    pub shadow_width: f32,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DialogueBoxData {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub asset_path: String,
    #[serde(default)]
    pub is_pixel: bool,
    #[serde(default)]
    pub flip_x: bool,
    #[serde(default)]
    pub flip_y: bool,
    #[serde(default)]
    pub offsets: Vec<f32>,
    #[serde(default = "default_scale")]
    pub scale: f32,
    #[serde(default)]
    pub text: DialogueBoxTextDef,
    #[serde(default)]
    pub animations: Vec<DialogueAnimDef>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerData {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub asset_path: String,
    #[serde(default)]
    pub flip_x: bool,
    #[serde(default)]
    pub is_pixel: bool,
    #[serde(default = "default_scale")]
    pub scale: f32,
    #[serde(default)]
    pub offsets: Vec<f32>,
    #[serde(default)]
    pub animations: Vec<DialogueAnimDef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DialoguePhase {
    #[default]
    Inactive,
    Opening,
    Speaking,
    Idle,
    Ending,
}

#[derive(Debug, Clone)]
pub enum DialogueRuntimeCmd {
    Start(String),
    Skip,
    Restart,
    SetMusicPlaying(bool),
    PlayBoxAnim(String),
    Cleanup,
}

#[derive(Component)]
pub struct DialogueBackdropVisual;

#[derive(Component)]
pub struct DialogueBoxVisual;

#[derive(Component)]
pub struct DialogueSpeakerVisual;

#[derive(Component)]
pub struct DialogueTextVisual;

#[derive(Component)]
pub struct DialogueMusicAudio;

#[derive(Component)]
pub struct DialoguePixelImage {
    pub is_pixel: bool,
    pub applied: bool,
}

#[derive(Resource, Default)]
pub struct DialogueRuntime {
    pub pending_cmds: VecDeque<DialogueRuntimeCmd>,
    pub active: bool,
    pub conversation_id: String,
    pub data: ConversationData,
    pub phase: DialoguePhase,
    pub entry_idx: usize,
    pub line_idx: usize,
    pub current_speaker_id: String,
    pub current_box_id: String,
    pub current_box_data: Option<DialogueBoxData>,
    pub current_speaker_data: Option<SpeakerData>,
    pub full_line: String,
    pub visible_chars: usize,
    pub type_timer_s: f64,
    pub backdrop_timer_s: f64,
    pub backdrop_dur_s: f64,
    pub backdrop_color: [f32; 4],
    pub music_fade_timer_s: f64,
    pub music_fade_dur_s: f64,
    pub music_paused_by_script: bool,
    pub outro_timer_s: f64,
    pub outro_dur_s: f64,
    pub click_sound: Option<Handle<AudioSource>>,
    pub click_advance_sound: Option<Handle<AudioSource>>,
    pub backdrop_entity: Option<Entity>,
    pub speaker_entity: Option<Entity>,
    pub box_entity: Option<Entity>,
    pub text_entity: Option<Entity>,
    pub music_entity: Option<Entity>,
}

impl DialogueRuntime {
    pub fn queue_start(&mut self, id: String) {
        self.pending_cmds.push_back(DialogueRuntimeCmd::Start(id));
    }

    pub fn queue_skip(&mut self) {
        self.pending_cmds.push_back(DialogueRuntimeCmd::Skip);
    }

    pub fn queue_restart(&mut self) {
        self.pending_cmds.push_back(DialogueRuntimeCmd::Restart);
    }

    pub fn queue_cleanup(&mut self) {
        self.pending_cmds.push_back(DialogueRuntimeCmd::Cleanup);
    }
}

fn to_base_anims(defs: &[DialogueAnimDef]) -> Vec<BaseAnimationData> {
    defs.iter()
        .map(|d| BaseAnimationData {
            name: d.name.clone(),
            prefix: d.prefix.clone(),
            offsets: if d.offsets.len() >= 2 {
                [d.offsets[0], d.offsets[1]]
            } else {
                [0.0, 0.0]
            },
            frame_rate: if d.frame_rate > 0.0 { d.frame_rate } else { 24.0 },
            looped: d.looped,
            flip_x: false,
            flip_y: false,
            frame_indices: d.frame_indices.clone(),
            asset_path: None,
        })
        .collect()
}

fn resolve_font_path(mods: &ModRegistry, font_family: &str) -> Option<String> {
    let lower = font_family.trim().to_ascii_lowercase();
    let candidates: &[&str] = if lower.contains("pixel") || lower.is_empty() {
        &["ui/fonts/Pixel Arial 11.otf", "ui/fonts/VCR OSD Mono.ttf"]
    } else if lower.contains("vcr") {
        &["ui/fonts/VCR OSD Mono.ttf"]
    } else {
        &["ui/fonts/Pixel Arial 11.otf"]
    };
    for rel in candidates {
        if let Some(srv) = mods.resolve_asset_server_path(rel) {
            return Some(srv);
        }
    }
    None
}

fn resolve_audio_handle(
    mods: &ModRegistry,
    asset_server: &AssetServer,
    base_rel: &str,
) -> Option<Handle<AudioSource>> {
    if let Some(srv) = mods.resolve_asset_server_path(base_rel) {
        return Some(asset_server.load(srv));
    }
    for ext in [".ogg", ".wav", ".mp3"] {
        let with_ext = format!("{base_rel}{ext}");
        if let Some(srv) = mods.resolve_asset_server_path(&with_ext) {
            return Some(asset_server.load(srv));
        }
    }
    None
}

fn load_sparrow_from_rel(
    mods: &ModRegistry,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    rel_path: &str,
    anims: &[DialogueAnimDef],
) -> Option<(Handle<Image>, Handle<TextureAtlasLayout>, AnimationController)> {
    let img_path = mods
        .resolve_file(rel_path)
        .or_else(|| mods.resolve_file(&format!("{rel_path}.png")))?;
    let xml_path = img_path.with_extension("xml");
    let atlas = SparrowAtlas::from_file(&xml_path).ok()?;
    let (layout, indices) = atlas.build_layout();
    let mut controller = AnimationController::default();
    let base_anims = to_base_anims(anims);
    if base_anims.is_empty() {
        register_default_sparrow_idle(&atlas, &indices, &mut controller);
    } else {
        add_sparrow_animations(&atlas, &indices, &mut controller, &base_anims);
    }
    let tex = asset_server.load(mods.to_asset_server_path(&img_path));
    let layout_handle = layouts.add(layout);
    Some((tex, layout_handle, controller))
}

fn cleanup_dialogue_entities(commands: &mut Commands, dlg: &mut DialogueRuntime) {
    for slot in [
        &mut dlg.backdrop_entity,
        &mut dlg.speaker_entity,
        &mut dlg.box_entity,
        &mut dlg.text_entity,
        &mut dlg.music_entity,
    ] {
        if let Some(ent) = slot.take() {
            commands.entity(ent).despawn();
        }
    }
    dlg.active = false;
    dlg.phase = DialoguePhase::Inactive;
    dlg.current_speaker_id.clear();
    dlg.current_box_id.clear();
    dlg.current_box_data = None;
    dlg.current_speaker_data = None;
    dlg.full_line.clear();
    dlg.visible_chars = 0;
    dlg.music_paused_by_script = false;
}

fn play_sfx_one_shot(
    commands: &mut Commands,
    handle: &Option<Handle<AudioSource>>,
    volume: f32,
) {
    if let Some(h) = handle {
        commands.spawn((
            AudioPlayer(h.clone()),
            PlaybackSettings {
                mode: PlaybackMode::Despawn,
                volume: Volume::Linear(volume.clamp(0.0, 2.0)),
                ..default()
            },
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_current_entry_visuals(
    commands: &mut Commands,
    mods: &ModRegistry,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    dlg: &mut DialogueRuntime,
    is_new_box_anim: bool,
) {
    let Some(entry) = dlg.data.dialogue.get(dlg.entry_idx).cloned() else {
        return;
    };

    // Speaker setup if changed or missing
    let speaker_changed = dlg.current_speaker_id != entry.speaker || dlg.speaker_entity.is_none();
    if speaker_changed {
        if let Some(old) = dlg.speaker_entity.take() {
            commands.entity(old).despawn();
        }
        dlg.current_speaker_id = entry.speaker.clone();
        let spk_rel = format!("gameplay/dialogue/speakers/{0}/{0}.json", entry.speaker);
        if let Ok(raw) = mods.read_to_string_merged(&spk_rel)
            && let Ok(spk) = serde_json::from_str::<SpeakerData>(&raw)
        {
            if let Some((tex, layout_h, mut ctrl)) =
                load_sparrow_from_rel(mods, asset_server, layouts, &spk.asset_path, &spk.animations)
            {
                let anim_to_play = if !entry.speaker_animation.is_empty()
                    && ctrl.has_animation(&entry.speaker_animation)
                {
                    entry.speaker_animation.as_str()
                } else {
                    "talkEnter"
                };
                if ctrl.has_animation(anim_to_play) {
                    ctrl.play(anim_to_play, true, false, 0);
                }
                let initial_idx = ctrl.current_atlas_index().unwrap_or(0);
                let ox = spk.offsets.first().copied().unwrap_or(0.0);
                let oy = spk.offsets.get(1).copied().unwrap_or(0.0);
                let scale = spk.scale.max(0.01);
                let mut sprite = Sprite::from_atlas_image(
                    tex,
                    TextureAtlas {
                        layout: layout_h,
                        index: initial_idx,
                    },
                );
                sprite.flip_x = spk.flip_x;
                let ent = commands
                    .spawn((
                        sprite,
                        Anchor::TOP_LEFT,
                        Transform::from_xyz(ox, -oy, DIALOGUE_SPEAKER_Z).with_scale(Vec3::splat(scale)),
                        Visibility::Visible,
                        ctrl,
                        hud_render_layer(),
                        DialogueSpeakerVisual,
                        DialoguePixelImage {
                            is_pixel: spk.is_pixel,
                            applied: false,
                        },
                    ))
                    .id();
                dlg.speaker_entity = Some(ent);
            }
            dlg.current_speaker_data = Some(spk);
        }
    }

    // Box setup if changed or missing
    let box_changed = dlg.current_box_id != entry.box_id || dlg.box_entity.is_none();
    if box_changed {
        if let Some(old) = dlg.box_entity.take() {
            commands.entity(old).despawn();
        }
        if let Some(old_txt) = dlg.text_entity.take() {
            commands.entity(old_txt).despawn();
        }
        dlg.current_box_id = entry.box_id.clone();
        dlg.click_sound = resolve_audio_handle(
            mods,
            asset_server,
            &format!("gameplay/dialogue/boxes/{}/click", entry.box_id),
        )
        .or_else(|| resolve_audio_handle(mods, asset_server, "gameplay/dialogue/boxes/roses/click"));
        dlg.click_advance_sound = resolve_audio_handle(
            mods,
            asset_server,
            &format!("gameplay/dialogue/boxes/{}/click-advance", entry.box_id),
        );

        let box_rel = format!("gameplay/dialogue/boxes/{0}/{0}.json", entry.box_id);
        if let Ok(raw) = mods.read_to_string_merged(&box_rel)
            && let Ok(bdata) = serde_json::from_str::<DialogueBoxData>(&raw)
        {
            if let Some((tex, layout_h, mut ctrl)) = load_sparrow_from_rel(
                mods,
                asset_server,
                layouts,
                &bdata.asset_path,
                &bdata.animations,
            ) {
                let anim_to_play = if !entry.box_animation.is_empty()
                    && ctrl.has_animation(&entry.box_animation)
                {
                    entry.box_animation.as_str()
                } else {
                    "enter"
                };
                if ctrl.has_animation(anim_to_play) {
                    ctrl.play(anim_to_play, true, false, 0);
                }
                let initial_idx = ctrl.current_atlas_index().unwrap_or(0);
                let ox = bdata.offsets.first().copied().unwrap_or(0.0);
                let oy = bdata.offsets.get(1).copied().unwrap_or(0.0);
                let scale = bdata.scale.max(0.01);
                let mut sprite = Sprite::from_atlas_image(
                    tex,
                    TextureAtlas {
                        layout: layout_h,
                        index: initial_idx,
                    },
                );
                sprite.flip_x = bdata.flip_x;
                sprite.flip_y = bdata.flip_y;
                let ent = commands
                    .spawn((
                        sprite,
                        Anchor::TOP_LEFT,
                        Transform::from_xyz(ox, -oy, DIALOGUE_BOX_Z).with_scale(Vec3::splat(scale)),
                        Visibility::Visible,
                        ctrl,
                        hud_render_layer(),
                        DialogueBoxVisual,
                        DialoguePixelImage {
                            is_pixel: bdata.is_pixel,
                            applied: false,
                        },
                    ))
                    .id();
                dlg.box_entity = Some(ent);
            }

            // Spawn dialogue text anchored in Flixel top-left space relative to screen
            let text_ox = bdata.offsets.first().copied().unwrap_or(0.0)
                + bdata.text.offsets.first().copied().unwrap_or(225.0);
            let text_oy = bdata.offsets.get(1).copied().unwrap_or(0.0)
                + bdata.text.offsets.get(1).copied().unwrap_or(450.0);
            let text_color = if bdata.text.color.is_empty() {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                parse_hex_rgba(&bdata.text.color)
            };
            let mut text_font = TextFont {
                font_size: FontSize::Px(32.0),
                ..default()
            };
            if let Some(font_srv) = resolve_font_path(mods, &bdata.text.font_family) {
                text_font.font = asset_server.load(font_srv);
            }
            let wrap_w = if bdata.text.width > 10.0 {
                bdata.text.width
            } else {
                800.0
            };
            let txt_ent = commands
                .spawn((
                    Text2d::new(""),
                    text_font,
                    TextColor(Color::srgba(
                        text_color[0],
                        text_color[1],
                        text_color[2],
                        text_color[3],
                    )),
                    bevy::text::TextBounds::from(Vec2::new(wrap_w, 300.0)),
                    Anchor::TOP_LEFT,
                    Transform::from_xyz(text_ox, -text_oy, DIALOGUE_TEXT_Z),
                    Visibility::Visible,
                    hud_render_layer(),
                    DialogueTextVisual,
                ))
                .id();
            dlg.text_entity = Some(txt_ent);
            dlg.current_box_data = Some(bdata);
        }
    } else if is_new_box_anim {
        // Same box, potentially new speaker/box anim; handled by caller query if needed
    }

    dlg.full_line = entry
        .text
        .get(dlg.line_idx)
        .cloned()
        .unwrap_or_default();
    dlg.visible_chars = 0;
    dlg.type_timer_s = 0.0;
    dlg.phase = if box_changed {
        DialoguePhase::Opening
    } else {
        DialoguePhase::Speaking
    };
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn tick_dialogue_system(
    time: Res<Time>,
    mut commands: Commands,
    mods: Res<ModRegistry>,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut images: ResMut<Assets<Image>>,
    keys: Res<ButtonInput<KeyCode>>,
    controls: Res<Controls>,
    sound_tray: Option<Res<SoundTrayState>>,
    mut state: ResMut<PlayState>,
    mut playlist: ResMut<PlayStatePlaylist>,
    mut dlg: ResMut<DialogueRuntime>,
    mut sprites: Query<
        (
            &mut Sprite,
            &mut Visibility,
            Option<&mut AnimationController>,
            Option<&mut DialoguePixelImage>,
            Option<&DialogueBackdropVisual>,
            Option<&DialogueBoxVisual>,
            Option<&DialogueSpeakerVisual>,
        ),
        Without<DialogueTextVisual>,
    >,
    mut texts: Query<(&mut Text2d, &mut TextColor, &mut Visibility), With<DialogueTextVisual>>,
    mut sinks: Query<&mut AudioSink, With<DialogueMusicAudio>>,
) {
    let dt = f64::from(time.delta_secs()).min(0.1);
    let paused = matches!(
        state.phase,
        PlayPhase::Loading | PlayPhase::Paused(_) | PlayPhase::GameOver | PlayPhase::Ended
    );
    let master_vol = sound_tray
        .as_deref()
        .map_or(1.0_f32, |st| st.effective_volume());

    // Process pending commands
    let cmds: Vec<DialogueRuntimeCmd> = dlg.pending_cmds.drain(..).collect();
    for cmd in cmds {
        match cmd {
            DialogueRuntimeCmd::Cleanup => {
                cleanup_dialogue_entities(&mut commands, &mut dlg);
            }
            DialogueRuntimeCmd::SetMusicPlaying(play) => {
                dlg.music_paused_by_script = !play;
                for mut sink in &mut sinks {
                    if play {
                        sink.play();
                    } else {
                        sink.pause();
                    }
                }
            }
            DialogueRuntimeCmd::PlayBoxAnim(anim_name) => {
                if let Some(box_ent) = dlg.box_entity
                    && let Ok((_, _, Some(mut ctrl), _, _, _, _)) = sprites.get_mut(box_ent)
                    && ctrl.has_animation(&anim_name)
                {
                    ctrl.play(&anim_name, true, false, 0);
                }
            }
            DialogueRuntimeCmd::Start(conv_id) => {
                cleanup_dialogue_entities(&mut commands, &mut dlg);
                let conv_rel = format!("gameplay/dialogue/conversations/{conv_id}/{conv_id}.json");
                let Ok(raw) = mods.read_to_string_merged(&conv_rel) else {
                    warn!("start_dialogue('{conv_id}'): missing '{conv_rel}'");
                    state.is_in_cutscene = false;
                    if state.is_in_countdown && state.countdown_paused {
                        state.set_hud_visible(true, false);
                        state.start_countdown();
                    }
                    continue;
                };
                let Ok(data) = serde_json::from_str::<ConversationData>(&raw) else {
                    warn!("start_dialogue('{conv_id}'): failed to parse JSON");
                    state.is_in_cutscene = false;
                    if state.is_in_countdown && state.countdown_paused {
                        state.set_hud_visible(true, false);
                        state.start_countdown();
                    }
                    continue;
                };

                dlg.active = true;
                dlg.conversation_id = conv_id.clone();
                dlg.data = data.clone();
                dlg.entry_idx = 0;
                dlg.line_idx = 0;
                dlg.outro_timer_s = 0.0;
                dlg.outro_dur_s = data.outro.as_ref().map_or(0.0, |o| o.fade_time.max(0.0));

                state.is_in_cutscene = true;

                // Backdrop
                if let Some(bd) = &data.backdrop
                    && !bd.color.is_empty()
                {
                    let col = parse_hex_rgba(&bd.color);
                    dlg.backdrop_color = col;
                    dlg.backdrop_dur_s = bd.fade_time.max(0.0);
                    dlg.backdrop_timer_s = 0.0;
                    let init_alpha = if dlg.backdrop_dur_s > 0.0 { 0.0 } else { col[3] };
                    let bd_ent = commands
                        .spawn((
                            Sprite::from_color(
                                Color::srgba(col[0], col[1], col[2], init_alpha),
                                Vec2::new(SCREEN_WIDTH * 2.0, SCREEN_HEIGHT * 2.0),
                            ),
                            Transform::from_xyz(0.0, 0.0, DIALOGUE_BACKDROP_Z),
                            Visibility::Visible,
                            hud_render_layer(),
                            DialogueBackdropVisual,
                        ))
                        .id();
                    dlg.backdrop_entity = Some(bd_ent);
                }

                // Music
                if let Some(mus) = &data.music
                    && !mus.asset.is_empty()
                {
                    let file_stem = mus.asset.rsplit('/').next().unwrap_or(&mus.asset);
                    let candidate_a = format!("{}/{file_stem}", mus.asset);
                    let handle = resolve_audio_handle(&mods, &asset_server, &candidate_a)
                        .or_else(|| resolve_audio_handle(&mods, &asset_server, &mus.asset));
                    if let Some(h) = handle {
                        dlg.music_fade_dur_s = mus.fade_time.max(0.0);
                        dlg.music_fade_timer_s = 0.0;
                        let init_vol = if dlg.music_fade_dur_s > 0.0 { 0.0 } else { 1.0 };
                        let mus_ent = commands
                            .spawn((
                                AudioPlayer(h),
                                PlaybackSettings {
                                    mode: if mus.looped {
                                        PlaybackMode::Loop
                                    } else {
                                        PlaybackMode::Despawn
                                    },
                                    volume: Volume::Linear(init_vol * master_vol),
                                    ..default()
                                },
                                DialogueMusicAudio,
                            ))
                            .id();
                        dlg.music_entity = Some(mus_ent);
                    }
                }

                apply_current_entry_visuals(
                    &mut commands,
                    &mods,
                    &asset_server,
                    &mut layouts,
                    &mut dlg,
                    true,
                );
                let speaker_id = dlg.current_speaker_id.clone();
                let box_id = dlg.current_box_id.clone();
                state.pending_hooks.push((
                    HookKind::DialogueStart,
                    ScriptEvent::new(HookKind::DialogueStart)
                        .with_field("conversation", conv_id)
                        .with_field("speaker", speaker_id)
                        .with_field("box", box_id),
                ));
            }
            DialogueRuntimeCmd::Skip => {
                if dlg.active && dlg.phase != DialoguePhase::Ending {
                    dlg.phase = DialoguePhase::Ending;
                    dlg.outro_timer_s = 0.0;
                    let conv_id = dlg.conversation_id.clone();
                    state.pending_hooks.push((
                        HookKind::DialogueEnd,
                        ScriptEvent::new(HookKind::DialogueEnd).with_field("conversation", conv_id),
                    ));
                }
            }
            DialogueRuntimeCmd::Restart => {
                if dlg.active {
                    let conv_id = dlg.conversation_id.clone();
                    dlg.pending_cmds.push_back(DialogueRuntimeCmd::Start(conv_id));
                }
            }
        }
    }

    if !dlg.active {
        return;
    }

    // Pause/resume dialogue music when game pauses
    for mut sink in &mut sinks {
        if paused || dlg.music_paused_by_script {
            sink.pause();
        } else {
            sink.play();
        }
    }

    if paused {
        return;
    }

    // Apply pixel nearest sampler on dialogue images once loaded
    for (spr, _, _, px_opt, _, _, _) in &mut sprites {
        if let Some(mut px) = px_opt
            && px.is_pixel
            && !px.applied
            && let Some(img) = images.get_mut(&spr.image)
        {
            img.sampler = ImageSampler::nearest();
            px.applied = true;
        }
    }

    // Fade in backdrop & music
    if dlg.phase != DialoguePhase::Ending {
        if dlg.backdrop_dur_s > 0.0 && dlg.backdrop_timer_s < dlg.backdrop_dur_s {
            dlg.backdrop_timer_s = (dlg.backdrop_timer_s + dt).min(dlg.backdrop_dur_s);
            let frac = (dlg.backdrop_timer_s / dlg.backdrop_dur_s) as f32;
            let stepped = (frac * 8.0).floor() / 8.0;
            if let Some(bd_ent) = dlg.backdrop_entity
                && let Ok((mut spr, _, _, _, _, _, _)) = sprites.get_mut(bd_ent)
            {
                let c = dlg.backdrop_color;
                spr.color = Color::srgba(c[0], c[1], c[2], c[3] * stepped);
            }
        }
        if dlg.music_fade_dur_s > 0.0 && dlg.music_fade_timer_s < dlg.music_fade_dur_s {
            dlg.music_fade_timer_s = (dlg.music_fade_timer_s + dt).min(dlg.music_fade_dur_s);
            let frac = (dlg.music_fade_timer_s / dlg.music_fade_dur_s) as f32;
            for mut sink in &mut sinks {
                sink.set_volume(Volume::Linear(frac * master_vol));
            }
        } else {
            for mut sink in &mut sinks {
                sink.set_volume(Volume::Linear(master_vol));
            }
        }
    }

    // Advance box/speaker animations
    for (mut spr, _, ctrl_opt, _, _, is_box, _) in &mut sprites {
        if let Some(mut ctrl) = ctrl_opt {
            ctrl.update(dt);
            if let Some(idx) = ctrl.current_atlas_index() {
                set_atlas_index(&mut spr, idx);
            }
            if is_box.is_some() && ctrl.finished {
                let cur = ctrl.current_name().unwrap_or("").to_string();
                if cur == "enter" || cur == "click" {
                    if ctrl.has_animation("speaking") {
                        ctrl.play("speaking", true, false, 0);
                    }
                    if dlg.phase == DialoguePhase::Opening {
                        dlg.phase = DialoguePhase::Speaking;
                    }
                } else if cur == "sentenceEnd" && ctrl.has_animation("idle") {
                    ctrl.play("idle", true, false, 0);
                }
            }
        }
    }

    // Update speaker talk/idle
    if let Some(spk_ent) = dlg.speaker_entity
        && let Ok((_, _, Some(mut ctrl), _, _, _, _)) = sprites.get_mut(spk_ent)
    {
        if dlg.phase == DialoguePhase::Speaking && ctrl.finished && ctrl.has_animation("talk") {
            ctrl.play("talk", false, false, 0);
        }
    }

    // Handle typewriter progression
    if dlg.phase == DialoguePhase::Opening || dlg.phase == DialoguePhase::Speaking {
        let total_chars = dlg.full_line.chars().count();
        if dlg.visible_chars < total_chars {
            dlg.type_timer_s += dt;
            if dlg.type_timer_s >= CHAR_TYPE_INTERVAL_S {
                dlg.type_timer_s -= CHAR_TYPE_INTERVAL_S;
                dlg.visible_chars += 1;
                let partial: String = dlg.full_line.chars().take(dlg.visible_chars).collect();
                if let Some(txt_ent) = dlg.text_entity
                    && let Ok((mut t2d, _, _)) = texts.get_mut(txt_ent)
                {
                    **t2d = partial;
                }
                play_sfx_one_shot(&mut commands, &dlg.click_sound, 0.6 * master_vol);
                if dlg.visible_chars >= total_chars {
                    dlg.phase = DialoguePhase::Idle;
                    if let Some(box_ent) = dlg.box_entity
                        && let Ok((_, _, Some(mut ctrl), _, _, _, _)) = sprites.get_mut(box_ent)
                        && ctrl.has_animation("sentenceEnd")
                    {
                        ctrl.play("sentenceEnd", true, false, 0);
                    }
                }
            }
        } else {
            dlg.phase = DialoguePhase::Idle;
        }
    }

    // Handle Accept / Back input during dialogue
    let accept_pressed = controls.just_pressed(&keys, Action::Accept)
        || keys.just_pressed(KeyCode::Space)
        || keys.just_pressed(KeyCode::Enter);
    if accept_pressed && dlg.phase != DialoguePhase::Ending {
        let total_chars = dlg.full_line.chars().count();
        if dlg.visible_chars < total_chars {
            // Complete line immediately
            dlg.visible_chars = total_chars;
            if let Some(txt_ent) = dlg.text_entity
                && let Ok((mut t2d, _, _)) = texts.get_mut(txt_ent)
            {
                **t2d = dlg.full_line.clone();
            }
            dlg.phase = DialoguePhase::Idle;
            if let Some(box_ent) = dlg.box_entity
                && let Ok((_, _, Some(mut ctrl), _, _, _, _)) = sprites.get_mut(box_ent)
                && ctrl.has_animation("sentenceEnd")
            {
                ctrl.play("sentenceEnd", true, false, 0);
            }
        } else {
            // Advance to next line / entry
            play_sfx_one_shot(&mut commands, &dlg.click_advance_sound, 0.6 * master_vol);
            if let Some(box_ent) = dlg.box_entity
                && let Ok((_, _, Some(mut ctrl), _, _, _, _)) = sprites.get_mut(box_ent)
                && ctrl.has_animation("click")
            {
                ctrl.play("click", true, false, 0);
            }

            dlg.line_idx += 1;
            let cur_entry_lines = dlg
                .data
                .dialogue
                .get(dlg.entry_idx)
                .map_or(0, |e| e.text.len());
            if dlg.line_idx >= cur_entry_lines {
                dlg.line_idx = 0;
                dlg.entry_idx += 1;
            }

            if dlg.entry_idx >= dlg.data.dialogue.len() {
                dlg.phase = DialoguePhase::Ending;
                dlg.outro_timer_s = 0.0;
                if let Some(txt_ent) = dlg.text_entity
                    && let Ok((_, _, mut vis)) = texts.get_mut(txt_ent)
                {
                    set_visibility(&mut vis, Visibility::Hidden);
                }
                if let Some(box_ent) = dlg.box_entity
                    && let Ok((_, _, Some(mut ctrl), _, _, _, _)) = sprites.get_mut(box_ent)
                    && ctrl.has_animation("exit")
                {
                    ctrl.play("exit", true, false, 0);
                }
                let conv_id = dlg.conversation_id.clone();
                state.pending_hooks.push((
                    HookKind::DialogueEnd,
                    ScriptEvent::new(HookKind::DialogueEnd).with_field("conversation", conv_id),
                ));
            } else {
                apply_current_entry_visuals(
                    &mut commands,
                    &mods,
                    &asset_server,
                    &mut layouts,
                    &mut dlg,
                    true,
                );
                let conv_id = dlg.conversation_id.clone();
                let spk_id = dlg.current_speaker_id.clone();
                let box_id = dlg.current_box_id.clone();
                state.pending_hooks.push((
                    HookKind::DialogueLine,
                    ScriptEvent::new(HookKind::DialogueLine)
                        .with_field("conversation", conv_id)
                        .with_field("speaker", spk_id)
                        .with_field("box", box_id)
                        .with_field("entry", dlg.entry_idx as i64)
                        .with_field("line", dlg.line_idx as i64)
                        .with_field("text", dlg.full_line.clone()),
                ));
            }
        }
    }

    // Handle outro fade
    if dlg.phase == DialoguePhase::Ending {
        if dlg.outro_dur_s <= 0.0 {
            cleanup_dialogue_entities(&mut commands, &mut dlg);
            state.is_in_cutscene = false;
            if state.is_in_countdown && state.countdown_paused {
                state.set_hud_visible(true, false);
                state.start_countdown();
            }
            return;
        }
        dlg.outro_timer_s += dt;
        let frac = (dlg.outro_timer_s / dlg.outro_dur_s).clamp(0.0, 1.0) as f32;
        let alpha = (1.0 - (frac * 8.0).floor() / 8.0).clamp(0.0, 1.0);
        for (mut spr, _, _, _, is_bd, _, _) in &mut sprites {
            let base_a = if is_bd.is_some() {
                dlg.backdrop_color[3]
            } else {
                1.0
            };
            spr.color = spr.color.with_alpha(base_a * alpha);
        }
        for (_, mut tc, _) in &mut texts {
            tc.0 = tc.0.with_alpha(alpha);
        }
        for mut sink in &mut sinks {
            sink.set_volume(Volume::Linear(alpha * master_vol));
        }
        if dlg.outro_timer_s >= dlg.outro_dur_s {
            cleanup_dialogue_entities(&mut commands, &mut dlg);
            state.is_in_cutscene = false;
            if state.is_in_countdown && state.countdown_paused {
                state.set_hud_visible(true, false);
                state.start_countdown();
            }
            let _ = &mut playlist;
        }
    }
}
