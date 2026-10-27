// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in illustrations (cookbook design §7.2): an engraving-style drawing of a dish from an
//! OpenAI-compatible image endpoint, drawn once and kept as `images/<id>.ill.jpg`.
//!
//! The site picture `images/<id>.jpg` is never touched; the book chooses which of the two to
//! print (`book::printed_picture`). Only the recipe's title, its chapter's name and up to eight
//! key ingredients leave the machine. A failure of any kind keeps the picture the recipe had and
//! returns one generic line; what the provider said is dropped here and goes nowhere.

use serde::Serialize;

use super::book::shown_title;
use super::filing::EVERYTHING_ELSE;
use super::index::key_ingredient;
use super::store::{IllustrationsConfig, ImageKind, Saved, Store, write_atomic};
use crate::article::eink_photo;

/// The one line a failed drawing gives, whatever went wrong.
pub const FAILED: &str = "Couldn't draw this one; kept the photo";

/// What `illustrate` says when the book's illustrations are off.
pub const OFF: &str = "Illustrations are off";

/// The endpoint used when `[illustrations] endpoint` is empty.
pub const DEFAULT_ENDPOINT: &str = "https://api.openai.com/v1";

/// The model used when `[illustrations] model` is empty.
pub const DEFAULT_MODEL: &str = "gpt-image-1";

const SIZE: &str = "1536x1024";
const MAX_KEYS: usize = 8;
/// A page's title can be anything; the prompt keeps its first characters.
const TITLE_MAX: usize = 200;

/// `(url, headers, json body) -> response body`: [`deckle_xray::llm::http_post`]'s signature, so
/// tests (and `DECKLE_COOKBOOK_FAKE_IMAGES`) can stand in for the network.
pub type Post<'a> = &'a dyn Fn(&str, &[(String, String)], &str) -> Result<String, String>;

/// `OPENAI_API_KEY` from the environment, as X-Ray and the Daily editor find theirs. `None` when
/// it is unset or blank.
pub fn api_key() -> Option<String> {
    let key = std::env::var(deckle_xray::llm::Provider::OpenAi.env_key()).ok()?;
    let key = key.trim();
    (!key.is_empty()).then(|| key.to_owned())
}

/// The prompt (§7.2): a fixed template with the title, the chapter and at most eight key
/// ingredients. Nothing else about a recipe is ever in it.
pub fn prompt(title: &str, chapter: &str, keys: &[String]) -> String {
    let title: String = title.trim().chars().take(TITLE_MAX).collect();
    let mut out = format!(
        "A single plated dish, {title}, drawn as a 19th-century copperplate engraving: black \
         ink line work and cross-hatching on white, no color, no text, no hands, no background \
         scene."
    );
    let chapter = chapter.trim();
    if !chapter.is_empty() {
        out.push_str(&format!(
            " It belongs in the \"{chapter}\" chapter of a cookbook."
        ));
    }
    let keys: Vec<&str> = keys.iter().take(MAX_KEYS).map(String::as_str).collect();
    if !keys.is_empty() {
        out.push_str(&format!(" Visible ingredients: {}.", keys.join(", ")));
    }
    out
}

/// The key ingredients of a recipe, in the order it lists them, without repeats: at most eight.
pub fn keys(saved: &Saved) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in saved.recipe.ingredients.iter().flat_map(|g| &g.items) {
        if let Some(key) = key_ingredient(line)
            && !out.contains(&key)
        {
            out.push(key);
        }
        if out.len() == MAX_KEYS {
            break;
        }
    }
    out
}

#[derive(Serialize)]
struct Request<'a> {
    model: &'a str,
    prompt: &'a str,
    size: &'static str,
    response_format: &'static str,
    n: u32,
}

/// The name of the chapter `saved` is printed in (one since deleted reads as "Everything else").
fn chapter_name(store: &Store, saved: &Saved) -> Result<String, ()> {
    let chapters = store.config().map_err(|_| ())?.chapters;
    let find = |id: &str| chapters.iter().find(|c| c.id == id).map(|c| c.name.clone());
    Ok(find(&saved.chapter)
        .or_else(|| find(EVERYTHING_ELSE))
        .unwrap_or_else(|| "Everything else".to_owned()))
}

/// The grayscale JPEG in an image endpoint's answer (`data[0].b64_json`).
fn picture(reply: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    let v: serde_json::Value = serde_json::from_str(reply).ok()?;
    let b64: String = v["data"][0]["b64_json"]
        .as_str()?
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    eink_photo(&bytes)
}

/// Draws `saved` and keeps it as `images/<id>.ill.jpg` (replacing an earlier one), then marks the
/// book changed. One request, no retry. The site picture is left alone.
///
/// `Err` is always one fixed line for the owner ([`FAILED`], or [`OFF`] when illustrations are
/// off); nothing the provider said survives it. A failure leaves every picture as it was.
pub fn illustrate(
    store: &Store,
    saved: &Saved,
    cfg: &IllustrationsConfig,
    key: Option<&str>,
    post: Post,
) -> Result<(), &'static str> {
    if !cfg.enabled {
        return Err(OFF);
    }
    let key = key.map(str::trim).filter(|k| !k.is_empty()).ok_or(FAILED)?;
    let chapter = chapter_name(store, saved).map_err(|()| FAILED)?;
    let prompt = prompt(&shown_title(saved), &chapter, &keys(saved));
    let model = match cfg.model.trim() {
        "" => DEFAULT_MODEL,
        m => m,
    };
    let endpoint = match cfg.endpoint.trim().trim_end_matches('/') {
        "" => DEFAULT_ENDPOINT,
        e => e,
    };
    let body = serde_json::to_string(&Request {
        model,
        prompt: &prompt,
        size: SIZE,
        response_format: "b64_json",
        n: 1,
    })
    .map_err(|_| FAILED)?;
    let headers = [
        ("content-type".to_owned(), "application/json".to_owned()),
        ("authorization".to_owned(), format!("Bearer {key}")),
    ];
    let reply =
        post(&format!("{endpoint}/images/generations"), &headers, &body).map_err(|_| FAILED)?;
    let jpeg = picture(&reply).ok_or(FAILED)?;
    write_atomic(&store.image_path(&saved.id, ImageKind::Ill), jpeg).map_err(|_| FAILED)?;
    store.mark_dirty().map_err(|_| FAILED)
}
