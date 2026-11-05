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

fn tools_available() -> bool {
    let ffmpeg = Command::new("ffmpeg").arg("-version").output();
    let ffprobe = Command::new("ffprobe").arg("-version").output();
    match (ffmpeg, ffprobe) {
        (Ok(first), Ok(second)) if first.status.success() && second.status.success() => true,
        _ => {
            eprintln!("skipping xtask media-check test: ffmpeg/ffprobe missing");
            false
        }
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

fn line_value<'a>(stdout: &'a str, prefix: &str) -> &'a str {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(prefix))
        .unwrap_or_else(|| panic!("missing report line {prefix:?} in:\n{stdout}"))
}

#[test]
fn mini_media_check_is_deterministic_and_well_formed() {
    if !tools_available() {
        return;
    }
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
    assert_eq!(line_value(&first_stdout, "align_ok: "), "1/1");
    assert_eq!(line_value(&first_stdout, "duration_ok: "), "0/0");
    assert_eq!(
        line_value(&first_stdout, "image_pixels_identical: "),
        "1/1   (pixels differing only where alpha==0: 1)"
    );
    assert!(first_stdout.contains("(converted 0, copied 4)"));
    assert!(first_stdout.contains("(converted 1, kept png 0)"));
    assert!(first_stdout.contains("(converted 2, excluded 1)"));
    assert!(first_stdout.contains("deflate in pak, 3 files"));
    assert!(first_stdout.contains("mp3 dropped: 0 files"));
    assert!(first_stdout.contains("threshold 0.93"));
    assert!(first_stdout.contains("total_after: "));
    assert!(first_stdout.contains(
        "excluded_videos: mobileRelease.mp4, riftCollabTrailer.mp4, boyfriendEverywhere.mp4"
    ));
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
        let text = entry.name().ends_with(".json")
            || entry.name().ends_with(".hxc")
            || entry.name().ends_with(".txt");
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

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn media_check_refuses_foreign_output_directory() {
    if !tools_available() {
        return;
    }
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
fn twice_run_cleans_its_temporary_output() {
    if !tools_available() {
        return;
    }
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
