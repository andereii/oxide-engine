use std::collections::HashMap;

use bevy::prelude::Entity;

use crate::animations::{AnimationController, Bopper};
use crate::play::stage::schema::StageCharacterSlot;
use crate::utils::conductor::{Conductor, advance_step_range};
use crate::utils::constants::{DEFAULT_BPM, DEFAULT_CHARACTER_DANCE_EVERY, MS_PER_SEC, STEPS_PER_BEAT};

use super::behavior::CharacterActor;
use super::schema::{CharacterData, CharacterRole};

const TEST_FPS: f32 = 24.0;
const ALT_FRAMES_PER_DANCE: usize = 15;
const IDLE_FRAME_COUNT: usize = 14;
const IDLE_STEP_RESOLUTION: usize = 2;

fn make_actor(alternating: bool) -> CharacterActor {
    let mut controller = AnimationController::default();
    if alternating {
        controller.add_anim("danceLeft", (0..ALT_FRAMES_PER_DANCE).collect(), TEST_FPS, false);
        controller.add_anim(
            "danceRight",
            (ALT_FRAMES_PER_DANCE..(ALT_FRAMES_PER_DANCE * 2)).collect(),
            TEST_FPS,
            false,
        );
    } else {
        controller.add_anim("idle", (0..IDLE_FRAME_COUNT).collect(), TEST_FPS, false);
    }
    let data = CharacterData::default();
    let bopper = Bopper::new(data.dance_every);
    let mut actor = CharacterActor {
        id: "test".to_string(),
        role: CharacterRole::Player,
        slot: StageCharacterSlot::default_for_player(),
        data,
        controller,
        bopper,
        sources: Vec::new(),
        clips: HashMap::new(),
        idle_size: [100.0, 100.0],
        facing_flip_x: false,
        camera_focus_point: [0.0, 0.0],
        hold_timer: 0.0,
        cur_no_anim: false,
        cur_suffix: String::new(),
        combo_counts: Vec::new(),
        drop_counts: Vec::new(),
        sampler_applied: false,
        mod_tint: [1.0; 4],
        mod_visible: true,
        sheet_entity: Entity::PLACEHOLDER,
        piece_entities: Vec::new(),
    };
    actor.reset_character();
    actor
}

#[test]
fn characters_repeat_idle_and_dance_cycles_on_beats() {
    let conductor = Conductor::new(DEFAULT_BPM);
    let cycle_duration_s = (IDLE_FRAME_COUNT as f64 / f64::from(TEST_FPS)) + 0.05;
    let first_song_ms = cycle_duration_s * MS_PER_SEC;
    let step_length_ms = conductor.step_length_ms();

    let mut bf = make_actor(false);
    let mut gf = make_actor(true);
    assert_eq!(bf.bopper.dance_every, DEFAULT_CHARACTER_DANCE_EVERY);
    assert_eq!(gf.controller.current_name(), Some("danceLeft"));

    bf.on_update(cycle_duration_s, Some(first_song_ms), step_length_ms, false);
    assert!(bf.controller.is_finished());

    let skipped_before = STEPS_PER_BEAT - 1;
    let skipped_after = STEPS_PER_BEAT + 1;
    let mut last_step = Some(skipped_before);
    let range = advance_step_range(&mut last_step, skipped_after, false, false).unwrap();
    for s in range {
        bf.on_step_hit(s);
        gf.on_step_hit(s);
    }
    assert!(!bf.controller.is_finished(), "bf idle should restart on beat 1");
    assert_eq!(bf.controller.frame_cursor, 0);
    assert_eq!(gf.controller.current_name(), Some("danceRight"));

    let mid_dt_s = cycle_duration_s / (IDLE_STEP_RESOLUTION as f64);
    bf.on_update(mid_dt_s, Some(first_song_ms + mid_dt_s * MS_PER_SEC), step_length_ms, false);
    let cursor_mid = bf.controller.frame_cursor;
    assert!(cursor_mid > 0 && !bf.controller.is_finished());
    bf.on_step_hit(STEPS_PER_BEAT * 2);
    assert_eq!(bf.controller.frame_cursor, cursor_mid);

    bf.on_update(
        cycle_duration_s,
        Some(first_song_ms + (mid_dt_s + cycle_duration_s) * MS_PER_SEC),
        step_length_ms,
        false,
    );
    assert!(bf.controller.is_finished());
    bf.on_step_hit(STEPS_PER_BEAT * 3);
    assert!(!bf.controller.is_finished());
    assert_eq!(bf.controller.frame_cursor, 0);
}
