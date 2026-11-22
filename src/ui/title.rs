use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::sprite::Anchor;

use crate::animations::atlas_composite::{
    CompositeClip, FlxAnimateTransformParams, build_composite_clip, compose_flx_piece_bevy_affine,
    modulate_color,
};
use crate::animations::{AnimateAtlas, AnimationController, BaseAnimationData};
use crate::input::{Action, Controls};
use crate::mods::ModRegistry;
use crate::utils::alphabet::{AlphabetFontCache, AtlasFont, spawn_alphabet_text};
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::utils::ecs::{set_atlas_index, set_sprite_atlas, set_sprite_color, set_visibility};

use super::menu_shared::{
    ActiveScene, AppScene, MENU_BPM, MenuSharedState, flixel_to_bevy_tl, load_sparrow_sprite,
    play_menu_sfx, pseudo_random_index,
};

const TITLE_INTRO_PATH: &str = "ui/title/intro-text.txt";
const TITLE_LOGO_PATH: &str = "ui/title/logo-bumpin";
const TITLE_GF_PATH: &str = "ui/title/gf-dance-title";
const TITLE_TEXT_DIR: &str = "ui/title/title-screen-text";
const TITLE_NG_PATH: &str = "ui/title/newgrounds-logo.png";
const TITLE_NG_CLASSIC_PATH: &str = "ui/title/newgrounds-logo-classic.png";
const FREAKY_MENU_PATH: &str = "ui/main-menu/freaky-menu/freaky-menu.ogg";
const RINGTONE_PATH: &str = "ui/title/girlfriends-ringtone/girlfriends-ringtone.ogg";
const CONFIRM_SFX_PATH: &str = "ui/main-menu/confirm-menu.ogg";

#[derive(Component)]
pub struct TitleSceneEntity;

#[derive(Component)]
pub struct TitleLogoSprite;

#[derive(Component)]
pub struct TitleGfSprite;

#[derive(Component)]
pub struct TitleCredBackdrop;

#[derive(Component)]
pub struct TitleNgSprite;

#[derive(Component)]
pub struct TitleCredText;

#[derive(Component)]
pub struct TitleStartPiece(pub usize);

#[derive(Clone, Copy, PartialEq, Eq)]
enum CheatDir {
    Down,
    Up,
    Left,
    Right,
}

const CHEAT_SEQUENCE: [CheatDir; 8] = [
    CheatDir::Down,
    CheatDir::Up,
    CheatDir::Down,
    CheatDir::Up,
    CheatDir::Left,
    CheatDir::Right,
    CheatDir::Left,
    CheatDir::Right,
];

#[derive(Resource)]
pub struct TitleState {
    pub active: bool,
    pub initialized: bool,
    pub started_intro: bool,
    pub boot_delay_s: f32,
    pub skipped_intro: bool,
    pub transitioning: bool,
    pub transition_timer_s: f32,
    pub last_beat: i32,
    pub dance_left: bool,
    pub cur_wacky: [String; 2],
    pub cheat_pos: usize,
    pub cheat_active: bool,
    pub cred_backdrop: Option<Entity>,
    pub ng_entity: Option<Entity>,
    pub text_entities: Vec<Entity>,
    pub logo_entity: Option<Entity>,
    pub gf_entity: Option<Entity>,
    pub press_enter_ctrl: Option<AnimationController>,
    pub press_enter_clips: Option<(
        Vec<(Handle<Image>, Handle<TextureAtlasLayout>)>,
        Option<[f32; 6]>,
        CompositeClip,
        CompositeClip,
        Vec<Entity>,
    )>,
    pub press_enter_confirmed: bool,
}

impl Default for TitleState {
    fn default() -> Self {
        Self {
            active: false,
            initialized: false,
            started_intro: false,
            boot_delay_s: 0.0,
            skipped_intro: false,
            transitioning: false,
            transition_timer_s: 0.0,
            last_beat: 0,
            dance_left: false,
            cur_wacky: ["FRIDAY NIGHT FUNKIN".to_string(), "OXIDE ENGINE".to_string()],
            cheat_pos: 0,
            cheat_active: false,
            cred_backdrop: None,
            ng_entity: None,
            text_entities: Vec::new(),
            logo_entity: None,
            gf_entity: None,
            press_enter_ctrl: None,
            press_enter_clips: None,
            press_enter_confirmed: false,
        }
    }
}

fn load_wacky_pair(mods: &ModRegistry) -> [String; 2] {
    let Ok(raw) = mods.read_to_string_merged(TITLE_INTRO_PATH) else {
        return ["FRIDAY NIGHT".to_string(), "FUNKIN".to_string()];
    };
    let mut pairs: Vec<[String; 2]> = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split("--");
        let a = parts.next().unwrap_or("").trim().to_string();
        let b = parts.next().unwrap_or("").trim().to_string();
        pairs.push([a, b]);
    }
    if pairs.is_empty() {
        return ["FRIDAY NIGHT".to_string(), "FUNKIN".to_string()];
    }
    pairs[pseudo_random_index(pairs.len())].clone()
}

fn clear_cred_texts(commands: &mut Commands, title: &mut TitleState) {
    for ent in title.text_entities.drain(..) {
        commands.entity(ent).despawn();
    }
}

fn add_cred_lines(
    commands: &mut Commands,
    title: &mut TitleState,
    mods: &ModRegistry,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    alpha_cache: &mut AlphabetFontCache,
    lines: &[&str],
) {
    let Some(font_data) = alpha_cache
        .get_or_load(AtlasFont::Bold, Some(mods), asset_server, layouts)
        .cloned()
    else {
        return;
    };
    let existing = title.text_entities.len();
    for (idx, line) in lines.iter().enumerate() {
        let text = line.trim();
        if text.is_empty() {
            continue;
        }
        let width = font_data.measure_width(text);
        let flx_x = (SCREEN_WIDTH - width) * 0.5;
        let flx_y = ((existing + idx) as f32) * 60.0 + 200.0;
        let pos = flixel_to_bevy_tl(flx_x, flx_y, 25.0);
        let ent = spawn_alphabet_text(
            commands,
            AtlasFont::Bold,
            &font_data,
            text,
            pos,
            1.0,
            1.0,
            None,
        );
        commands
            .entity(ent)
            .insert((TitleSceneEntity, TitleCredText));
        title.text_entities.push(ent);
    }
}

fn cleanup_title_scene(
    commands: &mut Commands,
    title: &mut TitleState,
    entities: &Query<Entity, With<TitleSceneEntity>>,
) {
    for ent in entities {
        commands.entity(ent).despawn();
    }
    title.active = false;
    title.started_intro = false;
    title.skipped_intro = false;
    title.transitioning = false;
    title.transition_timer_s = 0.0;
    title.cred_backdrop = None;
    title.ng_entity = None;
    title.text_entities.clear();
    title.logo_entity = None;
    title.gf_entity = None;
    title.press_enter_ctrl = None;
    title.press_enter_clips = None;
    title.press_enter_confirmed = false;
}

fn spawn_title_visuals(
    commands: &mut Commands,
    mods: &ModRegistry,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    title: &mut TitleState,
    shared: &mut MenuSharedState,
) {
    // Solid black BG
    commands.spawn((
        Sprite::from_color(Color::BLACK, Vec2::new(SCREEN_WIDTH + 4.0, SCREEN_HEIGHT + 4.0)),
        Transform::from_xyz(0.0, 0.0, 0.0),
        Visibility::Visible,
        TitleSceneEntity,
    ));

    // Logo Bumpin
    let logo_anims = vec![BaseAnimationData {
        name: "bump".to_string(),
        prefix: "logo bumpin".to_string(),
        frame_rate: 24.0,
        looped: false,
        ..Default::default()
    }];
    if let Some((spr, ctrl)) =
        load_sparrow_sprite(mods, asset_server, layouts, TITLE_LOGO_PATH, &logo_anims)
    {
        let pos = flixel_to_bevy_tl(-150.0, -100.0, 2.0);
        let ent = commands
            .spawn((
                spr,
                Anchor::TOP_LEFT,
                Transform::from_translation(pos),
                Visibility::Visible,
                ctrl,
                TitleSceneEntity,
                TitleLogoSprite,
            ))
            .id();
        title.logo_entity = Some(ent);
    }

    // GF Dance Title
    let gf_anims = vec![
        BaseAnimationData {
            name: "danceLeft".to_string(),
            prefix: "gfDance".to_string(),
            frame_rate: 24.0,
            looped: false,
            frame_indices: vec![30, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14],
            ..Default::default()
        },
        BaseAnimationData {
            name: "danceRight".to_string(),
            prefix: "gfDance".to_string(),
            frame_rate: 24.0,
            looped: false,
            frame_indices: (15..=29).collect(),
            ..Default::default()
        },
    ];
    if let Some((spr, ctrl)) =
        load_sparrow_sprite(mods, asset_server, layouts, TITLE_GF_PATH, &gf_anims)
    {
        let pos = flixel_to_bevy_tl(SCREEN_WIDTH * 0.4, SCREEN_HEIGHT * 0.07, 1.0);
        let ent = commands
            .spawn((
                spr,
                Anchor::TOP_LEFT,
                Transform::from_translation(pos),
                Visibility::Visible,
                ctrl,
                TitleSceneEntity,
                TitleGfSprite,
            ))
            .id();
        title.gf_entity = Some(ent);
    }

    // Press Enter AnimateAtlas
    if let Some(dir) = mods.resolve_dir(TITLE_TEXT_DIR)
        && let Ok(atlas) = AnimateAtlas::from_dir(&dir)
    {
        let (atlas_layouts, indices) = atlas.build_layouts();
        let textures: Vec<(Handle<Image>, Handle<TextureAtlasLayout>)> = atlas_layouts
            .into_iter()
            .enumerate()
            .map(|(i, layout)| {
                let img_rel = mods.to_asset_server_path(&dir.join(format!("spritemap{}.png", i + 1)));
                let tex: Handle<Image> = asset_server.load(img_rel);
                (tex, layouts.add(layout))
            })
            .collect();
        let idle_def = BaseAnimationData {
            name: "idle".to_string(),
            prefix: "Idle".to_string(),
            frame_rate: 24.0,
            looped: true,
            anim_type: "framelabel".to_string(),
            ..Default::default()
        };
        let press_def = BaseAnimationData {
            name: "press".to_string(),
            prefix: "Confirm".to_string(),
            frame_rate: 24.0,
            looped: true,
            anim_type: "framelabel".to_string(),
            ..Default::default()
        };
        if let (Some(idle_clip), Some(press_clip), Some((base_tex, base_layout))) = (
            build_composite_clip(&atlas, &indices, &idle_def),
            build_composite_clip(&atlas, &indices, &press_def),
            textures.first(),
        ) {
            let mut ctrl = AnimationController::default();
            ctrl.add_anim(
                "idle",
                (0..idle_clip.frames.len()).collect(),
                idle_clip.fps,
                true,
            );
            ctrl.add_anim(
                "press",
                (0..press_clip.frames.len()).collect(),
                press_clip.fps,
                true,
            );
            ctrl.play("idle", true, false, 0);
            let max_pieces = idle_clip
                .frames
                .iter()
                .chain(press_clip.frames.iter())
                .map(Vec::len)
                .max()
                .unwrap_or(1)
                .max(1);
            let mut pieces = Vec::with_capacity(max_pieces);
            for i in 0..max_pieces {
                let pe = commands
                    .spawn((
                        Sprite::from_atlas_image(
                            base_tex.clone(),
                            TextureAtlas {
                                layout: base_layout.clone(),
                                index: 0,
                            },
                        ),
                        Transform::from_xyz(0.0, 0.0, 5.0 + i as f32 * 0.001),
                        Visibility::Hidden,
                        TitleSceneEntity,
                        TitleStartPiece(i),
                    ))
                    .id();
                pieces.push(pe);
            }
            title.press_enter_ctrl = Some(ctrl);
            title.press_enter_clips =
                Some((textures, atlas.stage_matrix, idle_clip, press_clip, pieces));
        }
    }

    if !title.initialized {
        let cred_bg = commands
            .spawn((
                Sprite::from_color(Color::BLACK, Vec2::new(SCREEN_WIDTH + 4.0, SCREEN_HEIGHT + 4.0)),
                Transform::from_xyz(0.0, 0.0, 20.0),
                Visibility::Visible,
                TitleSceneEntity,
                TitleCredBackdrop,
            ))
            .id();
        title.cred_backdrop = Some(cred_bg);

        let ng_path = if pseudo_random_index(100) == 0 {
            TITLE_NG_CLASSIC_PATH
        } else {
            TITLE_NG_PATH
        };
        let ng_srv = mods
            .resolve_asset_server_path(ng_path)
            .unwrap_or_else(|| ng_path.to_string());
        let ng_tex: Handle<Image> = asset_server.load(ng_srv);
        let ng_ent = commands
            .spawn((
                Sprite::from_image(ng_tex),
                Anchor::TOP_CENTER,
                Transform::from_xyz(0.0, SCREEN_HEIGHT * 0.5 - SCREEN_HEIGHT * 0.52, 22.0)
                    .with_scale(Vec3::splat(0.8)),
                Visibility::Hidden,
                TitleSceneEntity,
                TitleNgSprite,
            ))
            .id();
        title.ng_entity = Some(ng_ent);
        title.initialized = true;
    } else {
        title.skipped_intro = true;
        shared.trigger_flash([1.0, 1.0, 1.0], 1.0);
    }

    shared.ensure_menu_music(commands, mods, asset_server, FREAKY_MENU_PATH, 4.0, false);
}

fn do_skip_intro(
    commands: &mut Commands,
    title: &mut TitleState,
    shared: &mut MenuSharedState,
) {
    if title.skipped_intro {
        return;
    }
    title.skipped_intro = true;
    clear_cred_texts(commands, title);
    if let Some(bd) = title.cred_backdrop.take() {
        commands.entity(bd).despawn();
    }
    if let Some(ng) = title.ng_entity.take() {
        commands.entity(ng).despawn();
    }
    shared.trigger_flash([1.0, 1.0, 1.0], 4.0);
}

#[allow(clippy::too_many_arguments)]
pub fn tick_title_system(
    time: Res<Time>,
    mut commands: Commands,
    mods: Res<ModRegistry>,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut alpha_cache: ResMut<AlphabetFontCache>,
    keys: Res<ButtonInput<KeyCode>>,
    controls: Res<Controls>,
    mut app_scene: ResMut<AppScene>,
    mut shared: ResMut<MenuSharedState>,
    mut title: ResMut<TitleState>,
    mut exit: MessageWriter<AppExit>,
    scene_entities: Query<Entity, With<TitleSceneEntity>>,
    mut ng_vis_q: Query<&mut Visibility, With<TitleNgSprite>>,
    mut logo_gf_q: Query<
        (&mut Sprite, &mut AnimationController, Option<&TitleLogoSprite>, Option<&TitleGfSprite>),
        (Without<TitleStartPiece>, Without<IsDefaultUiCamera>),
    >,
    mut start_pieces_q: Query<
        (&TitleStartPiece, &mut Sprite, &mut Transform, &mut Visibility),
        (Without<TitleLogoSprite>, Without<TitleGfSprite>, Without<IsDefaultUiCamera>),
    >,
    mut cam_q: Query<(&mut Transform, &mut Projection), (With<Camera2d>, With<IsDefaultUiCamera>, Without<TitleSceneEntity>)>,
) {
    if app_scene.current != ActiveScene::Title {
        if title.active {
            cleanup_title_scene(&mut commands, &mut title, &scene_entities);
        }
        return;
    }

    let dt = time.delta_secs().min(0.1);

    if !title.active {
        title.active = true;
        title.started_intro = false;
        title.boot_delay_s = if title.initialized { 0.0 } else { 1.0 };
        title.cur_wacky = load_wacky_pair(&mods);
        title.last_beat = 0;
        title.cheat_pos = 0;
        for (mut tf, mut proj) in &mut cam_q {
            tf.translation = Vec3::ZERO;
            tf.rotation = Quat::IDENTITY;
            if let Projection::Orthographic(ref mut ortho) = *proj {
                ortho.scale = 1.0;
            }
        }
        if title.boot_delay_s <= 0.0 {
            title.started_intro = true;
            spawn_title_visuals(
                &mut commands,
                &mods,
                &asset_server,
                &mut layouts,
                &mut title,
                &mut shared,
            );
        }
        return;
    }

    if !title.started_intro {
        title.boot_delay_s -= dt;
        if title.boot_delay_s <= 0.0 {
            title.started_intro = true;
            spawn_title_visuals(
                &mut commands,
                &mods,
                &asset_server,
                &mut layouts,
                &mut title,
                &mut shared,
            );
        }
        return;
    }

    // Exit on Back
    if controls.just_pressed(&keys, Action::Back) {
        exit.write(AppExit::Success);
        return;
    }

    // Step logo & GF sprite animations
    for (mut spr, mut ctrl, _, _) in &mut logo_gf_q {
        ctrl.update(dt, None);
        if let Some(idx) = ctrl.current_atlas_index() {
            set_atlas_index(&mut spr, idx);
        }
    }

    // Beat conductor
    let beat_ms = 60_000.0 / MENU_BPM;
    let cur_beat = ((shared.elapsed_s * 1000.0) / beat_ms).floor() as i32;
    if cur_beat > title.last_beat {
        let prev = title.last_beat;
        title.last_beat = cur_beat;
        if !title.skipped_intro {
            for b in prev..cur_beat {
                match b + 1 {
                    1 => add_cred_lines(
                        &mut commands,
                        &mut title,
                        &mods,
                        &asset_server,
                        &mut layouts,
                        &mut alpha_cache,
                        &["The", "Funkin Crew Inc"],
                    ),
                    3 => add_cred_lines(
                        &mut commands,
                        &mut title,
                        &mods,
                        &asset_server,
                        &mut layouts,
                        &mut alpha_cache,
                        &["presents"],
                    ),
                    4 => clear_cred_texts(&mut commands, &mut title),
                    5 => add_cred_lines(
                        &mut commands,
                        &mut title,
                        &mods,
                        &asset_server,
                        &mut layouts,
                        &mut alpha_cache,
                        &["In association", "with"],
                    ),
                    7 => {
                        add_cred_lines(
                            &mut commands,
                            &mut title,
                            &mods,
                            &asset_server,
                            &mut layouts,
                            &mut alpha_cache,
                            &["newgrounds"],
                        );
                        if let Some(ng) = title.ng_entity
                            && let Ok(mut vis) = ng_vis_q.get_mut(ng)
                        {
                            *vis = Visibility::Visible;
                        }
                    }
                    8 => {
                        clear_cred_texts(&mut commands, &mut title);
                        if let Some(ng) = title.ng_entity
                            && let Ok(mut vis) = ng_vis_q.get_mut(ng)
                        {
                            *vis = Visibility::Hidden;
                        }
                    }
                    9 => {
                        let line0 = title.cur_wacky[0].clone();
                        add_cred_lines(
                            &mut commands,
                            &mut title,
                            &mods,
                            &asset_server,
                            &mut layouts,
                            &mut alpha_cache,
                            &[&line0],
                        );
                    }
                    11 => {
                        let line1 = title.cur_wacky[1].clone();
                        add_cred_lines(
                            &mut commands,
                            &mut title,
                            &mods,
                            &asset_server,
                            &mut layouts,
                            &mut alpha_cache,
                            &[&line1],
                        );
                    }
                    12 => clear_cred_texts(&mut commands, &mut title),
                    13 => add_cred_lines(
                        &mut commands,
                        &mut title,
                        &mods,
                        &asset_server,
                        &mut layouts,
                        &mut alpha_cache,
                        &["Friday"],
                    ),
                    14 => {
                        let line = if title.cur_wacky[0].eq_ignore_ascii_case("trending") {
                            "Nigth"
                        } else {
                            "Night"
                        };
                        add_cred_lines(
                            &mut commands,
                            &mut title,
                            &mods,
                            &asset_server,
                            &mut layouts,
                            &mut alpha_cache,
                            &[line],
                        );
                    }
                    15 => add_cred_lines(
                        &mut commands,
                        &mut title,
                        &mods,
                        &asset_server,
                        &mut layouts,
                        &mut alpha_cache,
                        &["Funkin"],
                    ),
                    16 => do_skip_intro(&mut commands, &mut title, &mut shared),
                    _ => {}
                }
            }
        }

        if title.skipped_intro {
            title.dance_left = !title.dance_left;
            for (_, mut ctrl, is_logo, is_gf) in &mut logo_gf_q {
                if is_logo.is_some() {
                    ctrl.play("bump", true, false, 0);
                } else if is_gf.is_some() {
                    let anim = if title.dance_left {
                        "danceRight"
                    } else {
                        "danceLeft"
                    };
                    ctrl.play(anim, true, false, 0);
                }
            }
        }
    }

    // Step & sync "Press Enter to Begin" AnimateAtlas
    let confirmed = title.press_enter_confirmed;
    if let Some(ref mut ctrl) = title.press_enter_ctrl {
        ctrl.update(dt, None);
        if confirmed && ctrl.current_name() != Some("press") {
            ctrl.play("press", true, false, 0);
        }
        let frame_idx = ctrl.current_atlas_index().unwrap_or(0);
        let anim_name = ctrl.current_name().unwrap_or("idle").to_string();
        if let Some((ref textures, stage_matrix, ref idle_clip, ref press_clip, _)) =
            title.press_enter_clips
        {
            let clip = if anim_name == "press" {
                press_clip
            } else {
                idle_clip
            };
            let pieces = clip.frames.get(frame_idx).map_or(&[][..], Vec::as_slice);
            let params = FlxAnimateTransformParams {
                bounds_min: clip.bounds_min,
                bounds_size: clip.bounds_size,
                top_left: [100.0, SCREEN_HEIGHT * 0.8],
                scale: [1.0, 1.0],
                angle_deg: 0.0,
                flip: [false, false],
                stage_matrix,
                apply_stage_matrix: false,
                post_stage_matrix_apply: false,
                screen_size: [SCREEN_WIDTH, SCREEN_HEIGHT],
                zoom: 1.0,
            };
            for (piece_idx_comp, mut spr, mut tf, mut vis) in &mut start_pieces_q {
                if let Some(piece) = pieces.get(piece_idx_comp.0) {
                    if let Some((tex, layout)) = textures
                        .get(piece.spritemap_idx)
                        .or_else(|| textures.first())
                    {
                        let ([bx, by], cols) = compose_flx_piece_bevy_affine(piece, &params);
                        let sx = Vec2::new(cols[0], cols[1]).length().max(1e-4);
                        let sy = Vec2::new(cols[2], cols[3]).length().max(1e-4);
                        let rot = cols[1].atan2(cols[0]);
                        set_sprite_atlas(&mut spr, tex, layout, piece.sprite_index);
                        set_sprite_color(&mut spr, modulate_color(Color::WHITE, piece.color));
                        tf.translation = Vec3::new(bx, by, 5.0 + piece_idx_comp.0 as f32 * 0.001);
                        tf.rotation = Quat::from_rotation_z(rot);
                        tf.scale = Vec3::new(sx, sy, 1.0);
                        set_visibility(&mut vis, Visibility::Visible);
                    }
                } else {
                    set_visibility(&mut vis, Visibility::Hidden);
                }
            }
        }
    }

    // Accept / Enter input
    let pressed_enter = controls.just_pressed(&keys, Action::Accept)
        || keys.just_pressed(KeyCode::Enter)
        || keys.just_pressed(KeyCode::Space);
    if pressed_enter {
        if !title.skipped_intro {
            do_skip_intro(&mut commands, &mut title, &mut shared);
        } else if title.transitioning {
            shared.transition_to(ActiveScene::MainMenu, 0.35);
            title.transitioning = false;
        } else {
            title.press_enter_confirmed = true;
            title.transitioning = true;
            title.transition_timer_s = 2.0;
            shared.trigger_flash([1.0, 1.0, 1.0], 1.0);
            play_menu_sfx(&mut commands, &mods, &asset_server, CONFIRM_SFX_PATH, 0.7);
        }
    }

    if title.transitioning {
        title.transition_timer_s -= dt;
        if title.transition_timer_s <= 0.0 {
            title.transitioning = false;
            shared.transition_to(ActiveScene::MainMenu, 0.35);
        }
    }

    // Konami-style ringtone easter egg
    if !title.cheat_active && title.skipped_intro {
        let dir = if controls.just_pressed(&keys, Action::UiDown)
            || controls.just_pressed(&keys, Action::NoteDown)
        {
            Some(CheatDir::Down)
        } else if controls.just_pressed(&keys, Action::UiUp)
            || controls.just_pressed(&keys, Action::NoteUp)
        {
            Some(CheatDir::Up)
        } else if controls.just_pressed(&keys, Action::UiLeft)
            || controls.just_pressed(&keys, Action::NoteLeft)
        {
            Some(CheatDir::Left)
        } else if controls.just_pressed(&keys, Action::UiRight)
            || controls.just_pressed(&keys, Action::NoteRight)
        {
            Some(CheatDir::Right)
        } else {
            None
        };
        if let Some(d) = dir {
            if d == CHEAT_SEQUENCE[title.cheat_pos] {
                title.cheat_pos += 1;
                if title.cheat_pos >= CHEAT_SEQUENCE.len() {
                    title.cheat_active = true;
                    shared.ensure_menu_music(
                        &mut commands,
                        &mods,
                        &asset_server,
                        RINGTONE_PATH,
                        4.0,
                        true,
                    );
                    shared.trigger_flash([1.0, 1.0, 1.0], 1.0);
                    play_menu_sfx(&mut commands, &mods, &asset_server, CONFIRM_SFX_PATH, 0.7);
                }
            } else {
                title.cheat_pos = 0;
            }
        }
    }
}

pub struct TitlePlugin;

impl Plugin for TitlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TitleState>()
            .add_systems(Update, tick_title_system);
    }
}
