use std::time::Duration;

use bevy::app::AppExit;
use bevy::audio::{AudioPlayer, AudioSink, AudioSinkPlayback, PlaybackMode, PlaybackSettings, Volume};
use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::window::PrimaryWindow;

use crate::input::{Action, Controls};
use crate::mods::ModRegistry;
use crate::utils::alphabet::{
    AlphabetChar, AlphabetFontCache, AlphabetText, AtlasFont, set_alphabet_text_alpha,
    spawn_alphabet_text,
};
use crate::utils::constants::{FONT_VCR_OSD_MONO, SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::utils::easing::EaseKind;

use super::super::plugin::PendingSongReload;
use super::super::scene::PlayState;
use super::super::song::list_song_difficulties_in;
use super::super::types::{PauseMode, PlayPhase};
use super::components::{
    BG_TARGET_ALPHA, BG_TWEEN_DURATION_S, CHARTER_FADE_DELAY_S, CHARTER_FADE_DURATION_S,
    CharterCyclePhase, MENU_BASE_X_FLX, MENU_SCROLL_SOUND_PATH, MENU_SCROLL_SOUND_VOLUME,
    MENU_TWEEN_DURATION_S, META_STAGGER_DELAY_S, META_TWEEN_DURATION_S, MUSIC_FADE_IN_TIME_S,
    MUSIC_FINAL_VOLUME, OFFSET_FAST_DELAY_S, OFFSET_FAST_RATE_MS_PER_S, OFFSET_MAX_MS,
    OFFSET_MIN_MS, PAUSE_BG_Z, PAUSE_CAMERA_ORDER, PAUSE_MENU_Z, PAUSE_META_Z,
    PauseBackgroundVisual, PauseMenuAction, PauseMenuEntryVisual, PauseMetadataItem,
    PauseMusicAudio, PauseOverlayCamera, PauseSubSceneEntity, PauseSubSceneState,
    pause_render_layer,
};

fn resolve_asset(mods: Option<&ModRegistry>, rel: &str) -> String {
    mods.and_then(|m| m.resolve_asset_server_path(rel))
        .unwrap_or_else(|| rel.to_string())
}

fn to_title_case(raw: &str) -> String {
    raw.split(['-', '_', ' '])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    let mut word = first.to_uppercase().to_string();
                    word.push_str(&chars.as_str().to_ascii_lowercase());
                    word
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn extract_artist_and_charter(credits: &[String]) -> (String, String) {
    let mut artist: Option<String> = None;
    let mut charter: Option<String> = None;

    for line in credits {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("artist:") {
            artist = Some(trimmed.to_string());
        } else if lower.starts_with("charter:") {
            charter = Some(trimmed.to_string());
        } else if artist.is_none() && !trimmed.is_empty() {
            artist = Some(format!("Artist: {trimmed}"));
        }
    }

    (
        artist.unwrap_or_else(|| "Artist: Kawai Sprite".to_string()),
        charter.unwrap_or_else(|| "Charter: Unknown".to_string()),
    )
}

fn resolve_pause_music_asset(mods: Option<&ModRegistry>, state: &PlayState) -> String {
    let player_id = state
        .chart
        .characters
        .players
        .first()
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    let suffix = if player_id.contains("pixel") {
        "-pixel"
    } else if player_id.starts_with("pico") {
        "-pico"
    } else {
        ""
    };

    if !suffix.is_empty() {
        let candidate = format!("ui/pause/music/breakfast{suffix}/breakfast{suffix}.ogg");
        if let Some(m) = mods
            && let Some(resolved) = m.resolve_asset_server_path(&candidate)
        {
            return resolved;
        }
    }
    resolve_asset(mods, "ui/pause/music/breakfast/breakfast.ogg")
}

#[inline]
fn flx_to_world(x_flx: f32, y_flx: f32, z: f32) -> Vec3 {
    Vec3::new(-SCREEN_WIDTH * 0.5 + x_flx, SCREEN_HEIGHT * 0.5 - y_flx, z)
}

fn build_menu_entries(
    mode: PauseMode,
    state: &PlayState,
    mods: Option<&ModRegistry>,
) -> Vec<(String, PauseMenuAction)> {
    match mode {
        PauseMode::Standard => {
            let mut entries = vec![
                ("Resume".to_string(), PauseMenuAction::Resume),
                ("Restart Song".to_string(), PauseMenuAction::RestartSong),
                (
                    "Change Difficulty".to_string(),
                    PauseMenuAction::SwitchMode(PauseMode::Difficulty),
                ),
            ];
            if !state.is_practice_mode {
                entries.push((
                    "Enable Practice Mode".to_string(),
                    PauseMenuAction::EnablePracticeMode,
                ));
            }
            entries.push(("Exit to Menu".to_string(), PauseMenuAction::ExitToMenu));
            entries
        }
        PauseMode::Charting => vec![
            ("Resume".to_string(), PauseMenuAction::Resume),
            ("Restart Song".to_string(), PauseMenuAction::RestartSong),
            (
                "Return to Chart Editor".to_string(),
                PauseMenuAction::ReturnToChartEditor,
            ),
        ],
        PauseMode::Difficulty => {
            let mut entries = Vec::new();
            if let Some(m) = mods {
                for diff in list_song_difficulties_in(m, &state.chart.song_id, &state.current_difficulty) {
                    entries.push((to_title_case(&diff), PauseMenuAction::ChangeDifficulty(diff)));
                }
            }
            entries.push((
                "Back".to_string(),
                PauseMenuAction::SwitchMode(PauseMode::Standard),
            ));
            entries
        }
        PauseMode::Conversation => vec![
            ("Resume".to_string(), PauseMenuAction::Resume),
            ("Skip Dialogue".to_string(), PauseMenuAction::SkipConversation),
            (
                "Restart Dialogue".to_string(),
                PauseMenuAction::RestartConversation,
            ),
            ("Exit to Menu".to_string(), PauseMenuAction::ExitToMenu),
        ],
        PauseMode::Cutscene => vec![
            ("Resume".to_string(), PauseMenuAction::Resume),
            ("Skip Cutscene".to_string(), PauseMenuAction::SkipCutscene),
            (
                "Restart Cutscene".to_string(),
                PauseMenuAction::RestartCutscene,
            ),
            ("Exit to Menu".to_string(), PauseMenuAction::ExitToMenu),
        ],
    }
}

fn menu_target_coords(entry_index: usize, current_entry: usize) -> (f32, f32) {
    let diff = (entry_index as f32) - (current_entry as f32);
    let scaled = diff * 1.3;
    let target_x = scaled * 20.0 + MENU_BASE_X_FLX;
    let target_y = scaled * 120.0 + (SCREEN_HEIGHT * 0.48);
    (target_x, target_y)
}

fn spawn_pause_menu_entries(
    commands: &mut Commands,
    pause: &mut PauseSubSceneState,
    state: &PlayState,
    mods: Option<&ModRegistry>,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    font_cache: &mut AlphabetFontCache,
) {
    let entries = build_menu_entries(pause.mode, state, mods);
    pause.current_entry = 0;
    pause.entry_count = entries.len();

    let Some(font_data) = font_cache.get_or_load(AtlasFont::Bold, mods, asset_server, layouts) else {
        return;
    };
    let layer = pause_render_layer();

    for (idx, (label, action)) in entries.into_iter().enumerate() {
        let initial_x = 0.0;
        let initial_y = 70.0 * (idx as f32) + 30.0;
        let (target_x, target_y) = menu_target_coords(idx, 0);
        let alpha = if idx == 0 { 1.0 } else { 0.6 };

        let ent = spawn_alphabet_text(
            commands,
            AtlasFont::Bold,
            font_data,
            &label,
            flx_to_world(initial_x, initial_y, PAUSE_MENU_Z),
            1.0,
            alpha,
            Some(layer.clone()),
        );
        commands.entity(ent).insert((
            PauseSubSceneEntity,
            PauseMenuEntryVisual {
                index: idx,
                action,
                start_x_flx: initial_x,
                start_y_flx: initial_y,
                target_x_flx: target_x,
                target_y_flx: target_y,
                current_x_flx: initial_x,
                current_y_flx: initial_y,
                tween_elapsed_s: 0.0,
                alpha,
            },
        ));
    }
}

fn deaths_label(mode: PauseMode, death_counter: u32) -> String {
    match mode {
        PauseMode::Standard | PauseMode::Difficulty => format!("{death_counter} Blue Balls"),
        PauseMode::Charting => "Chart Editor Preview".to_string(),
        PauseMode::Conversation => "Dialogue Paused".to_string(),
        PauseMode::Cutscene => "Video Paused".to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn sync_pause_subscene(
    mut commands: Commands,
    state: Res<PlayState>,
    mods: Option<Res<ModRegistry>>,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut font_cache: ResMut<AlphabetFontCache>,
    mut pause: ResMut<PauseSubSceneState>,
    entities: Query<Entity, With<PauseSubSceneEntity>>,
    menu_entries: Query<Entity, With<PauseMenuEntryVisual>>,
    mut meta_texts: Query<(&PauseMetadataItem, &mut Text2d, &mut Visibility)>,
) {
    let PlayPhase::Paused(requested_mode) = state.phase else {
        if pause.active {
            for ent in &entities {
                commands.entity(ent).despawn();
            }
            *pause = PauseSubSceneState::default();
        }
        return;
    };

    let mods_ref = mods.as_deref();

    if !pause.active {
        let (artist_label, charter_label) = extract_artist_and_charter(&state.chart.credits);
        *pause = PauseSubSceneState {
            active: true,
            mode: requested_mode,
            current_entry: 0,
            entry_count: 0,
            just_opened: true,
            allow_input: true,
            debug_hidden: false,
            bg_elapsed_s: 0.0,
            charter_phase: CharterCyclePhase::WaitingToFadeOut,
            charter_timer_s: CHARTER_FADE_DELAY_S,
            showing_charter: false,
            artist_label: artist_label.clone(),
            charter_label,
            fast_offset: false,
            last_offset_press_s: 0.0,
            needs_menu_rebuild: false,
            music_was_focused: true,
        };

        let layer = pause_render_layer();

        commands.spawn((
            Camera2d,
            Camera {
                order: PAUSE_CAMERA_ORDER,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            Msaa::Off,
            layer.clone(),
            PauseOverlayCamera,
            PauseSubSceneEntity,
        ));

        commands.spawn((
            Sprite::from_color(
                Color::srgba(0.0, 0.0, 0.0, 0.0),
                Vec2::new(SCREEN_WIDTH * 4.0, SCREEN_HEIGHT * 4.0),
            ),
            Transform::from_xyz(0.0, 0.0, PAUSE_BG_Z),
            layer.clone(),
            PauseBackgroundVisual,
            PauseSubSceneEntity,
        ));

        let music_path = resolve_pause_music_asset(mods_ref, &state);
        let music_handle: Handle<AudioSource> = asset_server.load(music_path);
        commands.spawn((
            AudioPlayer(music_handle),
            PlaybackSettings {
                mode: PlaybackMode::Loop,
                volume: Volume::Linear(0.0),
                ..default()
            },
            PauseMusicAudio {
                fade_elapsed_s: 0.0,
                random_seek_done: false,
            },
            PauseSubSceneEntity,
        ));

        let font_handle: Handle<Font> =
            asset_server.load(resolve_asset(mods_ref, FONT_VCR_OSD_MONO));
        let right_x = SCREEN_WIDTH * 0.5 - 20.0;

        let diff_title = to_title_case(&state.current_difficulty);
        let meta_specs: [(String, f32, f32, bool, bool, bool, bool); 7] = [
            (state.chart.song_name.clone(), 15.0, 32.0, false, false, false, false),
            (artist_label, 47.0, 32.0, false, true, false, false),
            (format!("Difficulty: {diff_title}"), 79.0, 32.0, false, false, false, false),
            (
                deaths_label(pause.mode, state.death_counter),
                111.0,
                32.0,
                false,
                false,
                true,
                false,
            ),
            ("PRACTICE MODE".to_string(), 143.0, 32.0, true, false, false, false),
            (
                format!("Global Offset: {}ms", state.conductor.global_offset_ms.round() as i32),
                SCREEN_HEIGHT - 72.0,
                16.0,
                false,
                false,
                false,
                true,
            ),
            (
                "Hold SHIFT-UP/DOWN,\nto change the offset.".to_string(),
                SCREEN_HEIGHT - 52.0,
                16.0,
                false,
                false,
                false,
                false,
            ),
        ];

        for (idx, (label, start_y_flx, px, is_practice, is_artist, is_deaths, is_offset)) in
            meta_specs.into_iter().enumerate()
        {
            let vis = if is_practice && !state.is_practice_mode {
                Visibility::Hidden
            } else {
                Visibility::Visible
            };
            commands.spawn((
                Text2d::new(label),
                TextFont {
                    font: font_handle.clone().into(),
                    font_size: FontSize::Px(px),
                    ..default()
                },
                TextColor(Color::srgba(1.0, 1.0, 1.0, 0.0)),
                TextLayout::justify(Justify::Right),
                Anchor::TOP_RIGHT,
                Transform::from_xyz(right_x, SCREEN_HEIGHT * 0.5 - start_y_flx, PAUSE_META_Z),
                vis,
                layer.clone(),
                PauseMetadataItem {
                    start_y_flx,
                    target_y_flx: start_y_flx + 5.0,
                    delay_s: META_STAGGER_DELAY_S * (idx as f32 + 1.0),
                    elapsed_s: 0.0,
                    is_practice_only: is_practice,
                    is_artist_slot: is_artist,
                    is_deaths_slot: is_deaths,
                    is_offset_slot: is_offset,
                },
                PauseSubSceneEntity,
            ));
        }

        spawn_pause_menu_entries(
            &mut commands,
            &mut pause,
            &state,
            mods_ref,
            &asset_server,
            &mut layouts,
            &mut font_cache,
        );
        return;
    }

    if pause.needs_menu_rebuild {
        pause.needs_menu_rebuild = false;
        for ent in &menu_entries {
            commands.entity(ent).despawn();
        }
        for (meta, mut txt, mut vis) in &mut meta_texts {
            if meta.is_practice_only {
                let target = if state.is_practice_mode && !pause.debug_hidden {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                };
                if *vis != target {
                    *vis = target;
                }
            }
            if meta.is_deaths_slot {
                **txt = deaths_label(pause.mode, state.death_counter);
            }
        }
        spawn_pause_menu_entries(
            &mut commands,
            &mut pause,
            &state,
            mods_ref,
            &asset_server,
            &mut layouts,
            &mut font_cache,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_pause_input(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    controls: Res<Controls>,
    mods: Option<Res<ModRegistry>>,
    asset_server: Res<AssetServer>,
    mut state: ResMut<PlayState>,
    mut pause: ResMut<PauseSubSceneState>,
    mut pending_reload: ResMut<PendingSongReload>,
    mut dialogue: ResMut<crate::play::cutscene::DialogueRuntime>,
    mut video: ResMut<crate::play::cutscene::VideoCutsceneState>,
    mut app_scene: ResMut<crate::ui::AppScene>,
    mut exit: MessageWriter<AppExit>,
    mut menu_entries: Query<&mut PauseMenuEntryVisual>,
    mut meta_items: Query<(&PauseMetadataItem, &mut Text2d, &mut Visibility), Without<PauseBackgroundVisual>>,
    mut bg_query: Query<&mut Visibility, With<PauseBackgroundVisual>>,
) {
    if !pause.active || !matches!(state.phase, PlayPhase::Paused(_)) || !pause.allow_input {
        return;
    }

    // Offset modification (Hold SHIFT + UP/DOWN) matching PauseSubState.hx.
    let shift_held = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let up_held = controls.pressed(&keys, Action::UiUp);
    let down_held = controls.pressed(&keys, Action::UiDown);
    if shift_held && (up_held || down_held) {
        let up_just = controls.just_pressed(&keys, Action::UiUp);
        let down_just = controls.just_pressed(&keys, Action::UiDown);
        pause.last_offset_press_s += time.delta_secs();

        let mut offset = state.conductor.global_offset_ms;
        if !pause.fast_offset {
            if pause.last_offset_press_s > OFFSET_FAST_DELAY_S {
                pause.fast_offset = true;
                pause.last_offset_press_s = 0.0;
            }
            if up_just || down_just {
                offset += if up_just || up_held { 1.0 } else { -1.0 };
            }
        } else {
            let dir = if up_just || up_held { 1.0 } else { -1.0 };
            offset += dir * (f64::from(time.delta_secs()) * OFFSET_FAST_RATE_MS_PER_S);
        }
        offset = offset.clamp(OFFSET_MIN_MS, OFFSET_MAX_MS);
        state.conductor.global_offset_ms = offset;

        for (meta, mut txt, _) in &mut meta_items {
            if meta.is_offset_slot {
                **txt = format!("Global Offset: {}ms", offset.round() as i32);
            }
        }
        return;
    } else {
        pause.fast_offset = false;
        pause.last_offset_press_s = 0.0;
    }

    // H key toggles pause overlay visibility for clean screenshots (PauseSubState.hx:870).
    if keys.just_pressed(KeyCode::KeyH) {
        pause.debug_hidden = !pause.debug_hidden;
        let vis = if pause.debug_hidden {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };
        for mut bg_vis in &mut bg_query {
            *bg_vis = vis;
        }
        for (meta, _, mut m_vis) in &mut meta_items {
            if meta.is_practice_only && !state.is_practice_mode {
                *m_vis = Visibility::Hidden;
            } else {
                *m_vis = vis;
            }
        }
    }

    let mut selection_delta: isize = 0;
    if controls.just_pressed(&keys, Action::UiUp) {
        selection_delta -= 1;
    }
    if controls.just_pressed(&keys, Action::UiDown) {
        selection_delta += 1;
    }

    if selection_delta != 0 && pause.entry_count > 0 {
        let len = pause.entry_count as isize;
        let prev = pause.current_entry;
        pause.current_entry = ((pause.current_entry as isize + selection_delta).rem_euclid(len)) as usize;

        if pause.current_entry != prev {
            let scroll_path = resolve_asset(mods.as_deref(), MENU_SCROLL_SOUND_PATH);
            let scroll_handle: Handle<AudioSource> = asset_server.load(scroll_path);
            commands.spawn((
                AudioPlayer(scroll_handle),
                PlaybackSettings {
                    mode: PlaybackMode::Despawn,
                    volume: Volume::Linear(MENU_SCROLL_SOUND_VOLUME),
                    ..default()
                },
            ));

            for mut entry in &mut menu_entries {
                let is_selected = entry.index == pause.current_entry;
                entry.alpha = if is_selected { 1.0 } else { 0.6 };
                entry.start_x_flx = entry.current_x_flx;
                entry.start_y_flx = entry.current_y_flx;
                let (tx, ty) = menu_target_coords(entry.index, pause.current_entry);
                entry.target_x_flx = tx;
                entry.target_y_flx = ty;
                entry.tween_elapsed_s = 0.0;
            }
        }
    }

    // Ignore Accept/Pause on the first frame so opening the pause menu doesn't immediately close it.
    if pause.just_opened {
        pause.just_opened = false;
        return;
    }

    if controls.just_pressed(&keys, Action::Accept) && pause.entry_count > 0 {
        let selected_action = menu_entries
            .iter()
            .find(|e| e.index == pause.current_entry)
            .map(|e| e.action.clone());

        if let Some(action) = selected_action {
            match action {
                PauseMenuAction::Resume => {
                    state.resume();
                }
                PauseMenuAction::SkipCutscene => {
                    video.queue_skip();
                    state.resume();
                }
                PauseMenuAction::SkipConversation => {
                    dialogue.queue_skip();
                    state.resume();
                }
                PauseMenuAction::RestartCutscene => {
                    video.queue_restart();
                    state.resume();
                }
                PauseMenuAction::RestartConversation => {
                    dialogue.queue_restart();
                    state.resume();
                }
                PauseMenuAction::RestartSong => {
                    state.needs_reset = true;
                    state.resume();
                }
                PauseMenuAction::SwitchMode(target_mode) => {
                    pause.mode = target_mode;
                    pause.needs_menu_rebuild = true;
                }
                PauseMenuAction::ChangeDifficulty(diff) => {
                    pending_reload.request = Some((state.chart.song_id.clone(), Some(diff), None));
                    state.resume();
                }
                PauseMenuAction::EnablePracticeMode => {
                    state.is_practice_mode = true;
                    pause.needs_menu_rebuild = true;
                }
                PauseMenuAction::ExitToMenu | PauseMenuAction::ReturnToChartEditor => {
                    pause.allow_input = false;
                    if app_scene.started_from_cli {
                        exit.write(AppExit::Success);
                    } else {
                        app_scene.exit_to_menu_requested = true;
                        state.resume();
                    }
                }
            }
        }
    } else if controls.just_pressed(&keys, Action::Pause) {
        state.resume();
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn animate_pause_subscene(
    time: Res<Time>,
    windows: Query<&Window, With<PrimaryWindow>>,
    audio_sources: Res<Assets<AudioSource>>,
    mut pause: ResMut<PauseSubSceneState>,
    mut bg_query: Query<&mut Sprite, With<PauseBackgroundVisual>>,
    mut meta_query: Query<
        (&mut PauseMetadataItem, &mut Text2d, &mut TextColor, &mut Transform),
        Without<PauseBackgroundVisual>,
    >,
    mut menu_query: Query<
        (Entity, &mut PauseMenuEntryVisual, &mut Transform, &mut Visibility),
        (Without<PauseBackgroundVisual>, Without<PauseMetadataItem>),
    >,
    mut alpha_texts: Query<&mut AlphabetText>,
    mut alpha_chars: Query<&mut Sprite, (With<AlphabetChar>, Without<PauseBackgroundVisual>)>,
    mut music_query: Query<(&AudioPlayer, &mut PauseMusicAudio, &mut AudioSink)>,
) {
    if !pause.active {
        return;
    }

    let dt = time.delta_secs();

    // Background alpha fade-in (0.0 -> 0.6 over 0.8s quartOut).
    pause.bg_elapsed_s = (pause.bg_elapsed_s + dt).min(BG_TWEEN_DURATION_S);
    let bg_t = EaseKind::QuartOut.eval(f64::from(pause.bg_elapsed_s / BG_TWEEN_DURATION_S)) as f32;
    let bg_alpha = BG_TARGET_ALPHA * bg_t;
    for mut sprite in &mut bg_query {
        sprite.color = Color::srgba(0.0, 0.0, 0.0, bg_alpha);
    }

    // Artist <-> Charter cycling timer (15s wait -> 0.75s fade out -> swap -> 0.75s fade in).
    let mut artist_cycle_alpha = 1.0_f32;
    let mut swap_artist_text = false;
    match pause.charter_phase {
        CharterCyclePhase::WaitingToFadeOut => {
            pause.charter_timer_s -= dt;
            if pause.charter_timer_s <= 0.0 {
                pause.charter_phase = CharterCyclePhase::FadingOut;
                pause.charter_timer_s = 0.0;
            }
        }
        CharterCyclePhase::FadingOut => {
            pause.charter_timer_s += dt;
            let u = (pause.charter_timer_s / CHARTER_FADE_DURATION_S).clamp(0.0, 1.0);
            artist_cycle_alpha = 1.0 - EaseKind::QuartOut.eval(f64::from(u)) as f32;
            if u >= 1.0 {
                pause.showing_charter = !pause.showing_charter;
                swap_artist_text = true;
                pause.charter_phase = CharterCyclePhase::FadingIn;
                pause.charter_timer_s = 0.0;
            }
        }
        CharterCyclePhase::FadingIn => {
            pause.charter_timer_s += dt;
            let u = (pause.charter_timer_s / CHARTER_FADE_DURATION_S).clamp(0.0, 1.0);
            artist_cycle_alpha = EaseKind::QuartOut.eval(f64::from(u)) as f32;
            if u >= 1.0 {
                pause.charter_phase = CharterCyclePhase::WaitingToFadeOut;
                pause.charter_timer_s = CHARTER_FADE_DELAY_S;
            }
        }
    }

    // Staggered metadata fade-in & drop-in.
    for (mut meta, mut text, mut color, mut tf) in &mut meta_query {
        meta.elapsed_s += dt;
        let active_t = ((meta.elapsed_s - meta.delay_s) / META_TWEEN_DURATION_S).clamp(0.0, 1.0);
        let eased = EaseKind::QuartOut.eval(f64::from(active_t)) as f32;
        let y_flx = meta.start_y_flx + (meta.target_y_flx - meta.start_y_flx) * eased;
        tf.translation.y = SCREEN_HEIGHT * 0.5 - y_flx;

        let mut alpha = eased;
        if meta.is_artist_slot {
            if swap_artist_text {
                **text = if pause.showing_charter {
                    pause.charter_label.clone()
                } else {
                    pause.artist_label.clone()
                };
            }
            if active_t >= 1.0 {
                alpha *= artist_cycle_alpha;
            }
        }
        color.0 = Color::srgba(1.0, 1.0, 1.0, alpha);
    }

    // Menu entry position tweens & alpha.
    let menu_vis = if pause.debug_hidden {
        Visibility::Hidden
    } else {
        Visibility::Visible
    };
    for (ent, mut entry, mut tf, mut vis) in &mut menu_query {
        if *vis != menu_vis {
            *vis = menu_vis;
        }
        entry.tween_elapsed_s = (entry.tween_elapsed_s + dt).min(MENU_TWEEN_DURATION_S);
        let eased = EaseKind::QuartOut.eval(f64::from(entry.tween_elapsed_s / MENU_TWEEN_DURATION_S)) as f32;
        entry.current_x_flx = entry.start_x_flx + (entry.target_x_flx - entry.start_x_flx) * eased;
        entry.current_y_flx = entry.start_y_flx + (entry.target_y_flx - entry.start_y_flx) * eased;
        tf.translation = flx_to_world(entry.current_x_flx, entry.current_y_flx, PAUSE_MENU_Z);
        set_alphabet_text_alpha(ent, entry.alpha, &mut alpha_texts, &mut alpha_chars);
    }

    // Pause music fade-in, initial random seek, and focus pause/resume.
    let is_focused = windows.iter().next().map(|w| w.focused).unwrap_or(true);
    let focus_changed = is_focused != pause.music_was_focused;
    pause.music_was_focused = is_focused;

    for (player, mut music, mut sink) in &mut music_query {
        if !music.random_seek_done
            && let Some(_source) = audio_sources.get(&player.0)
        {
            music.random_seek_done = true;
            // Start near a pseudo-random point in the first half of the track (10-30s).
            let seed_secs = (time.elapsed_secs_f64() * 1000.0) as u64 % 25;
            let _ = sink.try_seek(Duration::from_secs(seed_secs));
        }

        if focus_changed {
            if is_focused {
                sink.play();
            } else {
                sink.pause();
            }
        }

        if music.fade_elapsed_s < MUSIC_FADE_IN_TIME_S {
            music.fade_elapsed_s = (music.fade_elapsed_s + dt).min(MUSIC_FADE_IN_TIME_S);
            let vol = MUSIC_FINAL_VOLUME * (music.fade_elapsed_s / MUSIC_FADE_IN_TIME_S);
            sink.set_volume(Volume::Linear(vol));
        }
    }
}
