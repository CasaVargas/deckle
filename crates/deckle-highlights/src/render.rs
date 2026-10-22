// SPDX-License-Identifier: GPL-3.0-or-later
//! The Markdown Deckle writes (highlights export design §5).

use crate::book::{Book, Item, edition_date};
use crate::parse::Entry;

/// How many words of a passage a later note quotes.
const QUOTE_WORDS: usize = 8;

/// The start of a passage, for a note that can't sit under it.
pub fn passage_start(text: &str) -> String {
    text.split_whitespace()
        .take(QUOTE_WORDS)
        .collect::<Vec<_>>()
        .join(" ")
}

fn yaml(s: &str) -> String {
    if s.contains([':', '#', '"']) || s.starts_with(['-', '[', '{', '\'', '&', '*', '!', '|', '>'])
    {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_owned()
    }
}

/// A new file: frontmatter, the title, then every item.
pub fn render_new_file(book: &Book) -> String {
    let mut out = format!("---\ntitle: {}\n", yaml(&book.title));
    if !book.author.is_empty() {
        out += &format!("author: {}\n", yaml(&book.author));
    }
    out += &format!(
        "source: kindle\ntags: [highlights]\n---\n\n# {}\n\n",
        book.title
    );
    out += &render_items(book, &book.items.iter().collect::<Vec<_>>());
    out
}

fn location(e: &Entry) -> Option<String> {
    e.location.map(|(a, b)| match a == b {
        true => format!("location {a}"),
        false => format!("location {a}–{b}"),
    })
}

/// `- page 88 · location 1338–1345 · 2025-03-04`, or the Kindle's own line when Deckle couldn't
/// read it.
fn meta(e: &Entry) -> String {
    if let Some(raw) = &e.raw_meta {
        return raw.clone();
    }
    let date = e.added.date.clone().or_else(|| {
        let raw = e.added.raw.trim();
        (!raw.is_empty()).then(|| raw.to_owned())
    });
    let parts: Vec<String> = [
        e.page.as_ref().map(|p| format!("page {p}")),
        location(e),
        date,
    ]
    .into_iter()
    .flatten()
    .collect();
    format!("- {}", parts.join(" · "))
}

fn quote(text: &str) -> String {
    text.lines()
        .map(|l| match l.trim_end() {
            "" => ">".to_owned(),
            l => format!("> {l}"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn note(e: &Entry, of: Option<&str>) -> String {
    let at = location(e).or_else(|| e.page.as_ref().map(|p| format!("page {p}")));
    let text = e.text.replace('\n', " ");
    match (of, at) {
        (Some(of), Some(at)) => format!("- Note on “{}…” ({at}): {text}\n", passage_start(of)),
        (Some(of), None) => format!("- Note on “{}…”: {text}\n", passage_start(of)),
        (None, Some(at)) => format!("- Note ({at}): {text}\n"),
        (None, None) => format!("- Note: {text}\n"),
    }
}

/// `items` of `book`, as appended to its file. Edition books get a `## <date>` heading whenever
/// the edition changes.
pub fn render_items(book: &Book, items: &[&Item]) -> String {
    let mut out = String::new();
    let mut edition: Option<&str> = None;
    for item in items {
        let e = item.entry();
        if book.editions {
            let date = edition_date(e);
            if date.is_some() && date != edition {
                out += &format!("## {}\n\n", date.unwrap_or_default());
                edition = date;
            }
        }
        match item {
            Item::Highlight { entry, notes } => {
                out += &format!("{}\n\n{}\n", quote(&entry.text), meta(entry));
                for n in notes {
                    out += &format!("- Note: {}\n", n.text.replace('\n', " "));
                }
            }
            Item::Note { entry, of } => out += &note(entry, of.as_deref()),
        }
        out += "\n";
    }
    out
}
