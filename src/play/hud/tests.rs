use bevy::image::ImageSampler;
use bevy::prelude::*;

use super::popups::{
    PREALLOC_DIGITS_PER_KIND, PREALLOC_RATINGS_PER_KIND, PopupAssets, PopupKind,
    PopupSamplerState, PopupSpawnParams, PopupVisual, apply_pixel_popup_samplers, spawn_or_reuse,
    spawn_popup_pool,
};
use crate::play::notes::data::{NoteSprite, SongNoteData};
use crate::play::notes::render::sustain_trail::compute_sustain_geometry;
use crate::play::notes::skin::NoteStyle;
use crate::play::notes::strum::Strumline;
use crate::play::scene::PlayState;
use crate::play::types::PlayStateParams;
use crate::utils::constants::{HEALTH_MAX, HEALTH_STARTING};

#[derive(Resource, Default)]
struct TestSpawnInput(Option<PopupSpawnParams>);

fn run_spawn_pool_system(mut commands: Commands, assets: Res<PopupAssets>) {
    spawn_popup_pool(&mut commands, &assets);
}

fn run_spawn_or_reuse_system(
    mut commands: Commands,
    mut pool: Query<(&mut PopupVisual, &mut Sprite, &mut Transform, &mut Visibility)>,
    mut input: ResMut<TestSpawnInput>,
) {
    if let Some(params) = input.0.take() {
        spawn_or_reuse(&mut commands, &mut pool, params);
    }
}

fn test_popup_assets(configured_ratings: usize, configured_digits: usize) -> PopupAssets {
    let mut ratings: [Option<(Handle<Image>, f32, f32, [f32; 2])>; 4] = Default::default();
    for slot in ratings.iter_mut().take(configured_ratings) {
        *slot = Some((Handle::<Image>::default(), 0.7, 1.0, [0.0, 0.0]));
    }
    let mut digits: [Option<Handle<Image>>; 10] = Default::default();
    for slot in digits.iter_mut().take(configured_digits) {
        *slot = Some(Handle::<Image>::default());
    }
    PopupAssets {
        ratings,
        digits,
        combo_scale: 0.5,
        combo_alpha: 1.0,
        combo_offset: [0.0, 0.0],
        is_pixel: false,
        rng_state: 0xA5A5_5A5A,
        next_z_step: 0,
    }
}

fn make_params(kind: PopupKind, alpha: f32, delay_s: f32) -> PopupSpawnParams {
    PopupSpawnParams {
        kind,
        texture: Handle::<Image>::default(),
        x: 10.0,
        y: 20.0,
        z: 11.0,
        scale: 1.0,
        alpha,
        vx: 0.0,
        vy: 0.0,
        accel_y: 0.0,
        delay_s,
        is_pixel: false,
    }
}

#[test]
fn spawn_popup_pool_preallocates_hidden_slots() {
    let configured_ratings = 2_usize;
    let configured_digits = 2_usize;
    let mut world = World::new();
    world.insert_resource(test_popup_assets(configured_ratings, configured_digits));
    let mut schedule = Schedule::default();
    schedule.add_systems(run_spawn_pool_system);
    schedule.run(&mut world);

    let mut rating_counts = [0usize; 4];
    let mut digit_counts = [0usize; 10];
    let mut q = world.query::<(&PopupVisual, &Visibility)>();
    for (vis, visibility) in q.iter(&world) {
        assert!(!vis.active);
        assert_eq!(*visibility, Visibility::Hidden);
        match vis.kind {
            PopupKind::Rating(i) => rating_counts[i] += 1,
            PopupKind::Digit(d) => digit_counts[d] += 1,
        }
    }
    for (i, &count) in rating_counts.iter().enumerate() {
        let expected = if i < configured_ratings {
            PREALLOC_RATINGS_PER_KIND
        } else {
            0
        };
        assert_eq!(count, expected);
    }
    for (d, &count) in digit_counts.iter().enumerate() {
        let expected = if d < configured_digits {
            PREALLOC_DIGITS_PER_KIND
        } else {
            0
        };
        assert_eq!(count, expected);
    }
}

#[test]
fn spawn_or_reuse_prefers_inactive_and_recycles_oldest_active() {
    let mut world = World::new();
    world.insert_resource(test_popup_assets(2, 2));
    world.insert_resource(TestSpawnInput::default());

    let mut init_sched = Schedule::default();
    init_sched.add_systems(run_spawn_pool_system);
    init_sched.run(&mut world);

    let mut spawn_sched = Schedule::default();
    spawn_sched.add_systems(run_spawn_or_reuse_system);

    for _ in 0..PREALLOC_RATINGS_PER_KIND {
        world.resource_mut::<TestSpawnInput>().0 =
            Some(make_params(PopupKind::Rating(0), 1.0, 0.5));
        spawn_sched.run(&mut world);
    }

    let mut oldest_entity = Entity::PLACEHOLDER;
    let mut rating_zero_count = 0usize;
    let mut q = world.query::<(Entity, &mut PopupVisual, &Visibility)>();
    for (idx, (ent, mut vis, visibility)) in q.iter_mut(&mut world).enumerate() {
        if vis.kind == PopupKind::Rating(0) {
            assert!(vis.active);
            assert_eq!(*visibility, Visibility::Visible);
            let age = (idx as f32 + 1.0) * 0.25;
            vis.age_s = age;
            oldest_entity = ent;
            rating_zero_count += 1;
        }
    }
    assert_eq!(rating_zero_count, PREALLOC_RATINGS_PER_KIND);

    let recycled_alpha = 0.42_f32;
    let recycled_delay_s = 3.25_f32;
    world.resource_mut::<TestSpawnInput>().0 = Some(make_params(
        PopupKind::Rating(0),
        recycled_alpha,
        recycled_delay_s,
    ));
    spawn_sched.run(&mut world);

    let mut after_count = 0usize;
    let mut q_after = world.query::<(Entity, &PopupVisual, &Visibility)>();
    for (ent, vis, visibility) in q_after.iter(&world) {
        if vis.kind == PopupKind::Rating(0) {
            after_count += 1;
            if ent == oldest_entity {
                assert!(vis.active);
                assert!(vis.age_s.abs() < 1e-6);
                assert!((vis.delay_s - recycled_delay_s).abs() < 1e-6);
                assert!((vis.base_alpha - recycled_alpha).abs() < 1e-6);
                assert_eq!(*visibility, Visibility::Visible);
            }
        }
    }
    assert_eq!(after_count, PREALLOC_RATINGS_PER_KIND);
}

#[test]
fn spawn_or_reuse_spawns_only_when_zero_entities_exist() {
    let mut world = World::new();
    world.insert_resource(TestSpawnInput::default());

    let mut sched = Schedule::default();
    sched.add_systems(run_spawn_or_reuse_system);

    let target_kind = PopupKind::Rating(3);
    world.resource_mut::<TestSpawnInput>().0 = Some(make_params(target_kind, 1.0, 0.5));
    sched.run(&mut world);

    let updated_delay_s = 1.25_f32;
    world.resource_mut::<TestSpawnInput>().0 =
        Some(make_params(target_kind, 0.8, updated_delay_s));
    sched.run(&mut world);

    let mut q = world.query::<&PopupVisual>();
    let entries: Vec<&PopupVisual> = q.iter(&world).filter(|v| v.kind == target_kind).collect();
    assert_eq!(entries.len(), 1);
    assert!((entries[0].delay_s - updated_delay_s).abs() < 1e-6);
}

#[test]
fn botplay_sets_score_text_and_full_health_lerp_until_custom_override() {
    let mut bot_state = PlayState::new(PlayStateParams {
        bot_play_mode: true,
        ..Default::default()
    });
    assert_eq!(bot_state.score_text, "Bot Play Enabled");

    bot_state.health = HEALTH_STARTING;
    bot_state.health_lerp = HEALTH_STARTING;
    bot_state.update_health_bar(0.016);
    assert!((bot_state.health_lerp - HEALTH_MAX).abs() < 1e-9);

    bot_state.custom_score_text = Some("Custom Bot Text".to_string());
    bot_state.update_score_text();
    assert_eq!(bot_state.score_text, "Custom Bot Text");

    bot_state.custom_score_text = None;
    bot_state.update_score_text();
    assert_eq!(bot_state.score_text, "Bot Play Enabled");

    let mut normal_state = PlayState::new(PlayStateParams::default());
    assert_eq!(
        normal_state.score_text,
        "Score: 0 | Combo: 0 | Misses: 0 | Health: 50%"
    );
    normal_state.update_health_bar(0.016);
    assert!((normal_state.health_lerp - HEALTH_STARTING).abs() < 1e-9);
}

#[test]
fn pixel_popup_samplers_wait_for_async_load_and_apply_nearest_to_all_handles() {
    let mut images = Assets::<Image>::default();
    let r0 = images.add(Image::default());
    let r1 = images.reserve_handle();
    let d0 = images.add(Image::default());
    let d1 = images.reserve_handle();

    let mut assets = test_popup_assets(0, 0);
    assets.is_pixel = true;
    assets.ratings[0] = Some((r0.clone(), 4.2, 1.0, [0.0, 0.0]));
    assets.ratings[1] = Some((r1.clone(), 4.2, 1.0, [0.0, 0.0]));
    assets.digits[0] = Some(d0.clone());
    assets.digits[1] = Some(d1.clone());

    let mut sampler_state = PopupSamplerState::default();
    apply_pixel_popup_samplers(&assets, &mut sampler_state, &mut images);

    assert!(sampler_state.ratings[0]);
    assert!(!sampler_state.ratings[1]);
    assert!(sampler_state.digits[0]);
    assert!(!sampler_state.digits[1]);
    assert_eq!(
        images.get(&r0).map(|img| &img.sampler),
        Some(&ImageSampler::nearest())
    );
    assert_eq!(
        images.get(&d0).map(|img| &img.sampler),
        Some(&ImageSampler::nearest())
    );

    assert!(images.insert(&r1, Image::default()).is_ok());
    assert!(images.insert(&d1, Image::default()).is_ok());
    apply_pixel_popup_samplers(&assets, &mut sampler_state, &mut images);

    assert!(sampler_state.ratings[1]);
    assert!(sampler_state.digits[1]);
    assert_eq!(
        images.get(&r1).map(|img| &img.sampler),
        Some(&ImageSampler::nearest())
    );
    assert_eq!(
        images.get(&d1).map(|img| &img.sampler),
        Some(&ImageSampler::nearest())
    );

    assets.is_pixel = false;
    apply_pixel_popup_samplers(&assets, &mut sampler_state, &mut images);
    assert_eq!(sampler_state.ratings, [false; 4]);
    assert_eq!(sampler_state.digits, [false; 10]);
}

#[test]
fn pixel_short_holds_keep_contiguous_end_cap_without_gaps() {
    let mut style = NoteStyle::fallback_funkin();
    style.is_pixel = true;
    style.sustains_scale = 6.0;
    style.sustains_end_offset = [0.0, 0.0];

    let end_raw_h = 6.0_f32;
    let rendered_end_px = end_raw_h * style.sustains_scale;
    let scroll_speed = 1.6_f64;

    for downscroll in [false, true] {
        let mut strum = Strumline::new(true, scroll_speed, style.clone());
        strum.is_downscroll = downscroll;
        strum.visual_song_position = 1000.0;
        let sign = if downscroll { 1.0_f32 } else { -1.0_f32 };

        let medium_note = NoteSprite::from_data(SongNoteData {
            id: 1,
            time_ms: 1000.0,
            direction: 0,
            sustain_length_ms: 208.0,
            kind: None,
            low_priority: false,
            scoreable: true,
        });
        assert!(medium_note.is_some());
        if let Some(note) = medium_note {
            let geom = compute_sustain_geometry(&note, &strum, &style, 1000.0, end_raw_h);
            assert!(geom.visible);
            assert!(geom.body_height_px > 0.5);
            assert!((geom.end_height_px - rendered_end_px).abs() < 1e-4);
            let body_end = geom.body_y + sign * (geom.body_height_px * 0.5);
            let end_start = geom.end_y - sign * (geom.end_height_px * 0.5);
            assert!((body_end - end_start).abs() < 1e-4);
        }

        // total_px = 36.3px -> raw_body_px = 0.3px (<= 0.5 threshold) -> body hides cleanly and end stays flush at start_y.
        let threshold_ms = 36.3_f64 / (0.45 * scroll_speed);
        let threshold_note = NoteSprite::from_data(SongNoteData {
            id: 2,
            time_ms: 1000.0,
            direction: 1,
            sustain_length_ms: threshold_ms,
            kind: None,
            low_priority: false,
            scoreable: true,
        });
        assert!(threshold_note.is_some());
        if let Some(note) = threshold_note {
            let geom = compute_sustain_geometry(&note, &strum, &style, 1000.0, end_raw_h);
            assert!(geom.visible);
            assert!(geom.body_height_px.abs() < 1e-6);
            assert!((geom.end_height_px - rendered_end_px).abs() < 1e-4);
            let start_y = strum.note_y_at(1, note.data.time_ms, 1000.0);
            let end_start = geom.end_y - sign * (geom.end_height_px * 0.5);
            assert!((end_start - start_y).abs() < 1e-4);
        }

        // total_px = 18.0px (< rendered_end_px = 36.0px) -> end cap shrinks smoothly and stays visible.
        let tiny_ms = 18.0_f64 / (0.45 * scroll_speed);
        let tiny_note = NoteSprite::from_data(SongNoteData {
            id: 3,
            time_ms: 1000.0,
            direction: 2,
            sustain_length_ms: tiny_ms,
            kind: None,
            low_priority: false,
            scoreable: true,
        });
        assert!(tiny_note.is_some());
        if let Some(note) = tiny_note {
            let geom = compute_sustain_geometry(&note, &strum, &style, 1000.0, end_raw_h);
            assert!(geom.visible);
            assert!(geom.body_height_px.abs() < 1e-6);
            assert!((geom.end_height_px - 18.0).abs() < 1e-3);
            let start_y = strum.note_y_at(2, note.data.time_ms, 1000.0);
            let end_start = geom.end_y - sign * (geom.end_height_px * 0.5);
            assert!((end_start - start_y).abs() < 1e-4);
        }
    }
}
