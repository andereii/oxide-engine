//! Gate B: committed font fixtures plus guarded checks against the real
//! shipped Sparrow fonts.
//!
//! The real-asset checks skip (with a notice) when the assets directory is not
//! present, so a checkout without the Haxe game still stays green.

use std::path::{Path, PathBuf};

use oxide_atlas::parse_sparrow;
use oxide_font::{AngelFont, Case, FontError, SparrowFont};

fn fixture(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/fonts")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

fn micro_font() -> SparrowFont {
    let xml = fixture("sparrow/micro-font.xml");
    let atlas = parse_sparrow(&xml).expect("micro-font.xml is a Sparrow atlas");
    SparrowFont::from_atlas(atlas).expect("micro-font.xml is a valid font")
}

fn angel_font(relative: &str) -> AngelFont {
    AngelFont::parse(&fixture(relative)).unwrap_or_else(|err| panic!("parse {relative}: {err}"))
}

#[test]
fn micro_font_glyphs_case_and_max_height() {
    let font = micro_font();

    let keys: Vec<&str> = font.glyphs.keys().map(String::as_str).collect();
    assert_eq!(keys, ["-dash-", "A", "B"]);

    let a = font.glyphs.get("A").expect("glyph A");
    assert_eq!((a.width, a.height), (45, 59));
    let b = font.glyphs.get("B").expect("glyph B");
    assert_eq!((b.width, b.height), (38, 59));
    let dash = font.glyphs.get("-dash-").expect("glyph -dash-");
    assert_eq!((dash.width, dash.height), (30, 18));

    assert_eq!(font.max_height, 59);
    assert_eq!(font.case_allowed, Case::Upper);
    assert_eq!(font.atlas.image_path, "micro-font.png");
}

#[test]
fn micro_font_measure_matches_atlas_text_semantics() {
    let font = micro_font();

    assert_eq!(font.measure("AB"), (83, 59));
    assert_eq!(font.measure("A B"), (123, 59));
    // widest-line semantics per orchestrator contract fix (measure reports max over lines)
    assert_eq!(font.measure("A\nB"), (45, 118));
    assert_eq!(font.measure("Q"), (0, 59));
    assert_eq!(font.measure("-"), (30, 59));
}

#[test]
fn glyph_prefix_is_the_verbatim_haxe_table() {
    let font = micro_font();

    let cases = [
        ('&', "-andpersand-"),
        ('\u{1F620}', "-angry faic-"),
        ('\'', "-apostraphie-"),
        ('\\', "-back slash-"),
        (',', "-comma-"),
        ('-', "-dash-"),
        ('\u{2193}', "-down arrow-"),
        ('\u{201D}', "-end quote-"),
        ('!', "-exclamation point-"),
        ('/', "-forward slash-"),
        ('>', "-greater than-"),
        ('\u{2665}', "-heart-"),
        ('\u{2661}', "-heart-"),
        ('\u{2190}', "-left arrow-"),
        ('<', "-less than-"),
        ('*', "-multiply x-"),
        ('.', "-period-"),
        ('?', "-question mark-"),
        ('\u{2192}', "-right arrow-"),
        ('\u{201C}', "-start quote-"),
        ('\u{2191}', "-up arrow-"),
        ('A', "A"),
        ('0', "0"),
        (' ', " "),
    ];

    for (ch, expected) in cases {
        assert_eq!(font.glyph_prefix(ch), expected, "prefix for {ch:?}");
    }
}

#[test]
fn angel_vcr_lite_is_parsed_faithfully() {
    let font = angel_font("angel/vcr-lite.fnt");

    assert_eq!(font.info.face, "vcr-bmp");
    assert_eq!(font.info.size, 16);
    assert_eq!(font.common.line_height, 18);
    assert_eq!(font.common.base, 14);
    assert_eq!(font.common.scale_w, 122);
    assert_eq!(font.common.scale_h, 122);

    assert_eq!(font.pages.len(), 1);
    assert_eq!(font.pages[0].id, 0);
    assert_eq!(font.pages[0].file, "vcr-bmp.png");

    let ids: Vec<u32> = font.chars.keys().copied().collect();
    assert_eq!(ids, [32, 48, 49, 65, 66]);

    let space = font.chars.get(&32).expect("space");
    assert_eq!(
        (
            space.x,
            space.y,
            space.width,
            space.height,
            space.xoffset,
            space.yoffset,
            space.xadvance,
            space.page,
        ),
        (0, 0, 0, 0, 0, 0, 10, 0)
    );

    let zero = font.chars.get(&48).expect("0");
    assert_eq!((zero.x, zero.y, zero.width, zero.height), (78, 0, 11, 14));

    let one = font.chars.get(&49).expect("1");
    assert_eq!((one.x, one.y, one.width, one.height), (90, 0, 8, 14));

    let a = font.chars.get(&65).expect("A");
    assert_eq!(
        (
            a.x, a.y, a.width, a.height, a.xoffset, a.yoffset, a.xadvance, a.page,
        ),
        (27, 32, 11, 14, -1, 2, 10, 0)
    );

    let b = font.chars.get(&66).expect("B");
    assert_eq!((b.x, b.y, b.width, b.height), (39, 32, 11, 14));

    assert!(font.kerning.is_empty());
}

#[test]
fn angel_vcr_lite_measure_matches_hand_computed_values() {
    let font = angel_font("angel/vcr-lite.fnt");

    assert_eq!(font.measure("AB"), (20, 18));
    assert_eq!(font.measure("01"), (20, 18));
    assert_eq!(font.measure("A\nB"), (10, 36));
    assert_eq!(font.measure("A?"), (10, 18));
}

#[test]
fn angel_vcr_lite_rects_fit_the_page() {
    let font = angel_font("angel/vcr-lite.fnt");
    font.validate().expect("vcr-lite rects fit the page");
}

#[test]
fn angel_overflow_fixture_is_rejected() {
    let font = angel_font("overflow/angel-overflow.fnt");
    let err = font.validate().expect_err("overflow must be rejected");

    match err {
        FontError::GlyphOverflowsPage {
            glyph,
            x,
            y,
            w,
            h,
            sw,
            sh,
        } => {
            assert_eq!(glyph, "A");
            assert_eq!((x, y, w, h), (110, 32, 14, 14));
            assert_eq!((sw, sh), (122, 122));
        }
        other => panic!("expected GlyphOverflowsPage, got {other:?}"),
    }
}

#[test]
fn angel_kerning_is_parsed_when_present() {
    let xml = concat!(
        "<font>",
        "<info face=\"kern-test\" size=\"16\"/>",
        "<common lineHeight=\"20\" base=\"16\" scaleW=\"64\" scaleH=\"64\"/>",
        "<pages><page id=\"0\" file=\"kern.png\"/></pages>",
        "<chars>",
        "<char id=\"65\" x=\"0\" y=\"0\" width=\"8\" height=\"8\" xoffset=\"0\" yoffset=\"0\" xadvance=\"10\" page=\"0\"/>",
        "<char id=\"66\" x=\"8\" y=\"0\" width=\"8\" height=\"8\" xoffset=\"0\" yoffset=\"0\" xadvance=\"10\" page=\"0\"/>",
        "</chars>",
        "<kernings count=\"1\">",
        "<kerning first=\"65\" second=\"66\" amount=\"-3\"/>",
        "</kernings>",
        "</font>",
    );
    let font = AngelFont::parse(xml).expect("kerning font parses");

    assert_eq!(font.kerning.get(&(65, 66)), Some(&-3));
    assert_eq!(font.measure("AB"), (17, 20));
    assert_eq!(font.measure("BA"), (20, 20));
}

#[test]
fn angel_malformed_input_is_a_font_error() {
    let err = AngelFont::parse("<font><info face=\"x\" size=\"not-a-number\"/></font>")
        .expect_err("non-numeric size must fail");

    match err {
        FontError::Malformed { message, .. } => {
            assert!(
                message.contains("size"),
                "message names the attribute: {message}"
            );
        }
        other => panic!("expected Malformed, got {other:?}"),
    }

    let err = AngelFont::parse("<font><info face=\"x\" size=\"16\"/></font>")
        .expect_err("missing <common> must fail");
    assert!(matches!(err, FontError::Malformed { .. }));
}

fn real_fonts_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let dir = PathBuf::from(home).join("Descargas/fnf/Funkin/assets/preload/images/fonts");
    dir.is_dir().then_some(dir)
}

fn load_real_font(dir: &Path, name: &str) -> SparrowFont {
    let path = dir.join(name);
    let xml = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    let atlas = parse_sparrow(&xml).unwrap_or_else(|err| panic!("parse {}: {err}", path.display()));
    SparrowFont::from_atlas(atlas)
        .unwrap_or_else(|err| panic!("font metrics for {}: {err}", path.display()))
}

#[test]
fn real_sparrow_fonts_match_the_audited_counts() {
    let Some(dir) = real_fonts_dir() else {
        eprintln!("skipping real-font checks: Sparrow assets directory not present");
        return;
    };

    for name in ["bold.xml", "default.xml", "freeplay-clear.xml"] {
        if !dir.join(name).is_file() {
            eprintln!(
                "skipping real-font checks: {} not present",
                dir.join(name).display()
            );
            return;
        }
    }

    let default = load_real_font(&dir, "default.xml");
    assert_eq!(default.glyphs.len(), 99);
    assert_eq!(default.max_height, 70);
    assert_eq!(default.case_allowed, Case::Both);
    assert_eq!(default.measure("AB"), (83, 70));
    assert_eq!(default.glyph_prefix('&'), "-andpersand-");
    assert!(default.glyphs.contains_key("&"));
    assert!(!default.glyphs.contains_key("-andpersand-"));
    assert_eq!(default.measure("&"), (0, 70));

    let bold = load_real_font(&dir, "bold.xml");
    assert_eq!(bold.glyphs.len(), 63);
    assert_eq!(bold.max_height, 77);
    assert_eq!(bold.case_allowed, Case::Upper);

    let freeplay_clear = load_real_font(&dir, "freeplay-clear.xml");
    assert_eq!(freeplay_clear.glyphs.len(), 10);
    assert_eq!(freeplay_clear.max_height, 24);
    assert_eq!(freeplay_clear.case_allowed, Case::Both);
    assert_eq!(
        freeplay_clear
            .glyphs
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"]
    );

    for font in [&default, &bold, &freeplay_clear] {
        for frame in &font.atlas.subtextures {
            assert!(
                frame.x >= 0 && frame.y >= 0,
                "{}: frame {:?} has a negative origin ({}, {})",
                font.atlas.image_path,
                frame.name,
                frame.x,
                frame.y
            );
            assert!(
                frame.width > 0 && frame.height > 0,
                "{}: frame {:?} has an empty rect ({}x{})",
                font.atlas.image_path,
                frame.name,
                frame.width,
                frame.height
            );
        }
    }
}

#[test]
fn micro_font_measure_uses_the_widest_line() {
    let font = micro_font();

    // Last-vs-widest regression guard: "AA" is 90 wide and the trailing "A"
    // is 45, so reporting the last line instead of the widest line would
    // yield 45 here.
    assert_eq!(font.measure("AA\nA"), (90, 118));
}

#[test]
fn angel_measure_uses_the_widest_line() {
    let font = angel_font("angel/vcr-lite.fnt");

    // "AB" advances 10 + 10 = 20; the second line "C" is not in vcr-lite and
    // contributes 0, so the widest line is 20 and the height is 2 * 18 = 36.
    assert_eq!(font.measure("AB\nC"), (20, 36));

    // "A" advances 10; "BC" advances B's 10 plus unknown C's 0 = 10, so both
    // lines are 10 wide and the height is 2 * 18 = 36.
    assert_eq!(font.measure("A\nBC"), (10, 36));
}

#[test]
fn sparrow_glyph_uses_lowest_numeric_suffix_not_document_order() {
    let xml = concat!(
        "<TextureAtlas imagePath=\"x.png\">",
        "<SubTexture name=\"A0001\" x=\"50\" y=\"2\" width=\"70\" height=\"60\" />",
        "<SubTexture name=\"A0000\" x=\"2\" y=\"2\" width=\"45\" height=\"59\" />",
        "</TextureAtlas>",
    );
    let atlas = parse_sparrow(xml).expect("inline atlas parses");
    let font = SparrowFont::from_atlas(atlas).expect("inline atlas is a valid font");

    // A0000 has the lower numeric suffix, so its metrics win even though
    // A0001 is seen first in document order (which would give 70 x 60).
    let a = font.glyphs.get("A").expect("glyph A");
    assert_eq!((a.width, a.height), (45, 59));
}

#[test]
fn angel_kerning_does_not_cross_newlines() {
    let xml = concat!(
        "<font>",
        "<info face=\"kern-test\" size=\"16\"/>",
        "<common lineHeight=\"18\" base=\"14\" scaleW=\"256\" scaleH=\"256\"/>",
        "<pages><page id=\"0\" file=\"kern.png\"/></pages>",
        "<chars>",
        "<char id=\"65\" x=\"0\" y=\"0\" width=\"8\" height=\"8\" xoffset=\"0\" yoffset=\"0\" xadvance=\"10\" page=\"0\"/>",
        "<char id=\"66\" x=\"8\" y=\"0\" width=\"8\" height=\"8\" xoffset=\"0\" yoffset=\"0\" xadvance=\"10\" page=\"0\"/>",
        "</chars>",
        "<kernings count=\"1\"><kerning first=\"65\" second=\"66\" amount=\"7\"/></kernings>",
        "</font>",
    );
    let font = AngelFont::parse(xml).expect("inline kerning font parses");

    // "AB" = (xadvance A + kern A,B) + xadvance B = (10 + 7) + 10 = 27,
    // one line of height 18.
    assert_eq!(font.measure("AB"), (27, 18));

    // The pair (A, B) must not apply across the newline: each line is its
    // own 10-wide A/B advance, widest = 10, two lines = 2 * 18 = 36.
    assert_eq!(font.measure("A\nB"), (10, 36));
}
