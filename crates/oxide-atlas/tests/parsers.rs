//! Parser tests over the committed atlas fixtures plus inline documents for
//! paths the fixtures do not cover (Animate Atlas, not-an-atlas roots, paired
//! tags, lenient attribute values).

use std::path::PathBuf;

use oxide_atlas::{AtlasError, parse_animate, parse_packer, parse_sparrow};

fn fixture(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/atlas")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

#[test]
fn sparrow_fixture_matches_expected_geometry() {
    let atlas = parse_sparrow(&fixture("ok/sparrow-tiny.xml")).expect("sparrow fixture parses");

    assert_eq!(atlas.image_path, "tiny.png");
    assert_eq!(atlas.subtextures.len(), 5);

    let full = &atlas.subtextures[0];
    assert_eq!(full.name, "full0000");
    assert_eq!((full.x, full.y, full.width, full.height), (0, 0, 64, 64));
    assert_eq!(
        (
            full.frame_x,
            full.frame_y,
            full.frame_width,
            full.frame_height
        ),
        (0, 0, 0, 0)
    );
    assert!(!full.rotated);

    let trimmed = &atlas.subtextures[1];
    assert_eq!(trimmed.name, "trimmed0000");
    assert_eq!(
        (
            trimmed.frame_x,
            trimmed.frame_y,
            trimmed.frame_width,
            trimmed.frame_height
        ),
        (-5, 3, 30, 40)
    );

    assert!(atlas.subtextures[2].rotated);
    assert!(!atlas.subtextures[3].rotated);
    assert_eq!(
        (
            atlas.subtextures[4].frame_x,
            atlas.subtextures[4].frame_y,
            atlas.subtextures[4].frame_width,
            atlas.subtextures[4].frame_height
        ),
        (2, -4, 12, 16)
    );
}

#[test]
fn sparrow_numeric_garbage_is_malformed() {
    let err = parse_sparrow(&fixture("broken/numeric-garbage.xml")).expect_err("garbage width");
    match err {
        AtlasError::Malformed { message, .. } => {
            assert!(
                message.contains("width") && message.contains("16abc"),
                "message should name the attribute and value: {message}"
            );
        }
        other => panic!("expected Malformed, got {other:?}"),
    }
}

#[test]
fn sparrow_non_atlas_root_is_rejected_with_its_tag() {
    let err = parse_sparrow("<?xml version=\"1.0\"?>\n<dialog width=\"500\"><box/></dialog>")
        .expect_err("dialog is not an atlas");
    match err {
        AtlasError::NotAnAtlas { root_tag, .. } => assert_eq!(root_tag, "dialog"),
        other => panic!("expected NotAnAtlas, got {other:?}"),
    }
}

#[test]
fn sparrow_accepts_leading_blank_bom_paired_tags_and_raw_angle_brackets() {
    let xml = "\u{FEFF}\n<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<TextureAtlas imagePath='p.png'>\n  <SubTexture name=\"<0000\" x=\"+1\" y=\"-2\"></SubTexture>\n  <SubTexture name=\"&0000\" x=\"3\" y=\"4\" width=\"5\" height=\"6\" rotated=\"false\"/>\n</TextureAtlas>";
    let atlas = parse_sparrow(xml).expect("lenient sparrow parse");

    assert_eq!(atlas.image_path, "p.png");
    assert_eq!(atlas.subtextures.len(), 2);
    assert_eq!(atlas.subtextures[0].name, "<0000");
    assert_eq!(atlas.subtextures[0].x, 1);
    assert_eq!(atlas.subtextures[0].y, -2);
    assert_eq!(atlas.subtextures[1].name, "&0000");
    assert_eq!(
        (
            atlas.subtextures[1].x,
            atlas.subtextures[1].y,
            atlas.subtextures[1].width,
            atlas.subtextures[1].height
        ),
        (3, 4, 5, 6)
    );
    assert!(!atlas.subtextures[1].rotated);
}

#[test]
fn sparrow_missing_geometry_defaults_to_zero() {
    let atlas = parse_sparrow(
        "<TextureAtlas imagePath=\"none.png\"><SubTexture name=\"bare\"/></TextureAtlas>",
    )
    .expect("missing geometry is lenient");

    let bare = &atlas.subtextures[0];
    assert_eq!(
        (
            bare.x,
            bare.y,
            bare.width,
            bare.height,
            bare.frame_x,
            bare.frame_y,
            bare.frame_width,
            bare.frame_height
        ),
        (0, 0, 0, 0, 0, 0, 0, 0)
    );
    assert!(!bare.rotated);
}

#[test]
fn sparrow_missing_name_is_malformed() {
    let err = parse_sparrow("<TextureAtlas><SubTexture x=\"1\"/></TextureAtlas>")
        .expect_err("missing name");
    assert!(matches!(err, AtlasError::Malformed { .. }));
}

#[test]
fn packer_hash_fixture() {
    let atlas = parse_packer(&fixture("packer/hash.json")).expect("hash packer fixture");

    assert_eq!(atlas.image, "packer-tiny.png");
    assert_eq!(atlas.frames.len(), 2);

    let idle0 = &atlas.frames[0];
    assert_eq!(idle0.name, "idle0000");
    assert_eq!(idle0.frame, [2, 2, 24, 30]);
    assert!(!idle0.rotated);
    assert!(!idle0.trimmed);
    assert_eq!(idle0.sprite_source_size, [0, 0, 24, 30]);
    assert_eq!(idle0.source_size, [24, 30]);

    let idle1 = &atlas.frames[1];
    assert_eq!(idle1.name, "idle0001");
    assert_eq!(idle1.frame, [28, 2, 18, 26]);
    assert!(idle1.rotated);
    assert!(idle1.trimmed);
    assert_eq!(idle1.sprite_source_size, [3, 4, 18, 26]);
    assert_eq!(idle1.source_size, [24, 30]);
}

#[test]
fn packer_array_fixture() {
    let atlas = parse_packer(&fixture("packer/array.json")).expect("array packer fixture");

    assert_eq!(atlas.image, "packer-array.png");
    assert_eq!(atlas.frames.len(), 2);
    assert_eq!(atlas.frames[0].name, "walk0000");
    assert!(atlas.frames[0].trimmed);
    assert_eq!(atlas.frames[0].sprite_source_size, [1, 2, 20, 24]);
    assert_eq!(atlas.frames[1].name, "walk0001");
    assert!(!atlas.frames[1].trimmed);
    assert_eq!(atlas.frames[1].source_size, [22, 26]);
}

#[test]
fn packer_without_meta_image_yields_empty_string() {
    let atlas = parse_packer(
        "{\"frames\":{\"a\":{\"frame\":{\"x\":0,\"y\":0,\"w\":1,\"h\":1},\"rotated\":false,\"trimmed\":false,\"spriteSourceSize\":{\"x\":0,\"y\":0,\"w\":1,\"h\":1},\"sourceSize\":{\"w\":1,\"h\":1}}}}",
    )
    .expect("packer without meta");
    assert_eq!(atlas.image, "");
    assert_eq!(atlas.frames.len(), 1);
}

#[test]
fn packer_malformed_geometry_is_json_error() {
    let err = parse_packer(
        "{\"frames\":{\"a\":{\"frame\":{\"x\":0,\"y\":0,\"w\":\"wide\",\"h\":1},\"rotated\":false,\"trimmed\":false,\"spriteSourceSize\":{\"x\":0,\"y\":0,\"w\":1,\"h\":1},\"sourceSize\":{\"w\":1,\"h\":1}}}}",
    )
    .expect_err("non-integer width");
    assert!(matches!(err, AtlasError::Json { .. }));
}

#[test]
fn animate_parses_sprites_and_meta_image() {
    let json = "\u{FEFF}{\"ATLAS\":{\"SPRITES\":[{\"SPRITE\":{\"name\":\"0000\",\"x\":0,\"y\":0,\"w\":1401,\"h\":100,\"rotated\":false}},{\"SPRITE\":{\"name\":\"0001\",\"x\":7,\"y\":8,\"w\":9,\"h\":10}}]},\"meta\":{\"image\":\"spritemap1.png\"}}";
    let atlas = parse_animate(json).expect("animate parse");

    assert_eq!(atlas.image, "spritemap1.png");
    assert_eq!(atlas.sprites.len(), 2);
    assert_eq!(
        atlas.sprites[0],
        oxide_atlas::AnimateSprite {
            name: "0000".into(),
            x: 0,
            y: 0,
            w: 1401,
            h: 100,
            rotated: false,
        }
    );
    assert_eq!(
        atlas.sprites[1],
        oxide_atlas::AnimateSprite {
            name: "0001".into(),
            x: 7,
            y: 8,
            w: 9,
            h: 10,
            rotated: false,
        }
    );
}

#[test]
fn animate_missing_sprites_is_json_error() {
    let err = parse_animate("{\"ATLAS\":{}}").expect_err("missing SPRITES");
    assert!(matches!(err, AtlasError::Json { .. }));
}
