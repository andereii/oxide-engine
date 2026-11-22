use std::fs;

use bevy::prelude::*;
use bevy::sprite::Anchor;
use serde::Deserialize;

use crate::animations::{AnimationController, BaseAnimationData};
use crate::input::{Action, Controls};
use crate::mods::ModRegistry;
use crate::utils::constants::{FONT_VCR_OSD_MONO, SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::utils::ecs::set_atlas_index;

use super::menu_shared::{
    ActiveScene, AppScene, MenuSharedState, PendingSongLaunch, load_sparrow_sprite, play_menu_sfx,
};

const MENU_BG_PATH: &str = "ui/main-menu/menu-bg.png";
const MENU_MAGENTA_PATH: &str = "ui/main-menu/menu-bg-magenta.png";
const MENU_DESAT_PATH: &str = "ui/main-menu/menu-desat.png";
const FREAKY_MENU_PATH: &str = "ui/main-menu/freaky-menu/freaky-menu.ogg";
const SCROLL_SFX_PATH: &str = "ui/main-menu/scroll-menu.ogg";
const CONFIRM_SFX_PATH: &str = "ui/main-menu/confirm-menu.ogg";
const CANCEL_SFX_PATH: &str = "ui/main-menu/cancel-menu.ogg";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MainMenuSubScreen {
    #[default]
    Root,
    StorySelect,
    FreeplaySelect,
    Options,
    Credits,
}

#[derive(Debug, Clone, Deserialize)]
struct LevelData {
    #[serde(default)]
    name: String,
    #[serde(default)]
    songs: Vec<String>,
}

#[derive(Debug, Clone)]
struct StoryWeekEntry {
    id: String,
    name: String,
    songs: Vec<String>,
}

#[derive(Debug, Clone)]
struct FreeplaySongEntry {
    id: String,
    name: String,
    difficulties: Vec<String>,
    variation: String,
}

#[derive(Component)]
pub struct MainMenuEntity;

#[derive(Component)]
pub struct MainMenuBg(pub f32);

#[derive(Component)]
pub struct MainMenuMagentaBg;

#[derive(Component)]
pub struct MainMenuItemVisual {
    pub index: usize,
    pub base_y_world: f32,
}

#[derive(Component)]
pub struct MainMenuSubOverlay;

#[derive(Component)]
pub struct MainMenuOverlayText;

#[derive(Resource)]
pub struct MainMenuState {
    pub active: bool,
    pub remembered_index: usize,
    pub selected_index: usize,
    pub interacting: bool,
    pub flicker_timer_s: f32,
    pub flicker_accum_s: f32,
    pub magenta_visible: bool,
    pub fade_out_timer_s: f32,
    pub cam_follow_y: f32,
    pub cam_current_y: f32,
    pub sub_screen: MainMenuSubScreen,
    pub weeks: Vec<StoryWeekEntry>,
    pub week_idx: usize,
    pub week_diff_idx: usize,
    pub songs: Vec<FreeplaySongEntry>,
    pub song_idx: usize,
    pub song_diff_idx: usize,
    pub options_idx: usize,
    pub overlay_dirty: bool,
}

impl Default for MainMenuState {
    fn default() -> Self {
        Self {
            active: false,
            remembered_index: 0,
            selected_index: 0,
            interacting: false,
            flicker_timer_s: 0.0,
            flicker_accum_s: 0.0,
            magenta_visible: false,
            fade_out_timer_s: 0.0,
            cam_follow_y: 0.0,
            cam_current_y: 0.0,
            sub_screen: MainMenuSubScreen::Root,
            weeks: Vec::new(),
            week_idx: 0,
            week_diff_idx: 2, // hard
            songs: Vec::new(),
            song_idx: 0,
            song_diff_idx: 2,
            options_idx: 0,
            overlay_dirty: false,
        }
    }
}

const MENU_ITEM_DEFS: [(&str, &str, &str); 5] = [
    ("storymode", "ui/main-menu/items/story-mode", "storymode"),
    ("freeplay", "ui/main-menu/items/freeplay", "freeplay"),
    ("merch", "ui/main-menu/items/merch", "merch"),
    ("options", "ui/main-menu/items/options", "options"),
    ("credits", "ui/main-menu/items/credits", "credits"),
];

fn load_story_weeks(mods: &ModRegistry) -> Vec<StoryWeekEntry> {
    let ordered_ids = [
        "tutorial",
        "week1",
        "week2",
        "week3",
        "week4",
        "week5",
        "week6",
        "week7",
        "weekend1",
        "sserafim",
    ];
    let mut out = Vec::new();
    for id in ordered_ids {
        let rel = format!("ui/story-mode/levels/{id}.json");
        if let Ok(raw) = mods.read_to_string_merged(&rel)
            && let Ok(parsed) = serde_json::from_str::<LevelData>(&raw)
            && !parsed.songs.is_empty()
        {
            out.push(StoryWeekEntry {
                id: id.to_string(),
                name: if parsed.name.is_empty() {
                    id.to_uppercase()
                } else {
                    parsed.name
                },
                songs: parsed.songs,
            });
        }
    }
    if out.is_empty() {
        out.push(StoryWeekEntry {
            id: "week1".to_string(),
            name: "DADDY DEAREST".to_string(),
            songs: vec![
                "bopeebo".to_string(),
                "fresh".to_string(),
                "dadbattle".to_string(),
            ],
        });
    }
    out
}

fn load_freeplay_songs(mods: &ModRegistry) -> Vec<FreeplaySongEntry> {
    let mut entries = Vec::new();
    let Some(songs_dir) = mods.resolve_dir("gameplay/songs") else {
        return entries;
    };
    let Ok(read) = fs::read_dir(&songs_dir) else {
        return entries;
    };
    let mut dirs: Vec<_> = read
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    dirs.sort();
    for id in dirs {
        let meta_rel = format!("gameplay/songs/{id}/chart/meta.json");
        let mut display = id.clone();
        let mut diffs = Vec::new();
        if let Ok(raw) = mods.read_to_string_merged(&meta_rel)
            && let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw)
        {
            if let Some(s) = val.get("songName").and_then(|v| v.as_str()) {
                display = s.to_string();
            }
            if let Some(obj) = val.get("difficulties").and_then(|v| v.as_object()) {
                for key in obj.keys() {
                    diffs.push(key.clone());
                }
            }
        }
        if diffs.is_empty() {
            diffs = vec![
                "easy".to_string(),
                "normal".to_string(),
                "hard".to_string(),
            ];
        } else {
            diffs.sort();
        }
        entries.push(FreeplaySongEntry {
            id,
            name: display,
            difficulties: diffs,
            variation: "default".to_string(),
        });
    }
    entries
}

fn cleanup_main_menu(
    commands: &mut Commands,
    menu: &mut MainMenuState,
    entities: &Query<Entity, With<MainMenuEntity>>,
) {
    for ent in entities {
        commands.entity(ent).despawn();
    }
    menu.active = false;
    menu.interacting = false;
    menu.flicker_timer_s = 0.0;
    menu.fade_out_timer_s = 0.0;
    menu.magenta_visible = false;
    menu.sub_screen = MainMenuSubScreen::Root;
}

fn spawn_main_menu_root(
    commands: &mut Commands,
    mods: &ModRegistry,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    menu: &mut MainMenuState,
    shared: &mut MenuSharedState,
) {
    shared.ensure_menu_music(commands, mods, asset_server, FREAKY_MENU_PATH, 0.0, false);

    // Menu BG & Magenta BG (1.2x screen width)
    let bg_srv = mods
        .resolve_asset_server_path(MENU_BG_PATH)
        .unwrap_or_else(|| MENU_BG_PATH.to_string());
    let bg_tex: Handle<Image> = asset_server.load(bg_srv);
    commands.spawn((
        Sprite::from_image(bg_tex),
        Anchor::CENTER,
        Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::splat(1.2)),
        Visibility::Visible,
        MainMenuEntity,
        MainMenuBg(0.17),
    ));

    let mag_srv = mods
        .resolve_asset_server_path(MENU_MAGENTA_PATH)
        .unwrap_or_else(|| MENU_MAGENTA_PATH.to_string());
    let mag_tex: Handle<Image> = asset_server.load(mag_srv);
    commands.spawn((
        Sprite::from_image(mag_tex),
        Anchor::CENTER,
        Transform::from_xyz(0.0, 0.0, 0.5).with_scale(Vec3::splat(1.2)),
        Visibility::Hidden,
        MainMenuEntity,
        MainMenuMagentaBg,
        MainMenuBg(0.17),
    ));

    let spacing: f32 = 160.0;
    let count = MENU_ITEM_DEFS.len();
    let top_flx = (SCREEN_HEIGHT - spacing * ((count - 1) as f32)) * 0.5;
    menu.selected_index = menu.remembered_index.min(count - 1);

    for (index, (_name, atlas_path, prefix_base)) in MENU_ITEM_DEFS.iter().enumerate() {
        let anims = vec![
            BaseAnimationData {
                name: "idle".to_string(),
                prefix: format!("{prefix_base} idle"),
                frame_rate: 24.0,
                looped: true,
                ..Default::default()
            },
            BaseAnimationData {
                name: "selected".to_string(),
                prefix: format!("{prefix_base} selected"),
                frame_rate: 24.0,
                looped: true,
                ..Default::default()
            },
        ];
        if let Some((spr, mut ctrl)) =
            load_sparrow_sprite(mods, asset_server, layouts, atlas_path, &anims)
        {
            let target_anim = if index == menu.selected_index {
                "selected"
            } else {
                "idle"
            };
            ctrl.play(target_anim, true, false, 0);
            let flx_y = top_flx + spacing * (index as f32);
            let bevy_y = SCREEN_HEIGHT * 0.5 - flx_y;
            commands.spawn((
                spr,
                Anchor::CENTER,
                Transform::from_xyz(0.0, bevy_y, 2.0),
                Visibility::Visible,
                ctrl,
                MainMenuEntity,
                MainMenuItemVisual {
                    index,
                    base_y_world: bevy_y,
                },
            ));
            if index == menu.selected_index {
                menu.cam_follow_y = bevy_y;
                menu.cam_current_y = bevy_y;
            }
        }
    }

    let font_srv = mods
        .resolve_asset_server_path(FONT_VCR_OSD_MONO)
        .unwrap_or_else(|| FONT_VCR_OSD_MONO.to_string());
    let font_h: Handle<Font> = asset_server.load(font_srv);
    commands.spawn((
        Text2d::new("Oxide Engine (Friday Night Funkin')"),
        TextFont {
            font: font_h.clone().into(),
            font_size: FontSize::Px(16.0),
            ..default()
        },
        TextColor(Color::WHITE),
        bevy::sprite::Text2dShadow {
            offset: Vec2::new(1.0, -1.0),
            color: Color::BLACK,
        },
        Anchor::BOTTOM_LEFT,
        Transform::from_xyz(-SCREEN_WIDTH * 0.5 + 12.0, -SCREEN_HEIGHT * 0.5 + 10.0, 10.0),
        Visibility::Visible,
        MainMenuEntity,
    ));

    // Subscreen overlay (hidden on Root)
    let desat_srv = mods
        .resolve_asset_server_path(MENU_DESAT_PATH)
        .unwrap_or_else(|| MENU_DESAT_PATH.to_string());
    let desat_tex: Handle<Image> = asset_server.load(desat_srv);
    commands.spawn((
        Sprite {
            image: desat_tex,
            color: Color::srgba(0.18, 0.22, 0.42, 0.94),
            ..default()
        },
        Anchor::CENTER,
        Transform::from_xyz(0.0, 0.0, 20.0).with_scale(Vec3::splat(1.1)),
        Visibility::Hidden,
        MainMenuEntity,
        MainMenuSubOverlay,
    ));

    commands.spawn((
        Text2d::new(""),
        TextFont {
            font: font_h.into(),
            font_size: FontSize::Px(26.0),
            ..default()
        },
        TextColor(Color::WHITE),
        bevy::sprite::Text2dShadow {
            offset: Vec2::new(2.0, -2.0),
            color: Color::BLACK,
        },
        Anchor::CENTER,
        Transform::from_xyz(0.0, 0.0, 22.0),
        Visibility::Hidden,
        MainMenuEntity,
        MainMenuOverlayText,
    ));
}

fn format_subscreen_text(menu: &MainMenuState, app_scene: &AppScene) -> String {
    match menu.sub_screen {
        MainMenuSubScreen::Root => String::new(),
        MainMenuSubScreen::StorySelect => {
            let mut out = String::from("STORY MODE\n\n");
            let diff = ["easy", "normal", "hard"][menu.week_diff_idx.min(2)];
            for (i, w) in menu.weeks.iter().enumerate() {
                let marker = if i == menu.week_idx { "> " } else { "  " };
                out.push_str(&format!("{marker}{}  ({})\n", w.name, w.songs.join(", ")));
            }
            out.push_str(&format!(
                "\nDifficulty: < {} >\n[Up/Down] Week  [Left/Right] Difficulty  [Enter] Play  [Esc] Back",
                diff.to_uppercase()
            ));
            out
        }
        MainMenuSubScreen::FreeplaySelect => {
            let mut out = String::from("FREEPLAY\n\n");
            let total = menu.songs.len();
            if total == 0 {
                out.push_str("No songs found.\n");
            } else {
                let start = menu.song_idx.saturating_sub(5);
                let end = (start + 11).min(total);
                for i in start..end {
                    let s = &menu.songs[i];
                    let marker = if i == menu.song_idx { "> " } else { "  " };
                    out.push_str(&format!("{marker}{} [{}]\n", s.name, s.id));
                }
                let cur = &menu.songs[menu.song_idx];
                let diff = cur
                    .difficulties
                    .get(menu.song_diff_idx.min(cur.difficulties.len().saturating_sub(1)))
                    .map(String::as_str)
                    .unwrap_or("hard");
                out.push_str(&format!(
                    "\nDifficulty: < {} >\n[Up/Down] Song  [Left/Right] Diff  [Enter] Play  [Esc] Back",
                    diff.to_uppercase()
                ));
            }
            out
        }
        MainMenuSubScreen::Options => {
            let o = &app_scene.options;
            let rows = [
                ("Downscroll", if o.downscroll { "ON" } else { "OFF" }),
                ("Ghost Tapping", if o.ghost_tap { "ON" } else { "OFF" }),
                ("Botplay", if o.botplay { "ON" } else { "OFF" }),
                ("Practice Mode", if o.practice_mode { "ON" } else { "OFF" }),
                ("Shaders", if !o.no_shaders { "ON" } else { "OFF" }),
            ];
            let mut out = String::from("OPTIONS\n\n");
            for (i, (label, val)) in rows.iter().enumerate() {
                let marker = if i == menu.options_idx { "> " } else { "  " };
                out.push_str(&format!("{marker}{label}: {val}\n"));
            }
            out.push_str("\n[Up/Down] Select  [Enter/Left/Right] Toggle  [Esc] Back");
            out
        }
        MainMenuSubScreen::Credits => String::from(
            "CREDITS\n\nFRIDAY NIGHT FUNKIN'\nThe Funkin' Crew Inc.\n\nOXIDE ENGINE\nPorted in Rust + Bevy\n\n[Esc] Back",
        ),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn tick_main_menu_system(
    time: Res<Time>,
    mut commands: Commands,
    mods: Res<ModRegistry>,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    keys: Res<ButtonInput<KeyCode>>,
    controls: Res<Controls>,
    mut app_scene: ResMut<AppScene>,
    mut shared: ResMut<MenuSharedState>,
    mut menu: ResMut<MainMenuState>,
    entities: Query<Entity, With<MainMenuEntity>>,
    mut items_q: Query<(&MainMenuItemVisual, &mut Sprite, &mut AnimationController, &mut Transform, &mut Visibility)>,
    mut mag_q: Query<&mut Visibility, (With<MainMenuMagentaBg>, Without<MainMenuItemVisual>, Without<MainMenuSubOverlay>, Without<MainMenuOverlayText>)>,
    mut bg_q: Query<(&MainMenuBg, &mut Transform), Without<MainMenuItemVisual>>,
    mut sub_bg_q: Query<&mut Visibility, (With<MainMenuSubOverlay>, Without<MainMenuItemVisual>, Without<MainMenuMagentaBg>, Without<MainMenuOverlayText>)>,
    mut sub_txt_q: Query<(&mut Text2d, &mut Visibility), (With<MainMenuOverlayText>, Without<MainMenuSubOverlay>, Without<MainMenuItemVisual>, Without<MainMenuMagentaBg>)>,
) {
    if app_scene.current != ActiveScene::MainMenu {
        if menu.active {
            cleanup_main_menu(&mut commands, &mut menu, &entities);
        }
        return;
    }

    let dt = time.delta_secs().min(0.1);

    if !menu.active {
        menu.active = true;
        menu.weeks = load_story_weeks(&mods);
        menu.songs = load_freeplay_songs(&mods);
        menu.sub_screen = MainMenuSubScreen::Root;
        menu.overlay_dirty = true;
        spawn_main_menu_root(
            &mut commands,
            &mods,
            &asset_server,
            &mut layouts,
            &mut menu,
            &mut shared,
        );
        return;
    }

    // Smoothly track cam_follow_y
    let lerp_t = (1.0 - (1.0 - 0.06_f32).powf(dt * 60.0)).clamp(0.0, 1.0);
    menu.cam_current_y += (menu.cam_follow_y - menu.cam_current_y) * lerp_t;

    for (bg, mut tf) in &mut bg_q {
        tf.translation.y = -menu.cam_current_y * bg.0;
    }

    // Step item animations + positions
    let fade_alpha = if menu.interacting && menu.fade_out_timer_s > 0.0 {
        (menu.fade_out_timer_s / 0.4).clamp(0.0, 1.0)
    } else {
        1.0
    };
    for (item, mut spr, mut ctrl, mut tf, mut vis) in &mut items_q {
        ctrl.update(dt, None);
        if let Some(idx) = ctrl.current_atlas_index() {
            set_atlas_index(&mut spr, idx);
        }
        tf.translation.y = item.base_y_world - menu.cam_current_y * 0.4;
        if menu.sub_screen != MainMenuSubScreen::Root {
            *vis = Visibility::Hidden;
        } else if menu.interacting {
            if item.index == menu.selected_index {
                *vis = Visibility::Visible;
                spr.color = Color::WHITE;
            } else {
                *vis = Visibility::Visible;
                spr.color = Color::srgba(1.0, 1.0, 1.0, fade_alpha);
            }
        } else {
            *vis = Visibility::Visible;
            spr.color = Color::WHITE;
        }
    }

    // Magenta flicker during item confirm
    if menu.interacting {
        if menu.flicker_timer_s > 0.0 {
            menu.flicker_timer_s = (menu.flicker_timer_s - dt).max(0.0);
            menu.flicker_accum_s += dt;
            if menu.flicker_accum_s >= 0.15 {
                menu.flicker_accum_s -= 0.15;
                menu.magenta_visible = !menu.magenta_visible;
            }
        } else {
            menu.magenta_visible = false;
        }
        for mut vis in &mut mag_q {
            *vis = if menu.magenta_visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }

        menu.fade_out_timer_s -= dt;
        if menu.fade_out_timer_s <= 0.0 {
            menu.interacting = false;
            menu.magenta_visible = false;
            for mut vis in &mut mag_q {
                *vis = Visibility::Hidden;
            }
            match menu.selected_index {
                0 => menu.sub_screen = MainMenuSubScreen::StorySelect,
                1 => menu.sub_screen = MainMenuSubScreen::FreeplaySelect,
                2 => {
                    // Merch stays on menu
                }
                3 => menu.sub_screen = MainMenuSubScreen::Options,
                4 => menu.sub_screen = MainMenuSubScreen::Credits,
                _ => {}
            }
            menu.overlay_dirty = true;
        }
        return;
    }

    // Sync subscreen overlay visibility & text
    let show_overlay = menu.sub_screen != MainMenuSubScreen::Root;
    for mut vis in &mut sub_bg_q {
        *vis = if show_overlay {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    if menu.overlay_dirty {
        menu.overlay_dirty = false;
        let body = format_subscreen_text(&menu, &app_scene);
        for (mut t2d, mut vis) in &mut sub_txt_q {
            **t2d = body.clone();
            *vis = if show_overlay {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }

    let up_hit = controls.just_pressed(&keys, Action::UiUp)
        || keys.just_pressed(KeyCode::ArrowUp)
        || keys.just_pressed(KeyCode::KeyW);
    let down_hit = controls.just_pressed(&keys, Action::UiDown)
        || keys.just_pressed(KeyCode::ArrowDown)
        || keys.just_pressed(KeyCode::KeyS);
    let left_hit = controls.just_pressed(&keys, Action::UiLeft)
        || keys.just_pressed(KeyCode::ArrowLeft)
        || keys.just_pressed(KeyCode::KeyA);
    let right_hit = controls.just_pressed(&keys, Action::UiRight)
        || keys.just_pressed(KeyCode::ArrowRight)
        || keys.just_pressed(KeyCode::KeyD);
    let accept_hit = controls.just_pressed(&keys, Action::Accept)
        || keys.just_pressed(KeyCode::Enter)
        || keys.just_pressed(KeyCode::Space);
    let back_hit = controls.just_pressed(&keys, Action::Back)
        || keys.just_pressed(KeyCode::Escape)
        || keys.just_pressed(KeyCode::Backspace);

    match menu.sub_screen {
        MainMenuSubScreen::Root => {
            let count = MENU_ITEM_DEFS.len();
            let mut changed = false;
            if up_hit {
                menu.selected_index = (menu.selected_index + count - 1) % count;
                changed = true;
            } else if down_hit {
                menu.selected_index = (menu.selected_index + 1) % count;
                changed = true;
            }
            if changed {
                play_menu_sfx(&mut commands, &mods, &asset_server, SCROLL_SFX_PATH, 0.7);
                for (item, _, mut ctrl, _, _) in &mut items_q {
                    if item.index == menu.selected_index {
                        ctrl.play("selected", true, false, 0);
                        menu.cam_follow_y = item.base_y_world;
                    } else {
                        ctrl.play("idle", true, false, 0);
                    }
                }
            }
            if accept_hit {
                menu.remembered_index = menu.selected_index;
                menu.interacting = true;
                menu.flicker_timer_s = 1.1;
                menu.flicker_accum_s = 0.0;
                menu.magenta_visible = true;
                menu.fade_out_timer_s = 0.4;
                play_menu_sfx(&mut commands, &mods, &asset_server, CONFIRM_SFX_PATH, 0.7);
            } else if back_hit {
                menu.remembered_index = menu.selected_index;
                play_menu_sfx(&mut commands, &mods, &asset_server, CANCEL_SFX_PATH, 0.7);
                shared.transition_to(ActiveScene::Title, 0.35);
            }
        }
        MainMenuSubScreen::StorySelect => {
            let count = menu.weeks.len().max(1);
            if up_hit {
                menu.week_idx = (menu.week_idx + count - 1) % count;
                menu.overlay_dirty = true;
                play_menu_sfx(&mut commands, &mods, &asset_server, SCROLL_SFX_PATH, 0.7);
            } else if down_hit {
                menu.week_idx = (menu.week_idx + 1) % count;
                menu.overlay_dirty = true;
                play_menu_sfx(&mut commands, &mods, &asset_server, SCROLL_SFX_PATH, 0.7);
            } else if left_hit {
                menu.week_diff_idx = (menu.week_diff_idx + 2) % 3;
                menu.overlay_dirty = true;
                play_menu_sfx(&mut commands, &mods, &asset_server, SCROLL_SFX_PATH, 0.7);
            } else if right_hit {
                menu.week_diff_idx = (menu.week_diff_idx + 1) % 3;
                menu.overlay_dirty = true;
                play_menu_sfx(&mut commands, &mods, &asset_server, SCROLL_SFX_PATH, 0.7);
            } else if accept_hit {
                if let Some(week) = menu.weeks.get(menu.week_idx)
                    && let Some(first_song) = week.songs.first()
                {
                    let diff = ["easy", "normal", "hard"][menu.week_diff_idx.min(2)].to_string();
                    play_menu_sfx(&mut commands, &mods, &asset_server, CONFIRM_SFX_PATH, 0.7);
                    app_scene.pending_launch = Some(PendingSongLaunch {
                        song_id: first_song.clone(),
                        difficulty: diff,
                        variation: "default".to_string(),
                        story_mode: true,
                        playlist_songs: week.songs.clone(),
                    });
                    shared.transition_to(ActiveScene::Play, 0.35);
                }
            } else if back_hit {
                menu.sub_screen = MainMenuSubScreen::Root;
                menu.overlay_dirty = true;
                play_menu_sfx(&mut commands, &mods, &asset_server, CANCEL_SFX_PATH, 0.7);
            }
        }
        MainMenuSubScreen::FreeplaySelect => {
            let count = menu.songs.len();
            if count > 0 {
                if up_hit {
                    menu.song_idx = (menu.song_idx + count - 1) % count;
                    menu.overlay_dirty = true;
                    play_menu_sfx(&mut commands, &mods, &asset_server, SCROLL_SFX_PATH, 0.7);
                } else if down_hit {
                    menu.song_idx = (menu.song_idx + 1) % count;
                    menu.overlay_dirty = true;
                    play_menu_sfx(&mut commands, &mods, &asset_server, SCROLL_SFX_PATH, 0.7);
                } else if left_hit {
                    let dlen = menu.songs[menu.song_idx].difficulties.len().max(1);
                    menu.song_diff_idx = (menu.song_diff_idx + dlen - 1) % dlen;
                    menu.overlay_dirty = true;
                    play_menu_sfx(&mut commands, &mods, &asset_server, SCROLL_SFX_PATH, 0.7);
                } else if right_hit {
                    let dlen = menu.songs[menu.song_idx].difficulties.len().max(1);
                    menu.song_diff_idx = (menu.song_diff_idx + 1) % dlen;
                    menu.overlay_dirty = true;
                    play_menu_sfx(&mut commands, &mods, &asset_server, SCROLL_SFX_PATH, 0.7);
                } else if accept_hit {
                    let entry = &menu.songs[menu.song_idx];
                    let diff = entry
                        .difficulties
                        .get(menu.song_diff_idx.min(entry.difficulties.len().saturating_sub(1)))
                        .cloned()
                        .unwrap_or_else(|| "hard".to_string());
                    play_menu_sfx(&mut commands, &mods, &asset_server, CONFIRM_SFX_PATH, 0.7);
                    app_scene.pending_launch = Some(PendingSongLaunch {
                        song_id: entry.id.clone(),
                        difficulty: diff,
                        variation: entry.variation.clone(),
                        story_mode: false,
                        playlist_songs: Vec::new(),
                    });
                    shared.transition_to(ActiveScene::Play, 0.35);
                }
            }
            if back_hit {
                menu.sub_screen = MainMenuSubScreen::Root;
                menu.overlay_dirty = true;
                play_menu_sfx(&mut commands, &mods, &asset_server, CANCEL_SFX_PATH, 0.7);
            }
        }
        MainMenuSubScreen::Options => {
            if up_hit {
                menu.options_idx = (menu.options_idx + 4) % 5;
                menu.overlay_dirty = true;
                play_menu_sfx(&mut commands, &mods, &asset_server, SCROLL_SFX_PATH, 0.7);
            } else if down_hit {
                menu.options_idx = (menu.options_idx + 1) % 5;
                menu.overlay_dirty = true;
                play_menu_sfx(&mut commands, &mods, &asset_server, SCROLL_SFX_PATH, 0.7);
            } else if left_hit || right_hit || accept_hit {
                match menu.options_idx {
                    0 => app_scene.options.downscroll = !app_scene.options.downscroll,
                    1 => app_scene.options.ghost_tap = !app_scene.options.ghost_tap,
                    2 => app_scene.options.botplay = !app_scene.options.botplay,
                    3 => app_scene.options.practice_mode = !app_scene.options.practice_mode,
                    4 => app_scene.options.no_shaders = !app_scene.options.no_shaders,
                    _ => {}
                }
                menu.overlay_dirty = true;
                play_menu_sfx(&mut commands, &mods, &asset_server, CONFIRM_SFX_PATH, 0.5);
            } else if back_hit {
                menu.sub_screen = MainMenuSubScreen::Root;
                menu.overlay_dirty = true;
                play_menu_sfx(&mut commands, &mods, &asset_server, CANCEL_SFX_PATH, 0.7);
            }
        }
        MainMenuSubScreen::Credits => {
            if back_hit || accept_hit {
                menu.sub_screen = MainMenuSubScreen::Root;
                menu.overlay_dirty = true;
                play_menu_sfx(&mut commands, &mods, &asset_server, CANCEL_SFX_PATH, 0.7);
            }
        }
    }
}

pub struct MainMenuPlugin;

impl Plugin for MainMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MainMenuState>()
            .add_systems(Update, tick_main_menu_system);
    }
}
