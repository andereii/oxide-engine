use std::collections::HashMap;

use crate::animations::BaseAnimationData;
use crate::animations::atlas_composite::{CompositeClip, CompositePiece};

use super::behavior::{CharacterTrailConfig, CharacterTrailSample, CharacterTrailState, ClipVisual};
use super::loader::{max_composite_pieces_in, partition_character_animations};
use super::schema::is_gameover_only_anim;

fn make_anim(name: &str, asset_path: Option<&str>) -> BaseAnimationData {
    BaseAnimationData {
        name: name.to_string(),
        asset_path: asset_path.map(str::to_string),
        ..Default::default()
    }
}

#[test]
fn gameover_anim_classifier_matches_expected_prefixes() {
    for name in [
        "firstDeath",
        "deathLoop",
        "deathConfirm",
        "firstDeath-explosion",
        "deathLoop-explosion",
        "deathConfirm-explosion",
        "deathConfirm-loop",
    ] {
        assert!(is_gameover_only_anim(name, "idle"), "{name} should defer");
    }

    for name in [
        "idle",
        "singLEFT",
        "singRIGHT-censor",
        "singUPmiss",
        "shootMISS",
        "hey",
        "cheer",
        "intro1",
        "death",
    ] {
        assert!(
            !is_gameover_only_anim(name, "idle"),
            "{name} must load eagerly"
        );
    }

    assert!(!is_gameover_only_anim("firstDeath", "firstDeath"));
    assert!(is_gameover_only_anim("deathLoop", "firstDeath"));
}

#[test]
fn partition_defers_gameover_variants_and_preserves_gameplay_anims() {
    let anims = vec![
        make_anim("idle", None),
        make_anim("singLEFT", None),
        make_anim("singRIGHT-censor", Some("custom-censor")),
        make_anim("shootMISS", Some("custom-extra")),
        make_anim("firstDeath", Some("death-sheet")),
        make_anim("deathLoop", Some("death-sheet")),
        make_anim("deathConfirm", Some("death-sheet")),
        make_anim("firstDeath-explosion", Some("death-alt")),
        make_anim("deathLoop-explosion", Some("death-alt")),
        make_anim("deathConfirm-explosion", Some("death-alt")),
        make_anim("hey", Some("custom-extra")),
        make_anim("intro1", Some("custom-extra")),
    ];

    let (initial, deferred) = partition_character_animations(&anims, "idle");
    let initial_names: Vec<&str> = initial.iter().map(|a| a.name.as_str()).collect();
    let deferred_names: Vec<&str> = deferred.iter().map(|a| a.name.as_str()).collect();

    assert_eq!(
        initial_names,
        vec![
            "idle",
            "singLEFT",
            "singRIGHT-censor",
            "shootMISS",
            "hey",
            "intro1"
        ]
    );
    assert_eq!(
        deferred_names,
        vec![
            "firstDeath",
            "deathLoop",
            "deathConfirm",
            "firstDeath-explosion",
            "deathLoop-explosion",
            "deathConfirm-explosion",
        ]
    );
}

#[test]
fn partition_loads_all_when_every_animation_is_gameover() {
    let anims = vec![
        make_anim("firstDeath", None),
        make_anim("deathLoop", None),
        make_anim("deathConfirm", None),
    ];

    let (initial, deferred) = partition_character_animations(&anims, "idle");
    assert_eq!(initial.len(), anims.len());
    assert!(deferred.is_empty());
}

#[test]
fn partition_keeps_starting_gameover_anim_eager() {
    let anims = vec![
        make_anim("firstDeath", None),
        make_anim("deathLoop", None),
        make_anim("deathConfirm", None),
    ];

    let (initial, deferred) = partition_character_animations(&anims, "firstDeath");
    let initial_names: Vec<&str> = initial.iter().map(|a| a.name.as_str()).collect();
    let deferred_names: Vec<&str> = deferred.iter().map(|a| a.name.as_str()).collect();

    assert_eq!(initial_names, vec!["firstDeath"]);
    assert_eq!(deferred_names, vec!["deathLoop", "deathConfirm"]);
}

#[test]
fn max_composite_pieces_tracks_largest_loaded_frame() {
    let dummy_piece = CompositePiece {
        sprite_index: 0,
        spritemap_idx: 0,
        atlas_size: [16, 16],
        rotated: false,
        matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        color: [1.0; 4],
    };
    let mut clips = HashMap::new();
    assert_eq!(max_composite_pieces_in(&clips), 0);

    clips.insert("sheet".to_string(), ClipVisual::Sheet { source_idx: 0 });
    assert_eq!(max_composite_pieces_in(&clips), 0);

    let smaller_frame_count = 2_usize;
    let larger_frame_count = 4_usize;
    clips.insert(
        "idle".to_string(),
        ClipVisual::Composite {
            source_idx: 0,
            clip: CompositeClip {
                frames: vec![
                    vec![dummy_piece.clone(); smaller_frame_count],
                    vec![dummy_piece; larger_frame_count],
                ],
                fps: 24.0,
                looped: false,
                offset: [0.0, 0.0],
                flip: [false, false],
                bounds_min: [0.0, 0.0],
                bounds_size: [100.0, 100.0],
            },
            stage_matrix: None,
        },
    );
    assert_eq!(max_composite_pieces_in(&clips), larger_frame_count);
}

#[test]
fn character_trail_captures_fifo_samples_decays_alpha_and_resets() {
    let mut trail = CharacterTrailState::new(CharacterTrailConfig {
        length: 4,
        delay_s: 0.4,
        alpha: 0.3,
        diff: 0.069,
        z_layer: -481.0,
    });
    let sample = |frame_idx: usize, x: f32| CharacterTrailSample {
        source_idx: 0,
        frame_idx,
        world_cx: x,
        world_cy: 648.0,
        foff: [0.0, 0.0],
        scale: [6.0, 6.0],
        flip: [false, false],
        rot: 0.0,
    };

    assert!(!trail.step_and_capture(0.2, sample(0, 10.0)));
    assert!(trail.samples.is_empty());

    assert!(trail.step_and_capture(0.25, sample(1, 20.0)));
    assert_eq!(trail.samples.len(), 1);

    for i in 2..=6 {
        assert!(trail.step_and_capture(0.4, sample(i, i as f32 * 10.0)));
    }
    let frames: Vec<usize> = trail.samples.iter().map(|s| s.frame_idx).collect();
    assert_eq!(frames, vec![6, 5, 4, 3]);
    assert!((trail.ghost_alpha(0, 1.0) - 0.3).abs() < 1e-5);
    assert!((trail.ghost_alpha(3, 1.0) - (0.3 - 3.0 * 0.069)).abs() < 1e-5);

    trail.reset();
    assert!(trail.samples.is_empty());
    assert_eq!(trail.elapsed_s, 0.0);
}

#[test]
fn blazin_characters_build_nonempty_idle_composite_clips() {
    use crate::animations::animate_atlas::AnimateAtlas;
    use crate::animations::atlas_composite::{
        FlxAnimateTransformParams, build_composite_clip, compose_flx_piece_bevy_affine,
    };
    use std::path::Path;

    for (id, pos, offsets, scale, cam_off) in [
        ("darnell-blazin", [-237.0_f32, 150.0], [523.0_f32, 453.0], 1.75_f32, [500.0_f32, 200.0]),
        ("pico-blazin", [-237.0_f32, 100.0], [611.0_f32, 652.0], 1.75_f32, [-350.0_f32, -100.0]),
    ] {
        let dir = Path::new("assets/gameplay/characters").join(id).join(id);
        let atlas = AnimateAtlas::from_dir(&dir).expect("blazin atlas should parse");
        let (_, indices) = atlas.build_layouts();
        let anim = BaseAnimationData {
            name: "idle".to_string(),
            prefix: "Idle".to_string(),
            looped: true,
            ..Default::default()
        };
        let clip = build_composite_clip(&atlas, &indices, &anim).expect("idle clip should build");
        assert!(!clip.frames.is_empty(), "{id} should have idle frames");
        assert!(!clip.frames[0].is_empty(), "{id} frame 0 should have pieces");
        let fx = pos[0] + offsets[0] * (1.0 + scale);
        let fy = pos[1] + offsets[1] * (1.0 + scale);
        let cam_cx = pos[0] + offsets[0] + cam_off[0];
        let cam_cy = pos[1] - (clip.bounds_size[1] * scale) * 0.5 + offsets[1] + cam_off[1];
        let cam_scroll_x = 1353.0_f32 + 50.0 - 640.0;
        let cam_scroll_y = (1125.0_f32 - 200.0 + 20.0 - 90.0) - 360.0;
        let top_left = [
            fx - cam_scroll_x - clip.bounds_size[0] * scale * 0.5,
            fy - cam_scroll_y - clip.bounds_size[1] * scale,
        ];
        let p = FlxAnimateTransformParams {
            bounds_min: clip.bounds_min,
            bounds_size: clip.bounds_size,
            top_left,
            scale: [scale, scale],
            angle_deg: 0.0,
            flip: [id == "pico-blazin", false],
            stage_matrix: atlas.stage_matrix,
            apply_stage_matrix: false,
            post_stage_matrix_apply: false,
            screen_size: [1280.0, 720.0],
            zoom: 0.75,
        };
        let ([bx, by], _cols) = compose_flx_piece_bevy_affine(&clip.frames[0][0], &p);
        assert!(
            cam_cx.is_finite() && cam_cy.is_finite() && bx.is_finite() && by.is_finite(),
            "{id}: piece should resolve cleanly at ({fx},{fy})"
        );
    }
}

#[test]
fn underlay_alpha_shows_lit_actor_only_while_dark_parent_is_transparent() {
    use super::sync::compute_underlay_alpha;

    assert_eq!(compute_underlay_alpha(0.0), 1.0);
    assert_eq!(compute_underlay_alpha(0.5), 1.0);
    assert_eq!(compute_underlay_alpha(0.99), 1.0);
    assert_eq!(compute_underlay_alpha(0.9995), 0.0);
    assert_eq!(compute_underlay_alpha(1.0), 0.0);
}

#[test]
fn actor_matches_target_ignores_mirror_underlays() {
    use super::behavior::{CharacterActor, CharacterUnderlayConfig};
    use super::schema::{CharacterData, CharacterRole};
    use crate::animations::{AnimationController, Bopper};
    use crate::play::plugin::actor_matches_target;
    use crate::play::stage::schema::StageCharacterSlot;
    use bevy::prelude::Entity;

    let make_actor = |id: &str, mirror_of| CharacterActor {
        id: id.to_string(),
        role: CharacterRole::Player,
        slot: StageCharacterSlot::default_for_player(),
        data: CharacterData::default(),
        controller: AnimationController::default(),
        bopper: Bopper::default(),
        sources: Vec::new(),
        clips: HashMap::new(),
        deferred_gameover_anims: Vec::new(),
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
        mod_z: None,
        script_anim_override: false,
        sheet_entity: Entity::PLACEHOLDER,
        piece_entities: Vec::new(),
        trail: None,
        trail_entities: Vec::new(),
        mirror_of,
    };

    let primary = make_actor("bf-dark", None);
    let underlay = make_actor(
        "bf",
        Some(CharacterUnderlayConfig {
            parent_role: CharacterRole::Player,
            offset: [0.0, 0.0],
        }),
    );

    assert!(actor_matches_target(&primary, "bf"));
    assert!(actor_matches_target(&primary, "bf-dark"));
    assert!(!actor_matches_target(&underlay, "bf"));
    assert!(!actor_matches_target(&underlay, "bf-dark"));
}
