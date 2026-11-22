use std::time::{SystemTime, UNIX_EPOCH};

use bevy::audio::{AudioPlayer, AudioSink, AudioSinkPlayback, PlaybackMode, PlaybackSettings, Volume};
use bevy::prelude::*;
use bevy::sprite::Anchor;

use crate::animations::{AnimationController, BaseAnimationData, SparrowAtlas, add_sparrow_animations};
use crate::mods::ModRegistry;
use crate::ui::SoundTrayState;
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};

pub const MENU_BPM: f64 = 102.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ActiveScene {
    #[default]
    Title,
    MainMenu,
    Play,
}

#[derive(Debug, Clone)]
pub struct PendingSongLaunch {
    pub song_id: String,
    pub difficulty: String,
    pub variation: String,
    pub story_mode: bool,
    pub playlist_songs: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct MenuGameplayOptions {
    pub downscroll: bool,
    pub ghost_tap: bool,
    pub botplay: bool,
    pub practice_mode: bool,
    pub no_shaders: bool,
}

#[derive(Resource, Debug, Clone)]
pub struct AppScene {
    pub current: ActiveScene,
    pub started_from_cli: bool,
    pub pending_launch: Option<PendingSongLaunch>,
    pub exit_to_menu_requested: bool,
    pub play_hud_spawned: bool,
    pub options: MenuGameplayOptions,
}

impl Default for AppScene {
    fn default() -> Self {
        Self {
            current: ActiveScene::Title,
            started_from_cli: false,
            pending_launch: None,
            exit_to_menu_requested: false,
            play_hud_spawned: false,
            options: MenuGameplayOptions {
                ghost_tap: true,
                ..Default::default()
            },
        }
    }
}

pub fn in_title_scene(scene: Res<AppScene>) -> bool {
    scene.current == ActiveScene::Title
}

pub fn in_main_menu_scene(scene: Res<AppScene>) -> bool {
    scene.current == ActiveScene::MainMenu
}

pub fn in_play_scene(scene: Res<AppScene>) -> bool {
    scene.current == ActiveScene::Play
}

#[derive(Component)]
pub struct MenuMusicAudio;

#[derive(Component)]
pub struct MenuFlashVisual;

#[derive(Resource, Default)]
pub struct MenuSharedState {
    pub music_entity: Option<Entity>,
    pub current_track: String,
    pub music_volume: f32,
    pub target_volume: f32,
    pub fade_rate: f32,
    pub elapsed_s: f64,
    pub flash_entity: Option<Entity>,
    pub flash_color: [f32; 3],
    pub flash_timer_s: f32,
    pub flash_dur_s: f32,
    pub fade_out_timer_s: f32,
    pub fade_out_dur_s: f32,
    pub pending_scene: Option<ActiveScene>,
    pub pending_fade_in_black: bool,
}

impl MenuSharedState {
    pub fn ensure_menu_music(
        &mut self,
        commands: &mut Commands,
        mods: &ModRegistry,
        asset_server: &AssetServer,
        rel_path: &str,
        fade_in_s: f32,
        restart: bool,
    ) {
        if !restart && self.music_entity.is_some() && self.current_track == rel_path {
            return;
        }
        self.stop_menu_music(commands);
        let srv = mods
            .resolve_asset_server_path(rel_path)
            .unwrap_or_else(|| rel_path.to_string());
        let handle: Handle<AudioSource> = asset_server.load(srv);
        let start_vol = if fade_in_s > 0.0 { 0.0 } else { 0.8 };
        let ent = commands
            .spawn((
                AudioPlayer(handle),
                PlaybackSettings {
                    mode: PlaybackMode::Loop,
                    volume: Volume::Linear(start_vol),
                    ..default()
                },
                MenuMusicAudio,
            ))
            .id();
        self.music_entity = Some(ent);
        self.current_track = rel_path.to_string();
        self.music_volume = start_vol;
        self.target_volume = 0.8;
        self.fade_rate = if fade_in_s > 0.0 { 0.8 / fade_in_s } else { 10.0 };
        self.elapsed_s = 0.0;
    }

    pub fn stop_menu_music(&mut self, commands: &mut Commands) {
        if let Some(ent) = self.music_entity.take() {
            commands.entity(ent).despawn();
        }
        self.current_track.clear();
        self.music_volume = 0.0;
        self.elapsed_s = 0.0;
    }

    pub fn trigger_flash(&mut self, color: [f32; 3], duration_s: f32) {
        self.flash_color = color;
        self.flash_dur_s = duration_s.max(0.05);
        self.flash_timer_s = self.flash_dur_s;
    }

    pub fn transition_to(&mut self, target: ActiveScene, duration_s: f32) {
        self.pending_scene = Some(target);
        self.fade_out_dur_s = duration_s.max(0.01);
        self.fade_out_timer_s = self.fade_out_dur_s;
    }
}

pub fn pseudo_random_index(len: usize) -> usize {
    if len <= 1 {
        return 0;
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize ^ (d.as_secs() as usize))
        .unwrap_or(0);
    nanos % len
}

pub fn flixel_to_bevy_tl(flx_x: f32, flx_y: f32, z: f32) -> Vec3 {
    Vec3::new(flx_x - SCREEN_WIDTH * 0.5, SCREEN_HEIGHT * 0.5 - flx_y, z)
}

pub fn play_menu_sfx(
    commands: &mut Commands,
    mods: &ModRegistry,
    asset_server: &AssetServer,
    rel_path: &str,
    volume: f32,
) {
    let srv = mods
        .resolve_asset_server_path(rel_path)
        .unwrap_or_else(|| rel_path.to_string());
    let handle: Handle<AudioSource> = asset_server.load(srv);
    commands.spawn((
        AudioPlayer(handle),
        PlaybackSettings {
            mode: PlaybackMode::Despawn,
            volume: Volume::Linear(volume.clamp(0.0, 2.0)),
            ..default()
        },
    ));
}

pub fn load_sparrow_sprite(
    mods: &ModRegistry,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    rel_path: &str,
    anims: &[BaseAnimationData],
) -> Option<(Sprite, AnimationController)> {
    let img_path = mods
        .resolve_file(rel_path)
        .or_else(|| mods.resolve_file(&format!("{rel_path}.png")))?;
    let xml_path = img_path.with_extension("xml");
    let atlas = SparrowAtlas::from_file(&xml_path).ok()?;
    let (layout, indices) = atlas.build_layout();
    let mut controller = AnimationController::default();
    add_sparrow_animations(&atlas, &indices, &mut controller, anims);
    if let Some(first) = anims.first()
        && controller.has_animation(&first.name)
    {
        controller.play(&first.name, true, false, 0);
    }
    let initial_idx = controller.current_atlas_index().unwrap_or(0);
    let tex = asset_server.load(mods.to_asset_server_path(&img_path));
    let sprite = Sprite::from_atlas_image(
        tex,
        TextureAtlas {
            layout: layouts.add(layout),
            index: initial_idx,
        },
    );
    Some((sprite, controller))
}

#[allow(clippy::too_many_arguments)]
pub fn tick_menu_shared_system(
    time: Res<Time>,
    mut commands: Commands,
    sound_tray: Option<Res<SoundTrayState>>,
    mut app_scene: ResMut<AppScene>,
    mut shared: ResMut<MenuSharedState>,
    mut sinks: Query<&mut AudioSink, With<MenuMusicAudio>>,
    mut flashes: Query<(&mut Sprite, &mut Visibility, &mut Transform), With<MenuFlashVisual>>,
    cams: Query<&Transform, (With<Camera2d>, With<IsDefaultUiCamera>, Without<MenuFlashVisual>)>,
) {
    let dt = time.delta_secs().min(0.1);
    let master_vol = sound_tray
        .as_deref()
        .map_or(1.0_f32, |st| st.effective_volume());

    if shared.music_entity.is_some() {
        shared.elapsed_s += f64::from(dt);
        if shared.music_volume < shared.target_volume {
            shared.music_volume = (shared.music_volume + shared.fade_rate * dt).min(shared.target_volume);
        }
        for mut sink in &mut sinks {
            sink.set_volume(Volume::Linear(shared.music_volume * master_vol));
        }
    }

    if shared.flash_entity.is_none() {
        let ent = commands
            .spawn((
                Sprite::from_color(
                    Color::srgba(1.0, 1.0, 1.0, 0.0),
                    Vec2::new(SCREEN_WIDTH * 4.0, SCREEN_HEIGHT * 4.0),
                ),
                Anchor::CENTER,
                Transform::from_xyz(0.0, 0.0, 900.0),
                Visibility::Hidden,
                MenuFlashVisual,
            ))
            .id();
        shared.flash_entity = Some(ent);
    }

    let cam_pos = cams
        .iter()
        .next()
        .map_or(Vec3::ZERO, |tf| Vec3::new(tf.translation.x, tf.translation.y, 900.0));

    let mut overlay_color: Option<Color> = None;
    if shared.fade_out_timer_s > 0.0 && shared.pending_scene.is_some() {
        shared.fade_out_timer_s = (shared.fade_out_timer_s - dt).max(0.0);
        let prog = 1.0 - (shared.fade_out_timer_s / shared.fade_out_dur_s.max(0.01));
        overlay_color = Some(Color::srgba(0.0, 0.0, 0.0, prog.clamp(0.0, 1.0)));
        if shared.fade_out_timer_s <= 0.0
            && let Some(target) = shared.pending_scene.take()
        {
            if target == ActiveScene::Play {
                shared.stop_menu_music(&mut commands);
            }
            app_scene.current = target;
            shared.pending_fade_in_black = true;
            shared.fade_out_dur_s = 0.35;
            shared.fade_out_timer_s = 0.35;
        }
    } else if shared.pending_fade_in_black {
        shared.fade_out_timer_s = (shared.fade_out_timer_s - dt).max(0.0);
        let a = (shared.fade_out_timer_s / shared.fade_out_dur_s.max(0.01)).clamp(0.0, 1.0);
        overlay_color = Some(Color::srgba(0.0, 0.0, 0.0, a));
        if shared.fade_out_timer_s <= 0.0 {
            shared.pending_fade_in_black = false;
        }
    } else if shared.flash_timer_s > 0.0 {
        shared.flash_timer_s = (shared.flash_timer_s - dt).max(0.0);
        let a = (shared.flash_timer_s / shared.flash_dur_s.max(0.01)).clamp(0.0, 1.0);
        let c = shared.flash_color;
        overlay_color = Some(Color::srgba(c[0], c[1], c[2], a));
    }

    for (mut spr, mut vis, mut tf) in &mut flashes {
        tf.translation = cam_pos;
        if let Some(col) = overlay_color {
            spr.color = col;
            *vis = Visibility::Visible;
        } else {
            *vis = Visibility::Hidden;
        }
    }
}

pub struct MenuSharedPlugin;

impl Plugin for MenuSharedPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuSharedState>()
            .add_systems(Update, tick_menu_shared_system);
    }
}
