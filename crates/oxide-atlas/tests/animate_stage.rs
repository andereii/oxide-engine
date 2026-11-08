//! BTA animate-stage tests: synthetic fixtures and inline documents for the
//! parser/error paths, plus real-asset regressions requiring OXIDE_ASSETS_ROOT.

use std::path::PathBuf;

use oxide_atlas::{AnimateStage, AnimateStageError, SpriteQuad};
use serde_json::json;

const EMPTY_SPRITEMAP: &str = r#"{"ATLAS":{"SPRITES":[]},"meta":{"image":"spritemap1.png"}}"#;

fn fixture(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/atlas")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

fn synthetic_stage() -> AnimateStage {
    AnimateStage::parse(
        &fixture("animate/Animation.json"),
        &fixture("animate/spritemap1.json"),
    )
    .expect("synthetic animate fixture parses")
}

fn quad_names(quads: &[SpriteQuad]) -> Vec<&str> {
    quads.iter().map(|quad| quad.bitmap_name.as_str()).collect()
}

fn assert_close(actual: f64, expected: f64, context: &str) {
    assert!(
        (actual - expected).abs() <= 1e-6,
        "{context}: actual {actual}, expected {expected}"
    );
}

#[test]
fn synthetic_metadata_bounds_and_observed_rtrim_labels() {
    let stage = synthetic_stage();

    assert_eq!(stage.frame_rate(), 30);
    assert_eq!(stage.stage_size(), (640, 360));
    assert_eq!(stage.frame_count(), 14);
    assert_eq!(stage.bounds(), [0.0, 0.0, 20.0, 40.0]);

    // RENDER-BEHAVIOR §4 observed comparison: `name.rtrim() == label`
    // (FlxAnimateController.hx L266-290). Layer 0 is the first layer with
    // matches; its spans concatenate in keyframe order; both sides are
    // right-trimmed, with no case folding and no left trim.
    assert_eq!(stage.label_frames("Walk"), Some(vec![0, 1, 3, 5, 6]));
    assert_eq!(stage.label_frames("Walk  "), Some(vec![0, 1, 3, 5, 6]));
    assert_eq!(stage.label_frames("Other"), Some(vec![7]));
    // A label absent from layer 0 is still found in a later layer.
    assert_eq!(stage.label_frames("Later"), Some(vec![9, 10, 11, 12, 13]));
    // Case-exact and right-trim-only.
    assert_eq!(stage.label_frames("walk"), None);
    assert_eq!(stage.label_frames(" Walk"), None);
    assert_eq!(stage.label_frames("Missing"), None);
}

#[test]
fn synthetic_frame_zero_draws_elements_ascending_and_layers_descending() {
    let stage = synthetic_stage();
    let quads = stage.resolve_main_frame(0).expect("frame 0 resolves");

    assert_eq!(quad_names(&quads), ["plain", "nestedbg", "nested1"]);
    assert_eq!(quads[0].matrix, [1.0, 0.0, 0.0, 1.0, 10.0, 20.0]);
    assert_eq!(quads[0].atlas_rect, [0, 0, 10, 20]);
    assert!(!quads[0].rotated);
    assert_eq!(quads[1].matrix, [1.0, 0.0, 0.0, 1.0, 5.0, 6.0]);
    assert_eq!(quads[1].atlas_rect, [12, 30, 6, 2]);
    assert_eq!(quads[2].matrix, [1.0, 0.0, 0.0, 1.0, 5.0, 6.0]);
    assert_eq!(quads[2].atlas_rect, [4, 30, 4, 4]);
}

#[test]
fn synthetic_symbol_loop_modes_and_matrix_composition() {
    let stage = synthetic_stage();

    // SF freezes at FF=1; LP starts at FF=0 and the second instance is at t=0.
    let frame_two = stage.resolve_main_frame(2).expect("frame 2 resolves");
    assert_eq!(
        quad_names(&frame_two),
        ["nestedbg", "nested1", "nestedbg", "nested0"]
    );
    assert_eq!(frame_two[0].matrix, [2.0, 0.0, 0.0, 2.0, 0.0, 0.0]);
    assert_eq!(frame_two[1].matrix, [2.0, 0.0, 0.0, 2.0, 0.0, 0.0]);
    assert_eq!(frame_two[2].matrix, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    // PO clamps to the last child frame (2); missing/BOGUS loop modes wrap.
    let frame_twelve = stage.resolve_main_frame(12).expect("frame 12 resolves");
    assert_eq!(
        quad_names(&frame_twelve),
        [
            "nestedbg", "nested2", "nestedbg", "nested1", "nestedbg", "nested1"
        ]
    );
    // At the same offset a plain PO would keep frame 2 while the wrapped
    // instances land on frame 1.
    let frame_thirteen = stage.resolve_main_frame(13).expect("frame 13 resolves");
    assert_eq!(
        quad_names(&frame_thirteen),
        [
            "nestedbg", "nested2", "nestedbg", "nested2", "nestedbg", "nested2"
        ]
    );

    // Direct symbol resolution returns local coordinates.
    let symbol = stage
        .resolve_symbol_frame("Nested", 1)
        .expect("symbol frame resolves");
    assert_eq!(quad_names(&symbol), ["nestedbg", "nested1"]);
    assert_eq!(symbol[0].matrix, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
}

#[test]
fn synthetic_single_frame_mode_freezes_across_parent_frames() {
    let animation = r#"{
        "AN": {"TL": {"L": [{"FR": [{"I": 0, "DU": 4, "E": [
            {"SI": {"SN": "Nested", "FF": 1, "ST": "G", "LP": "SF", "MX": [1, 0, 0, 1, 0, 0]}},
            {"SI": {"SN": "Nested", "FF": 0, "ST": "G", "LP": "LP", "MX": [1, 0, 0, 1, 0, 0]}}
        ]}]}]}},
        "SD": {"S": [{"SN": "Nested", "TL": {"L": [{"FR": [
            {"I": 0, "DU": 1, "E": [{"ASI": {"N": "n0", "MX": [1, 0, 0, 1, 0, 0]}}]},
            {"I": 1, "DU": 1, "E": [{"ASI": {"N": "n1", "MX": [1, 0, 0, 1, 0, 0]}}]},
            {"I": 2, "DU": 1, "E": [{"ASI": {"N": "n2", "MX": [1, 0, 0, 1, 0, 0]}}]}
        ]}]}}]},
        "MD": {"W": 1, "H": 1, "FRT": 24}
    }"#;
    let spritemap = r#"{
        "ATLAS": {"SPRITES": [
            {"SPRITE": {"name": "n0", "x": 0, "y": 0, "w": 1, "h": 1}},
            {"SPRITE": {"name": "n1", "x": 1, "y": 0, "w": 1, "h": 1}},
            {"SPRITE": {"name": "n2", "x": 2, "y": 0, "w": 1, "h": 1}}
        ]},
        "meta": {"image": "spritemap1.png"}
    }"#;
    let stage = AnimateStage::parse(animation, spritemap).expect("multi-frame SF stage parses");

    // The SF instance stays on child frame FF=1 while the LP sibling advances
    // through 0, 1, 2 and wraps back to 0.
    for (frame, expected_names) in [
        vec!["n1", "n0"],
        vec!["n1", "n1"],
        vec!["n1", "n2"],
        vec!["n1", "n0"],
    ]
    .into_iter()
    .enumerate()
    {
        let quads = stage
            .resolve_main_frame(frame)
            .unwrap_or_else(|error| panic!("resolve SF frame {frame}: {error}"));
        assert_eq!(quad_names(&quads), expected_names, "frame {frame}");
    }

    let out_of_range = r#"{
        "AN": {"TL": {"L": [{"FR": [{"I": 0, "DU": 1, "E": [
            {"SI": {"SN": "Nested", "FF": 3, "LP": "SF", "MX": [1, 0, 0, 1, 0, 0]}}
        ]}]}]}},
        "SD": {"S": [{"SN": "Nested", "TL": {"L": [{"FR": [
            {"I": 0, "DU": 1, "E": []}
        ]}]}}]},
        "MD": {"W": 1, "H": 1, "FRT": 24}
    }"#;
    assert!(matches!(
        AnimateStage::parse(out_of_range, spritemap),
        Err(AnimateStageError::FrameOutOfRange {
            frame: 3,
            frame_count: 1
        })
    ));
}

#[test]
fn synthetic_rotated_tile_matrix_and_parent_chain() {
    let stage = synthetic_stage();
    let quads = stage.resolve_main_frame(3).expect("frame 3 resolves");

    assert_eq!(quads.len(), 1);
    assert!(quads[0].rotated);
    assert_eq!(quads[0].bitmap_name, "rot");
    assert_eq!(quads[0].atlas_rect, [10, 0, 8, 6]);
    // Rotated tile: local (0,0,8,6) becomes the upright (2,0,8,8) footprint.
    assert_eq!(quads[0].matrix, [0.0, 1.0, -1.0, 0.0, 8.0, 0.0]);

    let animation = r#"{
        "AN": {"TL": {"L": [{"FR": [{"I": 0, "DU": 1, "E": [
            {"SI": {"SN": "Holder", "MX": [2, 0, 0, 2, 5, 7]}}
        ]}]}]}},
        "SD": {"S": [{"SN": "Holder", "TL": {"L": [{"FR": [{"I": 0, "DU": 1, "E": [
            {"ASI": {"N": "tile", "MX": [1, 0, 0, 1, 1, 2]}}
        ]}]}]}}]},
        "MD": {"W": 1, "H": 1, "FRT": 24}
    }"#;
    let spritemap = r#"{
        "ATLAS": {"SPRITES": [
            {"SPRITE": {"name": "tile", "x": 0, "y": 0, "w": 8, "h": 6, "rotated": true}}
        ]},
        "meta": {"image": "spritemap1.png"}
    }"#;
    let chained = AnimateStage::parse(animation, spritemap).expect("chain stage parses");
    let quads = chained.resolve_main_frame(0).expect("frame resolves");

    // parent∘(SI MX) = [2,0,0,2,7,11]; then ∘(ASI MX) then ∘tile.
    assert_eq!(quads[0].matrix, [0.0, 2.0, -2.0, 0.0, 23.0, 11.0]);
}

#[test]
fn synthetic_unknown_keys_and_missing_mx_are_lenient() {
    let animation = r#"{
        "AN": {"TL": {"L": [{"FR": [{"I": 0, "DU": 1, "E": [
            {"SI": {"SN": "N"}, "C": {"ra": 0}}
        ]}]}]}},
        "SD": {"S": [{"SN": "N", "TL": {"L": [{"FR": [{"I": 0, "DU": 1, "E": [
            {"ASI": {"N": "p"}, "B": "add"}
        ]}]}]}}]},
        "MD": {"W": 1.0, "H": 1.0, "FRT": 24.0},
        "future_key": {"ignored": true}
    }"#;
    let spritemap = r#"{
        "ATLAS": {"SPRITES": [{"SPRITE": {"name": "p", "x": 1, "y": 2, "w": 3, "h": 4}}]},
        "meta": {"image": "spritemap1.png"}
    }"#;
    let stage = AnimateStage::parse(animation, spritemap).expect("unknown keys and missing MX");
    let quads = stage.resolve_main_frame(0).expect("frame resolves");

    assert_eq!(stage.frame_rate(), 24);
    assert_eq!(stage.stage_size(), (1, 1));
    assert_eq!(quads.len(), 1);
    assert_eq!(quads[0].matrix, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    assert_eq!(quads[0].atlas_rect, [1, 2, 3, 4]);
}

#[test]
fn synthetic_errors_are_typed_and_fail_closed() {
    let missing_an = r#"{"MD": {"W": 1, "H": 1, "FRT": 24}}"#;
    assert!(matches!(
        AnimateStage::parse(missing_an, EMPTY_SPRITEMAP),
        Err(AnimateStageError::Invalid { .. })
    ));

    let bad_matrix = r#"{
        "AN": {"TL": {"L": [{"FR": [{"I": 0, "DU": 1, "E": [
            {"ASI": {"N": "p", "MX": [1, 2, 3]}}
        ]}]}]}},
        "MD": {"W": 1, "H": 1, "FRT": 24}
    }"#;
    let chained = r#"{
        "AN": {"TL": {"L": [{"FR": [{"I": 0, "DU": 1, "E": [
            {"SI": {"SN": "Nested", "MX": [1, 0, 0, 1, 0, 0]}}
        ]}]}]}},
        "SD": {"S": [{"SN": "Nested", "TL": {"L": [{"FR": [{"I": 0, "DU": 1, "E": [
            {"ASI": {"N": "p", "MX": [1, 2, 3]}}
        ]}]}]}}]},
        "MD": {"W": 1, "H": 1, "FRT": 24}
    }"#;
    let spritemap = r#"{
        "ATLAS": {"SPRITES": [{"SPRITE": {"name": "p", "x": 0, "y": 0, "w": 1, "h": 1}}]},
        "meta": {"image": "spritemap1.png"}
    }"#;
    assert!(matches!(
        AnimateStage::parse(bad_matrix, spritemap),
        Err(AnimateStageError::BadMatrix { .. })
    ));
    assert!(matches!(
        AnimateStage::parse(chained, spritemap),
        Err(AnimateStageError::BadMatrix { .. })
    ));

    let unknown_bitmap = r#"{
        "AN": {"TL": {"L": [{"FR": [{"I": 0, "DU": 1, "E": [
            {"ASI": {"N": "ghost", "MX": [1, 0, 0, 1, 0, 0]}}
        ]}]}]}},
        "MD": {"W": 1, "H": 1, "FRT": 24}
    }"#;
    assert!(matches!(
        AnimateStage::parse(unknown_bitmap, EMPTY_SPRITEMAP),
        Err(AnimateStageError::UnknownBitmap { .. })
    ));

    let unknown_symbol = r#"{
        "AN": {"TL": {"L": [{"FR": [{"I": 0, "DU": 1, "E": [
            {"SI": {"SN": "Ghost", "MX": [1, 0, 0, 1, 0, 0]}}
        ]}]}]}},
        "MD": {"W": 1, "H": 1, "FRT": 24}
    }"#;
    assert!(matches!(
        AnimateStage::parse(unknown_symbol, EMPTY_SPRITEMAP),
        Err(AnimateStageError::UnknownSymbol { .. })
    ));

    let self_recursive = r#"{
        "AN": {"TL": {"L": [{"FR": [{"I": 0, "DU": 1, "E": [
            {"SI": {"SN": "Self", "MX": [1, 0, 0, 1, 0, 0]}}
        ]}]}]}},
        "SD": {"S": [{"SN": "Self", "TL": {"L": [{"FR": [{"I": 0, "DU": 1, "E": [
            {"SI": {"SN": "Self", "MX": [1, 0, 0, 1, 0, 0]}}
        ]}]}]}}]},
        "MD": {"W": 1, "H": 1, "FRT": 24}
    }"#;
    assert!(matches!(
        AnimateStage::parse(self_recursive, EMPTY_SPRITEMAP),
        Err(AnimateStageError::SymbolRecursion { .. })
    ));

    let zero_duration = r#"{
        "AN": {"TL": {"L": [{"FR": [{"I": 0, "DU": 0, "E": []}]}]}},
        "MD": {"W": 1, "H": 1, "FRT": 24}
    }"#;
    assert!(matches!(
        AnimateStage::parse(zero_duration, EMPTY_SPRITEMAP),
        Err(AnimateStageError::BadKeyframe { .. })
    ));

    let stage = synthetic_stage();
    assert!(matches!(
        stage.resolve_main_frame(14),
        Err(AnimateStageError::FrameOutOfRange {
            frame: 14,
            frame_count: 14
        })
    ));
    assert!(matches!(
        stage.resolve_symbol_frame("Nested", 3),
        Err(AnimateStageError::FrameOutOfRange {
            frame: 3,
            frame_count: 3
        })
    ));
    assert!(matches!(
        stage.resolve_symbol_frame("Absent", 0),
        Err(AnimateStageError::UnknownSymbol { .. })
    ));
}

#[test]
fn synthetic_deep_symbol_chain_hits_the_recursion_guard() {
    let mut symbols = Vec::new();
    for index in 0..70usize {
        let element = if index == 69 {
            json!({"ASI": {"N": "p", "MX": [1, 0, 0, 1, 0, 0]}})
        } else {
            json!({"SI": {"SN": format!("S{}", index + 1), "MX": [1, 0, 0, 1, 0, 0]}})
        };
        symbols.push(json!({
            "SN": format!("S{index}"),
            "TL": {"L": [{"FR": [{"I": 0, "DU": 1, "E": [element]}]}]}
        }));
    }
    let animation = json!({
        "AN": {"TL": {"L": [{"FR": [{"I": 0, "DU": 1, "E": [
            {"SI": {"SN": "S0", "MX": [1, 0, 0, 1, 0, 0]}}
        ]}]}]}},
        "SD": {"S": symbols},
        "MD": {"W": 1, "H": 1, "FRT": 24}
    });
    let spritemap = json!({
        "ATLAS": {"SPRITES": [{"SPRITE": {"name": "p", "x": 0, "y": 0, "w": 1, "h": 1}}]},
        "meta": {"image": "spritemap1.png"}
    });

    let result = AnimateStage::parse(&animation.to_string(), &spritemap.to_string());
    assert!(matches!(
        result,
        Err(AnimateStageError::SymbolRecursion { .. })
    ));
}

#[test]
fn add_collection_searches_main_first_then_insertion_order() {
    let mut stage = synthetic_stage();
    let main_bounds = stage.bounds();
    let collection = AnimateStage::parse(
        &fixture("animate-collection/Animation.json"),
        &fixture("animate-collection/spritemap1.json"),
    )
    .expect("collection fixture parses");

    assert_eq!(stage.label_frames("CollectLabel"), None);
    assert!(matches!(
        stage.resolve_symbol_frame("CollectOnly", 0),
        Err(AnimateStageError::UnknownSymbol { .. })
    ));

    stage.add_collection(collection);
    assert_eq!(stage.bounds(), main_bounds);
    // Main timeline shadows the collection's same-named label.
    assert_eq!(stage.label_frames("Walk"), Some(vec![0, 1, 3, 5, 6]));
    assert_eq!(stage.label_frames("CollectLabel"), Some(vec![3, 4, 6]));

    let quads = stage
        .resolve_symbol_frame("CollectOnly", 0)
        .expect("collection symbol resolves");
    assert_eq!(quad_names(&quads), ["collectBitmap", "nestedbg", "nested0"]);
    assert_eq!(quads[0].atlas_rect, [0, 0, 5, 5]);
    // The nested SI resolves through the main stage even though the parent
    // symbol lives in the collection.
    assert_eq!(quads[1].atlas_rect, [12, 30, 6, 2]);

    let second_animation = json!({
        "AN": {"TL": {"L": [{"FR": [{"I": 0, "DU": 1, "E": []}]}]}},
        "SD": {"S": [
            {"SN": "CollectOnly", "TL": {"L": [{"FR": [{"I": 0, "DU": 1, "E": [
                {"ASI": {"N": "secondBitmap", "MX": [1, 0, 0, 1, 0, 0]}}
            ]}]}]}},
            {"SN": "BitmapOrder", "TL": {"L": [{"FR": [{"I": 0, "DU": 1, "E": [
                {"ASI": {"N": "plain", "MX": [1, 0, 0, 1, 0, 0]}}
            ]}]}]}}
        ]},
        "MD": {"W": 1, "H": 1, "FRT": 24}
    });
    let second_spritemap = json!({
        "ATLAS": {"SPRITES": [
            {"SPRITE": {"name": "secondBitmap", "x": 0, "y": 0, "w": 9, "h": 9}},
            {"SPRITE": {"name": "plain", "x": 0, "y": 0, "w": 1, "h": 1}}
        ]},
        "meta": {"image": "spritemap1.png"}
    });
    let second = AnimateStage::parse(&second_animation.to_string(), &second_spritemap.to_string())
        .expect("second collection parses");
    stage.add_collection(second);

    // First-added collection wins over the later duplicate symbol.
    let quads = stage
        .resolve_symbol_frame("CollectOnly", 0)
        .expect("first collection symbol still resolves");
    assert_eq!(quads[0].bitmap_name, "collectBitmap");
    assert_eq!(quads[0].atlas_rect, [0, 0, 5, 5]);

    // Bitmap lookup prefers the main stage over a collection duplicate.
    let quads = stage
        .resolve_symbol_frame("BitmapOrder", 0)
        .expect("second collection bitmap-order symbol resolves");
    assert_eq!(quads[0].bitmap_name, "plain");
    assert_eq!(quads[0].atlas_rect, [0, 0, 10, 20]);
}

#[test]
fn resolution_is_deterministic() {
    let stage = synthetic_stage();
    let first = stage.resolve_main_frame(12).expect("frame resolves");
    let second = stage.resolve_main_frame(12).expect("frame resolves");
    assert_eq!(first, second);
}

fn real_assets_root() -> PathBuf {
    let root = PathBuf::from(
        std::env::var_os("OXIDE_ASSETS_ROOT")
            .filter(|value| !value.is_empty())
            .expect("set OXIDE_ASSETS_ROOT to the reference assets directory"),
    );
    assert!(
        root.is_dir(),
        "OXIDE_ASSETS_ROOT must point to an existing assets directory: {}",
        root.display()
    );
    root
}

fn real_stage(name: &str) -> AnimateStage {
    let directory = real_assets_root()
        .join("shared/images/characters")
        .join(name);
    let animation = std::fs::read_to_string(directory.join("Animation.json"))
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()));
    let spritemap = std::fs::read_to_string(directory.join("spritemap1.json"))
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()));
    AnimateStage::parse(&animation, &spritemap)
        .unwrap_or_else(|error| panic!("parse {name} stage: {error}"))
}

#[test]
#[ignore = "requires OXIDE_ASSETS_ROOT; run explicitly for milestone acceptance"]
fn real_gf_stage_labels_bounds_and_dance_frames() {
    let stage = real_stage("gf");

    assert_eq!(stage.frame_rate(), 24);
    assert_eq!(stage.stage_size(), (1920, 1080));
    assert_eq!(stage.frame_count(), 184);
    assert_eq!(stage.label_frames("Idle"), Some((0..30).collect()));
    assert_eq!(
        stage.label_frames("Idle (Hair blowing)"),
        Some((124..154).collect())
    );
    assert_eq!(stage.label_frames("Idle (hair blowing)"), None);
    assert_eq!(stage.label_frames("idle"), None);

    let dance_left: Vec<Vec<SpriteQuad>> = (0..=14)
        .map(|frame| {
            stage
                .resolve_main_frame(frame)
                .unwrap_or_else(|error| panic!("resolve gf frame {frame}: {error}"))
        })
        .collect();
    for (frame, quads) in dance_left.iter().enumerate() {
        assert!(!quads.is_empty(), "gf danceLeft frame {frame} is empty");
    }

    let frame_zero = stage.resolve_main_frame(0).expect("gf frame 0 resolves");
    println!(
        "gf frame 0 quads: {} {:?}",
        frame_zero.len(),
        quad_names(&frame_zero)
    );
    assert_eq!(frame_zero.len(), 17);
    assert_eq!(
        quad_names(&frame_zero),
        [
            "16", "44", "42", "42", "44", "42", "42", "41", "39", "39", "13", "14", "15", "11",
            "12", "9", "10"
        ]
    );
    assert_eq!(frame_zero, dance_left[0]);

    let bounds = stage.bounds();
    println!("gf bounds: {bounds:?}");
    assert_close(bounds[0], -194.9, "gf bounds x");
    assert_close(bounds[1], -333.65, "gf bounds y");
    assert_close(bounds[2], 748.8, "gf bounds w");
    assert_close(bounds[3], 674.85, "gf bounds h");
}

#[test]
#[ignore = "requires OXIDE_ASSETS_ROOT; run explicitly for milestone acceptance"]
fn real_bf_stage_labels_and_idle_draw_list() {
    let stage = real_stage("bf");

    assert_eq!(stage.label_frames("Idle"), Some((0..14).collect()));
    assert_eq!(stage.label_frames("Left"), Some((14..22).collect()));
    assert_eq!(stage.label_frames("Left Miss"), Some((22..56).collect()));
    assert_eq!(stage.label_frames("Right"), Some((56..63).collect()));
    assert_eq!(stage.label_frames("Right Miss"), Some((63..109).collect()));
    assert_eq!(stage.label_frames("Up"), Some((109..116).collect()));
    assert_eq!(stage.label_frames("Up Miss"), Some((116..140).collect()));
    assert_eq!(stage.label_frames("Down"), Some((140..147).collect()));
    assert_eq!(stage.label_frames("Down Miss"), Some((147..176).collect()));
    assert_eq!(stage.label_frames("Hey"), Some((176..202).collect()));
    assert_eq!(stage.label_frames("Cheer"), Some((202..211).collect()));
    assert_eq!(stage.label_frames("Scared"), Some((211..223).collect()));
    assert_eq!(stage.label_frames("left miss"), None);

    let quads = stage.resolve_main_frame(0).expect("bf frame 0 resolves");
    println!("bf frame 0 quads: {} {quads:?}", quads.len());
    assert_eq!(quads.len(), 4);
    assert_eq!(quad_names(&quads), ["21", "20", "18", "19"]);
    assert_eq!(
        stage.resolve_main_frame(0).expect("bf frame 0 resolves"),
        quads
    );

    let bounds = stage.bounds();
    println!("bf bounds: {bounds:?}");
    assert_close(bounds[0], -214.4, "bf bounds x");
    assert_close(bounds[1], -271.3, "bf bounds y");
    assert_close(bounds[2], 496.92, "bf bounds w");
    assert_close(bounds[3], 513.85, "bf bounds h");
}

#[test]
#[ignore = "requires OXIDE_ASSETS_ROOT; run explicitly for milestone acceptance"]
fn real_dad_stage_labels_rotated_tiles_and_bounds() {
    let stage = real_stage("dad");

    assert_eq!(stage.frame_count(), 36);
    // Observed inclusive label spans: Idle 0..11, Up 12..17, Down 18..23,
    // Left 24..29, Right 30..35.
    assert_eq!(stage.label_frames("Idle"), Some((0..=11).collect()));
    assert_eq!(stage.label_frames("Up"), Some((12..=17).collect()));
    assert_eq!(stage.label_frames("Down"), Some((18..=23).collect()));
    assert_eq!(stage.label_frames("Left"), Some((24..=29).collect()));
    assert_eq!(stage.label_frames("Right"), Some((30..=35).collect()));

    // `idle-hold` frameIndices semantics: indices address the label frame
    // list; out-of-range indices are skipped, leaving [11, 0, 1].
    let idle = stage.label_frames("Idle").expect("dad idle label");
    let hold_frames: Vec<usize> = [11usize, 12, 0, 1]
        .iter()
        .filter_map(|index| idle.get(*index).copied())
        .collect();
    assert_eq!(hold_frames, vec![11, 0, 1]);

    let rotated_symbols = [
        (
            "dad mic idle hand",
            "2",
            [202, 598, 113, 148],
            [0.0, 1.0, -1.0, 0.0, 113.0, 0.0],
        ),
        (
            "Dad default head",
            "3",
            [202, 756, 257, 197],
            [0.0, 1.0, -1.0, 0.0, 257.0, -0.05],
        ),
        (
            "Dad head UP + LEFT NOTE",
            "6",
            [719, 2, 270, 197],
            [0.0, 1.001, -1.001, 0.0, 270.22, -0.05],
        ),
        (
            "Dad Head RIGHT NOTE",
            "11",
            [719, 729, 255, 200],
            [0.0, 1.0, -1.0, 0.0, 254.75, -0.05],
        ),
    ];
    for (symbol, name, expected_rect, expected) in rotated_symbols {
        let quads = stage
            .resolve_symbol_frame(symbol, 0)
            .unwrap_or_else(|error| panic!("resolve dad symbol {symbol:?}: {error}"));
        let quad = quads
            .iter()
            .find(|quad| quad.bitmap_name == name)
            .unwrap_or_else(|| panic!("sprite {name:?} absent from {symbol:?}"));
        assert!(quad.rotated, "sprite {name:?} is not marked rotated");
        println!(
            "dad rotated sprite {name:?} in {symbol:?}: {:?}",
            quad.matrix
        );
        assert_eq!(
            quad.atlas_rect, expected_rect,
            "sprite {name:?} rect mismatch"
        );
        for (index, want) in expected.iter().enumerate() {
            assert!(
                (quad.matrix[index] - want).abs() < 1e-9,
                "sprite {name:?} matrix[{index}] = {} (expected {want})",
                quad.matrix[index]
            );
        }
    }

    let mut rotated_seen = std::collections::BTreeSet::new();
    for frame in 0..stage.frame_count() {
        for quad in stage
            .resolve_main_frame(frame)
            .unwrap_or_else(|error| panic!("resolve dad frame {frame}: {error}"))
        {
            if quad.rotated {
                rotated_seen.insert(quad.bitmap_name);
            }
        }
    }
    for name in ["2", "3", "6", "11"] {
        assert!(
            rotated_seen.contains(name),
            "rotated sprite {name:?} absent from the main timeline"
        );
    }
    println!("dad rotated sprites in main timeline: {rotated_seen:?}");

    let bounds = stage.bounds();
    println!("dad bounds: {bounds:?}");
    assert_close(bounds[0], -270.15, "dad bounds x");
    assert_close(bounds[1], -582.4044, "dad bounds y");
    assert_close(bounds[2], 566.785472, "dad bounds w");
    assert_close(bounds[3], 859.9684, "dad bounds h");
}
