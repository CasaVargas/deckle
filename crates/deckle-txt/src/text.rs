// SPDX-License-Identifier: GPL-3.0-or-later
//! From decoded text to book items: the Project Gutenberg frame, blocks, headings, paragraphs.

/// What a Gutenberg header says about the book.
#[derive(Debug, Default)]
pub struct Header {
    pub title: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
}

/// The text between the Gutenberg start and end markers, and the header before the start; the
/// whole text and an empty header when there is no start marker.
pub fn gutenberg(text: &str) -> (&str, Header) {
    let mut header = Header::default();
    let marker = |line: &str, which: &str| {
        let up = line.trim().to_uppercase();
        ["THE", "THIS"]
            .iter()
            .any(|t| up.starts_with(&format!("*** {which} OF {t} PROJECT GUTENBERG EBOOK")))
    };
    let mut offset = 0;
    let mut start = None;
    for line in text.split_inclusive('\n') {
        offset += line.len();
        if marker(line, "START") {
            start = Some(offset);
            break;
        }
        let field = |name: &str| {
            line.split_once(':')
                .filter(|(k, _)| k.trim().eq_ignore_ascii_case(name))
                .map(|(_, v)| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        header.title = header.title.take().or_else(|| field("Title"));
        header.author = header.author.take().or_else(|| field("Author"));
        header.language = header.language.take().or_else(|| field("Language"));
    }
    let Some(start) = start else {
        return (text, Header::default());
    };
    let body = &text[start..];
    let mut end = body.len();
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        if marker(line, "END") {
            end = offset;
            break;
        }
        offset += line.len();
    }
    (&body[..end], header)
}

/// A Gutenberg `Language:` value as a BCP 47 tag.
pub fn language_tag(name: &str) -> Option<String> {
    let lower = name.trim().to_lowercase();
    let tag = match lower.as_str() {
        "english" => "en",
        "french" => "fr",
        "german" => "de",
        "spanish" => "es",
        "italian" => "it",
        "portuguese" => "pt",
        "dutch" => "nl",
        "finnish" => "fi",
        "swedish" => "sv",
        "danish" => "da",
        "norwegian" => "no",
        "latin" => "la",
        "greek" => "el",
        "russian" => "ru",
        "polish" => "pl",
        "hungarian" => "hu",
        "czech" => "cs",
        "esperanto" => "eo",
        "chinese" => "zh",
        "japanese" => "ja",
        "catalan" => "ca",
        "welsh" => "cy",
        "irish" => "ga",
        _ if (2..=3).contains(&lower.len()) && lower.chars().all(|c| c.is_ascii_lowercase()) => {
            return Some(lower);
        }
        _ => return None,
    };
    Some(tag.to_owned())
}

#[derive(Debug, PartialEq, Eq)]
pub enum Item {
    Heading {
        level: u8,
        text: String,
        subtitle: Option<String>,
    },
    /// Paragraph HTML, already escaped.
    Para(String),
    /// Short lines joined by `<br/>`, already escaped.
    Verse(String),
    Scene,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// Blank lines between paragraphs, lines wrapped at 50–80 characters: join them.
    HardWrapped,
    /// No blank lines, or mostly single-line blocks: each line is a paragraph.
    PerLine,
    /// Anything else: short-line blocks keep their breaks.
    Other,
}

/// Blocks of lines between blank lines, each line trimmed.
fn blocks(text: &str) -> Vec<Vec<&str>> {
    let mut out: Vec<Vec<&str>> = Vec::new();
    let mut current = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn len(s: &str) -> usize {
    s.chars().count()
}

fn mode(blocks: &[Vec<&str>]) -> Mode {
    let multi: Vec<&str> = blocks
        .iter()
        .filter(|b| b.len() > 1)
        .flatten()
        .copied()
        .collect();
    let wrapped = multi.iter().filter(|l| (50..=80).contains(&len(l))).count();
    let has_blank = blocks.len() > 1;
    if has_blank && !multi.is_empty() && wrapped * 10 >= multi.len() * 6 {
        return Mode::HardWrapped;
    }
    let single = blocks.iter().filter(|b| b.len() == 1).count();
    if !has_blank || single * 2 > blocks.len() {
        Mode::PerLine
    } else {
        Mode::Other
    }
}

fn mostly_short(lines: &[&str]) -> bool {
    lines.iter().filter(|l| len(l) < 50).count() * 2 > lines.len()
}

/// Lines joined with spaces; a line-end hyphen between two lowercase letters stays and closes up.
fn join(lines: &[&str]) -> String {
    let mut out = String::new();
    for line in lines {
        let hyphenated = out.ends_with('-')
            && out.chars().rev().nth(1).is_some_and(|c| c.is_lowercase())
            && line.chars().next().is_some_and(|c| c.is_lowercase());
        if !out.is_empty() && !hyphenated {
            out.push(' ');
        }
        out.push_str(line);
    }
    out
}

fn is_scene_break(line: &str) -> bool {
    let marks = line.chars().filter(|c| !c.is_whitespace()).count();
    marks >= 3
        && line
            .chars()
            .all(|c| c.is_whitespace() || matches!(c, '*' | '#' | '~' | '-'))
}

const KEYWORDS: [&str; 10] = [
    "chapter",
    "part",
    "book",
    "volume",
    "prologue",
    "epilogue",
    "preface",
    "introduction",
    "afterword",
    "appendix",
];
const PARTS: [&str; 3] = ["part", "book", "volume"];

/// `Some(is_part)` when the line is a heading by the patterns in spec §2.
fn heading(line: &str) -> Option<bool> {
    if len(line) > 60 {
        return None;
    }
    let word_end = line
        .find(|c: char| !c.is_alphabetic())
        .unwrap_or(line.len());
    let word = line[..word_end].to_lowercase();
    if KEYWORDS.contains(&word.as_str()) {
        let rest = &line[word_end..];
        let after = if rest.starts_with(char::is_whitespace) {
            let rest = rest.trim_start();
            let token_end = rest
                .find(|c: char| !(c.is_alphanumeric() || c == '-'))
                .unwrap_or(rest.len());
            let token = &rest[..token_end];
            let number = !token.is_empty() && token.chars().all(|c| c.is_ascii_digit());
            let word = !token.is_empty() && token.chars().all(|c| c.is_alphabetic() || c == '-');
            if token.is_empty() || number || word {
                rest[token_end..].trim_start()
            } else {
                return None;
            }
        } else {
            rest
        };
        let subtitled = after.is_empty() || after.starts_with(['.', ':', '—', '–']);
        return subtitled.then(|| PARTS.contains(&word.as_str()));
    }
    let bare = line.strip_suffix('.').unwrap_or(line);
    let number = !bare.is_empty() && bare.len() <= 4 && bare.chars().all(|c| c.is_ascii_digit());
    (number || roman(bare).is_some_and(|n| (1..=200).contains(&n))).then_some(false)
}

/// The value of a canonical Roman numeral, all upper or all lower case.
fn roman(s: &str) -> Option<u32> {
    if s.is_empty() || !(s == s.to_uppercase() || s == s.to_lowercase()) {
        return None;
    }
    let up = s.to_uppercase();
    let value = |c| match c {
        'I' => Some(1),
        'V' => Some(5),
        'X' => Some(10),
        'L' => Some(50),
        'C' => Some(100),
        'D' => Some(500),
        'M' => Some(1000),
        _ => None,
    };
    let digits: Vec<u32> = up.chars().map(value).collect::<Option<_>>()?;
    let mut n = 0;
    for (i, d) in digits.iter().enumerate() {
        if digits.get(i + 1).is_some_and(|next| next > d) {
            n -= *d as i64;
        } else {
            n += *d as i64;
        }
    }
    let n = u32::try_from(n).ok()?;
    (to_roman(n) == up).then_some(n)
}

fn to_roman(mut n: u32) -> String {
    const TABLE: [(u32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (v, s) in TABLE {
        while n >= v {
            out.push_str(s);
            n -= v;
        }
    }
    out
}

fn all_caps(line: &str) -> bool {
    len(line) <= 60
        && line.chars().filter(|c| c.is_alphabetic()).count() >= 3
        && line
            .chars()
            .filter(|c| c.is_alphabetic())
            .all(char::is_uppercase)
}

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

/// The book body as headings, paragraphs and scene breaks.
pub fn items(text: &str) -> Vec<Item> {
    let blocks = blocks(text);
    let mode = mode(&blocks);
    let is_heading = |b: Option<&Vec<&str>>| {
        b.and_then(|b| b.first().copied())
            .is_some_and(|l| !is_scene_break(l) && heading(l).is_some())
            && b.is_some_and(|b| b.len() == 1)
    };
    let mut out = Vec::new();
    let mut in_parts = false;
    let mut subtitle: Option<String> = None;
    for (i, block) in blocks.iter().enumerate() {
        if let [line] = block[..] {
            if is_scene_break(line) {
                out.push(Item::Scene);
                continue;
            }
            if let Some(is_part) = heading(line) {
                in_parts |= is_part;
                let level = if is_part || !in_parts { 1 } else { 2 };
                out.push(Item::Heading {
                    level,
                    text: line.to_owned(),
                    subtitle: subtitle.take(),
                });
                continue;
            }
            if all_caps(line) && is_heading(blocks.get(i + 1)) {
                subtitle = Some(line.to_owned());
                continue;
            }
            out.push(Item::Para(escape(line)));
            continue;
        }
        let lines: Vec<String> = block.iter().map(|l| escape(l)).collect();
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        let short = mostly_short(block);
        if mode == Mode::HardWrapped && !short {
            out.push(Item::Para(join(&lines)));
        } else if mode != Mode::PerLine && short {
            out.push(Item::Verse(lines.join("<br/>")));
        } else {
            out.extend(lines.iter().map(|l| Item::Para((*l).to_owned())));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_patterns() {
        for yes in [
            "CHAPTER I.",
            "Chapter 12",
            "Chapter Twenty-One",
            "Chapter II — The Ink",
            "Chapter One: Beginnings",
            "Prologue",
            "PART ONE",
            "XIV.",
            "cc",
            "42",
        ] {
            assert!(heading(yes).is_some(), "{yes}");
        }
        for no in [
            "Chapter of accidents, said the doctor.",
            "Introduction to the problem",
            "Chapters",
            "MCMXCIX",
            "CCI",
            "IIII",
            "Did",
            "Xi",
            "",
        ] {
            assert!(heading(no).is_none(), "{no}");
        }
        assert_eq!(heading("Book the Second"), None);
        assert_eq!(heading("Volume 2"), Some(true));
        assert_eq!(heading("Epilogue."), Some(false));
    }

    #[test]
    fn scene_breaks() {
        assert!(is_scene_break("* * *"));
        assert!(is_scene_break("---"));
        assert!(is_scene_break("~~~~"));
        assert!(!is_scene_break("**"));
        assert!(!is_scene_break("* a *"));
    }

    #[test]
    fn hyphens_close_up_only_between_lowercase_letters() {
        assert_eq!(join(&["a well-", "known habit"]), "a well-known habit");
        assert_eq!(join(&["the 1990-", "2000 range"]), "the 1990- 2000 range");
        assert_eq!(join(&["Anglo-", "Saxon"]), "Anglo- Saxon");
    }

    #[test]
    fn gutenberg_without_markers_is_the_whole_text() {
        let (body, h) = gutenberg("Title: Not a header\n\nJust text.\n");
        assert_eq!(body, "Title: Not a header\n\nJust text.\n");
        assert!(h.title.is_none());
    }

    #[test]
    fn gutenberg_markers_any_case_and_this() {
        let t = "Author: Someone\n*** start of this project gutenberg ebook x ***\nBody\n\
                 *** End Of This Project Gutenberg EBook x ***\nLicence\n";
        let (body, h) = gutenberg(t);
        assert_eq!(body, "Body\n");
        assert_eq!(h.author.as_deref(), Some("Someone"));
    }

    #[test]
    fn languages() {
        assert_eq!(language_tag("English").as_deref(), Some("en"));
        assert_eq!(language_tag("FR").as_deref(), Some("fr"));
        assert_eq!(language_tag("Klingon"), None);
    }
}
