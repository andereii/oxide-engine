//! Portable CLI regressions: gates reject missing inputs and cannot certify a
//! reference corpus from an empty directory or a small, otherwise valid mod.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("xtask-coverage-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch directory");
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn parser(command: &str, input: &Path, corpus: bool) -> Output {
    let mut process = Command::new(env!("CARGO_BIN_EXE_xtask"));
    process.arg(command).arg("--dir").arg(input);
    if corpus {
        process.arg("--corpus");
    }
    process.output().expect("spawn xtask")
}

fn assert_failure(output: &Output, diagnostic: &str) {
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(diagnostic),
        "expected {diagnostic:?}: {output:?}"
    );
}

#[test]
fn parsers_reject_empty_missing_and_non_candidate_directories() {
    let root = Scratch::new("empty");
    for command in ["parse-all", "parse-atlas"] {
        assert_failure(&parser(command, &root.0, false), "no ");
        assert_failure(
            &parser(command, &root.0.join("missing"), false),
            "directory not found",
        );
        assert_failure(
            &parser(command, Path::new(""), false),
            "--dir requires a value",
        );
        let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
            .args([command, "--dir"])
            .output()
            .expect("spawn xtask");
        assert_failure(&output, "--dir requires a value");
    }
    std::fs::write(root.0.join("readme.txt"), "not parser input").unwrap();
    for command in ["parse-all", "parse-atlas"] {
        assert_failure(&parser(command, &root.0, false), "no ");
    }
}

#[test]
fn corpus_inventory_is_opt_in_not_a_mod_requirement() {
    let root = Scratch::new("mod");
    std::fs::write(root.0.join("mod.json"), "{}").unwrap();
    std::fs::write(
        root.0.join("atlas.xml"),
        "<TextureAtlas imagePath=\"mod.png\"><SubTexture name=\"a\" x=\"0\" y=\"0\" width=\"1\" height=\"1\"/></TextureAtlas>",
    )
    .unwrap();
    for command in ["parse-all", "parse-atlas"] {
        let output = parser(command, &root.0, false);
        assert!(output.status.success(), "{output:?}");
        assert_failure(&parser(command, &root.0, true), "corpus inventory");
    }
}

#[test]
fn invalid_packer_frames_affect_exit_status() {
    let root = Scratch::new("packer");
    std::fs::write(root.0.join("invalid.json"), r#"{"frames":42}"#).unwrap();
    let output = parser("parse-atlas", &root.0, false);
    assert_failure(&output, "packer failed:");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("packer ok 0 failed 1"), "{stdout}");
    assert!(stdout.contains("atlas failed: 0"), "{stdout}");
    assert!(stdout.contains("animate ok 0 failed 0"), "{stdout}");
}

#[test]
fn malformed_and_non_utf8_parser_candidates_fail() {
    for (name, bytes, commands) in [
        (
            "bad.json",
            b"{".as_slice(),
            vec!["parse-all", "parse-atlas"],
        ),
        (
            "bad.json",
            b"\xff".as_slice(),
            vec!["parse-all", "parse-atlas"],
        ),
        ("bad.xml", b"not XML".as_slice(), vec!["parse-atlas"]),
        ("spritemap1.json", b"{".as_slice(), vec!["parse-atlas"]),
    ] {
        let root = Scratch::new("malformed");
        // Valid neighbours prevent a silently skipped failure from being
        // rejected only by the empty-input guard.
        std::fs::write(root.0.join("good.json"), "{}").unwrap();
        std::fs::write(
            root.0.join("good.xml"),
            "<TextureAtlas imagePath=\"mod.png\"/>",
        )
        .unwrap();
        std::fs::write(root.0.join(name), bytes).unwrap();
        for command in commands {
            let output = parser(command, &root.0, false);
            assert_eq!(
                output.status.code(),
                Some(1),
                "{command} accepted {name}: {output:?}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn traversal_metadata_errors_are_not_silently_skipped() {
    use std::os::unix::fs::symlink;

    let root = Scratch::new("traversal");
    std::fs::write(root.0.join("good.json"), "{}").unwrap();
    std::fs::write(
        root.0.join("good.xml"),
        "<TextureAtlas imagePath=\"mod.png\"/>",
    )
    .unwrap();
    symlink(
        root.0.join("missing-directory"),
        root.0.join("unreadable-subtree"),
    )
    .unwrap();
    for command in ["parse-all", "parse-atlas"] {
        assert_failure(&parser(command, &root.0, false), "cannot inspect");
    }
}

#[cfg(target_os = "linux")]
#[test]
fn unreadable_json_candidate_is_not_ignored_by_atlas_gate() {
    use std::os::unix::fs::symlink;

    let root = Scratch::new("unreadable");
    // Reading offset zero of our own proc memory fails even as root, without
    // requiring socket permissions in a network-restricted test environment.
    symlink("/proc/self/mem", root.0.join("candidate.json")).unwrap();
    let output = parser("parse-atlas", &root.0, false);
    assert_failure(&output, "packer candidate failed:");
    assert!(String::from_utf8_lossy(&output.stdout).contains("packer ok 0 failed 1"));
}

#[test]
fn media_rejects_empty_and_missing_sources_before_tools_or_output() {
    let root = Scratch::new("media-empty");
    let empty = root.0.join("empty");
    std::fs::create_dir(&empty).unwrap();
    for command in ["media-check", "pack"] {
        for input in [&empty, &root.0.join("missing")] {
            let output = root.0.join("untouched-output");
            let result = Command::new(env!("CARGO_BIN_EXE_xtask"))
                .arg(command)
                .arg("--in")
                .arg(input)
                .arg("--out")
                .arg(&output)
                .env("PATH", &empty)
                .output()
                .expect("spawn xtask");
            assert_failure(
                &result,
                if input == &empty {
                    "empty source corpus"
                } else {
                    "directory not found"
                },
            );
            assert!(!output.exists(), "invalid source must not touch output");
        }
    }
}

#[test]
fn media_corpus_inventory_fails_before_conversion_or_output() {
    let root = Scratch::new("media-corpus");
    let input = root.0.join("input");
    std::fs::create_dir(&input).unwrap();
    std::fs::write(input.join("small.json"), "{}").unwrap();
    for create_required in [false, true] {
        if create_required {
            for directory in [
                "songs",
                "preload/images",
                "preload/data",
                "videos/videos",
                "fonts",
            ] {
                std::fs::create_dir_all(input.join(directory)).unwrap();
            }
        }
        let output = root.0.join("untouched-output");
        let result = Command::new(env!("CARGO_BIN_EXE_xtask"))
            .arg("media-check")
            .arg("--in")
            .arg(&input)
            .arg("--out")
            .arg(&output)
            .arg("--corpus")
            .env("PATH", &input)
            .output()
            .expect("spawn xtask");
        assert_failure(&result, "corpus inventory");
        assert_failure(
            &result,
            if create_required {
                "expected 62 Inst, found 0"
            } else {
                "missing mandatory directory"
            },
        );
        assert!(!output.exists(), "incomplete corpus must not create output");
        assert!(!String::from_utf8_lossy(&result.stdout).contains("tools:"));
    }
}

#[test]
fn gameplay_sim_fails_closed_without_explicit_assets() {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("gameplay-sim")
        .env_remove("OXIDE_ASSETS_ROOT")
        .output()
        .expect("spawn xtask");
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("missing --assets <dir> or OXIDE_ASSETS_ROOT"),
        "{stderr}"
    );

    let missing = std::env::temp_dir().join(format!(
        "xtask-gameplay-sim-missing-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["gameplay-sim", "--assets"])
        .arg(&missing)
        .env_remove("OXIDE_ASSETS_ROOT")
        .output()
        .expect("spawn xtask");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("prerequisite: assets directory is missing or not a directory"),
        "{stderr}"
    );
}

fn write_empty_asset(root: &Path, relative: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, []).unwrap();
}

fn media_without_tools(
    command: &str,
    input: &Path,
    output: &Path,
    tools: &Path,
    corpus: bool,
) -> Output {
    let mut process = Command::new(env!("CARGO_BIN_EXE_xtask"));
    process
        .arg(command)
        .arg("--in")
        .arg(input)
        .arg("--out")
        .arg(output)
        .env("PATH", tools);
    if corpus {
        process.arg("--corpus");
    }
    process.output().expect("spawn xtask")
}

#[test]
fn media_corpus_pairings_are_checked_before_tools_or_output() {
    let root = Scratch::new("media-pairings");
    let source = root.0.join("source");
    // Payloads are intentionally empty: only the complete planned inventory
    // is exercised, and conversion must never run in this regression.
    for index in 0..62 {
        write_empty_asset(&source, &format!("songs/song-{index}/Inst.ogg"));
    }
    for index in 0..126 {
        write_empty_asset(
            &source,
            &format!("songs/song-{}/Voices-{index}.ogg", index % 62),
        );
    }
    for (directory, extension, count) in [
        ("sounds", "ogg", 243),
        ("preload/images", "png", 686),
        ("videos/videos", "mp4", 11),
        ("preload/data", "json", 747),
        ("fonts", "ttf", 25),
        ("script", "hxc", 114),
        ("legacy", "mp3", 413),
    ] {
        for index in 0..count {
            write_empty_asset(&source, &format!("{directory}/stub-{index}.{extension}"));
        }
    }
    for stem in oxide_media::EXCLUDED_VIDEOS {
        write_empty_asset(&source, &format!("videos/videos/{stem}.mp4"));
    }

    let relocated = root.0.join("relocated-reference");
    std::fs::rename(&source, &relocated).unwrap();
    let tools = root.0.join("no-tools");
    std::fs::create_dir(&tools).unwrap();
    let output = root.0.join("untouched-output");
    let valid = media_without_tools("media-check", &relocated, &output, &tools, true);
    assert_failure(&valid, "ffmpeg/ffprobe");
    assert!(!String::from_utf8_lossy(&valid.stderr).contains("corpus inventory"));
    assert!(
        !output.exists(),
        "tool check must precede output preparation"
    );

    std::fs::create_dir(relocated.join("songs/orphan")).unwrap();
    std::fs::rename(
        relocated.join("songs/song-0/Voices-0.ogg"),
        relocated.join("songs/orphan/Voices-0.ogg"),
    )
    .unwrap();
    let invalid = media_without_tools("media-check", &relocated, &output, &tools, true);
    assert_failure(
        &invalid,
        "corpus inventory: unpaired Voices stem: songs/orphan/Voices-0.ogg",
    );
    assert!(
        invalid.stdout.is_empty(),
        "preflight must fail before the tools/report block"
    );
    assert!(!output.exists(), "unpaired corpus must not prepare output");
}

#[test]
fn override_only_mods_do_not_require_corpus_stem_pairs() {
    let root = Scratch::new("media-voice-override");
    let input = root.0.join("override");
    write_empty_asset(&input, "songs/override/Voices-bf.ogg");
    let tools = root.0.join("no-tools");
    std::fs::create_dir(&tools).unwrap();
    let output = root.0.join("untouched-output");
    for command in ["media-check", "pack"] {
        let result = media_without_tools(command, &input, &output, &tools, false);
        assert_failure(&result, "ffmpeg/ffprobe");
        assert!(!String::from_utf8_lossy(&result.stderr).contains("unpaired"));
        assert!(!output.exists());
    }
}
