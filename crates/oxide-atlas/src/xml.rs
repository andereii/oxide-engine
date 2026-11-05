//! Lenient, std-only XML scanner.
//!
//! Only start tags are interesting to the engine's two XML formats (Sparrow
//! atlases and angel-code fonts). Everything else — declarations, comments,
//! doctypes, processing instructions, end tags and text — is skipped.
//!
//! Leniency required by the shipped data:
//! - attribute values may be quoted with `"` or `'`;
//! - raw `<` and `&` are allowed inside quoted values;
//! - any content (including a BOM, whitespace or a declaration) may precede
//!   the first element;
//! - paired and self-closing tags are both accepted.

use std::fmt;

/// Error produced by the lenient scanner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlError {
    pub message: String,
}

impl XmlError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for XmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for XmlError {}

/// A start tag with its attributes preserved in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartTag<'a> {
    pub name: &'a str,
    pub attributes: Vec<(&'a str, &'a str)>,
    pub self_closing: bool,
}

impl<'a> StartTag<'a> {
    /// Value of `name`, or `None` when the attribute is absent.
    pub fn attribute(&self, name: &str) -> Option<&'a str> {
        self.attributes
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| *value)
    }

    /// Value of a required attribute.
    pub fn required_attribute(&self, name: &str) -> Result<&'a str, XmlError> {
        self.attribute(name).ok_or_else(|| {
            XmlError::new(format!(
                "element <{}> is missing attribute `{name}`",
                self.name
            ))
        })
    }

    /// A boolean attribute: exactly `"true"` is true, anything else is false.
    pub fn is_true(&self, name: &str) -> bool {
        self.attribute(name) == Some("true")
    }

    /// A required `i32` attribute (`[-+]?[0-9]+`).
    pub fn required_i32(&self, name: &str) -> Result<i32, XmlError> {
        let raw = self.required_attribute(name)?;
        self.parse_i32(name, raw)
    }

    /// An optional `i32` attribute; `None` when absent, an error when present
    /// but not an integer.
    pub fn optional_i32(&self, name: &str) -> Result<Option<i32>, XmlError> {
        self.attribute(name)
            .map(|raw| self.parse_i32(name, raw))
            .transpose()
    }

    fn parse_i32(&self, name: &str, raw: &str) -> Result<i32, XmlError> {
        raw.parse::<i32>().map_err(|err| {
            XmlError::new(format!(
                "attribute `{name}` on <{}> is not an i32 ({raw:?}: {err})",
                self.name
            ))
        })
    }
}

/// Sequential start-tag scanner over an XML document.
pub struct Scanner<'a> {
    input: &'a str,
    position: usize,
}

impl<'a> Scanner<'a> {
    pub fn new(input: &'a str) -> Self {
        let input = input.strip_prefix('\u{FEFF}').unwrap_or(input);
        Self { input, position: 0 }
    }

    /// Returns the next start tag, skipping prologue, comments, doctypes,
    /// processing instructions, end tags and text. `None` at end of input.
    pub fn next_start_tag(&mut self) -> Result<Option<StartTag<'a>>, XmlError> {
        loop {
            let remaining = &self.input[self.position..];
            let Some(offset) = remaining.find('<') else {
                self.position = self.input.len();
                return Ok(None);
            };
            let tag_start = self.position + offset;
            let rest = &self.input[tag_start..];

            let next = if rest.starts_with("<!--") {
                skip_past(rest, tag_start, "-->", "comment")?
            } else if rest.starts_with("<?") {
                skip_past(rest, tag_start, "?>", "processing instruction")?
            } else if rest.starts_with("<!") {
                skip_past(rest, tag_start, ">", "markup declaration")?
            } else if rest.starts_with("</") {
                skip_past(rest, tag_start, ">", "end tag")?
            } else {
                return self.parse_start_tag(tag_start).map(Some);
            };
            self.position = next;
        }
    }

    fn parse_start_tag(&mut self, start: usize) -> Result<StartTag<'a>, XmlError> {
        let bytes = self.input.as_bytes();
        let mut pos = start + 1;
        skip_whitespace(bytes, &mut pos);

        let name_start = pos;
        while pos < bytes.len() && !is_name_end(bytes[pos]) {
            pos += 1;
        }
        if name_start == pos {
            return Err(XmlError::new(format!(
                "missing element name at byte {start}"
            )));
        }
        let name = &self.input[name_start..pos];

        let mut attributes = Vec::new();
        loop {
            skip_whitespace(bytes, &mut pos);
            let Some(&byte) = bytes.get(pos) else {
                return Err(XmlError::new(format!(
                    "start tag <{name}> at byte {start} is not terminated"
                )));
            };
            if byte == b'>' {
                self.position = pos + 1;
                return Ok(StartTag {
                    name,
                    attributes,
                    self_closing: false,
                });
            }
            if byte == b'/' {
                if bytes.get(pos + 1) == Some(&b'>') {
                    self.position = pos + 2;
                    return Ok(StartTag {
                        name,
                        attributes,
                        self_closing: true,
                    });
                }
                return Err(XmlError::new(format!(
                    "stray `/` in start tag <{name}> at byte {pos}"
                )));
            }

            let attr_start = pos;
            while pos < bytes.len() && !is_name_end(bytes[pos]) {
                pos += 1;
            }
            if attr_start == pos {
                return Err(XmlError::new(format!(
                    "empty attribute name in start tag <{name}> at byte {pos}"
                )));
            }
            let attr_name = &self.input[attr_start..pos];

            skip_whitespace(bytes, &mut pos);
            if bytes.get(pos) != Some(&b'=') {
                return Err(XmlError::new(format!(
                    "attribute `{attr_name}` on <{name}> has no `=`"
                )));
            }
            pos += 1;
            skip_whitespace(bytes, &mut pos);

            let Some(&quote) = bytes.get(pos) else {
                return Err(XmlError::new(format!(
                    "attribute `{attr_name}` on <{name}> has no value"
                )));
            };
            if quote != b'"' && quote != b'\'' {
                return Err(XmlError::new(format!(
                    "attribute `{attr_name}` on <{name}> is not quoted"
                )));
            }
            pos += 1;
            let value_start = pos;
            while pos < bytes.len() && bytes[pos] != quote {
                pos += 1;
            }
            if pos >= bytes.len() {
                return Err(XmlError::new(format!(
                    "attribute `{attr_name}` on <{name}> is not terminated"
                )));
            }
            let value = &self.input[value_start..pos];
            pos += 1;
            attributes.push((attr_name, value));
        }
    }
}

fn skip_whitespace(bytes: &[u8], pos: &mut usize) {
    while *pos < bytes.len() && bytes[*pos].is_ascii_whitespace() {
        *pos += 1;
    }
}

fn is_name_end(byte: u8) -> bool {
    byte.is_ascii_whitespace() || matches!(byte, b'/' | b'>' | b'=')
}

fn skip_past(rest: &str, start: usize, marker: &str, what: &str) -> Result<usize, XmlError> {
    rest.find(marker)
        .map(|end| start + end + marker.len())
        .ok_or_else(|| XmlError::new(format!("{what} at byte {start} is not terminated")))
}

#[cfg(test)]
mod tests {
    use super::Scanner;

    #[test]
    fn accepts_raw_markup_bom_single_quotes_and_paired_tags() {
        let input = "\u{FEFF}\n  <?xml version=\"1.0\"?>\n<!-- a <weird> comment -->\n<Root a=\"<0000\" b='&0>0' empty=\"\">\n  <Child d=\"1\"></Child>\n</Root>";
        let mut scanner = Scanner::new(input);

        let root = scanner.next_start_tag().expect("root").expect("root tag");
        assert_eq!(root.name, "Root");
        assert_eq!(root.attribute("a"), Some("<0000"));
        assert_eq!(root.attribute("b"), Some("&0>0"));
        assert_eq!(root.attribute("empty"), Some(""));
        assert!(!root.self_closing);

        let child = scanner.next_start_tag().expect("child").expect("child tag");
        assert_eq!(child.name, "Child");
        assert_eq!(child.attribute("d"), Some("1"));
        assert!(!child.self_closing);

        assert!(scanner.next_start_tag().expect("end of input").is_none());
    }

    #[test]
    fn self_closing_tags_are_reported() {
        let mut scanner = Scanner::new("<A x=\"1\"/><B/>");
        assert!(
            scanner
                .next_start_tag()
                .expect("A")
                .expect("A tag")
                .self_closing
        );
        assert!(
            scanner
                .next_start_tag()
                .expect("B")
                .expect("B tag")
                .self_closing
        );
    }

    #[test]
    fn integer_attributes_use_i32_grammar() {
        let mut scanner = Scanner::new("<A x=\"+12\" y=\"-3\" z=\"16abc\"/>");
        let tag = scanner.next_start_tag().expect("A").expect("A tag");
        assert_eq!(tag.optional_i32("x").expect("x"), Some(12));
        assert_eq!(tag.optional_i32("y").expect("y"), Some(-3));
        assert_eq!(tag.optional_i32("missing").expect("missing"), None);
        assert!(tag.optional_i32("z").is_err());
    }
}
