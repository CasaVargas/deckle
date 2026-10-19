// SPDX-License-Identifier: GPL-3.0-or-later
//! Stage 3: find a real Amazon ASIN candidate without scraping Amazon. Amazon reuses the
//! print edition's ISBN-10 as its ASIN, so an ISBN is enough; without one we ask Open Library.

use crate::inspect::BookInfo;

#[derive(Clone, Debug, PartialEq)]
pub enum MatchOutcome {
    Candidate {
        asin: String,
        confidence: f32,
        source: &'static str,
    },
    NotFound,
    Error(String),
}

/// A blocking fetch of a URL's body; injected so tests stay offline.
pub type Fetcher<'a> = dyn FnMut(&str) -> Result<String, String> + 'a;

/// ISBN-13 (978 prefix) → ISBN-10; ISBN-10 passes through; anything else is `None`.
pub fn isbn13_to_isbn10(isbn: &str) -> Option<String> {
    let digits: Vec<char> = isbn
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == 'X' || *c == 'x')
        .collect();
    if digits.len() == 10 {
        return Some(digits.iter().collect::<String>().to_uppercase());
    }
    if digits.len() != 13 || !digits.iter().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if digits[..3] != ['9', '7', '8'] {
        return None;
    }
    let core: Vec<u32> = digits[3..12]
        .iter()
        .map(|c| c.to_digit(10).unwrap())
        .collect();
    let sum: u32 = core
        .iter()
        .enumerate()
        .map(|(i, d)| d * (10 - i as u32))
        .sum();
    let check = (11 - (sum % 11)) % 11;
    let check_char = if check == 10 {
        'X'
    } else {
        char::from_digit(check, 10).unwrap()
    };
    Some(format!(
        "{}{}",
        core.iter()
            .map(|d| char::from_digit(*d, 10).unwrap())
            .collect::<String>(),
        check_char
    ))
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn open_library_url(title: &str, author: &str) -> String {
    format!(
        "https://openlibrary.org/search.json?title={}&author={}&fields=key,title,author_name,isbn&limit=5",
        urlencode(title),
        urlencode(author)
    )
}

/// Looks for an ASIN candidate: the book's own ISBN first, else an Open Library search.
pub fn match_book(info: &BookInfo, fetch: &mut Fetcher) -> MatchOutcome {
    if let Some(isbn) = info.isbn.as_deref().and_then(isbn13_to_isbn10) {
        return MatchOutcome::Candidate {
            asin: isbn,
            confidence: 0.9,
            source: "isbn",
        };
    }
    let author = info.creators.first().map(|c| c.name.as_str()).unwrap_or("");
    let url = open_library_url(&info.title, author);
    let body = match fetch(&url) {
        Ok(b) => b,
        Err(e) => return MatchOutcome::Error(e),
    };
    let parsed: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => {
            return MatchOutcome::Error(format!("Open Library returned unreadable JSON: {e}"));
        }
    };
    let docs = parsed
        .get("docs")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_default();
    let wanted_title = info.title.to_lowercase();
    for (i, doc) in docs.iter().enumerate() {
        let title = doc
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_lowercase();
        let isbns = doc
            .get("isbn")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let isbn10 = isbns.iter().filter_map(|v| v.as_str()).find_map(|s| {
            let d: String = s.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
            (d.len() == 10).then_some(d)
        });
        let isbn10 = isbn10.or_else(|| {
            isbns
                .iter()
                .filter_map(|v| v.as_str())
                .find_map(isbn13_to_isbn10)
        });
        if let Some(asin) = isbn10 {
            let confidence = if title == wanted_title { 0.8 } else { 0.5 } - (i as f32) * 0.05;
            return MatchOutcome::Candidate {
                asin: asin.to_uppercase(),
                confidence,
                source: "openlibrary",
            };
        }
    }
    MatchOutcome::NotFound
}

/// Real fetcher over HTTPS (opt-in network use).
pub fn http_fetch(url: &str) -> Result<String, String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(15)))
        .user_agent("Deckle/0.1 (+https://github.com/prjoni99/deckle)")
        .build()
        .new_agent();
    let mut resp = agent.get(url).call().map_err(|e| e.to_string())?;
    resp.body_mut().read_to_string().map_err(|e| e.to_string())
}
