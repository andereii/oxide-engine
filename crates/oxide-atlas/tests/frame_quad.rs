//! `frame_quad` trimming geometry: hand-computed fixture/inline cases plus a
//! real-assets invariant sweep. The crate itself performs no I/O; this test
//! reads committed fixtures; explicitly included real-asset tests require
//! OXIDE_ASSETS_ROOT to point to the reference assets directory.

use std::path::{Path, PathBuf};

use oxide_atlas::{AtlasError, FrameQuad, frame_quad, parse_sparrow};

fn fixture(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/atlas")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

#[test]
fn frame_quad_plain_fixture_subtexture() {
    // Hand-computed: full0000 is x=0 y=0 width=64 height=64 with no frame*
    // attributes, so offset and dest size are the plain page geometry.
    let atlas = parse_sparrow(&fixture("ok/sparrow-tiny.xml")).expect("sparrow fixture parses");
    let quad = frame_quad(&atlas.subtextures[0]);

    assert_eq!(
        quad,
        FrameQuad {
            atlas_rect: [0, 0, 64, 64],
            dest_offset: [0, 0],
            dest_size: [64, 64],
        }
    );
}

#[test]
fn frame_quad_negative_frame_offsets() {
    // Hand-computed from the human spec analogue (inline so the numbers stay
    // independent of the fixture): offset = (-frameX, -frameY) = (10, 5);
    // dest size prefers the frame* attrs (120, 210); not rotated, so the page
    // rect is stored as parsed.
    let atlas = parse_sparrow(
        "<TextureAtlas imagePath=\"q.png\"><SubTexture name=\"spec\" x=\"3\" y=\"4\" width=\"100\" height=\"200\" frameX=\"-10\" frameY=\"-5\" frameWidth=\"120\" frameHeight=\"210\" /></TextureAtlas>",
    )
    .expect("inline spec atlas parses");
    let quad = frame_quad(&atlas.subtextures[0]);

    assert_eq!(
        quad,
        FrameQuad {
            atlas_rect: [3, 4, 100, 200],
            dest_offset: [10, 5],
            dest_size: [120, 210],
        }
    );
}

#[test]
fn frame_quad_rotated_and_trimmed_swaps_atlas_rect_not_dest_size() {
    // Hand-computed from the real-data shape: rotated frames store the page
    // rect as (x, y, height, width) = (342, 8, 303, 69) but the destination
    // size stays the unrotated trimmed footprint (318, 78).
    let atlas = parse_sparrow(
        "<TextureAtlas imagePath=\"r.png\"><SubTexture name=\"rot\" x=\"342\" y=\"8\" width=\"69\" height=\"303\" frameX=\"-12\" frameY=\"-5\" frameWidth=\"318\" frameHeight=\"78\" rotated=\"true\" /></TextureAtlas>",
    )
    .expect("inline rotated atlas parses");
    let quad = frame_quad(&atlas.subtextures[0]);

    assert_eq!(
        quad,
        FrameQuad {
            atlas_rect: [342, 8, 303, 69],
            dest_offset: [12, 5],
            dest_size: [318, 78],
        }
    );
}

#[test]
fn frame_quad_rotated_without_frame_attrs_swaps_dest_size() {
    // Hand-computed corner case (0 occurrences in shipped data): rotated and
    // no frame* attrs at all, so flixel's untrimmed branch uses the swapped
    // page dims (height, width) = (8, 16). The fixture's rotated0000 is
    // x=40 y=0 width=16 height=8 rotated="true".
    let atlas = parse_sparrow(&fixture("ok/sparrow-tiny.xml")).expect("sparrow fixture parses");
    let quad = frame_quad(&atlas.subtextures[2]);

    assert_eq!(
        quad,
        FrameQuad {
            atlas_rect: [40, 0, 8, 16],
            dest_offset: [0, 0],
            dest_size: [8, 16],
        }
    );
}

#[test]
#[ignore = "requires OXIDE_ASSETS_ROOT; run explicitly for milestone acceptance"]
fn real_asset_frame_quads_hold_the_rotated_invariant() {
    let root = real_assets_root();

    let mut files = Vec::new();
    collect_xml(&root, &mut files);
    files.sort();

    let mut total = 0usize;
    let mut rotated = 0usize;
    for path in &files {
        let xml = std::fs::read_to_string(path)
            .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
        let atlas = match parse_sparrow(&xml) {
            Ok(atlas) => atlas,
            Err(AtlasError::NotAnAtlas { .. }) => continue,
            Err(err) => panic!("parse {}: {err}", path.display()),
        };
        for subtexture in &atlas.subtextures {
            let quad = frame_quad(subtexture);
            total += 1;
            if subtexture.rotated {
                rotated += 1;
                // Swapped page rect: height becomes atlas_rect[3], which must
                // equal the parsed stored width.
                assert_eq!(
                    quad.atlas_rect[3],
                    subtexture.width,
                    "rotated subtexture {} in {} lost the swap invariant",
                    subtexture.name,
                    path.display()
                );
            }
        }
    }

    assert_eq!(total, 8486, "audited subtexture total changed");
    assert_eq!(rotated, 198, "audited rotated subtexture total changed");
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

fn collect_xml(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("read directory {}: {error}", dir.display()));
    for entry in entries {
        let entry =
            entry.unwrap_or_else(|error| panic!("read entry in {}: {error}", dir.display()));
        let path = entry.path();
        let metadata = std::fs::metadata(&path)
            .unwrap_or_else(|error| panic!("inspect {}: {error}", path.display()));
        if metadata.is_dir() {
            collect_xml(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("xml") {
            out.push(path);
        }
    }
}
