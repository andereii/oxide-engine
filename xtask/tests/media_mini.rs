//! `xtask media-check` over the committed mini fixture: the frozen report block
//! must have the contract shape and two runs must produce identical pak bytes.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use zip::{CompressionMethod, HasZipMetadata};

fn repo_root() -> PathBuf {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_dir
        .parent()
        .expect("crate dir has a parent")
        .to_path_buf()
}

fn fixture() -> PathBuf {
    repo_root().join("tests/fixtures/media/mini")
}

fn temp_dir(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("xtask-media-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("create temp dir");
    path
}

fn require_tools() {
    for tool in ["ffmpeg", "ffprobe"] {
        let output = Command::new(tool)
            .arg("-version")
            .output()
            .unwrap_or_else(|error| panic!("milestone acceptance requires {tool}: {error}"));
        assert!(output.status.success(), "{tool} -version failed");
    }
}

fn run_media_check(output: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("media-check")
        .arg("--in")
        .arg(fixture())
        .arg("--out")
        .arg(output)
        .output()
        .expect("spawn xtask")
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

fn line_value<'a>(stdout: &'a str, prefix: &str) -> &'a str {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(prefix))
        .unwrap_or_else(|| panic!("missing report line {prefix:?} in:\n{stdout}"))
}

#[test]
#[ignore = "requires ffmpeg/ffprobe; executed by milestone acceptance"]
fn mini_media_check_is_deterministic_and_well_formed() {
    require_tools();
    let root = temp_dir("determinism");
    let first_dir = root.join("first");
    let second_dir = root.join("second");

    let first = run_media_check(&first_dir);
    assert!(
        first.status.success(),
        "first media-check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );
    let second = run_media_check(&second_dir);
    assert!(
        second.status.success(),
        "second media-check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );

    let first_stdout = String::from_utf8_lossy(&first.stdout);
    let second_stdout = String::from_utf8_lossy(&second.stdout);
    assert!(first_stdout.contains("media-check: "));
    assert!(first_stdout.contains("tools: "));
    assert!(first_stdout.contains("audio  before: "));
    assert_eq!(line_value(&first_stdout, "files_grew: "), "0");
    assert_eq!(line_value(&first_stdout, "align_ok: "), "2/2");
    assert_eq!(line_value(&first_stdout, "duration_ok: "), "2/2");
    assert_eq!(
        line_value(&first_stdout, "visible_pixels_identical: "),
        "43/43   (pixels differing only where alpha==0: 20)"
    );
    assert!(first_stdout.contains("(converted 2, copied 4)"));
    assert!(first_stdout.contains("(converted 1, kept png 0)"));
    assert!(first_stdout.contains("(converted 2, excluded 1)"));
    assert!(first_stdout.contains("deflate in pak, 2 files"));
    assert!(first_stdout.contains("hxc excluded: 1 files, 0.0 MB"));
    assert!(first_stdout.contains("mp3 dropped: 0 files"));
    assert!(first_stdout.contains("threshold 0.93"));
    assert!(first_stdout.contains("total_after: "));
    assert!(first_stdout.contains(
        "excluded_videos: mobileRelease.mp4, riftCollabTrailer.mp4, boyfriendEverywhere.mp4"
    ));
    assert_audio_artifacts(&first_dir);
    assert_audio_artifacts(&second_dir);
    let first_sha = line_value(&first_stdout, "pak sha256: ").trim();
    let second_sha = line_value(&second_stdout, "pak sha256: ").trim();
    assert_eq!(first_sha, second_sha, "sha256 must be stable");
    assert_eq!(first_sha.len(), 64);

    assert!(first_dir.join("assets-dist.marker").is_file());
    assert!(second_dir.join("assets-dist.marker").is_file());
    let first_pak = std::fs::read(first_dir.join("assets.pak")).expect("first pak");
    let second_pak = std::fs::read(second_dir.join("assets.pak")).expect("second pak");
    assert_eq!(first_pak, second_pak, "pak bytes must be identical");

    let file = File::open(second_dir.join("assets.pak")).expect("open pak");
    let mut archive = zip::ZipArchive::new(file).expect("read pak");
    let mut names = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).expect("entry");
        names.push(entry.name().to_string());
        let time = entry.last_modified().expect("timestamp");
        assert_eq!((time.year(), time.month(), time.day()), (2000, 1, 1));
        assert_eq!((time.hour(), time.minute(), time.second()), (0, 0, 0));
        assert_eq!(entry.get_metadata().system, zip::System::Unix);
        let text = entry.name().ends_with(".json") || entry.name().ends_with(".txt");
        assert_eq!(
            entry.compression(),
            if text {
                CompressionMethod::Deflated
            } else {
                CompressionMethod::Stored
            },
            "{}",
            entry.name()
        );
    }
    assert!(names.windows(2).all(|pair| pair[0] < pair[1]), "{names:?}");
    assert!(
        !names.iter().any(|name| name.ends_with(".hxc")),
        "{names:?}"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
#[ignore = "requires ffmpeg/ffprobe; executed by milestone acceptance"]
fn media_check_refuses_foreign_output_directory() {
    require_tools();
    let root = temp_dir("foreign");
    let foreign = root.join("foreign");
    std::fs::create_dir_all(&foreign).expect("create foreign dir");
    std::fs::write(foreign.join("keep.txt"), "user data").expect("write foreign file");
    let output = run_media_check(&foreign);
    assert!(
        !output.status.success(),
        "media-check must refuse a foreign out dir"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("refusing to wipe"), "{stderr}");
    assert!(foreign.join("keep.txt").is_file());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
#[ignore = "requires ffmpeg/ffprobe; executed by milestone acceptance"]
fn twice_run_cleans_its_temporary_output() {
    require_tools();
    let root = temp_dir("twice");
    let output = root.join("dist");
    let result = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("media-check")
        .arg("--in")
        .arg(fixture())
        .arg("--out")
        .arg(&output)
        .arg("--twice")
        .output()
        .expect("spawn xtask");
    assert!(
        result.status.success(),
        "twice run failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("twice: pak bytes identical"), "{stdout}");
    let mut repeated = output.clone().into_os_string();
    repeated.push(".twice");
    assert!(!PathBuf::from(repeated).exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
#[ignore = "requires ffmpeg/ffprobe; executed by milestone acceptance"]
fn durable_output_keeps_foreign_files_and_removes_stale_artifacts() {
    require_tools();
    let root = temp_dir("durable");
    let output = root.join("dist");

    let first = run_media_check(&output);
    assert!(first.status.success(), "first media-check failed");
    let first_stdout = String::from_utf8_lossy(&first.stdout).to_string();
    let first_pak = std::fs::read(output.join("assets.pak")).expect("first pak");
    let first_sha = line_value(&first_stdout, "pak sha256: ").trim().to_string();
    assert!(output.join("assets-dist.marker").is_file());

    let second = run_media_check(&output);
    assert!(second.status.success(), "second media-check failed");
    let second_stdout = String::from_utf8_lossy(&second.stdout).to_string();
    assert_eq!(
        first_stdout, second_stdout,
        "repacking the same input must print the same block"
    );
    let second_pak = std::fs::read(output.join("assets.pak")).expect("second pak");
    assert_eq!(first_pak, second_pak, "pak bytes must be identical");

    std::fs::write(output.join("notes.txt"), "foreign note").expect("write foreign note");
    let third = run_media_check(&output);
    assert!(third.status.success(), "third media-check failed");
    assert!(
        output.join("notes.txt").is_file(),
        "foreign note must survive"
    );
    let third_stdout = String::from_utf8_lossy(&third.stdout);
    assert_eq!(
        line_value(&third_stdout, "pak sha256: ").trim(),
        first_sha,
        "artifacts must be overwritten deterministically"
    );

    let reduced = root.join("reduced");
    std::fs::create_dir_all(reduced.join("data")).expect("create reduced input");
    std::fs::write(reduced.join("data/micro.json"), "{}").expect("write reduced input");
    let pack = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("pack")
        .arg("--in")
        .arg(&reduced)
        .arg("--out")
        .arg(&output)
        .output()
        .expect("spawn xtask pack");
    assert!(
        pack.status.success(),
        "reduced pack failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&pack.stdout),
        String::from_utf8_lossy(&pack.stderr)
    );
    assert!(
        output.join("notes.txt").is_file(),
        "foreign note must survive"
    );
    assert!(
        !output.join("sounds/quiet.ogg").exists(),
        "stale artefact from the previous plan must be deleted"
    );
    assert!(
        !output.join("images/micro.webp").exists(),
        "stale artefact from the previous plan must be deleted"
    );
    assert!(output.join("assets.pak").is_file());

    let _ = std::fs::remove_dir_all(root);
}
