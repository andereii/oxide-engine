//! Structural M3 tests over the committed mini fixture: classification results,
//! deterministic pak bytes, entry policy and frozen timestamps.
//!
//! External-tool tests are explicitly ignored by default. Milestone acceptance
//! includes them and requires ffmpeg/ffprobe; portable policy tests always run.

use std::fs::File;
use std::path::{Path, PathBuf};

use oxide_media::{MediaError, PackReport, Plan};
use zip::{CompressionMethod, HasZipMetadata};

fn repo_root() -> PathBuf {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_dir
        .parent()
        .and_then(Path::parent)
        .expect("crate dir has a workspace root")
        .to_path_buf()
}

fn fixture() -> PathBuf {
    repo_root().join("tests/fixtures/media/mini")
}

fn temp_dir(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("oxide-media-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("create temp dir");
    path
}

fn require_tools() {
    oxide_media::check_tools().expect("milestone acceptance requires ffmpeg/ffprobe");
}

fn assert_audio_artifacts(output: &Path) {
    for stem in ["Inst", "Voices-bf"] {
        let relative = format!("songs/synthetic/{stem}");
        let source = fixture().join(format!("{relative}.ogg"));
        let converted = output.join(format!("{relative}.opus"));
        assert!(
            converted.is_file(),
            "missing conversion: {}",
            converted.display()
        );
        assert!(
            std::fs::metadata(converted).unwrap().len() < std::fs::metadata(source).unwrap().len(),
            "synthetic audio must shrink, not fall back to a copy"
        );
        assert!(!output.join(format!("{relative}.ogg")).exists());
    }
    for relative in [
        "songs/testsong/Inst.ogg",
        "songs/testsong/Voices-bf.ogg",
        "sounds/beep.ogg",
        "sounds/quiet.ogg",
    ] {
        assert_eq!(
            std::fs::read(fixture().join(relative)).unwrap(),
            std::fs::read(output.join(relative)).unwrap(),
            "low-bitrate audio must be byte-copied: {relative}"
        );
    }
}

fn assert_fixture_counts(report: &PackReport) {
    assert_eq!(
        report.audio_converted, 2,
        "synthetic high-bitrate stems must convert"
    );
    assert_eq!(report.audio_copied, 4);
    assert_eq!(report.images_converted, 1);
    assert_eq!(report.images_kept, 0);
    assert_eq!(report.video_converted, 2);
    assert_eq!(report.video_copied, 0);
    assert_eq!(report.video_converted_checked, report.video_converted);
    assert_eq!(report.video_excluded, 1);
    assert_eq!(report.text_files, 2);
    assert_eq!(report.font_files, 1);
    assert_eq!(report.skipped, 1);
    assert_eq!(report.hxc_excluded, 1);
    assert_eq!(report.hxc_bytes, 45);
    assert_eq!(report.mp3_dropped, 0);
    assert_eq!(report.files_grew, 0);
    assert_eq!(report.align_checked, 2, "copied and converted stem pairs");
    assert_eq!(report.align_ok, report.align_checked);
    assert_eq!(report.duration_checked, report.audio_converted);
    assert_eq!(report.duration_ok, report.duration_checked);
    assert_eq!(report.image_converted_checked, report.images_converted);
    assert_eq!(report.visible_pixels_identical, report.visible_pixels_total);
    assert!(report.visible_pixels_total > 0);
    assert!(report.alpha_zero_rgb_diff > 0);
    assert!(!report.visible_check_failed);
    assert!(report.video_ssim_min >= 0.93);
    assert_eq!(report.pak_entries, 12);
}

#[test]
#[ignore = "requires ffmpeg/ffprobe; executed by milestone acceptance"]
fn pack_is_byte_deterministic_and_policies_hold() {
    require_tools();
    let root = temp_dir("determinism");
    let first_dir = root.join("first");
    let second_dir = root.join("second");

    let first =
        oxide_media::pack_and_verify(&fixture(), &first_dir, 2).expect("first pack must succeed");
    let second =
        oxide_media::pack_and_verify(&fixture(), &second_dir, 2).expect("second pack must succeed");

    assert_fixture_counts(&first);
    assert_fixture_counts(&second);
    assert_audio_artifacts(&first_dir);
    assert_audio_artifacts(&second_dir);
    assert_eq!(first.pak_sha256, second.pak_sha256);
    assert!(first_dir.join(oxide_media::DIST_MARKER).is_file());
    assert!(second_dir.join(oxide_media::DIST_MARKER).is_file());

    let first_bytes = std::fs::read(first_dir.join("assets.pak")).expect("first pak");
    let second_bytes = std::fs::read(second_dir.join("assets.pak")).expect("second pak");
    assert_eq!(first_bytes, second_bytes, "pak bytes must be identical");

    let file = File::open(second_dir.join("assets.pak")).expect("open pak");
    let mut archive = zip::ZipArchive::new(file).expect("read pak");
    let mut names = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).expect("pak entry");
        names.push(entry.name().to_string());
        let time = entry.last_modified().expect("fixed timestamp");
        assert_eq!(
            (
                time.year(),
                time.month(),
                time.day(),
                time.hour(),
                time.minute(),
                time.second()
            ),
            (2000, 1, 1, 0, 0, 0),
            "entry {}",
            entry.name()
        );
        assert_eq!(entry.get_metadata().system, zip::System::Unix);
        let expected = if entry.name().ends_with(".json") || entry.name().ends_with(".txt") {
            CompressionMethod::Deflated
        } else {
            CompressionMethod::Stored
        };
        assert_eq!(
            entry.compression(),
            expected,
            "compression policy for {}",
            entry.name()
        );
    }
    assert!(
        names.windows(2).all(|pair| pair[0] < pair[1]),
        "pak entries must be sorted: {names:?}"
    );
    assert!(
        !names.iter().any(|name| name.ends_with(".hxc")),
        "hxc files must not enter the pak"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
#[ignore = "requires ffmpeg/ffprobe; executed by milestone acceptance"]
fn output_prep_requires_marker() {
    require_tools();
    let root = temp_dir("marker");
    let foreign = root.join("foreign");
    std::fs::create_dir_all(&foreign).expect("create foreign dir");
    std::fs::write(foreign.join("keep.txt"), "user data").expect("write foreign file");
    match oxide_media::pack(&fixture(), &foreign, 2) {
        Err(MediaError::Verify(message)) => {
            assert!(message.contains("refusing to wipe"), "{message}");
        }
        other => panic!("expected marker refusal, got {other:?}"),
    }
    assert!(foreign.join("keep.txt").is_file());

    let empty = root.join("empty");
    std::fs::create_dir_all(&empty).expect("create empty dir");
    oxide_media::pack(&fixture(), &empty, 2).expect("empty output dir is allowed");
    assert!(empty.join(oxide_media::DIST_MARKER).is_file());
    let marker_text =
        std::fs::read_to_string(empty.join(oxide_media::DIST_MARKER)).expect("marker");
    let marker: serde_json::Value = serde_json::from_str(&marker_text).expect("marker JSON");
    assert_eq!(marker["format"], "oxide-assets-dist");
    assert_eq!(marker["version"], 1);
    assert!(marker["pak_sha256"].as_str().is_some());
    let owned = marker["owned"].as_array().expect("owned list");
    assert!(owned.iter().any(|path| path == "assets.pak"));
    assert!(owned.iter().any(|path| path == "images/micro.webp"));
    oxide_media::pack(&fixture(), &empty, 2).expect("marked output dir is replaceable");

    let invalid = root.join("invalid-marker");
    std::fs::create_dir_all(&invalid).expect("create invalid marker dir");
    std::fs::write(invalid.join("notes.txt"), "keep").expect("write notes");
    std::fs::write(invalid.join(oxide_media::DIST_MARKER), "not json").expect("write marker");
    match oxide_media::pack(&fixture(), &invalid, 2) {
        Err(MediaError::Verify(message)) => {
            assert!(message.contains("refusing to wipe"), "{message}");
        }
        other => panic!("expected invalid marker refusal, got {other:?}"),
    }
    assert!(invalid.join("notes.txt").is_file());

    let foreign_version = root.join("foreign-version");
    std::fs::create_dir_all(&foreign_version).expect("create foreign version dir");
    std::fs::write(foreign_version.join("notes.txt"), "keep").expect("write notes");
    std::fs::write(
        foreign_version.join(oxide_media::DIST_MARKER),
        "{\"format\":\"other\",\"version\":1,\"owned\":[],\"pak_sha256\":\"\"}",
    )
    .expect("write foreign marker");
    match oxide_media::pack(&fixture(), &foreign_version, 2) {
        Err(MediaError::Verify(message)) => {
            assert!(message.contains("refusing to wipe"), "{message}");
        }
        other => panic!("expected foreign marker refusal, got {other:?}"),
    }
    assert!(foreign_version.join("notes.txt").is_file());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
#[ignore = "requires ffmpeg/ffprobe; executed by milestone acceptance"]
fn durable_output_preserves_foreign_files_and_removes_stale_artifacts() {
    require_tools();
    let root = temp_dir("durable");
    let out = root.join("out");
    let first = oxide_media::pack(&fixture(), &out, 2).expect("first pack");
    let first_sha = first.pak_sha256.clone();

    std::fs::write(out.join("notes.txt"), "foreign note").expect("write foreign note");
    let second = oxide_media::pack(&fixture(), &out, 2).expect("second pack");
    assert_eq!(second.pak_sha256, first_sha, "repack must be deterministic");
    assert!(out.join("notes.txt").is_file(), "foreign note must survive");
    assert!(out.join("sounds/quiet.ogg").is_file());

    let reduced_input = root.join("reduced");
    std::fs::create_dir_all(reduced_input.join("data")).expect("create reduced input");
    std::fs::write(reduced_input.join("data/micro.json"), "{}").expect("write reduced input");
    oxide_media::pack(&reduced_input, &out, 2).expect("reduced pack");
    assert!(out.join("notes.txt").is_file(), "foreign note must survive");
    assert!(
        !out.join("sounds/quiet.ogg").exists(),
        "stale previous artefact must be deleted"
    );
    assert!(
        !out.join("images/micro.webp").exists(),
        "stale previous artefact must be deleted"
    );
    assert!(out.join("assets.pak").is_file());

    let third = oxide_media::pack(&fixture(), &out, 2).expect("restored pack");
    assert_eq!(third.pak_sha256, first_sha);
    assert!(out.join("notes.txt").is_file());
    assert!(out.join("sounds/quiet.ogg").is_file());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn derived_output_collision_is_rejected() {
    let root = temp_dir("collision");
    let input = root.join("in");
    std::fs::create_dir_all(&input).expect("create input");
    std::fs::write(input.join("foo.png"), b"first").expect("write first");
    std::fs::write(input.join("foo.PNG"), b"second").expect("write second");
    let out = root.join("out");
    match oxide_media::pack(&input, &out, 2) {
        Err(MediaError::Verify(message)) => {
            assert!(message.contains("derived output collision"), "{message}");
            assert!(message.contains("foo.png"), "{message}");
            assert!(message.contains("foo.PNG"), "{message}");
        }
        other => panic!("expected collision error, got {other:?}"),
    }
    assert!(!out.exists(), "collision must fail before touching --out");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn empty_and_missing_sources_fail_before_tools_or_output_changes() {
    let root = temp_dir("empty-source");
    let input = root.join("input");
    std::fs::create_dir(&input).unwrap();
    let output = root.join("output");
    std::fs::create_dir(&output).unwrap();
    std::fs::write(output.join("keep.txt"), "foreign data").unwrap();
    for verify in [false, true] {
        let result = if verify {
            oxide_media::pack_and_verify(&input, &output, 1)
        } else {
            oxide_media::pack(&input, &output, 1)
        };
        assert!(
            matches!(result, Err(MediaError::Verify(ref message)) if message.contains("empty source corpus")),
            "{result:?}"
        );
    }
    assert!(matches!(
        Plan::build(&root.join("missing")),
        Err(MediaError::Io(_))
    ));
    assert_eq!(
        std::fs::read_to_string(output.join("keep.txt")).unwrap(),
        "foreign data"
    );
    assert!(!output.join(oxide_media::DIST_MARKER).exists());
    assert!(!output.join("assets.pak").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn excluded_only_sources_are_not_a_runtime_corpus() {
    let root = temp_dir("excluded-source");
    let input = root.join("input");
    std::fs::create_dir(&input).unwrap();
    std::fs::write(input.join("only.fla"), "skipped").unwrap();
    let output = root.join("output");
    let result = oxide_media::pack(&input, &output, 1);
    assert!(
        matches!(result, Err(MediaError::Verify(ref message)) if message.contains("no runtime assets")),
        "{result:?}"
    );
    assert!(!output.exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn plan_rejects_unknown_extensions() {
    let root = temp_dir("unknown");
    std::fs::write(root.join("evil.exe"), b"not an asset").expect("write fixture");
    match Plan::build(&root) {
        Err(MediaError::Unknown { ext, .. }) => assert_eq!(ext, "exe"),
        other => panic!("expected Unknown, got {other:?}"),
    }
    std::fs::write(root.join("bundle.fla"), b"stub").expect("write fla");
    std::fs::remove_file(root.join("evil.exe")).expect("remove evil");
    let plan = Plan::build(&root).expect("fla is skipped, not unknown");
    assert_eq!(plan.entries().len(), 1);
    assert_eq!(
        plan.entries()[0].disposition,
        oxide_media::Disposition::Skip
    );
    let _ = std::fs::remove_dir_all(root);
}
