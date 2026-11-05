//! Structural M3 tests over the committed mini fixture: classification results,
//! deterministic pak bytes, entry policy and frozen timestamps.
//!
//! ffmpeg/ffprobe are external binaries (ADR-0010); when neither is present the
//! test skips with a notice instead of silently passing.

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

fn tools_available() -> bool {
    match oxide_media::check_tools() {
        Ok(_) => true,
        Err(MediaError::Tools(message)) => {
            eprintln!("skipping media fixture test: {message}");
            false
        }
        Err(error) => panic!("unexpected tool check failure: {error}"),
    }
}

fn assert_fixture_counts(report: &PackReport) {
    assert_eq!(
        report.audio_converted, 0,
        "all mini audio is at/below target"
    );
    assert_eq!(report.audio_copied, 4);
    assert_eq!(report.images_converted, 1);
    assert_eq!(report.images_kept, 0);
    assert_eq!(report.video_converted, 2);
    assert_eq!(report.video_copied, 0);
    assert_eq!(report.video_converted_checked, report.video_converted);
    assert_eq!(report.video_excluded, 1);
    assert_eq!(report.text_files, 3);
    assert_eq!(report.font_files, 1);
    assert_eq!(report.skipped, 1);
    assert_eq!(report.mp3_dropped, 0);
    assert_eq!(report.files_grew, 0);
    assert_eq!(report.align_ok, 1);
    assert_eq!(report.align_checked, 1);
    assert_eq!(report.duration_checked, 0);
    assert_eq!(report.image_ok, 1);
    assert_eq!(report.image_checked, 1);
    assert_eq!(report.image_converted_checked, report.images_converted);
    assert_eq!(report.image_alpha_only, 1);
    assert!(report.video_ssim_min >= 0.93);
    assert_eq!(report.pak_entries, 11);
}

#[test]
fn pack_is_byte_deterministic_and_policies_hold() {
    if !tools_available() {
        return;
    }
    let root = temp_dir("determinism");
    let first_dir = root.join("first");
    let second_dir = root.join("second");

    let first =
        oxide_media::pack_and_verify(&fixture(), &first_dir, 2).expect("first pack must succeed");
    let second =
        oxide_media::pack_and_verify(&fixture(), &second_dir, 2).expect("second pack must succeed");

    assert_fixture_counts(&first);
    assert_fixture_counts(&second);
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
        let expected = if entry.name().ends_with(".json")
            || entry.name().ends_with(".hxc")
            || entry.name().ends_with(".txt")
        {
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

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn output_prep_requires_marker() {
    if !tools_available() {
        return;
    }
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
    let marker = std::fs::read_to_string(empty.join(oxide_media::DIST_MARKER)).expect("marker");
    assert_eq!(marker, "{\"oxide_assets_dist\":1}\n");
    oxide_media::pack(&fixture(), &empty, 2).expect("marked output dir is replaceable");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn derived_output_collision_is_rejected() {
    if !tools_available() {
        return;
    }
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
