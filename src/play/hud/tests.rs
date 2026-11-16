use bevy::prelude::*;

use super::popups::{
    PREALLOC_DIGITS_PER_KIND, PREALLOC_RATINGS_PER_KIND, PopupAssets, PopupKind, PopupSpawnParams,
    PopupVisual, spawn_or_reuse, spawn_popup_pool,
};

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
