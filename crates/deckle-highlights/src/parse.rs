// SPDX-License-Identifier: GPL-3.0-or-later
//! `My Clippings.txt` (highlights export design §4).

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Kind {
    Highlight,
    Note,
    Bookmark,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Added {
    pub raw: String,
    pub date: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Entry {
    pub title: String,
    pub author: String,
    pub kind: Kind,
    pub page: Option<String>,
    pub location: Option<(u32, u32)>,
    pub added: Added,
    pub text: String,
    pub raw_meta: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Parsed {
    pub entries: Vec<Entry>,
    pub error: Option<ParseError>,
}

const SEPARATOR: &str = "==========";

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Every entry of a clippings file. Bytes that aren't UTF-8 are replaced, not fatal; the first
/// malformed entry is reported in `error` and parsing carries on with the next one.
pub fn parse(bytes: &[u8]) -> Parsed {
    let text = String::from_utf8_lossy(bytes);
    let text = text
        .strip_prefix('\u{FEFF}')
        .unwrap_or(&text)
        .replace("\r\n", "\n");
    let mut parsed = Parsed::default();
    let mut block: Vec<(usize, &str)> = Vec::new();
    for (i, line) in text.split('\n').enumerate() {
        if line.trim_end() == SEPARATOR {
            entry(&block, &mut parsed);
            block.clear();
        } else {
            block.push((i + 1, line));
        }
    }
    entry(&block, &mut parsed);
    parsed
}

fn entry(block: &[(usize, &str)], parsed: &mut Parsed) {
    let lines: Vec<(usize, &str)> = block
        .iter()
        .copied()
        .skip_while(|(_, l)| l.trim().is_empty())
        .collect();
    let Some(&(title_line, title)) = lines.first() else {
        return;
    };
    let meta = match lines.get(1) {
        Some((_, m)) if m.trim_start().starts_with("- ") => m.trim(),
        other => {
            let line = other.map_or(title_line, |(n, _)| *n);
            if parsed.error.is_none() {
                parsed.error = Some(ParseError {
                    line,
                    message: "expected a \"- Your Highlight …\" line".into(),
                });
            }
            return;
        }
    };
    let text = lines[2..]
        .iter()
        .map(|(_, l)| *l)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned();
    let (title, author) = title_author(title.trim());
    let parts: Vec<&str> = meta[2..].split('|').map(str::trim).collect();
    let first = parts.first().copied().unwrap_or_default();
    let kind = if first.contains("Highlight") {
        Some(Kind::Highlight)
    } else if first.contains("Note") {
        Some(Kind::Note)
    } else if first.contains("Bookmark") {
        Some(Kind::Bookmark)
    } else {
        None
    };
    let added = parts
        .iter()
        .find_map(|p| p.strip_prefix("Added on "))
        .map(|raw| Added {
            raw: raw.to_owned(),
            date: english_date(raw),
        })
        .unwrap_or_else(|| Added {
            raw: parts.last().copied().unwrap_or_default().to_owned(),
            date: None,
        });
    parsed.entries.push(Entry {
        title,
        author,
        kind: kind.unwrap_or(if text.is_empty() {
            Kind::Bookmark
        } else {
            Kind::Highlight
        }),
        page: after_word(first, "page ").map(str::to_owned),
        location: parts.iter().find_map(|p| location(p)),
        added,
        text,
        raw_meta: kind.is_none().then(|| meta.to_owned()),
    });
}

/// `Title (Author)`: the author is the last parenthesised group, matched from the end so a
/// title may hold parentheses of its own.
fn title_author(line: &str) -> (String, String) {
    if line.ends_with(')') {
        let mut depth = 0;
        for (i, c) in line.char_indices().rev() {
            match c {
                ')' => depth += 1,
                '(' => {
                    depth -= 1;
                    if depth == 0 {
                        let title = line[..i].trim();
                        if !title.is_empty() {
                            return (
                                title.to_owned(),
                                line[i + 1..line.len() - 1].trim().to_owned(),
                            );
                        }
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    (line.to_owned(), String::new())
}

/// The word after `word` in `s`, if any.
fn after_word<'a>(s: &'a str, word: &str) -> Option<&'a str> {
    let at = s.find(word)? + word.len();
    s[at..].split_whitespace().next()
}

/// `Location 1340-1342`, `at location 220`.
fn location(part: &str) -> Option<(u32, u32)> {
    let at = part.find("ocation ")?;
    if !matches!(part[..at].chars().last(), Some('L' | 'l')) {
        return None;
    }
    let spec = part[at + "ocation ".len()..].split_whitespace().next()?;
    let (a, b) = spec.split_once('-').unwrap_or((spec, spec));
    let a: u32 = a.parse().ok()?;
    Some((a, b.parse().unwrap_or(a)))
}

/// `Monday, March 3, 2025 10:14:22 AM` → `2025-03-03`.
fn english_date(raw: &str) -> Option<String> {
    let rest = raw.split_once(", ")?.1;
    let mut words = rest.split_whitespace();
    let month = words.next()?;
    let month = MONTHS.iter().position(|m| *m == month)? + 1;
    let day: u32 = words.next()?.trim_end_matches(',').parse().ok()?;
    let year: u32 = words.next()?.parse().ok()?;
    (1..=31)
        .contains(&day)
        .then(|| format!("{year:04}-{month:02}-{day:02}"))
}
