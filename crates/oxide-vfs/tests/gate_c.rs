//! Gate C integration tests for the layered mod filesystem.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use oxide_vfs::{Manifest, ResKind, Source, Vfs, VfsError};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

static NEXT_TEMP: AtomicU32 = AtomicU32::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "oxide-vfs-gate-c-{}-{}-{label}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, rel: &str, bytes: &[u8]) {
        let path = self.path.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dir");
        }
        std::fs::write(path, bytes).expect("write temp file");
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/vfs")
        .join(rel)
}

fn valid_manifest_bytes() -> Vec<u8> {
    std::fs::read(fixture("valid-manifest/_polymod_meta.json")).expect("read valid fixture")
}

fn make_mod_dir(path: &Path) {
    std::fs::create_dir_all(path).expect("create mod dir");
    std::fs::write(path.join("_polymod_meta.json"), valid_manifest_bytes())
        .expect("write mod manifest");
}

fn read_text(vfs: &Vfs, source: &Source) -> String {
    String::from_utf8(vfs.read(source, 4096).expect("read source")).expect("utf-8 source")
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).expect("canonicalize test path")
}

fn canonical_child(root: &Path, rel: &str) -> PathBuf {
    canonical(root).join(rel)
}

fn build_zip(path: &Path, entries: &[(&str, &[u8])], compression: CompressionMethod) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create zip parent");
    }
    let file = std::fs::File::create(path).expect("create zip");
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(compression);
    for (name, bytes) in entries {
        writer.start_file(*name, options).expect("start zip entry");
        writer.write_all(bytes).expect("write zip entry");
    }
    writer.finish().expect("finish zip");
}

#[test]
fn a_last_added_mod_wins_then_earlier_mod_then_base() {
    let temp = TempDir::new("precedence");
    let base = temp.path().join("base");
    let mod1 = temp.path().join("mod-one");
    let mod2 = temp.path().join("mod-two");
    make_mod_dir(&mod1);
    make_mod_dir(&mod2);

    temp.write("base/images/shared.png", b"base-shared");
    temp.write("mod-one/images/shared.png", b"mod1-shared");
    temp.write("mod-two/images/shared.png", b"mod2-shared");

    temp.write("base/images/base-only.png", b"base-only");

    temp.write("base/images/earlier.png", b"base-earlier");
    temp.write("mod-one/images/earlier.png", b"mod1-earlier");

    temp.write("mod-two/images/later.png", b"mod2-later");

    let mut vfs = Vfs::new(base);
    vfs.add_mod_dir(mod1.clone(), "one".to_owned())
        .expect("add first mod");
    vfs.add_mod_dir(mod2.clone(), "two".to_owned())
        .expect("add second mod");
    assert_eq!(vfs.mod_ids(), ["one", "two"]);

    let shared = vfs.resolve("images/shared.png").expect("shared resolves");
    match &shared {
        Source::Dir(path) => assert_eq!(path, &canonical_child(&mod2, "images/shared.png")),
        Source::Zip { .. } => panic!("expected a directory source"),
    }
    assert_eq!(read_text(&vfs, &shared), "mod2-shared");

    let earlier = vfs.resolve("images/earlier.png").expect("earlier resolves");
    match &earlier {
        Source::Dir(path) => assert_eq!(path, &canonical_child(&mod1, "images/earlier.png")),
        Source::Zip { .. } => panic!("expected a directory source"),
    }
    assert_eq!(read_text(&vfs, &earlier), "mod1-earlier");

    let base_only = vfs.resolve("images/base-only.png").expect("base resolves");
    match &base_only {
        Source::Dir(path) => {
            assert_eq!(
                path,
                &canonical_child(&temp.path().join("base"), "images/base-only.png")
            );
        }
        Source::Zip { .. } => panic!("expected a directory source"),
    }
    assert_eq!(read_text(&vfs, &base_only), "base-only");

    let later = vfs
        .resolve("images/later.png")
        .expect("last mod only resolves");
    match &later {
        Source::Dir(path) => assert_eq!(path, &canonical_child(&mod2, "images/later.png")),
        Source::Zip { .. } => panic!("expected a directory source"),
    }
    assert_eq!(read_text(&vfs, &later), "mod2-later");
}

#[test]
fn b_zip_mod_resolves_reads_and_rejects_unsafe_entries() {
    let temp = TempDir::new("zip");
    let base = temp.path().join("base");
    temp.write("base/images/zip.png", b"base-image");

    let manifest = valid_manifest_bytes();
    let zip_path = temp.path().join("zip-mod.zip");
    build_zip(
        &zip_path,
        &[
            ("_polymod_meta.json", &manifest),
            ("images/zip.png", b"zip-image"),
            ("data/inside.json", b"[1,2,3]"),
        ],
        CompressionMethod::Deflated,
    );

    let mut vfs = Vfs::new(base);
    vfs.add_mod_zip(zip_path.clone(), "zip".to_owned())
        .expect("add zip mod");

    let canonical_zip = canonical(&zip_path);
    let image = vfs
        .resolve_kind(ResKind::Image, "zip")
        .expect("zip image resolves");
    match &image {
        Source::Zip { path, entry } => {
            assert_eq!(path, &canonical_zip);
            assert_eq!(entry, "images/zip.png");
        }
        Source::Dir(_) => panic!("expected a zip source"),
    }
    assert_eq!(read_text(&vfs, &image), "zip-image");

    let data = vfs.resolve("data/inside.json").expect("zip data resolves");
    match &data {
        Source::Zip { entry, .. } => assert_eq!(entry, "data/inside.json"),
        Source::Dir(_) => panic!("expected a zip source"),
    }
    assert_eq!(read_text(&vfs, &data), "[1,2,3]");

    for (label, evil_entry) in [("parent", "../evil.txt"), ("absolute", "/absolute.txt")] {
        let evil_zip = temp.path().join(format!("evil-{label}.zip"));
        build_zip(
            &evil_zip,
            &[("_polymod_meta.json", &manifest), (evil_entry, b"nope")],
            CompressionMethod::Stored,
        );
        let mut evil_vfs = Vfs::new(temp.path().join("unused-base"));
        let err = evil_vfs
            .add_mod_zip(evil_zip, format!("evil-{label}"))
            .expect_err("unsafe entry must be rejected");
        match err {
            VfsError::UnsafeZipEntry { entry, .. } => assert_eq!(entry, evil_entry),
            other => panic!("expected UnsafeZipEntry for {evil_entry}, got {other:?}"),
        }
        assert!(evil_vfs.mod_ids().is_empty());
    }

    let good_zip = std::fs::read(&zip_path).expect("read built zip");
    let corrupt = temp.path().join("corrupt.zip");
    std::fs::write(&corrupt, &good_zip[..good_zip.len() / 2]).expect("write corrupt zip");
    let mut corrupt_vfs = Vfs::new(temp.path().join("unused-base"));
    let err = corrupt_vfs
        .add_mod_zip(corrupt, "corrupt".to_owned())
        .expect_err("corrupt zip must fail");
    assert!(matches!(err, VfsError::Zip(_)), "got {err:?}");
    assert!(corrupt_vfs.mod_ids().is_empty());

    let mut huge_manifest = valid_manifest_bytes();
    huge_manifest.resize(1024 * 1024 + 1, b' ');
    let huge_zip = temp.path().join("huge-manifest.zip");
    build_zip(
        &huge_zip,
        &[("_polymod_meta.json", &huge_manifest)],
        CompressionMethod::Stored,
    );
    let mut huge_vfs = Vfs::new(temp.path().join("unused-base"));
    let err = huge_vfs
        .add_mod_zip(huge_zip, "huge".to_owned())
        .expect_err("oversized zip manifest must fail");
    match err {
        VfsError::Manifest { detail, .. } => {
            assert!(detail.contains("exceeds"), "unexpected detail: {detail}");
        }
        other => panic!("expected Manifest error, got {other:?}"),
    }
    assert!(huge_vfs.mod_ids().is_empty());
}

#[test]
fn c_lookup_is_case_exact() {
    let lower = TempDir::new("case-lower");
    lower.write("base/images/bf.png", b"lower");
    let lower_vfs = Vfs::new(lower.path().join("base"));

    let source = lower_vfs
        .resolve_kind(ResKind::Image, "bf")
        .expect("lowercase key resolves");
    assert_eq!(read_text(&lower_vfs, &source), "lower");
    assert!(lower_vfs.resolve_kind(ResKind::Image, "Bf").is_none());
    assert!(lower_vfs.resolve_kind(ResKind::Image, "BF").is_none());
    assert!(lower_vfs.resolve("images/Bf.png").is_none());

    let mixed = TempDir::new("case-mixed");
    mixed.write("base/images/Bf.PNG", b"mixed");
    let mixed_vfs = Vfs::new(mixed.path().join("base"));

    assert!(mixed_vfs.resolve_kind(ResKind::Image, "bf").is_none());
    assert!(mixed_vfs.resolve_kind(ResKind::Image, "Bf").is_none());
    assert!(mixed_vfs.resolve("images/bf.PNG").is_none());
    let exact = mixed_vfs
        .resolve("images/Bf.PNG")
        .expect("exact mixed-case path resolves");
    assert_eq!(read_text(&mixed_vfs, &exact), "mixed");
}

#[test]
fn d_duplicate_mod_ids_are_rejected_without_corrupting_ids() {
    let temp = TempDir::new("duplicate");
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    let missing_manifest = temp.path().join("missing-manifest");
    let third = temp.path().join("third");
    make_mod_dir(&first);
    make_mod_dir(&second);
    make_mod_dir(&third);
    std::fs::create_dir_all(&missing_manifest).expect("create empty mod dir");

    let mut vfs = Vfs::new(temp.path().join("base"));
    vfs.add_mod_dir(first, "same".to_owned())
        .expect("add first mod");
    let err = vfs
        .add_mod_dir(second, "same".to_owned())
        .expect_err("duplicate id must fail");
    assert!(matches!(err, VfsError::DuplicateMod { id } if id == "same"));
    assert_eq!(vfs.mod_ids(), ["same"]);

    let err = vfs
        .add_mod_dir(missing_manifest, "missing".to_owned())
        .expect_err("missing manifest must fail");
    assert!(matches!(err, VfsError::MissingManifest { .. }));
    assert_eq!(vfs.mod_ids(), ["same"]);

    vfs.add_mod_dir(third, "third".to_owned())
        .expect("add third mod");
    assert_eq!(vfs.mod_ids(), ["same", "third"]);
}

#[test]
fn e_invalidate_refreshes_changed_deleted_and_new_paths() {
    let temp = TempDir::new("invalidate");
    temp.write("base/data/x/x-chart.json", b"A");
    temp.write("base/data/songs/x/x-chart.json", b"chart-A");
    let chart = temp.path().join("base/data/x/x-chart.json");
    let chart_new = temp.path().join("base/data/x/x-chart-erect.json");
    let song_chart_new = temp.path().join("base/data/songs/x/x-chart-erect.json");
    let vfs = Vfs::new(temp.path().join("base"));

    // Build the name indexes before the new files exist.
    let source = vfs
        .resolve_kind(ResKind::Json, "x/x-chart")
        .expect("json chart resolves");
    assert_eq!(read_text(&vfs, &source), "A");
    let source = vfs
        .resolve_kind(ResKind::Chart, "x/x-chart")
        .expect("song chart resolves");
    assert_eq!(read_text(&vfs, &source), "chart-A");

    std::fs::write(&chart, b"B").expect("overwrite chart");
    vfs.invalidate(&chart);
    let source = vfs
        .resolve_kind(ResKind::Json, "x/x-chart")
        .expect("chart still resolves");
    assert_eq!(read_text(&vfs, &source), "B");

    std::fs::remove_file(&chart).expect("delete chart");
    vfs.invalidate(&chart);
    assert!(vfs.resolve_kind(ResKind::Json, "x/x-chart").is_none());

    std::fs::write(&chart, b"C").expect("recreate chart");
    vfs.invalidate(&chart);
    let source = vfs
        .resolve_kind(ResKind::Json, "x/x-chart")
        .expect("recreated chart resolves");
    assert_eq!(read_text(&vfs, &source), "C");

    // New files created after the cache is warm must stay invisible until
    // exactly their path is invalidated.
    temp.write("base/data/x/x-chart-erect.json", b"erect-A");
    assert!(vfs.resolve_kind(ResKind::Json, "x/x-chart-erect").is_none());
    vfs.invalidate(&chart_new);
    let source = vfs
        .resolve_kind(ResKind::Json, "x/x-chart-erect")
        .expect("new json chart resolves after invalidation");
    assert_eq!(read_text(&vfs, &source), "erect-A");

    temp.write("base/data/songs/x/x-chart-erect.json", b"chart-erect-A");
    assert!(
        vfs.resolve_kind(ResKind::Chart, "x/x-chart-erect")
            .is_none()
    );
    vfs.invalidate(&song_chart_new);
    let source = vfs
        .resolve_kind(ResKind::Chart, "x/x-chart-erect")
        .expect("new song chart resolves after invalidation");
    assert_eq!(read_text(&vfs, &source), "chart-erect-A");

    // The original song chart is still readable through the refreshed index.
    let source = vfs
        .resolve_kind(ResKind::Chart, "x/x-chart")
        .expect("original song chart still resolves");
    assert_eq!(read_text(&vfs, &source), "chart-A");

    // A new directory under a cached parent must become visible when that
    // directory itself is invalidated, even though its parent listing is warm.
    temp.write("base/data/songs/new-song/new-song-chart.json", b"new-song");
    let new_song_dir = temp.path().join("base/data/songs/new-song");
    assert!(
        vfs.resolve_kind(ResKind::Chart, "new-song/new-song-chart")
            .is_none()
    );
    vfs.invalidate(&new_song_dir);
    let source = vfs
        .resolve_kind(ResKind::Chart, "new-song/new-song-chart")
        .expect("new song directory resolves after invalidation");
    assert_eq!(read_text(&vfs, &source), "new-song");
}

#[cfg(unix)]
#[test]
fn h_invalidate_grounds_paths_through_a_symlinked_root() {
    let temp = TempDir::new("symlink-invalidate");
    let real_base = temp.path().join("real-base");
    std::fs::create_dir_all(real_base.join("data/x")).expect("create real base");
    std::fs::write(real_base.join("data/x/x-chart.json"), b"A").expect("write chart");

    let link_base = temp.path().join("link-base");
    if std::os::unix::fs::symlink(&real_base, &link_base).is_err() {
        return;
    }

    let vfs = Vfs::new(link_base.clone());
    let source = vfs
        .resolve_kind(ResKind::Json, "x/x-chart")
        .expect("chart resolves through symlinked root");
    assert_eq!(read_text(&vfs, &source), "A");

    let new_file = link_base.join("data/x/x-chart-erect.json");
    std::fs::write(&new_file, b"erect").expect("write new chart");
    assert!(vfs.resolve_kind(ResKind::Json, "x/x-chart-erect").is_none());

    vfs.invalidate(&new_file);
    let source = vfs
        .resolve_kind(ResKind::Json, "x/x-chart-erect")
        .expect("new chart resolves after symlinked invalidation");
    assert_eq!(read_text(&vfs, &source), "erect");
}

#[test]
fn f_manifest_fixtures_are_validated_and_parsed() {
    let temp = TempDir::new("manifest");

    let broken_root = temp.path().join("broken");
    std::fs::create_dir_all(&broken_root).expect("create broken mod dir");
    let broken_manifest = broken_root.join("_polymod_meta.json");
    std::fs::copy(
        fixture("malformed-manifest/_polymod_meta.json"),
        &broken_manifest,
    )
    .expect("copy malformed fixture");
    let mut vfs = Vfs::new(temp.path().join("base"));
    let err = vfs
        .add_mod_dir(broken_root, "broken".to_owned())
        .expect_err("malformed manifest must fail");
    let message = err.to_string();
    match err {
        VfsError::Manifest { path, .. } => {
            assert_eq!(path, canonical(&broken_manifest));
            assert!(
                message.contains(&path.display().to_string()),
                "message must name the manifest path: {message}"
            );
        }
        other => panic!("expected Manifest error, got {other:?}"),
    }
    assert!(vfs.mod_ids().is_empty());

    let valid_bytes = valid_manifest_bytes();
    let manifest: Manifest =
        serde_json::from_slice(&valid_bytes).expect("valid fixture parses as Manifest");
    assert_eq!(manifest.title, "Test Mod Alpha");
    assert_eq!(
        manifest.description,
        "Valid manifest fixture for oxide-vfs tests."
    );
    assert_eq!(manifest.contributors.len(), 1);
    assert_eq!(manifest.contributors[0].name, "Oxide Test");
    assert_eq!(manifest.api_version, "0.8.0");
    assert_eq!(manifest.mod_version, "1.0.0");
    assert_eq!(manifest.license, "CC BY 4.0");

    let valid_root = temp.path().join("valid");
    make_mod_dir(&valid_root);
    vfs.add_mod_dir(valid_root, "valid".to_owned())
        .expect("valid manifest is accepted");
    assert_eq!(vfs.mod_ids(), ["valid"]);

    let upper_root = temp.path().join("upper-manifest");
    std::fs::create_dir_all(&upper_root).expect("create uppercase mod dir");
    std::fs::write(upper_root.join("_POLYMOD_META.JSON"), &valid_bytes)
        .expect("write uppercase manifest");
    let err = vfs
        .add_mod_dir(upper_root, "upper".to_owned())
        .expect_err("differently-cased manifest must not be found");
    assert!(
        matches!(err, VfsError::MissingManifest { .. }),
        "got {err:?}"
    );
    assert_eq!(vfs.mod_ids(), ["valid"]);

    let mut huge_manifest = valid_bytes;
    huge_manifest.resize(1024 * 1024 + 1, b' ');
    let huge_root = temp.path().join("huge-manifest");
    std::fs::create_dir_all(&huge_root).expect("create huge mod dir");
    std::fs::write(huge_root.join("_polymod_meta.json"), &huge_manifest)
        .expect("write huge manifest");
    let err = vfs
        .add_mod_dir(huge_root, "huge".to_owned())
        .expect_err("oversized manifest must fail");
    match err {
        VfsError::Manifest { detail, .. } => {
            assert!(detail.contains("exceeds"), "unexpected detail: {detail}");
        }
        other => panic!("expected Manifest error, got {other:?}"),
    }
    assert_eq!(vfs.mod_ids(), ["valid"]);
}

#[test]
fn g_kind_fallbacks_lowercasing_and_read_limit() {
    let temp = TempDir::new("kinds");
    temp.write("base/sounds/click.ogg", b"ogg");
    temp.write("base/sounds/click.mp3", b"mp3");
    temp.write("base/sounds/only.mp3", b"only-mp3");
    temp.write("base/songs/bopeebo/Inst.ogg", b"inst-ogg");
    temp.write("base/songs/bopeebo/Inst.mp3", b"inst-mp3");
    temp.write("base/shaders/gray.frag", b"frag");
    temp.write("base/data/ui/foo.json", b"json");
    let vfs = Vfs::new(temp.path().join("base"));

    let sound = vfs
        .resolve_kind(ResKind::Sound, "click")
        .expect("ogg wins over mp3");
    assert_eq!(read_text(&vfs, &sound), "ogg");

    let fallback = vfs
        .resolve_kind(ResKind::Sound, "only")
        .expect("mp3 fallback resolves");
    assert_eq!(read_text(&vfs, &fallback), "only-mp3");

    let inst = vfs
        .resolve_kind(ResKind::Inst, "Bopeebo")
        .expect("inst key is lowercased");
    assert_eq!(read_text(&vfs, &inst), "inst-ogg");

    let frag = vfs
        .resolve_kind(ResKind::Frag, "gray")
        .expect("frag resolves");
    assert_eq!(read_text(&vfs, &frag), "frag");

    let json = vfs
        .resolve_kind(ResKind::Json, "ui/foo")
        .expect("nested json resolves");
    assert_eq!(read_text(&vfs, &json), "json");

    let err = vfs.read(&sound, 2).expect_err("oversized read must fail");
    match err {
        VfsError::Io(err) => assert!(
            err.to_string().contains("resource too large"),
            "unexpected message: {err}"
        ),
        other => panic!("expected Io, got {other:?}"),
    }
    assert_eq!(vfs.read(&sound, 3).expect("exact limit fits"), b"ogg");
}
