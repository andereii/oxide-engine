use std::collections::HashMap;

use crate::animations::BaseAnimationData;
use crate::animations::atlas_composite::{CompositeClip, CompositePiece};

use super::behavior::ClipVisual;
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
