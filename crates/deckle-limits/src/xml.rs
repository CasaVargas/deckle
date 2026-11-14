//! The depth check that runs before an XML parser that recurses (nesting-depth design,
//! mechanism 1).
//!
//! `roxmltree` and `feed-rs` (through `quick-xml`) recurse once per element level before they
//! reject anything, and a stack overflow aborts the process. So the text is scanned first, in a
//! single pass that never counts lower than either parser nests.

use std::fmt;

use crate::MAX_DEPTH;

/// The deepest element nesting in `xml`, counted from tags alone; stops at `limit + 1`.
///
/// True when `xml` nests more than `limit` elements deep, defines an entity whose value holds
/// markup, or ends inside a comment, CDATA section, processing instruction, quoted value or
/// DOCTYPE, or has a declaration or DOCTYPE subset that holds markup or text where none may
/// stand.
///
/// The encoding is found as quick-xml (feed-rs) finds it: a UTF-16 byte-order mark, or BOM-less
/// UTF-16 from `00 3C 00 3F` / `3C 00 3F 00`; anything else is read as bytes. Also true, so the
/// feed is refused, when an `<?xml … encoding="…"?>` declaration names an encoding the scan
/// cannot read as the parser will: ISO-2022-JP, the "replacement" labels (ISO-2022-KR,
/// ISO-2022-CN, HZ-GB-2312), a UTF-16 label on bytes that are not UTF-16 of that byte order,
/// or any other label on bytes that are UTF-16; when a declaration is not plain
/// `name="value"` pairs or names `encoding` twice; and when it names Shift_JIS, Big5, GBK or
/// gb18030 (or an alias) and a `[` or `]` follows a byte of 0x80 or above.
///
/// The answer is for UTF-8 input and for the UTF-16 it detects. For other multibyte encodings
/// it errs toward "over" and can refuse ordinary text in them: in Shift_JIS, `ー` is the bytes
/// `81 5B`, a high byte followed by `[`. Feeds must not be scanned with it; the feed code checks
/// depth with its own quick-xml pass. A caller whose parser honours other declared encodings
/// decodes the text to UTF-8 first and uses [`parse_xml`] or that pass.
///
/// Crate-private: nothing outside this crate calls it, and [`parse_xml`] runs the same scan
/// directly, with the encoding check off.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn xml_depth_over(xml: &[u8], limit: usize) -> bool {
    let utf16 = |bytes, little| Utf16 { bytes, little };
    let verdict = match xml {
        [0xFE, 0xFF, rest @ ..] => Scan::new(utf16(rest, false), true).run(limit),
        [0xFF, 0xFE, rest @ ..] => Scan::new(utf16(rest, true), true).run(limit),
        [0, b'<', 0, b'?', ..] => Scan::new(utf16(xml, false), true).run(limit),
        [b'<', 0, b'?', 0, ..] => Scan::new(utf16(xml, true), true).run(limit),
        _ => Scan::new(Bytes(xml), true).run(limit),
    };
    verdict != Verdict::Within
}

/// How the scanned units are encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// UTF-8 or another encoding that keeps ASCII markup as single bytes.
    Bytes,
    Utf16Le,
    Utf16Be,
}

/// What the scan found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// No deeper than the limit.
    Within,
    TooDeep,
    /// An `<!ENTITY` value holds a literal `<`, so it can hold elements.
    EntityMarkup,
    /// A comment, CDATA section, PI, quoted value or DOCTYPE runs to the end of the text, so
    /// what follows its start was not counted.
    Unterminated,
    /// A declaration holds `<`, or the DOCTYPE subset holds something that is not a
    /// declaration, comment or PI: roxmltree and quick-xml read such text differently, so the
    /// count could miss elements one of them builds.
    Malformed,
    /// The declared encoding cannot be read the way the parser will decode it.
    Encoding,
}

/// The text as code units: bytes for UTF-8 and other ASCII-compatible encodings, 16-bit units
/// for UTF-16. Markup characters are ASCII, so a unit is compared with an ASCII byte.
trait Units {
    fn len(&self) -> usize;
    fn at(&self, i: usize) -> u32;
    fn mode(&self) -> Mode;
}

struct Bytes<'a>(&'a [u8]);

impl Units for Bytes<'_> {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn at(&self, i: usize) -> u32 {
        u32::from(self.0[i])
    }
    fn mode(&self) -> Mode {
        Mode::Bytes
    }
}

struct Utf16<'a> {
    bytes: &'a [u8],
    little: bool,
}

impl Units for Utf16<'_> {
    fn len(&self) -> usize {
        self.bytes.len() / 2
    }
    fn at(&self, i: usize) -> u32 {
        let pair = [self.bytes[2 * i], self.bytes[2 * i + 1]];
        u32::from(if self.little {
            u16::from_le_bytes(pair)
        } else {
            u16::from_be_bytes(pair)
        })
    }
    fn mode(&self) -> Mode {
        if self.little {
            Mode::Utf16Le
        } else {
            Mode::Utf16Be
        }
    }
}

/// What a declared encoding label means for the scan, by the WHATWG labels `encoding_rs` (and
/// so quick-xml) uses: trimmed of ASCII whitespace, compared without case.
fn label_mode(label: &str) -> Option<Mode> {
    match label
        .trim_matches(|c: char| c.is_ascii_whitespace())
        .to_ascii_lowercase()
        .as_str()
    {
        "unicodefffe" | "utf-16be" => Some(Mode::Utf16Be),
        "csunicode" | "iso-10646-ucs-2" | "ucs-2" | "unicode" | "unicodefeff" | "utf-16"
        | "utf-16le" => Some(Mode::Utf16Le),
        // Not ASCII-transparent: ISO-2022-JP, and the "replacement" labels.
        "csiso2022jp" | "iso-2022-jp" | "csiso2022kr" | "hz-gb-2312" | "iso-2022-cn"
        | "iso-2022-cn-ext" | "iso-2022-kr" | "replacement" => None,
        _ => Some(Mode::Bytes),
    }
}

/// Whether a declared label names Shift_JIS, Big5, GBK or gb18030, whose second bytes overlap
/// ASCII (WHATWG labels, as `encoding_rs` reads them).
fn is_multibyte_label(label: &str) -> bool {
    matches!(
        label
            .trim_matches(|c: char| c.is_ascii_whitespace())
            .to_ascii_lowercase()
            .as_str(),
        "shift_jis"
            | "sjis"
            | "windows-31j"
            | "x-sjis"
            | "ms_kanji"
            | "csshiftjis"
            | "ms932"
            | "big5"
            | "big5-hkscs"
            | "cn-big5"
            | "csbig5"
            | "x-x-big5"
            | "gbk"
            | "gb2312"
            | "chinese"
            | "csgb2312"
            | "csiso58gb231280"
            | "gb_2312"
            | "gb_2312-80"
            | "iso-ir-58"
            | "x-gbk"
            | "gb18030"
    )
}

/// The scan. It follows the XML rules where `roxmltree` and `quick-xml` agree, and where they
/// differ on malformed input it follows `quick-xml`, which reads on where `roxmltree` stops
/// with an error, or refuses. So the count is never lower than the nesting either parser
/// reaches.
struct Scan<U> {
    u: U,
    n: usize,
    /// Whether an `<?xml … encoding=…?>` is checked against the units' encoding. Off for text
    /// that is already UTF-8, whatever it declares.
    check_encoding: bool,
}

impl<U: Units> Scan<U> {
    fn new(u: U, check_encoding: bool) -> Self {
        let n = u.len();
        Scan {
            u,
            n,
            check_encoding,
        }
    }

    fn is_space(&self, i: usize) -> bool {
        b" \t\r\n".iter().any(|&c| self.is(i, c))
    }

    /// The PI at `i` (`<?` … `?>`, ending at `end`) is an XML declaration whose encoding the
    /// scan cannot follow. quick-xml switches its decoder to any encoding a declaration names,
    /// wherever the declaration stands.
    fn bad_declared_encoding(&self, i: usize, end: usize) -> bool {
        let is_decl = i + 5 < end
            && (0..3).all(|k| {
                let c = self.u.at(i + 2 + k);
                c < 0x80 && (c as u8).eq_ignore_ascii_case(&b"xml"[k])
            })
            && self.is_space(i + 5);
        if !is_decl {
            return false;
        }
        // Read the pseudo-attributes one by one, quote-aware, so `encoding` is matched by its
        // exact name. Anything the scan cannot read as `name = "value"`, and a second
        // `encoding`, is refused.
        let skip_space = |mut j: usize| {
            while j < end && self.is_space(j) {
                j += 1;
            }
            j
        };
        let mut label = None;
        let mut j = i + 5;
        loop {
            j = skip_space(j);
            if j >= end {
                break;
            }
            let name = j;
            while j < end && !self.is_space(j) && !self.is(j, b'=') {
                j += 1;
            }
            let is_encoding = j - name == 8 && self.starts(name, b"encoding");
            j = skip_space(j);
            if !self.is(j, b'=') {
                return true;
            }
            j = skip_space(j + 1);
            if !(self.is(j, b'"') || self.is(j, b'\'')) {
                return true;
            }
            let quote = self.u.at(j);
            let Some(close) = (j + 1..end).find(|&k| self.u.at(k) == quote) else {
                return true;
            };
            if is_encoding {
                if label.is_some() {
                    return true;
                }
                label = Some(
                    (j + 1..close)
                        .map(|k| char::from_u32(self.u.at(k)).unwrap_or('\u{FFFD}'))
                        .collect::<String>(),
                );
            }
            j = close + 1;
        }
        let Some(label) = label else {
            return false;
        };
        if label_mode(&label) != Some(self.u.mode()) {
            return true;
        }
        // In Shift_JIS, Big5, GBK and gb18030 the second byte of a character can be `[` or
        // `]`, which the byte scan would take for markup. Such text is refused; callers decode
        // to UTF-8 first.
        is_multibyte_label(&label)
            && (1..self.n)
                .any(|k| self.u.at(k - 1) >= 0x80 && (self.is(k, b'[') || self.is(k, b']')))
    }

    /// Past the `;` of a parameter-entity reference (`%name;`) whose name starts at `from`.
    /// quick-xml passes over it; roxmltree stops with an error.
    fn pe_reference(&self, from: usize) -> Result<usize, Verdict> {
        let start_char = |c: u32| {
            c >= 0x80 || c == u32::from(b'_') || c == u32::from(b':') || {
                c < 0x80 && (c as u8).is_ascii_alphabetic()
            }
        };
        let name_char = |c: u32| {
            start_char(c)
                || (c < 0x80 && (c as u8).is_ascii_digit())
                || c == u32::from(b'-')
                || c == u32::from(b'.')
        };
        if from >= self.n || !start_char(self.u.at(from)) {
            return Err(Verdict::Malformed);
        }
        let mut j = from + 1;
        while j < self.n && name_char(self.u.at(j)) {
            j += 1;
        }
        if self.is(j, b';') {
            Ok(j + 1)
        } else {
            Err(Verdict::Malformed)
        }
    }

    fn is(&self, i: usize, c: u8) -> bool {
        i < self.n && self.u.at(i) == u32::from(c)
    }

    fn starts(&self, i: usize, pat: &[u8]) -> bool {
        i + pat.len() <= self.n && pat.iter().enumerate().all(|(k, &c)| self.is(i + k, c))
    }

    /// The first `pat` at or after `from`.
    fn find(&self, from: usize, pat: &[u8]) -> Result<usize, Verdict> {
        (from..self.n)
            .find(|&i| self.starts(i, pat))
            .ok_or(Verdict::Unterminated)
    }

    /// Past the first `pat` at or after `from`.
    fn past(&self, from: usize, pat: &[u8]) -> Result<usize, Verdict> {
        Ok(self.find(from, pat)? + pat.len())
    }

    fn run(&self, limit: usize) -> Verdict {
        match self.elements(limit) {
            Ok(()) => Verdict::Within,
            Err(v) => v,
        }
    }

    fn elements(&self, limit: usize) -> Result<(), Verdict> {
        let mut depth = 0usize;
        let mut i = 0;
        while i < self.n {
            if !self.is(i, b'<') {
                i += 1;
            } else if self.starts(i, b"<!-") {
                // A comment ends at the first `-->` after `<!--`; `<!-->` does not end it.
                // quick-xml takes `<!-x` for a comment too, with the same end.
                i = self.past(i + 4, b"-->")?;
            } else if self.starts(i, b"<![CDATA[") {
                i = self.past(i + 9, b"]]>")?;
            } else if self.starts(i, b"<!D") || self.starts(i, b"<!d") {
                // quick-xml takes any `<!D` or `<!d` for a DOCTYPE.
                i = self.doctype(i + 3)?;
            } else if self.starts(i, b"<!") {
                // Neither parser reads on past this; count what follows as content.
                i += 1;
            } else if self.starts(i, b"<?") {
                let end = self.find(i + 2, b"?>")?;
                if self.check_encoding && self.bad_declared_encoding(i, end) {
                    return Err(Verdict::Encoding);
                }
                i = end + 2;
            } else if self.starts(i, b"</") {
                depth = depth.saturating_sub(1);
                i = self.past(i + 2, b">").unwrap_or(self.n);
            } else if i + 1 == self.n {
                break;
            } else {
                // quick-xml reads anything else after `<` as a start tag.
                let (end, empty) = self.tag_end(i + 1)?;
                // A self-closing element is a level too, while it is parsed.
                if depth + 1 > limit {
                    return Err(Verdict::TooDeep);
                }
                if !empty {
                    depth += 1;
                }
                i = end + 1;
            }
        }
        Ok(())
    }

    /// The `>` that ends the start tag whose name begins at `from`, and whether the tag closes
    /// itself (`/>`). A quote opens a quoted run wherever it stands, as in quick-xml; in
    /// well-formed XML it only stands at the start of a value, so unquoted values (`href=x`)
    /// are ordinary text. A tag cut off by the end of the text ends there.
    fn tag_end(&self, from: usize) -> Result<(usize, bool), Verdict> {
        let mut quote = None;
        for j in from..self.n {
            let c = self.u.at(j);
            match quote {
                Some(q) if c == q => quote = None,
                Some(_) => {}
                None if c == u32::from(b'"') || c == u32::from(b'\'') => quote = Some(c),
                None if c == u32::from(b'>') => return Ok((j, j > from && self.is(j - 1, b'/'))),
                None => {}
            }
        }
        match quote {
            Some(_) => Err(Verdict::Unterminated),
            None => Ok((self.n, false)),
        }
    }

    /// Past the `>` that ends a DOCTYPE whose body starts at `from`, read as quick-xml reads
    /// it (`roxmltree` agrees on every DOCTYPE it accepts). Fails on an entity value holding
    /// `<`.
    fn doctype(&self, from: usize) -> Result<usize, Verdict> {
        // Before the internal subset: quoted ids, then `[` or `>`.
        let mut j = from;
        loop {
            if j >= self.n {
                return Err(Verdict::Unterminated);
            }
            if self.is(j, b'"') || self.is(j, b'\'') {
                j = self.past(j + 1, &[self.u.at(j) as u8])?;
            } else if self.is(j, b'[') {
                j += 1;
                break;
            } else if self.is(j, b'>') {
                return Ok(j + 1);
            } else {
                j += 1;
            }
        }
        // The internal subset: declarations, comments and PIs up to `]`, with whitespace
        // between. roxmltree refuses anything else there, and quick-xml reads it on in ways
        // the scan cannot follow (a byte of a Shift_JIS character can look like `[`), so
        // anything else is refused.
        while j < self.n {
            if self.is(j, b']') {
                // roxmltree allows only whitespace before `>`; quick-xml takes the first `>`.
                let end = self.find(j + 1, b">")?;
                if (j + 1..end).any(|k| self.is(k, b'<')) {
                    return Err(Verdict::Malformed);
                }
                return Ok(end + 1);
            } else if self.starts(j, b"<?") {
                j = self.past(j + 2, b"?>")?;
            } else if self.starts(j, b"<!--") {
                j = self.past(j + 4, b"-->")?;
            } else if self.starts(j, b"<!ENTITY") {
                j = self.declaration(j + 8, true)?;
            } else if self.starts(j, b"<!ATTLIST") || self.starts(j, b"<!NOTATION") {
                j = self.declaration(j + 9, false)?;
            } else if self.starts(j, b"<!ELEMENT") {
                // Both parsers end it at the first `>`.
                let end = self.find(j + 9, b">")?;
                if (j + 1..end).any(|k| self.is(k, b'<')) {
                    return Err(Verdict::Malformed);
                }
                j = end + 1;
            } else if self.is(j, b'%') {
                j = self.pe_reference(j + 1)?;
            } else if self.is_space(j) {
                j += 1;
            } else {
                return Err(Verdict::Malformed);
            }
        }
        Err(Verdict::Unterminated)
    }

    /// Past the `>` ending a declaration whose quoted values may hold `>`, as quick-xml reads
    /// it. roxmltree ends `<!ATTLIST` and `<!NOTATION` at the first `>`, quoted or not, so the
    /// two can disagree on where the declaration ends; any `<` before quick-xml's end is
    /// refused, which leaves nothing either reading could take for an element or a hidden
    /// declaration. In an entity value a `<` is markup roxmltree would expand into elements.
    fn declaration(&self, from: usize, entity: bool) -> Result<usize, Verdict> {
        let lt = u32::from(b'<');
        let mut quote = None;
        for j in from..self.n {
            let c = self.u.at(j);
            match quote {
                Some(q) if c == q => quote = None,
                Some(_) if entity && c == lt => return Err(Verdict::EntityMarkup),
                Some(_) if c == lt => return Err(Verdict::Malformed),
                Some(_) => {}
                None if c == u32::from(b'"') || c == u32::from(b'\'') => quote = Some(c),
                None if c == u32::from(b'>') => return Ok(j + 1),
                None if c == lt => return Err(Verdict::Malformed),
                None => {}
            }
        }
        Err(Verdict::Unterminated)
    }
}

/// Why [`parse_xml`] refused or failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XmlError {
    /// Elements nest more than [`MAX_DEPTH`] deep.
    TooDeep,
    /// The DOCTYPE defines an entity whose value holds markup.
    EntityMarkup,
    /// A comment, CDATA section, PI, quoted value, declaration or DOCTYPE is not closed where
    /// it should be, so the depth check cannot tell how deep the parser would go.
    Malformed,
    /// `roxmltree` could not parse it.
    Parse(roxmltree::Error),
}

impl fmt::Display for XmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            XmlError::TooDeep => write!(f, "this file is nested more than {MAX_DEPTH} levels deep"),
            XmlError::EntityMarkup => {
                f.write_str("this file defines an entity that contains markup")
            }
            XmlError::Malformed => f.write_str(
                "this file has a comment, quoted value or declaration that is not closed",
            ),
            XmlError::Parse(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for XmlError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            XmlError::Parse(e) => Some(e),
            _ => None,
        }
    }
}

impl From<roxmltree::Error> for XmlError {
    fn from(e: roxmltree::Error) -> Self {
        XmlError::Parse(e)
    }
}

/// `roxmltree::Document::parse_with_options`, after checking that `text` nests no more than
/// [`MAX_DEPTH`] deep and defines no entity holding markup. Every parse of file content goes
/// through this.
///
/// Text that ends inside a comment, CDATA section, PI, quoted value or DOCTYPE, or whose
/// DOCTYPE holds markup where none may stand, is refused with [`XmlError::Malformed`] before
/// `roxmltree` sees it. The declared encoding is not checked: `text` is already UTF-8.
///
/// An `entity_resolver` in `options` is not covered: text it returns is not scanned.
pub fn parse_xml<'input>(
    text: &'input str,
    options: roxmltree::ParsingOptions<'input>,
) -> Result<roxmltree::Document<'input>, XmlError> {
    // `text` is UTF-8 whatever it starts with, so it is scanned as bytes.
    match Scan::new(Bytes(text.as_bytes()), false).run(MAX_DEPTH) {
        Verdict::Within => Ok(roxmltree::Document::parse_with_options(text, options)?),
        Verdict::TooDeep => Err(XmlError::TooDeep),
        Verdict::EntityMarkup => Err(XmlError::EntityMarkup),
        Verdict::Unterminated | Verdict::Malformed | Verdict::Encoding => Err(XmlError::Malformed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The depth the scanner reports: the smallest limit it accepts. None when it refuses at
    /// every limit up to 1000 (an entity with markup, or an unterminated construct).
    fn depth(xml: &str) -> Option<usize> {
        depth_bytes(xml.as_bytes())
    }

    fn depth_bytes(xml: &[u8]) -> Option<usize> {
        (0..1000).find(|&l| !xml_depth_over(xml, l))
    }

    fn nested(open: &str, close: &str, n: usize, inner: &str) -> String {
        let mut s = String::with_capacity(n * (open.len() + close.len()) + inner.len());
        for _ in 0..n {
            s.push_str(open);
        }
        s.push_str(inner);
        for _ in 0..n {
            s.push_str(close);
        }
        s
    }

    /// Runs `f` on a thread with a `stack` byte stack and returns its result.
    fn on_stack<T: Send + 'static>(stack: usize, f: impl FnOnce() -> T + Send + 'static) -> T {
        std::thread::Builder::new()
            .stack_size(stack)
            .spawn(f)
            .unwrap()
            .join()
            .unwrap()
    }

    #[test]
    fn counts_nesting_not_siblings() {
        assert_eq!(depth(""), Some(0));
        assert_eq!(depth("just text"), Some(0));
        assert_eq!(depth("<a/>"), Some(1));
        assert_eq!(depth("<a></a>"), Some(1));
        assert_eq!(depth("<a><b></b><b></b><b>x</b></a>"), Some(2));
        assert_eq!(depth("<a><b><c/></b><b/></a>"), Some(3));
        assert_eq!(
            depth("<?xml version=\"1.0\"?>\n<a>\n  <b>1 &gt; 0</b>\n</a>\n"),
            Some(2)
        );
    }

    #[test]
    fn a_self_closing_tag_is_a_level_but_does_not_stay_open() {
        assert_eq!(depth("<a><b/><b/><b/><b/></a>"), Some(2));
        assert_eq!(depth("<a><b x=\"1\" /><b/></a>"), Some(2));
    }

    #[test]
    fn comments_cdata_and_pis_do_not_change_the_count() {
        assert_eq!(depth("<a><!-- <b><c><d> --></a>"), Some(1));
        assert_eq!(depth("<a><!-- <b> -> - > --></a>"), Some(1));
        assert_eq!(depth("<a><![CDATA[<b><c> ]] ]> ]]></a>"), Some(1));
        assert_eq!(depth("<a><?pi <b><c> > ? ?></a>"), Some(1));
        assert_eq!(depth("<!-- <x><x> --><?p <y> ?><a/>"), Some(1));
        // `<!-->` does not end the comment it starts.
        assert_eq!(depth("<a><!--> <b><c> --></a>"), Some(1));
    }

    #[test]
    fn markup_inside_attribute_values_does_not_end_a_tag() {
        // `<a x="/>">` opens an element.
        assert_eq!(depth("<r><a x=\"/>\"><b/></a></r>"), Some(3));
        assert_eq!(depth("<r><a x=\">\"><b/></a></r>"), Some(3));
        assert_eq!(depth("<r><a x=\"<b><c>\"/></r>"), Some(2));
        assert_eq!(depth("<r><a x = \"/>\" y=\"2\"><b/></a></r>"), Some(3));
    }

    #[test]
    fn single_quoted_attribute_values() {
        assert_eq!(depth("<r><a x='/>'><b/></a></r>"), Some(3));
        assert_eq!(depth("<r><a x='\"'><b/></a></r>"), Some(3));
        assert_eq!(depth("<r><a x=\"'\" y='>'><b/></a></r>"), Some(3));
    }

    #[test]
    fn unquoted_attribute_values_are_ordinary() {
        assert_eq!(depth("<rss><a href=x><b/></a></rss>"), Some(3));
        assert_eq!(depth("<rss><a href=x/></rss>"), Some(2));
        assert_eq!(
            depth("<rss><a href=http://x.test/a?b=c><b/></a></rss>"),
            Some(3)
        );
    }

    #[test]
    fn a_quote_anywhere_in_a_tag_hides_the_tag_end_as_quick_xml_reads_it() {
        // quick-xml (feed-rs) treats any quote in a tag as opening a quoted run, so here `a`
        // ends at `d>` and the three `x` are elements. Reading only `="…"` as quoted would put
        // them inside a value and count 2.
        assert_eq!(depth("<r><a b\"c=\"d><x><x><x>\"></r>"), Some(5));
    }

    #[test]
    fn end_tags_close_and_stray_ones_do_not_underflow() {
        assert_eq!(depth("</x></y><a><b/></a>"), Some(2));
        assert_eq!(depth("<a></a></a></a><b><c/></b>"), Some(2));
    }

    #[test]
    fn an_entity_whose_value_holds_markup_is_over() {
        let double = "<!DOCTYPE d [<!ENTITY e \"<x><x></x></x>\">]><d>&e;</d>";
        let single = "<!DOCTYPE d [<!ENTITY e '<x/>'>]><d>&e;</d>";
        let parameter = "<!DOCTYPE d [<!ENTITY % p \"<!ENTITY e '<x/>'>\">]><d/>";
        for xml in [double, single, parameter] {
            assert_eq!(depth(xml), None, "{xml}");
        }
    }

    #[test]
    fn entities_for_characters_and_doctypes_without_a_subset_are_fine() {
        assert_eq!(
            depth(
                "<!DOCTYPE d [<!ENTITY lt2 \"&#60;\"><!ENTITY amp2 '&lt;'><!ENTITY gt2 \">\">]><d>&lt2;</d>"
            ),
            Some(1)
        );
        let rss091 = "<?xml version=\"1.0\"?>\n<!DOCTYPE rss PUBLIC \"-//Netscape Communications//DTD RSS 0.91//EN\" \"http://my.netscape.com/publish/formats/rss-0.91.dtd\">\n<rss version=\"0.91\"><channel><title>x &nbsp; y</title></channel></rss>";
        assert_eq!(depth(rss091), Some(3));
        assert_eq!(depth("<!doctype html><html><body/></html>"), Some(2));
        // A comment, PI or other declaration in the subset may hold quotes and `<`.
        assert_eq!(
            depth(
                "<!DOCTYPE d [<!-- don't <x> --><?p \"<y>\" ?><!ELEMENT d (#PCDATA)><!ATTLIST d a CDATA \"v\">]><d/>"
            ),
            Some(1)
        );
        // `]` and `>` inside a quoted value of the subset do not end it.
        assert_eq!(
            depth("<!DOCTYPE d [<!ENTITY e \"]>\">]><d><e/></d>"),
            Some(2)
        );
    }

    #[test]
    fn the_doctype_does_not_hide_elements_after_it() {
        let deep = nested("<x>", "</x>", 20, "");
        assert_eq!(
            depth(&format!("<!DOCTYPE d [<!ENTITY e \"v\">]>{deep}")),
            Some(20)
        );
        assert_eq!(
            depth(&format!("<!DOCTYPE d SYSTEM \"[>\">{deep}")),
            Some(20)
        );
        // Anything in the subset that is not a declaration, comment or PI is refused:
        // roxmltree stops there, and quick-xml reads on in ways the scan cannot follow.
        assert_eq!(depth(&format!("<!DOCTYPE d [ \" ]>{deep}")), None);
        assert_eq!(depth(&format!("<!DOCTYPE d [ <x> ]>{deep}")), None);
    }

    #[test]
    fn unterminated_constructs_are_over() {
        for xml in [
            "<a><!-- never closed <b>",
            "<a><![CDATA[ never closed",
            "<a><?pi never closed",
            "<a><b x=\"never closed></b></a>",
            "<a><b x='never closed></b></a>",
            "<!DOCTYPE d [<!ENTITY e \"never closed>]><d/>",
            "<!DOCTYPE d [<!ENTITY e \"v\">",
            "<!DOCTYPE d",
        ] {
            assert_eq!(depth(xml), None, "{xml}");
        }
    }

    #[test]
    fn exactly_max_depth_passes_and_one_more_fails() {
        let at = nested("<a>", "</a>", MAX_DEPTH, "text");
        assert!(!xml_depth_over(at.as_bytes(), MAX_DEPTH));
        let over = nested("<a>", "</a>", MAX_DEPTH + 1, "text");
        assert!(xml_depth_over(over.as_bytes(), MAX_DEPTH));
        let leaf_over = nested("<a>", "</a>", MAX_DEPTH, "<b/>");
        assert!(xml_depth_over(leaf_over.as_bytes(), MAX_DEPTH));
    }

    fn latin1_feed(levels: usize) -> Vec<u8> {
        let mut out = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><rss><channel><item><title>Caf\xe9</title><description>".to_vec();
        for _ in 0..levels {
            out.extend_from_slice(b"<media:group>\xe9");
        }
        for _ in 0..levels {
            out.extend_from_slice(b"</media:group>");
        }
        out.extend_from_slice(b"</description></item></channel></rss>");
        out
    }

    fn utf16(text: &str, little: bool, bom: bool) -> Vec<u8> {
        let mut out = Vec::new();
        let mut push = |u: u16| {
            out.extend_from_slice(&if little {
                u.to_le_bytes()
            } else {
                u.to_be_bytes()
            })
        };
        if bom {
            push(0xFEFF);
        }
        text.encode_utf16().for_each(&mut push);
        out
    }

    fn utf16_feed(levels: usize) -> String {
        nested("<media:group>é", "</media:group>", levels, "Grüße — ✓").replace("é", "é😀")
    }

    #[test]
    fn a_deep_latin1_feed_is_refused_and_a_shallow_one_passes() {
        let deep = latin1_feed(5_000);
        assert!(
            std::str::from_utf8(&deep).is_err(),
            "the test feed is not UTF-8"
        );
        assert!(xml_depth_over(&deep, MAX_DEPTH));
        let shallow = latin1_feed(3);
        // rss, channel, item, description, then three groups.
        assert_eq!(depth_bytes(&shallow), Some(4 + 3));
    }

    #[test]
    fn a_deep_utf16_feed_is_refused_and_a_shallow_one_passes() {
        // Without a BOM, UTF-16 is known by the `<?` of a declaration, as quick-xml knows it.
        let feed = |levels, bom| {
            let decl = if bom { "" } else { "<?xml version=\"1.0\"?>" };
            format!("{decl}{}", utf16_feed(levels))
        };
        for (little, bom) in [(true, true), (false, true), (true, false), (false, false)] {
            let deep = utf16(&feed(5_000, bom), little, bom);
            assert!(
                xml_depth_over(&deep, MAX_DEPTH),
                "little {little} bom {bom}"
            );
            let shallow = utf16(&feed(3, bom), little, bom);
            assert_eq!(depth_bytes(&shallow), Some(3), "little {little} bom {bom}");
            // Without the declaration it is read as bytes, NUL and all, and `<\0/` is no end
            // tag there: the count only goes up.
            let bare = utf16(&utf16_feed(5_000), little, false);
            assert!(xml_depth_over(&bare, MAX_DEPTH), "little {little}");
        }
        // In UTF-16 the zero bytes do not hide a `<`, and a `<` byte inside another unit is
        // not a `<`: U+3C3C is one character, not two tags.
        let mut tricky = utf16("<a>", true, true);
        tricky.extend_from_slice(&0x3C3Cu16.to_le_bytes());
        tricky.extend(utf16("</a>", true, false));
        assert_eq!(depth_bytes(&tricky), Some(1));
    }

    #[test]
    fn utf16_with_a_bom_and_an_unterminated_comment_is_over() {
        let xml = utf16("<a><!-- open", true, true);
        assert!(xml_depth_over(&xml, MAX_DEPTH));
    }

    #[test]
    fn invalid_utf8_is_not_a_reason_to_refuse() {
        assert_eq!(depth_bytes(b"<a>\xff\xfe\x80<b/></a>"), Some(2));
    }

    #[test]
    fn a_hundred_thousand_levels_are_refused_on_a_one_mib_stack() {
        let refused = on_stack(1 << 20, || {
            let deep = nested("<a>", "</a>", 100_000, "");
            let over = xml_depth_over(deep.as_bytes(), MAX_DEPTH);
            let parsed = parse_xml(&deep, roxmltree::ParsingOptions::default()).map(|_| ());
            (over, parsed)
        });
        assert_eq!(refused, (true, Err(XmlError::TooDeep)));
    }

    #[test]
    fn parse_xml_gives_the_two_messages_of_the_design() {
        let deep = nested("<a>", "</a>", MAX_DEPTH + 1, "");
        let e = parse_xml(&deep, Default::default()).unwrap_err();
        assert_eq!(e, XmlError::TooDeep);
        assert_eq!(
            e.to_string(),
            "this file is nested more than 256 levels deep"
        );

        let entity = "<!DOCTYPE d [<!ENTITY e \"<x/>\">]><d>&e;</d>";
        let opts = roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        };
        let e = parse_xml(entity, opts).unwrap_err();
        assert_eq!(e, XmlError::EntityMarkup);
        assert_eq!(
            e.to_string(),
            "this file defines an entity that contains markup"
        );
    }

    #[test]
    fn parse_xml_uses_the_callers_options_and_passes_parse_errors_through() {
        let with_dtd = "<!DOCTYPE d [<!ENTITY e \"v\">]><d>&e;</d>";
        assert!(matches!(
            parse_xml(with_dtd, Default::default()),
            Err(XmlError::Parse(roxmltree::Error::DtdDetected))
        ));
        let opts = roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        };
        let doc = parse_xml(with_dtd, opts).unwrap();
        assert_eq!(doc.root_element().text(), Some("v"));

        assert!(matches!(
            parse_xml("<a>", Default::default()),
            Err(XmlError::Parse(_))
        ));
        // An unterminated comment is refused before roxmltree sees it.
        assert_eq!(
            parse_xml("<a><!-- open", Default::default()).map(|_| ()),
            Err(XmlError::Malformed)
        );
        let at = nested("<a>", "</a>", MAX_DEPTH, "");
        let doc = on_stack(64 << 20, move || {
            parse_xml(&at, Default::default()).map(|d| d.descendants().count())
        });
        assert_eq!(doc, Ok(MAX_DEPTH + 1));
    }

    #[test]
    fn roxmltree_reads_markup_from_an_entity_and_keeps_character_references_as_text() {
        // Why the scan refuses `<` in an entity value: roxmltree parses it as elements.
        let opts = || roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        };
        let markup = "<!DOCTYPE d [<!ENTITY e \"<x><y/></x>\">]><d>&e;</d>";
        let doc = roxmltree::Document::parse_with_options(markup, opts()).unwrap();
        assert!(doc.descendants().any(|n| n.has_tag_name("y")));
        // And why `&#60;` and `&lt;` in a value are allowed: they stay text.
        for xml in [
            "<!DOCTYPE d [<!ENTITY e \"&#60;x/>\">]><d>&e;</d>",
            "<!DOCTYPE d [<!ENTITY e \"&#x3C;x/>\">]><d>&e;</d>",
            "<!DOCTYPE d [<!ENTITY e \"&lt;x/>\">]><d>&e;</d>",
        ] {
            let doc = parse_xml(xml, opts()).unwrap();
            assert!(!doc.descendants().any(|n| n.has_tag_name("x")), "{xml}");
            assert_eq!(doc.root_element().text(), Some("<x/>"), "{xml}");
        }
    }

    /// A small xorshift generator, so the test needs no dependency.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
        fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
            items[self.below(items.len() as u64) as usize]
        }
    }

    const ATTRS: &[&str] = &[
        "",
        " x=\"/>\"",
        " x=\">\"",
        " x='/>' y=\"'\"",
        " x='\"'",
        " a = \"1\"",
        " h=\"&lt;b&gt;\"",
        " t=\"a &amp; b\"",
    ];
    const FILLER: &[&str] = &[
        "",
        "text",
        " 1 &gt; 0 > ",
        "<!-- <c><d> > -->",
        "<![CDATA[<c><d> ]] ]]>",
        "<?pi <c> > ?>",
        "<e/>",
        "<e x=\"/>\"/>",
        "<e></e>",
        "&ent;",
        "<f><g/></f>",
    ];

    /// A well-formed document whose deepest chain is `chain` elements, with random siblings,
    /// attributes, comments, CDATA and PIs. A sibling subtree may go one or two levels deeper
    /// than the chain.
    fn random_doc(rng: &mut Rng, chain: usize) -> String {
        let mut s = String::from(
            "<?xml version=\"1.0\"?><!DOCTYPE r [<!-- \"<x>\" --><!ENTITY ent \"a &#60; b\">]>",
        );
        let mut closes = Vec::new();
        for level in 0..chain {
            // Nothing but the root may stand outside the root.
            if level > 0 {
                s.push_str(rng.pick(FILLER));
            }
            let name = format!("n{}", rng.below(3));
            s.push('<');
            s.push_str(&name);
            s.push_str(rng.pick(ATTRS));
            s.push('>');
            s.push_str(rng.pick(FILLER));
            closes.push(name);
        }
        while let Some(name) = closes.pop() {
            s.push_str(rng.pick(FILLER));
            s.push_str("</");
            s.push_str(&name);
            s.push('>');
        }
        s
    }

    fn real_depth(xml: &str) -> usize {
        let opts = roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        };
        let doc = roxmltree::Document::parse_with_options(xml, opts).unwrap();
        doc.descendants()
            .filter(|n| n.is_element())
            .map(|n| n.ancestors().filter(|a| a.is_element()).count())
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn the_verdict_agrees_with_the_depth_roxmltree_builds() {
        let checked = on_stack(64 << 20, || {
            let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
            let mut checked = 0;
            for round in 0..120 {
                let (chain, limits): (usize, Vec<usize>) = if round % 4 == 0 {
                    let c = MAX_DEPTH - 4 + rng.below(8) as usize;
                    (c, vec![MAX_DEPTH])
                } else {
                    let c = 1 + rng.below(24) as usize;
                    (c, (c.saturating_sub(3)..c + 4).collect())
                };
                let xml = random_doc(&mut rng, chain);
                let real = real_depth(&xml);
                for limit in limits {
                    assert_eq!(
                        xml_depth_over(xml.as_bytes(), limit),
                        real > limit,
                        "real depth {real}, limit {limit}: {xml}"
                    );
                    checked += 1;
                }
            }
            checked
        });
        assert!(checked > 300);
    }

    // --- Repros from QA review. Each one crashed a real parser on a small stack while the
    // scan said "not over". ---

    fn dtd_opts<'a>() -> roxmltree::ParsingOptions<'a> {
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        }
    }

    #[test]
    fn repro_attlist_quote_runs_past_roxmltrees_end_of_the_declaration() {
        // roxmltree ends `<!ATTLIST` at the first `>`, quoted or not, then nests 5,000 deep.
        let xml = format!(
            "<!DOCTYPE r [<!ATTLIST r a CDATA \">]><r>{}</r>",
            nested("<a>", "</a>", 5_000, "")
        );
        assert!(xml_depth_over(xml.as_bytes(), MAX_DEPTH));
        let parsed = on_stack(1 << 20, move || parse_xml(&xml, dtd_opts()).map(|_| ()));
        assert_eq!(parsed, Err(XmlError::Malformed));
    }

    #[test]
    fn repro_attlist_quote_hides_an_entity_with_markup() {
        let xml = format!(
            "<!DOCTYPE r [<!ATTLIST r a CDATA ' > <!ENTITY e \"{}\"> <!ATTLIST r b CDATA '>]><r>&e;</r>",
            nested("<x>", "</x>", 5_000, "")
        );
        assert!(xml_depth_over(xml.as_bytes(), MAX_DEPTH));
        let parsed = on_stack(1 << 20, move || parse_xml(&xml, dtd_opts()).map(|_| ()));
        assert!(
            matches!(parsed, Err(XmlError::Malformed | XmlError::EntityMarkup)),
            "{parsed:?}"
        );
    }

    #[test]
    fn markup_inside_any_declaration_is_refused() {
        for xml in [
            "<!DOCTYPE r [<!ELEMENT r <x>]><r/>",
            "<!DOCTYPE r [<!NOTATION n SYSTEM \"<x>\">]><r/>",
            "<!DOCTYPE r [<!ATTLIST r a CDATA '<x>'>]><r/>",
            "<!DOCTYPE r [<!ENTITY e <x>]><r/>",
            "<!DOCTYPE r [<!UNKNOWN <x>]><r/>",
        ] {
            assert_eq!(depth(xml), None, "{xml}");
            assert!(parse_xml(xml, dtd_opts()).is_err(), "{xml}");
        }
        // Declarations without markup still pass.
        assert_eq!(
            depth(
                "<!DOCTYPE r [<!ELEMENT r ANY><!ATTLIST r a CDATA \"v\" b CDATA 'w'><!NOTATION n SYSTEM \"x\">]><r/>"
            ),
            Some(1)
        );
    }

    fn rss_with_groups(levels: usize) -> Vec<u8> {
        let mut out = b"<?xml version=\"1.0\"?><rss version=\"2.0\" xmlns:media=\"http://search.yahoo.com/mrss/\"><channel><item><description>".to_vec();
        out.extend(nested("<media:group>", "</media:group>", levels, "x").into_bytes());
        out.extend_from_slice(b"</description></item></channel></rss>");
        out
    }

    #[test]
    fn repro_a_leading_nul_is_not_utf16() {
        // quick-xml finds BOM-less UTF-16 only from `00 3C 00 3F` or `3C 00 3F 00`, so this is
        // read as bytes, NUL and all.
        let mut xml = b"\0".to_vec();
        xml.extend(rss_with_groups(5_000));
        assert!(xml_depth_over(&xml, MAX_DEPTH));

        let mut xml = b"<\0rss>".to_vec();
        xml.extend(nested("<a>", "</a>", 5_000, "").into_bytes());
        assert!(xml_depth_over(&xml, MAX_DEPTH));
    }

    #[test]
    fn repro_iso_2022_jp_is_refused() {
        // In ISO-2022-JP the bytes `<"` after `ESC $ B` are one character, so the scan cannot
        // read the text as ASCII.
        let mut xml = b"<?xml version=\"1.0\" encoding=\"ISO-2022-JP\"?><rss version=\"2.0\"><channel><item><title>\x1b$B<\"\x1b(B</title>".to_vec();
        xml.extend(nested("<media:group>", "</media:group>", 5_000, "").into_bytes());
        xml.extend_from_slice(b"<title>\x1b$B<\"\x1b(B></title></item></channel></rss>");
        assert!(xml_depth_over(&xml, MAX_DEPTH));
        let shallow = b"<?xml version='1.0' encoding='iso-2022-jp'?><rss/>";
        assert!(xml_depth_over(shallow, MAX_DEPTH), "refused at any depth");
    }

    #[test]
    fn encodings_the_scan_cannot_read_are_refused() {
        let decl =
            |enc: &str| format!("<?xml version=\"1.0\" encoding=\"{enc}\"?><rss><channel/></rss>");
        // The "replacement" labels, and UTF-16 named on bytes that are not UTF-16.
        for enc in [
            "ISO-2022-KR",
            "hz-gb-2312",
            "replacement",
            "UTF-16",
            " utf-16le ",
            "UTF-16BE",
            "unicodeFFFE",
            "ucs-2",
            "csiso2022jp",
        ] {
            assert!(xml_depth_over(decl(enc).as_bytes(), MAX_DEPTH), "{enc}");
        }
        // ASCII-compatible encodings are read as bytes.
        for enc in [
            "UTF-8",
            "ISO-8859-1",
            "windows-1252",
            "Shift_JIS",
            "EUC-JP",
            "GB18030",
            "Big5",
            "x-unknown",
        ] {
            assert!(!xml_depth_over(decl(enc).as_bytes(), MAX_DEPTH), "{enc}");
        }
        // UTF-16 bytes must name a UTF-16 encoding of the same byte order, or none.
        let doc =
            |enc: &str| format!("<?xml version=\"1.0\" encoding=\"{enc}\"?><rss><a>é</a></rss>");
        assert!(!xml_depth_over(
            &utf16(&doc("UTF-16"), true, true),
            MAX_DEPTH
        ));
        assert!(!xml_depth_over(
            &utf16(&doc("UTF-16LE"), true, false),
            MAX_DEPTH
        ));
        assert!(!xml_depth_over(
            &utf16(&doc("UTF-16BE"), false, true),
            MAX_DEPTH
        ));
        assert!(!xml_depth_over(
            &utf16(&doc("utf-16be"), false, false),
            MAX_DEPTH
        ));
        assert!(!xml_depth_over(
            &utf16("<?xml version=\"1.0\"?><rss/>", false, false),
            MAX_DEPTH
        ));
        for (enc, little, bom) in [
            ("UTF-8", true, true),
            ("ISO-8859-1", false, true),
            ("UTF-16", false, true),
            ("UTF-16BE", true, false),
        ] {
            assert!(
                xml_depth_over(&utf16(&doc(enc), little, bom), MAX_DEPTH),
                "{enc} little {little} bom {bom}"
            );
        }
        // A later `<?xml` with an encoding counts too.
        assert!(xml_depth_over(
            b"<rss><?xml version=\"1.0\" encoding=\"ISO-2022-JP\"?></rss>",
            MAX_DEPTH
        ));
    }

    #[test]
    fn parse_xml_does_not_check_the_declared_encoding() {
        // Its text is already UTF-8, whatever the declaration says.
        let xml = "<?xml version=\"1.0\" encoding=\"ISO-2022-JP\"?><r><a/></r>";
        assert!(parse_xml(xml, Default::default()).is_ok());
    }

    #[test]
    fn repro_a_shift_jis_lead_byte_does_not_open_a_doctype_subset() {
        // `\x81[` is one Shift_JIS character, so quick-xml ends the DOCTYPE at the `>` after
        // it; a scan that took `[` for the subset would skip the elements up to `]>`.
        let mut xml =
            b"<?xml version=\"1.0\" encoding=\"Shift_JIS\"?><!DOCTYPE r\x81[><r>".to_vec();
        xml.extend(nested("<a>", "</a>", 5_000, "").into_bytes());
        xml.extend_from_slice(b"</r>]>");
        assert!(xml_depth_over(&xml, MAX_DEPTH));
    }

    /// A generated document: a chain of nested elements with siblings, attributes in both
    /// quotes, comments, CDATA, PIs, and a DOCTYPE with ENTITY, ATTLIST, NOTATION and ELEMENT
    /// declarations. All of it Latin-1, so it can be encoded every way.
    fn varied_doc(rng: &mut Rng, chain: usize, encoding: &str) -> String {
        const DECLS: &[&str] = &[
            "<!ENTITY ent \"a &#60; b\">",
            "<!ENTITY ent 'x é y'>",
            "<!ENTITY mk \"<m><m/></m>\">",
            "<!ATTLIST n0 a CDATA \"v\">",
            "<!ATTLIST n1 b CDATA 'w' c CDATA \"q'q\">",
            "<!NOTATION gif SYSTEM \"image/gif\">",
            "<!ELEMENT n2 ANY>",
            "<!-- \"<x>\" ' -->",
            "<?pi in subset?>",
        ];
        const ATTRS: &[&str] = &[
            "",
            " x=\"/>\"",
            " x='>'",
            " x='\"' y=\"'\"",
            " a = \"1\"",
            " h=\"&lt;b&gt; é\"",
        ];
        const FILLER: &[&str] = &[
            "",
            "té xt",
            " 1 &gt; 0 > ] ]] ",
            "<!-- <c><d> > ' \" -->",
            "<![CDATA[<c><d> ]] ' \" ]]>",
            "<?pi <c> > ?>",
            "<e/>",
            "<e x=\"/>\" y='\"'/>",
            "&ent;",
            "&mk;",
            "<f><g/></f>",
        ];
        let mut s = format!("<?xml version=\"1.0\" encoding=\"{encoding}\"?><!DOCTYPE n0 [");
        for _ in 0..rng.below(5) {
            s.push_str(rng.pick(DECLS));
        }
        // roxmltree uses the first definition, so these only fill in what DECLS left out.
        s.push_str("<!ENTITY ent \"e\"><!ENTITY mk \"k\">]>");
        let mut closes = Vec::new();
        for level in 0..chain {
            if level > 0 {
                s.push_str(rng.pick(FILLER));
            }
            let name = format!("n{}", rng.below(3));
            s.push_str(&format!("<{name}{}>", rng.pick(ATTRS)));
            s.push_str(rng.pick(FILLER));
            closes.push(name);
        }
        while let Some(name) = closes.pop() {
            s.push_str(rng.pick(FILLER));
            s.push_str(&format!("</{name}>"));
        }
        s
    }

    /// Inserts up to three markup characters at random places; a third of the time none.
    fn mutate(rng: &mut Rng, s: &str) -> String {
        let mut chars: Vec<char> = s.chars().collect();
        for _ in 0..rng.below(4) {
            let at = rng.below(chars.len() as u64 + 1) as usize;
            let c =
                ['"', '\'', '<', '>', ']', '[', '/', '-', '?', '!', '='][rng.below(11) as usize];
            chars.insert(at, c);
        }
        chars.into_iter().collect()
    }

    /// The deepest element nesting roxmltree builds, entities expanded; None when it refuses.
    fn roxml_depth(xml: &str) -> Option<usize> {
        let doc = roxmltree::Document::parse_with_options(xml, dtd_opts()).ok()?;
        doc.descendants()
            .filter(|n| n.is_element())
            .map(|n| n.ancestors().filter(|a| a.is_element()).count())
            .max()
    }

    #[test]
    fn differential_the_scan_never_passes_what_roxmltree_nests_past_the_limit() {
        // (encoding label, how to encode)
        type Enc = fn(&str) -> Vec<u8>;
        let encodings: [(&str, Enc); 6] = [
            ("UTF-8", |s| s.as_bytes().to_vec()),
            ("ISO-8859-1", |s| {
                s.chars().map(|c| c as u32 as u8).collect()
            }),
            ("UTF-16", |s| utf16(s, true, true)),
            ("UTF-16BE", |s| utf16(s, false, true)),
            ("UTF-16LE", |s| utf16(s, true, false)),
            ("UTF-16BE", |s| utf16(s, false, false)),
        ];
        let (parsed, deep) = on_stack(64 << 20, move || {
            let mut rng = Rng(0x2545_F491_4F6C_DD1D);
            let (mut parsed, mut deep) = (0, 0);
            for round in 0..4_000 {
                let chain = if round % 50 == 0 {
                    MAX_DEPTH - 2 + rng.below(4) as usize
                } else {
                    1 + rng.below(12) as usize
                };
                let (label, encode) = encodings[round % encodings.len()];
                let doc = varied_doc(&mut rng, chain, label);
                let doc = mutate(&mut rng, &doc);
                let Some(real) = roxml_depth(&doc) else {
                    continue;
                };
                parsed += 1;
                let bytes = encode(&doc);
                for limit in [real.saturating_sub(1), real.saturating_sub(2)] {
                    if real > limit {
                        assert!(
                            xml_depth_over(&bytes, limit),
                            "{label}: real depth {real}, limit {limit}, scan passed: {doc}"
                        );
                    }
                }
                if real > MAX_DEPTH {
                    deep += 1;
                    assert!(
                        !matches!(parse_xml(&doc, dtd_opts()), Ok(_) | Err(XmlError::Parse(_))),
                        "parse_xml let roxmltree nest {real} deep: {doc}"
                    );
                }
            }
            (parsed, deep)
        });
        assert!(parsed > 500, "only {parsed} documents parsed");
        assert!(deep > 0);
    }

    // --- Second QA round. ---

    #[test]
    fn a_parameter_entity_reference_in_the_subset_is_allowed() {
        let xml = "<?xml version=\"1.0\"?><!DOCTYPE rss [<!ENTITY % HTMLlat1 PUBLIC \"-//W3C//ENTITIES Latin 1 for XHTML//EN\" \"http://www.w3.org/TR/xhtml1/DTD/xhtml-lat1.ent\"> %HTMLlat1; ]><rss version=\"2.0\"><channel><title>T</title><item><title>A</title><description>x</description></item></channel></rss>";
        assert!(!xml_depth_over(xml.as_bytes(), MAX_DEPTH));
        assert_eq!(depth(xml), Some(4));
        // A `%` that is not a reference is still refused.
        for bad in [
            "<!DOCTYPE r [ % ]><r/>",
            "<!DOCTYPE r [ %name ]><r/>",
            "<!DOCTYPE r [ %;]><r/>",
            "<!DOCTYPE r [ %a<b>; ]><r/>",
        ] {
            assert_eq!(depth(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_declaration_is_read_attribute_by_attribute() {
        for xml in [
            // `encoding` inside another value is not the encoding.
            "<?xml version=\"1.0 encoding='utf-8'\" encoding=\"ISO-2022-JP\"?><r/>",
            // Nor is an attribute whose name only ends in it.
            "<?xml version=\"1.0\" xencoding=\"utf-8\" encoding=\"ISO-2022-JP\"?><r/>",
            // Twice is refused, whatever the values.
            "<?xml version=\"1.0\" encoding=\"utf-8\" encoding=\"utf-8\"?><r/>",
            // An attribute the scan cannot read is refused.
            "<?xml version=\"1.0\" encoding=utf-8?><r/>",
            "<?xml version=\"1.0\" encoding \"utf-8\"?><r/>",
        ] {
            assert!(xml_depth_over(xml.as_bytes(), MAX_DEPTH), "{xml}");
        }
        for xml in [
            "<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"yes\"?><r/>",
            "<?xml version='1.0' encoding = 'ISO-8859-1' ?><r/>",
            "<?xml version=\"1.0\"?><r/>",
        ] {
            assert!(!xml_depth_over(xml.as_bytes(), MAX_DEPTH), "{xml}");
        }
    }

    #[test]
    fn multibyte_encodings_refuse_a_bracket_after_a_high_byte() {
        let doc = |enc: &str, body: &[u8]| {
            let mut out = format!(
                "<?xml version=\"1.0\" encoding=\"{enc}\"?><!DOCTYPE rss PUBLIC \"x\" \"y\">"
            )
            .into_bytes();
            out.extend_from_slice(body);
            out
        };
        for enc in [
            "Shift_JIS",
            "sjis",
            "windows-31j",
            "x-sjis",
            "ms_kanji",
            "csshiftjis",
            "MS932",
            "big5",
            "big5-hkscs",
            "cn-big5",
            "csbig5",
            "x-x-big5",
            "GBK",
            "gb2312",
            "chinese",
            "csgb2312",
            "csiso58gb231280",
            "gb_2312",
            "gb_2312-80",
            "iso-ir-58",
            "x-gbk",
            "GB18030",
        ] {
            for body in [&b"<rss>\x81[</rss>"[..], b"<rss><a t=\"\xa4]\"/></rss>"] {
                assert!(xml_depth_over(&doc(enc, body), MAX_DEPTH), "{enc}");
            }
            // The same encodings without such a pair pass.
            assert!(
                !xml_depth_over(&doc(enc, b"<rss>\x82\xa0 [x] \xa4\x40</rss>"), MAX_DEPTH),
                "{enc}"
            );
        }
        // UTF-8 never has a bracket inside a character, so `é[` is fine there.
        assert!(!xml_depth_over(
            &doc("UTF-8", "<rss>é[x]é]</rss>".as_bytes()),
            MAX_DEPTH
        ));
        assert!(!xml_depth_over("<rss>é[x]é]</rss>".as_bytes(), MAX_DEPTH));
    }
}
