use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use oxide_data::chart::SongEvent;
use oxide_data::events::EventRegistry;
use oxide_data::{
    DataError, DataKind, ParsedDocument, Typed, classify, implied_kind, parse_any, parse_file,
};
use serde_json::{Value, json};

/// Numeric equivalence: `3 == 3.0`, as required by the frozen contract.
fn numeric_eq(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => match (left.as_f64(), right.as_f64()) {
            (Some(left), Some(right)) => left == right,
            _ => false,
        },
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right.iter())
                    .all(|(left, right)| numeric_eq(left, right))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, value)| {
                    right.get(key).is_some_and(|other| numeric_eq(value, other))
                })
        }
        _ => left == right,
    }
}

fn serialized_typed(document: &ParsedDocument) -> Value {
    match &document.typed {
        Typed::Chart(value) => serde_json::to_value(value.as_ref()),
        Typed::Metadata(value) => serde_json::to_value(value.as_ref()),
        Typed::NoteStyle(value) => serde_json::to_value(value.as_ref()),
        Typed::Character(value) => serde_json::to_value(value.as_ref()),
        Typed::Stage(value) => serde_json::to_value(value.as_ref()),
        Typed::Level(value) => serde_json::to_value(value.as_ref()),
        Typed::Save(value) => serde_json::to_value(value.as_ref()),
        Typed::Player(value) => serde_json::to_value(value.as_ref()),
        Typed::Unknown(value) => Ok(value.clone()),
    }
    .expect("typed document should serialize")
}

fn assert_round_trip(document: &ParsedDocument, source: &str) {
    let actual = serialized_typed(document);
    let expected: Value = serde_json::from_str(source).expect("test source should be valid JSON");
    assert!(
        numeric_eq(&actual, &expected),
        "round-trip mismatch\nexpected: {expected}\nactual:   {actual}"
    );
}

// A subset of preload/data/songs/bopeebo/bopeebo-chart.json: the full 88-note
// easy difficulty plus representative events.
const BOPEBOO_CHART: &str = r#"{
  "version": "2.0.0",
  "scrollSpeed": { "easy": 1.2, "normal": 1.3, "hard": 1.6 },
  "events": [
    { "t": 0, "e": "FocusCamera", "v": 1 },
    { "t": 2400, "e": "FocusCamera", "v": 0 },
    { "t": 4200, "e": "PlayAnimation", "v": { "anim": "hey", "force": true, "target": "bf" } },
    { "t": 4800, "e": "FocusCamera", "v": 1 }
  ],
  "notes": {
    "easy": [
      {"t":0,"d":6},{"t":600,"d":7,"l":1125},{"t":2400,"d":2},{"t":3000,"d":3,"l":1125},{"t":4800,"d":5,"l":412.5},{"t":5400,"d":4,"l":375},{"t":6000,"d":7,"l":525},{"t":7200,"d":1,"l":412.5},{"t":7800,"d":0,"l":375},{"t":8400,"d":3,"l":525},{"t":9600,"d":5,"l":637.5},{"t":10500,"d":4},{"t":10800,"d":5,"l":525},{"t":12000,"d":1,"l":637.5},{"t":12900,"d":0},{"t":13200,"d":1,"l":525},{"t":14400,"d":7,"l":487.5},{"t":15300,"d":4},{"t":15600,"d":6,"l":525},{"t":16800,"d":3,"l":487.5},{"t":17700,"d":0},{"t":18000,"d":2,"l":525},{"t":19200,"d":4,"l":450},{"t":19800,"d":5,"l":825},{"t":21600,"d":0,"l":450},{"t":22200,"d":1,"l":825},{"t":24000,"d":5,"l":450},{"t":24600,"d":4,"l":825},{"t":26400,"d":1,"l":450},{"t":27000,"d":0,"l":825},{"t":28800,"d":6,"l":412.5},{"t":29400,"d":4,"l":1125},{"t":31200,"d":2,"l":412.5},{"t":31800,"d":0,"l":1125},{"t":33300,"d":6},{"t":33600,"d":4,"l":450},{"t":34500,"d":6},{"t":34800,"d":5,"l":675},{"t":35700,"d":2},{"t":36000,"d":0,"l":450},{"t":36900,"d":2},{"t":37200,"d":1,"l":675},{"t":38400,"d":6,"l":525},{"t":39000,"d":7,"l":412.5},{"t":39600,"d":4,"l":525},{"t":40800,"d":2,"l":525},{"t":41400,"d":3,"l":412.5},{"t":42000,"d":0,"l":525},{"t":43200,"d":5},{"t":43800,"d":6},{"t":44400,"d":5,"l":375},{"t":45000,"d":6},{"t":45600,"d":1},{"t":46200,"d":2},{"t":46800,"d":1,"l":375},{"t":47400,"d":2},{"t":48000,"d":6,"l":525},{"t":48600,"d":7,"l":412.5},{"t":49200,"d":4,"l":525},{"t":50400,"d":2,"l":525},{"t":51000,"d":3,"l":412.5},{"t":51600,"d":0,"l":525},{"t":52800,"d":7,"l":1875},{"t":55200,"d":3,"l":1875},{"t":57600,"d":6,"l":412.5},{"t":58200,"d":4,"l":1125},{"t":60000,"d":2,"l":412.5},{"t":60600,"d":0,"l":1125},{"t":62400,"d":4,"l":450},{"t":63300,"d":6},{"t":63600,"d":5,"l":675},{"t":64800,"d":0,"l":450},{"t":65700,"d":2},{"t":66000,"d":1,"l":675},{"t":67200,"d":6,"l":412.5},{"t":67800,"d":4,"l":450},{"t":68400,"d":5,"l":525},{"t":69600,"d":2,"l":412.5},{"t":70200,"d":0,"l":450},{"t":70800,"d":1,"l":525},{"t":71700,"d":6},{"t":72000,"d":4,"l":450},{"t":72900,"d":6},{"t":73200,"d":5,"l":675},{"t":74100,"d":2},{"t":74400,"d":0,"l":450},{"t":75300,"d":2},{"t":75600,"d":1,"l":675}
    ]
  },
  "generatedBy": "Friday Night Funkin' - v0.8.5"
}"#;

const BOPEBOO_METADATA: &str = r#"{
  "version": "2.2.4",
  "songName": "Bopeebo",
  "artist": "Kawai Sprite",
  "charter": "ninjamuffin99 + MtH",
  "offsets": {},
  "playData": {
    "songVariations": ["erect", "pico"],
    "difficulties": ["easy", "normal", "hard"],
    "characters": {
      "player": "bf",
      "girlfriend": "gf",
      "opponent": "dad",
      "altInstrumentals": ["pico"],
      "opponentVocals": ["dad"],
      "playerVocals": ["bf"]
    },
    "stage": "mainStage",
    "noteStyle": "funkin",
    "ratings": { "easy": 1, "normal": 1, "hard": 2 },
    "album": "volume1"
  },
  "generatedBy": "Friday Night Funkin' - v0.8.1",
  "timeChanges": [{ "t": 0, "b": 0, "bpm": 100, "bt": [4, 4, 4, 4] }]
}"#;

const LEGACY_METADATA: &str = r#"{
  "timeFormat": "ms",
  "artist": "Kawai Sprite",
  "songName": "Freaky Menu",
  "timeChanges": [{ "d": 4, "n": 4, "t": 0, "bt": [4, 4, 4, 4], "bpm": 102 }],
  "generatedBy": "Manual (by EliteMasterEric)",
  "looped": true,
  "version": "2.0.0"
}"#;

const CHARACTER_WITH_NULL_ICON: &str = r#"{
  "version": "1.0.0",
  "name": "Test Character",
  "assetPath": "characters/test",
  "healthIcon": null,
  "animations": [
    { "name": "idle", "prefix": "Idle" }
  ]
}"#;

const MAIN_STAGE: &str = r#"{
  "props": [
    {
      "danceEvery": 0,
      "zIndex": 10,
      "position": [-600, -200],
      "scale": [1, 1],
      "animType": "sparrow",
      "name": "stageBack",
      "isPixel": false,
      "assetPath": "stageback",
      "scroll": [0.9, 0.9],
      "animations": []
    },
    {
      "danceEvery": 0,
      "zIndex": 20,
      "position": [-650, 600],
      "scale": [1.1, 1.1],
      "animType": "sparrow",
      "name": "stageFront",
      "isPixel": false,
      "assetPath": "stagefront",
      "scroll": [0.9, 0.9],
      "animations": []
    },
    {
      "danceEvery": 0,
      "zIndex": 30,
      "position": [-500, -300],
      "scale": [0.9, 0.9],
      "animType": "sparrow",
      "name": "stageCurtains",
      "isPixel": false,
      "assetPath": "stagecurtains",
      "scroll": [1.3, 1.3],
      "animations": []
    }
  ],
  "cameraZoom": 1.1,
  "version": "1.0.0",
  "characters": {
    "bf": { "zIndex": 300, "position": [989.5, 885], "cameraOffsets": [-100, -100] },
    "dad": { "zIndex": 200, "position": [335, 885], "cameraOffsets": [150, -100] },
    "gf": { "zIndex": 100, "cameraOffsets": [0, 0], "position": [751.5, 787] }
  },
  "name": "Main Stage",
  "directory": "week1"
}"#;

const WEEK1_LEVEL: &str = r##"{
  "version": "1.0.0",
  "name": "DADDY DEAREST",
  "titleAsset": "storymenu/titles/week1",
  "props": [
    {
      "assetPath": "storymenu/props/dad",
      "scale": 1.0,
      "offsets": [100, 60],
      "animations": [
        { "name": "idle", "prefix": "idle0", "frameRate": 24 }
      ]
    }
  ],
  "background": "#F9CF51",
  "songs": [
    { "songId": "bopeebo", "difficulties": ["easy", "normal", "hard"] },
    { "songId": "fresh", "difficulties": ["easy", "normal", "hard"] },
    { "songId": "dadbattle", "difficulties": ["easy", "normal", "hard"] }
  ]
}"##;

const WEEK1_LEVEL_STRING_SONGS: &str = r##"{
  "version": "1.0.0",
  "name": "DADDY DEAREST",
  "titleAsset": "storymenu/titles/week1",
  "props": [],
  "background": "#F9CF51",
  "songs": ["bopeebo", "fresh", "dadbattle"]
}"##;

const NOTE_STYLE: &str = r#"{
  "version": "1.1.0",
  "name": "Funkin'",
  "author": "PhantomArcade",
  "fallback": null,
  "assets": {}
}"#;

const MINIMAL_SAVE: &str = r#"{
  "options": { "volume": 1.0, "downscroll": false },
  "songScores": {},
  "songRankings": {},
  "unlocks": { "charactersSeen": ["bf"] },
  "newCharacter": "pico",
  "vars": { "seenIntro": true }
}"#;

#[test]
fn chart_round_trip() {
    let document = parse_any(BOPEBOO_CHART.as_bytes()).expect("chart should parse");
    assert_eq!(document.kind, DataKind::Chart);

    let Typed::Chart(chart) = document.typed else {
        panic!("expected a typed chart");
    };
    assert_eq!(chart.notes["easy"].len(), 88);
    assert_eq!(chart.events[0].event, "FocusCamera");
    assert_eq!(chart.events[0].value, Some(json!(1)));

    assert_round_trip(
        &ParsedDocument {
            kind: DataKind::Chart,
            typed: Typed::Chart(chart),
        },
        BOPEBOO_CHART,
    );
}

#[test]
fn metadata_round_trip() {
    let document = parse_any(BOPEBOO_METADATA.as_bytes()).expect("metadata should parse");
    assert_eq!(document.kind, DataKind::Metadata);

    let Typed::Metadata(metadata) = document.typed else {
        panic!("expected typed metadata");
    };
    let characters = metadata
        .play_data
        .as_ref()
        .and_then(|play_data| play_data.characters.as_ref())
        .expect("play data characters should be present");
    assert_eq!(characters.opponent.as_deref(), Some("dad"));
    assert_eq!(
        metadata
            .time_changes
            .as_ref()
            .expect("time changes should be present")[0]
            .bpm,
        100.0
    );

    assert_round_trip(
        &ParsedDocument {
            kind: DataKind::Metadata,
            typed: Typed::Metadata(metadata),
        },
        BOPEBOO_METADATA,
    );
}

#[test]
fn legacy_metadata_parses_into_same_struct() {
    let document = parse_any(LEGACY_METADATA.as_bytes()).expect("legacy metadata should parse");
    assert_eq!(document.kind, DataKind::Metadata);

    let Typed::Metadata(metadata) = document.typed else {
        panic!("expected typed metadata");
    };
    assert_eq!(metadata.version, "2.0.0");
    assert_eq!(metadata.song_name.as_deref(), Some("Freaky Menu"));
    assert_eq!(metadata.time_format.as_deref(), Some("ms"));
    assert_eq!(metadata.looped, Some(true));
    assert_eq!(
        metadata
            .time_changes
            .as_ref()
            .expect("time changes should be present")[0]
            .bpm,
        102.0
    );
    assert!(metadata.play_data.is_none());
}

#[test]
fn character_tolerates_null_health_icon() {
    let document = parse_any(CHARACTER_WITH_NULL_ICON.as_bytes()).expect("character should parse");
    assert_eq!(document.kind, DataKind::Character);

    let Typed::Character(character) = document.typed else {
        panic!("expected typed character");
    };
    assert_eq!(character.health_icon, None);
    assert_eq!(character.render_type, None);
    assert_eq!(character.offsets, None);
    assert_eq!(character.animations.len(), 1);
}

#[test]
fn stage_props_round_trip() {
    let document = parse_any(MAIN_STAGE.as_bytes()).expect("stage should parse");
    assert_eq!(document.kind, DataKind::Stage);

    let Typed::Stage(stage) = document.typed else {
        panic!("expected typed stage");
    };
    assert_eq!(stage.props.len(), 3);
    assert_eq!(stage.props[0].anim_type.as_deref(), Some("sparrow"));

    assert_round_trip(
        &ParsedDocument {
            kind: DataKind::Stage,
            typed: Typed::Stage(stage),
        },
        MAIN_STAGE,
    );
}

#[test]
fn level_round_trip() {
    let document = parse_any(WEEK1_LEVEL.as_bytes()).expect("level should parse");
    assert_eq!(document.kind, DataKind::Level);

    let Typed::Level(level) = document.typed else {
        panic!("expected typed level");
    };
    assert_eq!(level.songs[0].song_id, "bopeebo");
    assert_eq!(
        level.songs[0].difficulties,
        Some(vec![
            "easy".to_string(),
            "normal".to_string(),
            "hard".to_string()
        ])
    );

    assert_round_trip(
        &ParsedDocument {
            kind: DataKind::Level,
            typed: Typed::Level(level),
        },
        WEEK1_LEVEL,
    );
}

#[test]
fn level_accepts_shipped_string_song_list() {
    let document =
        parse_any(WEEK1_LEVEL_STRING_SONGS.as_bytes()).expect("string song list should parse");
    assert_eq!(document.kind, DataKind::Level);

    let Typed::Level(level) = document.typed else {
        panic!("expected typed level");
    };
    assert_eq!(level.songs.len(), 3);
    assert_eq!(level.songs[0].song_id, "bopeebo");
    assert_eq!(level.songs[0].difficulties, None);

    assert_round_trip(
        &ParsedDocument {
            kind: DataKind::Level,
            typed: Typed::Level(level),
        },
        WEEK1_LEVEL_STRING_SONGS,
    );
}

#[test]
fn note_style_null_fallback_is_none() {
    let document = parse_any(NOTE_STYLE.as_bytes()).expect("note style should parse");
    assert_eq!(document.kind, DataKind::NoteStyle);

    let Typed::NoteStyle(note_style) = document.typed else {
        panic!("expected typed note style");
    };
    assert_eq!(note_style.fallback, None);

    assert_round_trip(
        &ParsedDocument {
            kind: DataKind::NoteStyle,
            typed: Typed::NoteStyle(note_style),
        },
        NOTE_STYLE,
    );
}

#[test]
fn save_round_trip() {
    let document = parse_any(MINIMAL_SAVE.as_bytes()).expect("save should parse");
    assert_eq!(document.kind, DataKind::Save);

    let Typed::Save(save) = document.typed else {
        panic!("expected typed save");
    };
    assert_eq!(save.new_character.as_deref(), Some("pico"));
    assert!(save.song_scores.is_some());

    assert_round_trip(
        &ParsedDocument {
            kind: DataKind::Save,
            typed: Typed::Save(save),
        },
        MINIMAL_SAVE,
    );
}

#[test]
fn unknown_round_trips_with_strict_value_equality() {
    let source = r#"{"ATLAS":{},"meta":{}}"#;
    let document = parse_any(source.as_bytes()).expect("unknown JSON should parse");
    assert_eq!(document.kind, DataKind::Unknown);

    let Typed::Unknown(value) = document.typed else {
        panic!("expected verbatim unknown value");
    };
    let expected: Value = serde_json::from_str(source).expect("test source should be valid JSON");
    assert_eq!(value, expected);
}

#[test]
fn one_leading_bom_is_stripped() {
    let mut bytes = b"\xEF\xBB\xBF".to_vec();
    bytes.extend_from_slice(br#"{"ATLAS":{},"meta":{}}"#);

    let document = parse_any(&bytes).expect("BOM-prefixed JSON should parse");
    assert_eq!(document.kind, DataKind::Unknown);
    let Typed::Unknown(value) = document.typed else {
        panic!("expected verbatim unknown value");
    };
    assert_eq!(value, json!({ "ATLAS": {}, "meta": {} }));
}

#[test]
fn event_registry_knows_base_events_and_reports_unknowns() {
    let mut registry = EventRegistry::base();
    for name in [
        "FocusCamera",
        "ZoomCamera",
        "SetCameraBop",
        "PlayAnimation",
        "ScrollSpeed",
        "SetHealthIcon",
        "EnableMask",
    ] {
        assert!(registry.knows(name), "base event {name} should be known");
    }

    assert!(registry.register("CustomEvent"));
    assert!(!registry.register("CustomEvent"));
    assert!(registry.knows("CustomEvent"));

    let events = vec![
        SongEvent {
            time: 0.0,
            event: "FocusCamera".to_string(),
            value: Some(json!(1)),
        },
        SongEvent {
            time: 100.0,
            event: "sserafimPulseLights".to_string(),
            value: None,
        },
        SongEvent {
            time: 200.0,
            event: "sserafimPulseLights".to_string(),
            value: Some(json!({ "value": 1 })),
        },
    ];
    assert_eq!(
        registry.unknown_event_names(&events),
        vec!["sserafimPulseLights".to_string()]
    );
}

#[test]
fn classify_covers_every_kind() {
    let chart = json!({ "version": "2.0.0", "notes": {} });
    let metadata = json!({ "version": "2.2.4", "songName": "Bopeebo", "timeChanges": [] });
    let note_style = json!({ "version": "1.1.0", "assets": {} });
    let character = json!({ "version": "1.0.1", "assetPath": "characters/bf", "animations": [] });
    let stage = json!({ "version": "1.0.0", "name": "Main Stage", "props": [] });
    let level = json!({ "version": "1.0.0", "name": "Week 1", "props": [], "songs": [] });
    let save = json!({ "options": {}, "vars": {} });
    let player = json!({ "version": "1.0.0", "name": "Boyfriend", "ownedChars": ["bf"] });
    let unknown = json!({ "version": "9.9.9", "something": true });

    assert_eq!(classify(&chart), DataKind::Chart);
    assert_eq!(classify(&metadata), DataKind::Metadata);
    assert_eq!(classify(&note_style), DataKind::NoteStyle);
    assert_eq!(classify(&character), DataKind::Character);
    assert_eq!(classify(&stage), DataKind::Stage);
    assert_eq!(classify(&level), DataKind::Level);
    assert_eq!(classify(&save), DataKind::Save);
    assert_eq!(classify(&player), DataKind::Unknown);
    assert_eq!(classify(&unknown), DataKind::Unknown);
}

#[test]
fn parse_errors_are_typed() {
    assert!(matches!(parse_any(b"{"), Err(DataError::Parse(_))));
    assert!(matches!(parse_any(&[0xFF, 0xFE]), Err(DataError::NotUtf8)));

    let result = parse_any(br#"{"version":"2.0.0","notes":{"easy":"not-a-list"}}"#);
    match result {
        Err(DataError::Schema { kind, .. }) => assert_eq!(kind, "chart"),
        other => panic!("expected a schema error, got {other:?}"),
    }
}

#[test]
fn implied_kind_covers_layout_rules() {
    let cases = [
        (
            "/assets/preload/data/songs/bopeebo/bopeebo-chart.json",
            Some(DataKind::Chart),
        ),
        (
            "/assets/preload/data/songs/bopeebo/bopeebo-metadata.json",
            Some(DataKind::Metadata),
        ),
        (
            "/assets/mods/my-mod/data/characters/bf.json",
            Some(DataKind::Character),
        ),
        (
            "/assets/preload/data/stages/mainStage.json",
            Some(DataKind::Stage),
        ),
        (
            "/assets/preload/data/levels/week1.json",
            Some(DataKind::Level),
        ),
        (
            "/assets/preload/data/notestyles/funkin.json",
            Some(DataKind::NoteStyle),
        ),
        (
            "/assets/preload/data/players/bf.json",
            Some(DataKind::Player),
        ),
        ("/assets/scripts/characters/bf.json", None),
        ("/assets/images/players/bf.json", None),
        ("/assets/preload/data/songs/bopeebo/something.json", None),
        ("/assets/preload/data/characters/sub/bf.json", None),
        ("/assets/preload/data/other/file.json", None),
        ("/assets/preload/data/characters.json", None),
    ];

    for (path, expected) in cases {
        assert_eq!(implied_kind(Path::new(path)), expected, "path: {path}");
    }
}

#[test]
fn parse_file_implied_chart_rejects_array_notes() {
    let path = Path::new("/assets/preload/data/songs/regression/regression-chart.json");
    let bytes = br#"{
        "version": "2.0.0",
        "notes": [{ "t": 0, "d": 0 }],
        "events": [],
        "scrollSpeed": { "easy": 1, "normal": 1, "hard": 1 }
    }"#;

    match parse_file(path, bytes) {
        Err(DataError::Schema { kind, .. }) => assert_eq!(kind, "chart"),
        other => panic!("expected a schema error, got {other:?}"),
    }
}

#[test]
fn parse_file_implied_kind_wins_over_shape() {
    let path = Path::new("/assets/preload/data/players/bf.json");
    let bytes = br#"{
        "version": "1.0.0",
        "name": "Shape Character",
        "assetPath": "characters/bf",
        "animations": []
    }"#;

    let document = parse_file(path, bytes).expect("implied player should parse");
    assert_eq!(document.kind, DataKind::Player);
    match document.typed {
        Typed::Player(player) => assert_eq!(player.name, "Shape Character"),
        other => panic!("expected a typed player, got {other:?}"),
    }
}

const METADATA_WITHOUT_VARIATIONS: &str = r#"{
  "version": "2.2.4",
  "songName": "No Variations",
  "artist": "Artist",
  "playData": {
    "difficulties": ["normal"],
    "stage": "mainStage",
    "noteStyle": "funkin"
  },
  "timeChanges": [{ "t": 0, "bpm": 100 }]
}"#;

#[test]
fn metadata_without_song_variations_does_not_emit_the_key() {
    let document =
        parse_any(METADATA_WITHOUT_VARIATIONS.as_bytes()).expect("metadata should parse");
    assert_eq!(document.kind, DataKind::Metadata);
    assert_round_trip(&document, METADATA_WITHOUT_VARIATIONS);

    let Typed::Metadata(metadata) = document.typed else {
        panic!("expected typed metadata");
    };
    let play_data = metadata.play_data.as_ref().expect("play data should exist");
    assert_eq!(play_data.song_variations, None);

    let value = serde_json::to_value(metadata.as_ref()).expect("metadata should serialize");
    let serialized_play_data = value
        .get("playData")
        .and_then(Value::as_object)
        .expect("serialized play data should be an object");
    assert!(!serialized_play_data.contains_key("songVariations"));
}

const METADATA_WITH_TIME_SIGNATURE: &str = r#"{
  "version": "2.0.0",
  "songName": "Time Signature",
  "artist": "Artist",
  "timeChanges": [{ "t": 0, "bpm": 100, "d": 4, "n": 4 }]
}"#;

#[test]
fn time_change_time_signature_round_trip() {
    let document =
        parse_any(METADATA_WITH_TIME_SIGNATURE.as_bytes()).expect("metadata should parse");
    assert_eq!(document.kind, DataKind::Metadata);
    assert_round_trip(&document, METADATA_WITH_TIME_SIGNATURE);

    let Typed::Metadata(metadata) = document.typed else {
        panic!("expected typed metadata");
    };
    let time_change = &metadata
        .time_changes
        .as_ref()
        .expect("time changes should be present")[0];
    assert_eq!(time_change.time_signature_num, Some(4));
    assert_eq!(time_change.time_signature_den, Some(4));
}

const METADATA_WITHOUT_TIME_CHANGES: &str = r#"{
  "version": "2.2.4",
  "songName": "No Time Changes",
  "artist": "Artist"
}"#;

const METADATA_WITH_EMPTY_TIME_CHANGES: &str = r#"{
  "version": "2.2.4",
  "songName": "Empty Time Changes",
  "artist": "Artist",
  "timeChanges": []
}"#;

#[test]
fn metadata_time_changes_presence_round_trip() {
    let path = Path::new("/assets/preload/data/songs/no-time/no-time-metadata.json");

    let document =
        parse_file(path, METADATA_WITHOUT_TIME_CHANGES.as_bytes()).expect("metadata should parse");
    assert_eq!(document.kind, DataKind::Metadata);
    assert_round_trip(&document, METADATA_WITHOUT_TIME_CHANGES);

    let Typed::Metadata(metadata) = document.typed else {
        panic!("expected typed metadata");
    };
    assert_eq!(metadata.time_changes, None);
    let value = serde_json::to_value(metadata.as_ref()).expect("metadata should serialize");
    assert!(
        !value
            .as_object()
            .expect("metadata object")
            .contains_key("timeChanges")
    );

    let document = parse_file(path, METADATA_WITH_EMPTY_TIME_CHANGES.as_bytes())
        .expect("metadata should parse");
    assert_eq!(document.kind, DataKind::Metadata);
    assert_round_trip(&document, METADATA_WITH_EMPTY_TIME_CHANGES);

    let Typed::Metadata(metadata) = document.typed else {
        panic!("expected typed metadata");
    };
    assert_eq!(metadata.time_changes, Some(Vec::new()));
    let value = serde_json::to_value(metadata.as_ref()).expect("metadata should serialize");
    assert_eq!(value.get("timeChanges"), Some(&json!([])));
}

const STAGE_WITH_EMPTY_ANIMATIONS: &str = r#"{
  "version": "1.0.0",
  "name": "Stage",
  "props": [{ "name": "prop", "assetPath": "prop", "animations": [] }]
}"#;

const STAGE_WITHOUT_ANIMATIONS_KEY: &str = r#"{
  "version": "1.0.0",
  "name": "Stage",
  "props": [{ "name": "prop", "assetPath": "prop" }]
}"#;

fn serialized_first_stage_prop(source: &str) -> Value {
    let document = parse_any(source.as_bytes()).expect("stage should parse");
    assert_eq!(document.kind, DataKind::Stage);
    assert_round_trip(&document, source);

    let Typed::Stage(stage) = document.typed else {
        panic!("expected typed stage");
    };
    let value = serde_json::to_value(stage.as_ref()).expect("stage should serialize");
    value
        .get("props")
        .and_then(Value::as_array)
        .and_then(|props| props.first())
        .cloned()
        .expect("serialized stage should have one prop")
}

#[test]
fn stage_prop_empty_animations_array_is_preserved() {
    let document = parse_any(STAGE_WITH_EMPTY_ANIMATIONS.as_bytes()).expect("stage should parse");
    let Typed::Stage(stage) = document.typed else {
        panic!("expected typed stage");
    };
    assert_eq!(stage.props[0].animations, Some(Vec::new()));

    let prop = serialized_first_stage_prop(STAGE_WITH_EMPTY_ANIMATIONS);
    assert_eq!(prop.get("animations"), Some(&json!([])));
}

#[test]
fn stage_prop_missing_animations_key_stays_absent() {
    let document = parse_any(STAGE_WITHOUT_ANIMATIONS_KEY.as_bytes()).expect("stage should parse");
    let Typed::Stage(stage) = document.typed else {
        panic!("expected typed stage");
    };
    assert_eq!(stage.props[0].animations, None);

    let prop = serialized_first_stage_prop(STAGE_WITHOUT_ANIMATIONS_KEY);
    assert!(
        !prop
            .as_object()
            .expect("prop should be an object")
            .contains_key("animations")
    );
}

const LEVEL_WITH_CAPSULE: &str = r#"{
  "version": "1.0.0",
  "name": "LE SSERAFIM",
  "titleAsset": "storymenu/titles/sserafim",
  "props": [],
  "songs": ["spaghetti"],
  "capsule": { "name": "SP. COLLAB 1" }
}"#;

#[test]
fn level_capsule_round_trip() {
    let document = parse_any(LEVEL_WITH_CAPSULE.as_bytes()).expect("level should parse");
    assert_eq!(document.kind, DataKind::Level);
    assert_round_trip(&document, LEVEL_WITH_CAPSULE);

    let Typed::Level(level) = document.typed else {
        panic!("expected typed level");
    };
    assert_eq!(level.capsule, Some(json!({ "name": "SP. COLLAB 1" })));
}

const STAGE_WITH_ALPHA: &str = r#"{
  "version": "1.0.0",
  "name": "Philly",
  "props": [{ "name": "prop", "assetPath": "prop", "alpha": 1, "animations": [] }]
}"#;

#[test]
fn stage_prop_alpha_round_trip() {
    let document = parse_any(STAGE_WITH_ALPHA.as_bytes()).expect("stage should parse");
    assert_eq!(document.kind, DataKind::Stage);
    assert_round_trip(&document, STAGE_WITH_ALPHA);

    let Typed::Stage(stage) = document.typed else {
        panic!("expected typed stage");
    };
    assert_eq!(stage.props[0].alpha, Some(1.0));
}

const CHARACTER_WITH_ANIMATION_METADATA: &str = r#"{
  "version": "1.0.0",
  "name": "Pico",
  "assetPath": "characters/pico",
  "animations": [
    {
      "name": "singRIGHT-censor",
      "prefix": "pico swear right",
      "offsets": [-169, -64],
      "frameIndices": [0, 1, 2],
      "assetPath": "characters/Pico_Censored",
      "renderType": "sparrow"
    }
  ]
}"#;

#[test]
fn animation_frame_indices_and_asset_path_round_trip() {
    let document =
        parse_any(CHARACTER_WITH_ANIMATION_METADATA.as_bytes()).expect("character should parse");
    assert_eq!(document.kind, DataKind::Character);
    assert_round_trip(&document, CHARACTER_WITH_ANIMATION_METADATA);

    let Typed::Character(character) = document.typed else {
        panic!("expected typed character");
    };
    let animation = &character.animations[0];
    assert_eq!(animation.frame_indices, Some(vec![0, 1, 2]));
    assert_eq!(
        animation.asset_path.as_deref(),
        Some("characters/Pico_Censored")
    );
    assert_eq!(animation.render_type.as_deref(), Some("sparrow"));
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

#[test]
#[ignore = "requires OXIDE_ASSETS_ROOT; run explicitly for milestone acceptance"]
fn real_bopeebo_chart_smoke() {
    let path = real_assets_root().join("preload/data/songs/bopeebo/bopeebo-chart.json");
    let bytes =
        std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let document = parse_file(&path, &bytes).expect("live bopeebo chart should parse");
    assert_eq!(document.kind, DataKind::Chart);

    let Typed::Chart(chart) = document.typed else {
        panic!("expected typed chart");
    };
    assert_eq!(chart.notes["easy"].len(), 88);
    assert_eq!(chart.notes["normal"].len(), 102);
    assert_eq!(chart.notes["hard"].len(), 124);
    assert_eq!(chart.events[0].event, "FocusCamera");
    assert_eq!(chart.events[0].value, Some(json!(1)));
}

#[test]
#[ignore = "requires OXIDE_ASSETS_ROOT; run explicitly for milestone acceptance"]
fn real_senpai_character_smoke() {
    let path = real_assets_root().join("preload/data/characters/senpai.json");
    let bytes =
        std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let document = parse_file(&path, &bytes).expect("live senpai character should parse");
    assert_eq!(document.kind, DataKind::Character);

    let Typed::Character(character) = document.typed else {
        panic!("expected typed character");
    };
    assert_eq!(character.name, "Senpai");
    assert_eq!(character.render_type, None);
    assert_eq!(character.is_pixel, Some(true));
    assert_eq!(character.scale, Some(6.0));
    assert_eq!(character.animations.len(), 5);
}

#[test]
#[ignore = "requires OXIDE_ASSETS_ROOT; run explicitly for milestone acceptance"]
fn real_bf_player_round_trip_via_parse_file() {
    let path = real_assets_root().join("preload/data/players/bf.json");
    let bytes =
        std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let source = std::str::from_utf8(&bytes).expect("live bf player should be UTF-8");
    let document = parse_file(&path, &bytes).expect("live bf player should parse");
    assert_eq!(document.kind, DataKind::Player);
    assert_round_trip(&document, source);

    let Typed::Player(player) = document.typed else {
        panic!("expected typed player");
    };
    assert_eq!(player.version, "1.0.0");
    assert_eq!(player.name, "Boyfriend");
    assert_eq!(player.owned_chars.len(), 6);
    assert_eq!(player.owned_chars[0], "bf");
    assert!(player.show_unowned_chars);
    assert!(player.unlocked);
    assert_eq!(player.freeplay_style.as_deref(), Some("bf"));
    assert_eq!(player.sticker_pack.as_deref(), Some("standard-bf"));
    assert!(player.freeplay_dj.is_some());
    assert!(player.char_select.is_some());
    assert!(player.results.is_some());
}

#[test]
#[ignore = "requires OXIDE_ASSETS_ROOT; run explicitly for milestone acceptance"]
fn real_assets_smoke_via_parse_file() {
    let root = real_assets_root();

    let cases = [
        (
            "preload/data/songs/bopeebo/bopeebo-chart.json",
            DataKind::Chart,
        ),
        (
            "preload/data/songs/bopeebo/bopeebo-metadata.json",
            DataKind::Metadata,
        ),
        ("preload/data/characters/senpai.json", DataKind::Character),
        ("preload/data/stages/mainStage.json", DataKind::Stage),
        ("preload/data/levels/week1.json", DataKind::Level),
        ("preload/data/notestyles/funkin.json", DataKind::NoteStyle),
        ("preload/data/players/bf.json", DataKind::Player),
    ];

    for (relative, expected) in cases {
        let path = root.join(relative);
        let bytes =
            std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let document = parse_file(&path, &bytes)
            .unwrap_or_else(|error| panic!("{}: {error:?}", path.display()));
        assert_eq!(document.kind, expected, "path: {}", path.display());
    }
}

fn kind_label(kind: DataKind) -> &'static str {
    match kind {
        DataKind::Chart => "chart",
        DataKind::Metadata => "metadata",
        DataKind::NoteStyle => "notestyle",
        DataKind::Character => "character",
        DataKind::Stage => "stage",
        DataKind::Level => "level",
        DataKind::Save => "save",
        DataKind::Player => "player",
        DataKind::Unknown => "unknown",
    }
}

fn collect_json_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("read directory {}: {error}", dir.display()));
    for entry in entries {
        let entry =
            entry.unwrap_or_else(|error| panic!("read entry in {}: {error}", dir.display()));
        let path = entry.path();
        let metadata = std::fs::metadata(&path)
            .unwrap_or_else(|error| panic!("inspect {}: {error}", path.display()));
        if metadata.is_dir() {
            collect_json_files(&path, out);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("json") {
            out.push(path);
        }
    }
}

#[test]
#[ignore = "requires OXIDE_ASSETS_ROOT; run explicitly for milestone acceptance"]
fn real_assets_all_round_trip_via_parse_file() {
    let root = real_assets_root();
    let mut files = Vec::new();
    collect_json_files(&root, &mut files);
    files.sort();
    assert_eq!(files.len(), 432, "unexpected real asset JSON count");

    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut failures = Vec::new();

    for path in &files {
        let bytes =
            std::fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let source = match std::str::from_utf8(&bytes) {
            Ok(source) => source,
            Err(error) => {
                failures.push(format!("{}: not UTF-8: {error}", path.display()));
                continue;
            }
        };
        let document = match parse_file(path, &bytes) {
            Ok(document) => document,
            Err(error) => {
                failures.push(format!("{}: {error:?}", path.display()));
                continue;
            }
        };
        *counts.entry(kind_label(document.kind)).or_default() += 1;

        let source = source.strip_prefix('\u{FEFF}').unwrap_or(source);
        let expected: Value = match serde_json::from_str(source) {
            Ok(expected) => expected,
            Err(error) => {
                failures.push(format!("{}: reparse error: {error}", path.display()));
                continue;
            }
        };
        let actual = serialized_typed(&document);
        let matches = if matches!(document.typed, Typed::Unknown(_)) {
            actual == expected
        } else {
            numeric_eq(&actual, &expected)
        };
        if !matches {
            failures.push(format!("{}: round-trip mismatch", path.display()));
        }
    }

    assert!(
        failures.is_empty(),
        "{} round-trip failures:\n{}",
        failures.len(),
        failures
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert_eq!(counts.get("player"), Some(&2));
    assert_eq!(counts.get("unknown"), Some(&203));
    assert_eq!(counts.values().sum::<usize>(), 432);
    eprintln!("real-assets counts: {counts:?}");
}
