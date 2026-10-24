// SPDX-License-Identifier: GPL-3.0-or-later
//! Open Library's covers for a book, for the Books tab's cover picker (Books and Kindle design
//! §8). Network, so only when the owner has Open Library on (the metadata setting).

use serde::Serialize;

use crate::matcher::urlencode;
use crate::wordwise::FetchBytes;

const SEARCH: &str = "https://openlibrary.org/search.json";
const COVERS: &str = "https://covers.openlibrary.org/b";
/// How many covers the picker offers.
pub const MAX_COVERS: usize = 6;

/// One cover Open Library has: a small JPEG to show, and the large one's address to fetch when
/// it's picked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Candidate {
    #[serde(skip)]
    pub thumb: Vec<u8>,
    pub large: String,
}

/// An image Open Library sent, not its 1×1 "no cover" placeholder.
fn real_image(bytes: &[u8]) -> bool {
    image::load_from_memory(bytes).is_ok_and(|i| i.width() > 10 && i.height() > 10)
}

/// The covers of the book's ISBN, then of its editions by title and author, de-duplicated.
pub fn candidates(
    title: &str,
    author: &str,
    isbn: Option<&str>,
    fetch: &mut FetchBytes<'_>,
) -> Result<Vec<Candidate>, String> {
    let mut sizes: Vec<(String, String)> = Vec::new();
    if let Some(isbn) = isbn
        .map(|i| i.replace(['-', ' '], ""))
        .filter(|i| !i.is_empty())
    {
        sizes.push((
            format!("{COVERS}/isbn/{isbn}-M.jpg?default=false"),
            format!("{COVERS}/isbn/{isbn}-L.jpg"),
        ));
    }
    let url = format!(
        "{SEARCH}?title={}&author={}&fields=cover_i,edition_count&limit=10",
        urlencode(title),
        urlencode(author)
    );
    let found = fetch(&url);
    let ids: Vec<i64> = match &found {
        Ok(body) => serde_json::from_slice::<serde_json::Value>(body)
            .ok()
            .and_then(|v| v.get("docs").and_then(|d| d.as_array()).cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|d| d.get("cover_i").and_then(|c| c.as_i64()))
            .collect(),
        Err(_) => Vec::new(),
    };
    for id in ids {
        sizes.push((
            format!("{COVERS}/id/{id}-M.jpg"),
            format!("{COVERS}/id/{id}-L.jpg"),
        ));
    }
    let mut out = Vec::new();
    let mut seen: Vec<Vec<u8>> = Vec::new();
    for (thumb, large) in sizes {
        if out.len() >= MAX_COVERS {
            break;
        }
        let Ok(bytes) = fetch(&thumb) else { continue };
        if !real_image(&bytes) || seen.contains(&bytes) {
            continue;
        }
        seen.push(bytes.clone());
        out.push(Candidate {
            thumb: bytes,
            large,
        });
    }
    match (out.is_empty(), found) {
        (true, Err(e)) => Err(format!("Open Library didn't answer: {e}")),
        _ => Ok(out),
    }
}

/// The large cover at `url` (an address [`candidates`] gave), checked to be an image.
pub fn fetch_large(url: &str, fetch: &mut FetchBytes<'_>) -> Result<Vec<u8>, String> {
    if !url.starts_with(COVERS) {
        return Err(format!("{url}: not an Open Library cover"));
    }
    let bytes = fetch(url)?;
    if real_image(&bytes) {
        Ok(bytes)
    } else {
        Err("Open Library sent no image for that cover".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpeg(shade: u8) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(40, 60, image::Rgb([shade, 0, 0]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
        out.into_inner()
    }

    #[test]
    fn isbn_first_then_editions_without_placeholders_or_repeats() {
        let mut asked = Vec::new();
        let mut fetch = |url: &str| -> Result<Vec<u8>, String> {
            asked.push(url.to_owned());
            if url.starts_with(SEARCH) {
                return Ok(br#"{"docs":[{"cover_i":1},{"cover_i":2},{"cover_i":3},{}]}"#.to_vec());
            }
            if url.contains("/isbn/") {
                return Ok(jpeg(10));
            }
            if url.contains("/id/1-") {
                return Ok(jpeg(10)); // the same as the ISBN's
            }
            if url.contains("/id/2-") {
                // Open Library's 1×1 "no cover".
                let img = image::RgbImage::new(1, 1);
                let mut out = std::io::Cursor::new(Vec::new());
                img.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
                return Ok(out.into_inner());
            }
            Ok(jpeg(200))
        };
        let got = candidates(
            "The Lantern",
            "Ada Field",
            Some("978-0-14-044913-6"),
            &mut fetch,
        )
        .unwrap();
        assert_eq!(
            got.iter().map(|c| c.large.as_str()).collect::<Vec<_>>(),
            [
                "https://covers.openlibrary.org/b/isbn/9780140449136-L.jpg",
                "https://covers.openlibrary.org/b/id/3-L.jpg"
            ]
        );
        assert!(asked[0].contains("title=The+Lantern"));
        let mut deny = |_: &str| -> Result<Vec<u8>, String> { Err("offline".into()) };
        assert!(candidates("X", "Y", None, &mut deny).is_err());
        assert!(fetch_large("https://example.com/x.jpg", &mut deny).is_err());
    }
}
