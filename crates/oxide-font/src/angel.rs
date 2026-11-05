//! Angel-code (BMFont) XML fonts, as loaded by `FlxBitmapFont.fromAngelCode`.
//!
//! The parser accepts the shipped attribute set: `<info face size>`,
//! `<common lineHeight base scaleW scaleH>`, `<pages><page id file>`,
//! `<chars><char ...>` and the optional `<kernings><kerning first second
//! amount>` block.

use std::collections::BTreeMap;

use oxide_atlas::xml::{Scanner, StartTag, XmlError};

use crate::{FontError, malformed};

/// `<info>` attributes used for metrics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AngelInfo {
    pub face: String,
    pub size: i32,
}

/// `<common>` attributes used for metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AngelCommon {
    pub line_height: u32,
    pub base: u32,
    pub scale_w: u32,
    pub scale_h: u32,
}

/// One `<page>` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AngelPage {
    pub id: u8,
    pub file: String,
}

/// One `<char>` entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AngelChar {
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub xoffset: i32,
    pub yoffset: i32,
    pub xadvance: i32,
    pub page: u8,
}

/// A parsed angel-code font.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AngelFont {
    pub info: AngelInfo,
    pub common: AngelCommon,
    pub pages: Vec<AngelPage>,
    pub chars: BTreeMap<u32, AngelChar>,
    pub kerning: BTreeMap<(u32, u32), i32>,
}

impl AngelFont {
    /// Parses an angel-code XML font.
    pub fn parse(xml: &str) -> Result<AngelFont, FontError> {
        let mut scanner = Scanner::new(xml);
        let mut info = None;
        let mut common = None;
        let mut pages = Vec::new();
        let mut chars = BTreeMap::new();
        let mut kerning = BTreeMap::new();

        while let Some(tag) = scanner.next_start_tag().map_err(xml_error)? {
            match tag.name {
                "info" => {
                    info = Some(AngelInfo {
                        face: required_attribute(&tag, "face")?.to_string(),
                        size: required_i32(&tag, "size")?,
                    });
                }
                "common" => {
                    common = Some(AngelCommon {
                        line_height: required_u32(&tag, "lineHeight")?,
                        base: required_u32(&tag, "base")?,
                        scale_w: required_u32(&tag, "scaleW")?,
                        scale_h: required_u32(&tag, "scaleH")?,
                    });
                }
                "page" => pages.push(AngelPage {
                    id: required_u8(&tag, "id")?,
                    file: required_attribute(&tag, "file")?.to_string(),
                }),
                "char" => {
                    let glyph = AngelChar {
                        id: required_u32(&tag, "id")?,
                        x: required_i32(&tag, "x")?,
                        y: required_i32(&tag, "y")?,
                        width: required_u32(&tag, "width")?,
                        height: required_u32(&tag, "height")?,
                        xoffset: required_i32(&tag, "xoffset")?,
                        yoffset: required_i32(&tag, "yoffset")?,
                        xadvance: required_i32(&tag, "xadvance")?,
                        page: required_u8(&tag, "page")?,
                    };
                    chars.insert(glyph.id, glyph);
                }
                "kerning" => {
                    let first = required_u32(&tag, "first")?;
                    let second = required_u32(&tag, "second")?;
                    kerning.insert((first, second), required_i32(&tag, "amount")?);
                }
                _ => {}
            }
        }

        let info = info.ok_or_else(|| malformed("missing <info> element"))?;
        let common = common.ok_or_else(|| malformed("missing <common> element"))?;

        Ok(AngelFont {
            info,
            common,
            pages,
            chars,
            kerning,
        })
    }

    /// Checks that every glyph rectangle lies inside the page: `x`/`y` are
    /// non-negative and `x + width` / `y + height` do not exceed
    /// `common.scale_w` / `common.scale_h`.
    pub fn validate(&self) -> Result<(), FontError> {
        let page_w = i64::from(self.common.scale_w);
        let page_h = i64::from(self.common.scale_h);

        for glyph in self.chars.values() {
            let x = i64::from(glyph.x);
            let y = i64::from(glyph.y);
            let w = i64::from(glyph.width);
            let h = i64::from(glyph.height);

            if x < 0 || y < 0 || x + w > page_w || y + h > page_h {
                return Err(FontError::GlyphOverflowsPage {
                    glyph: glyph_name(glyph.id),
                    x: glyph.x,
                    y: glyph.y,
                    w: saturating_i32(glyph.width),
                    h: saturating_i32(glyph.height),
                    sw: saturating_i32(self.common.scale_w),
                    sh: saturating_i32(self.common.scale_h),
                });
            }
        }

        Ok(())
    }

    /// Measures `text`: the width is the widest line's advance sum
    /// (`xadvance` plus the kerning pair with the previous glyph on the same
    /// line) and the height is `lines * line_height`. Unknown characters
    /// contribute nothing; a newline starts a new line and clears the kerning
    /// pair, so the final glyph of a line is advanced without a following pair.
    pub fn measure(&self, text: &str) -> (u32, u32) {
        if text.is_empty() {
            return (0, 0);
        }

        let mut line_width: i64 = 0;
        let mut max_width: i64 = 0;
        let mut previous: Option<u32> = None;

        for ch in text.chars() {
            if ch == '\n' {
                max_width = max_width.max(line_width);
                line_width = 0;
                previous = None;
                continue;
            }

            let id = ch as u32;
            let Some(glyph) = self.chars.get(&id) else {
                previous = None;
                continue;
            };

            line_width += i64::from(glyph.xadvance);
            if let Some(first) = previous
                && let Some(amount) = self.kerning.get(&(first, id))
            {
                line_width += i64::from(*amount);
            }
            previous = Some(id);
        }
        max_width = max_width.max(line_width);

        let lines = text.split('\n').count() as u64;
        let width = max_width.max(0).min(i64::from(u32::MAX)) as u32;
        let height = (u64::from(self.common.line_height) * lines).min(u64::from(u32::MAX)) as u32;
        (width, height)
    }
}

fn xml_error(err: XmlError) -> FontError {
    malformed(err.message)
}

fn required_attribute<'a>(tag: &StartTag<'a>, name: &str) -> Result<&'a str, FontError> {
    tag.attribute(name).ok_or_else(|| {
        malformed(format!(
            "element <{}> is missing attribute `{name}`",
            tag.name
        ))
    })
}

fn required_i32(tag: &StartTag<'_>, name: &str) -> Result<i32, FontError> {
    tag.required_i32(name).map_err(xml_error)
}

fn required_u32(tag: &StartTag<'_>, name: &str) -> Result<u32, FontError> {
    let value = required_i32(tag, name)?;
    u32::try_from(value).map_err(|_| {
        malformed(format!(
            "attribute `{name}` on <{}> is negative ({value})",
            tag.name
        ))
    })
}

fn required_u8(tag: &StartTag<'_>, name: &str) -> Result<u8, FontError> {
    let value = required_i32(tag, name)?;
    u8::try_from(value).map_err(|_| {
        malformed(format!(
            "attribute `{name}` on <{}> is not a byte ({value})",
            tag.name
        ))
    })
}

fn glyph_name(id: u32) -> String {
    match char::from_u32(id) {
        Some(ch) if !ch.is_control() => ch.to_string(),
        _ => id.to_string(),
    }
}

fn saturating_i32(value: u32) -> i32 {
    value.min(i32::MAX as u32) as i32
}
