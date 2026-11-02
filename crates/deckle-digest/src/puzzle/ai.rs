// SPDX-License-Identifier: GPL-3.0-or-later
//! The AI's part in a puzzle book (Puzzles tab design §2): the words and clues for a subject
//! you name, and clues rewritten in a style for any crossword. It runs wherever Settings → AI
//! says (an Ollama on this computer, or a service), through the same client as the editor;
//! every word it gives is checked before it's used, and a failure leaves the book as it was.

use deckle_xray::llm::{self, Options, Post, Provider};
use serde::{Deserialize, Serialize};

use crate::puzzle::words;

/// Where the model is, as the app's AI setting says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiConfig {
    pub provider: Provider,
    pub model: String,
    pub endpoint: String,
    pub api_key: String,
}

impl AiConfig {
    fn model(&self) -> &str {
        if self.model.trim().is_empty() {
            self.provider.default_model()
        } else {
            self.model.trim()
        }
    }
}

/// How the clues read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Style {
    /// No AI: the list's clues, or the headlines'.
    #[default]
    Plain,
    /// Facts about the answer, pub-quiz style.
    Trivia,
    /// Wordplay, in the British way.
    Cryptic,
    /// Simple words, for a child.
    Kid,
}

impl Style {
    pub fn parse(s: &str) -> Option<Style> {
        match s.trim().to_ascii_lowercase().as_str() {
            "plain" | "" => Some(Style::Plain),
            "trivia" => Some(Style::Trivia),
            "cryptic" => Some(Style::Cryptic),
            "kid" | "child" => Some(Style::Kid),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Style::Plain => "plain",
            Style::Trivia => "trivia",
            Style::Cryptic => "cryptic",
            Style::Kid => "kid",
        }
    }

    fn brief(self) -> &'static str {
        match self {
            Style::Plain => {
                "Plain, direct clues: a short definition or description, as a newspaper's easy crossword gives."
            }
            Style::Trivia => {
                "Trivia clues: a fact about the answer a pub quiz would ask, never a definition."
            }
            Style::Cryptic => {
                "Cryptic clues in the British style: a definition plus wordplay (anagram, hidden word, charade, double meaning), with the letter count in brackets."
            }
            Style::Kid => {
                "Clues for a child of about eight: simple words, no trick, often a short example sentence with the answer missing."
            }
        }
    }
}

const SYSTEM: &str = "You write crossword and word-search material. Answer with JSON only, no prose, no markdown. Never include an answer word inside its own clue.";
/// Tokens for an answer: ~40 per entry with headroom.
fn budget(n: usize) -> u64 {
    (512 + n as u64 * 40).min(8192)
}
/// Context for a request on Ollama: the longest list of entries with their clues fits well
/// under this.
const CONTEXT: u64 = 16_384;

fn schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "entries": {"type": "array", "items": {"type": "object",
                "properties": {"word": {"type": "string"}, "clue": {"type": "string"}},
                "required": ["word", "clue"]}}
        },
        "required": ["entries"]
    })
}

#[derive(Deserialize)]
struct Answer {
    entries: Vec<Entry>,
}
#[derive(Deserialize)]
struct Entry {
    word: String,
    clue: String,
}

fn fold(w: &str) -> String {
    deunicode::deunicode(w)
        .chars()
        .filter(char::is_ascii_alphabetic)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// A clue is usable when it's a sentence that doesn't give its answer away.
fn clean_clue(word: &str, clue: &str) -> Option<String> {
    let c = clue.split_whitespace().collect::<Vec<_>>().join(" ");
    if c.is_empty() || c.chars().count() > 200 {
        return None;
    }
    let lower = c.to_ascii_lowercase();
    let w = word.to_ascii_lowercase();
    let gives_away = lower
        .split(|ch: char| !ch.is_alphanumeric())
        .any(|t| t == w || (w.len() > 4 && t.starts_with(&w)));
    (!gives_away).then_some(c)
}

fn ask(cfg: &AiConfig, user: &str, n: usize, post: &mut Post<'_>) -> Result<Vec<Entry>, String> {
    let opts = Options {
        num_ctx: Some(CONTEXT),
        schema: Some(schema()),
    };
    let raw = llm::complete_with(
        cfg.provider,
        cfg.model(),
        &cfg.api_key,
        &cfg.endpoint,
        SYSTEM,
        user,
        budget(n),
        &opts,
        post,
    )
    .map_err(|e| e.to_string())?;
    let json = llm::extract_json_object(&raw).ok_or("the AI's answer wasn't JSON")?;
    let a: Answer = serde_json::from_value(json)
        .map_err(|e| format!("the AI's answer wasn't the shape asked for: {e}"))?;
    Ok(a.entries)
}

/// `count` words of `min`..=`max` letters about `subject`, each with a clue in `style`,
/// checked: letters only after folding, in the size, not blocklisted, no duplicates, and the
/// clue never names its answer. Fewer come back when the model gave fewer that pass.
pub fn subject_words(
    cfg: &AiConfig,
    subject: &str,
    style: Style,
    count: usize,
    min: usize,
    max: usize,
    post: &mut Post<'_>,
) -> Result<Vec<(String, String)>, String> {
    let style_brief = match style {
        Style::Plain => Style::Plain.brief(),
        s => s.brief(),
    };
    let user = format!(
        "Subject: {subject}\n\nGive {count} single words closely connected to the subject, each {min} to {max} letters long, letters only (no spaces, hyphens or accents), common nouns and proper names both welcome, no two alike. For each, a clue. {style_brief}\n\nReturn {{\"entries\": [{{\"word\": ..., \"clue\": ...}}, ...]}}."
    );
    let entries = ask(cfg, &user, count, post)?;
    let block = words::blocklist();
    let mut seen = std::collections::HashSet::new();
    Ok(entries
        .into_iter()
        .filter_map(|e| {
            let w = fold(&e.word);
            if w.len() < min || w.len() > max || block.contains(&w) || !seen.insert(w.clone()) {
                return None;
            }
            let clue = clean_clue(&w, &e.clue)?;
            Some((w, clue))
        })
        .collect())
}

/// Each `(answer, clue)` with its clue rewritten in `style`, the old clue given to the model
/// as what the answer means. An entry the model skipped, or whose new clue gives the answer
/// away, keeps its old clue. `about` names the book's theme, when it has one.
pub fn restyle(
    cfg: &AiConfig,
    entries: &[(String, String)],
    style: Style,
    about: &str,
    post: &mut Post<'_>,
) -> Result<Vec<(String, String)>, String> {
    if style == Style::Plain || entries.is_empty() {
        return Ok(entries.to_vec());
    }
    let list: String = entries
        .iter()
        .map(|(w, c)| format!("- {}: {c}", w.to_uppercase()))
        .collect::<Vec<_>>()
        .join("\n");
    let theme = if about.trim().is_empty() {
        String::new()
    } else {
        format!("The puzzle is themed on: {about}. ")
    };
    let user = format!(
        "{theme}Rewrite every clue below in a new style, keeping the same answer. {}\n\nEntries, as ANSWER: current clue\n{list}\n\nReturn {{\"entries\": [{{\"word\": \"ANSWER\", \"clue\": \"new clue\"}}, ...]}} with every answer exactly once.",
        style.brief()
    );
    let got = ask(cfg, &user, entries.len(), post)?;
    let by_word: std::collections::HashMap<String, String> = got
        .into_iter()
        .filter_map(|e| {
            let w = fold(&e.word);
            Some((w.clone(), clean_clue(&w, &e.clue)?))
        })
        .collect();
    Ok(entries
        .iter()
        .map(|(w, c)| {
            (
                w.clone(),
                by_word.get(&fold(w)).cloned().unwrap_or_else(|| c.clone()),
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> AiConfig {
        AiConfig {
            provider: Provider::Ollama,
            model: String::new(),
            endpoint: String::new(),
            api_key: String::new(),
        }
    }

    fn reply(entries: &[(&str, &str)]) -> String {
        let list: Vec<serde_json::Value> = entries
            .iter()
            .map(|(w, c)| serde_json::json!({"word": w, "clue": c}))
            .collect();
        serde_json::json!({"message": {"content": serde_json::json!({"entries": list}).to_string()}, "done_reason": "stop"}).to_string()
    }

    #[test]
    fn subject_words_are_checked() {
        let mut seen_body = String::new();
        let got = subject_words(
            &cfg(),
            "the Roman Empire",
            Style::Trivia,
            6,
            3,
            6,
            &mut |url, _, body| {
                seen_body = body.to_owned();
                assert_eq!(url, "http://localhost:11434/api/chat");
                Ok(reply(&[
                    ("Caesar", "Crossed the Rubicon in 49 BC"),
                    ("legion", "About 5,000 men, in cohorts"),
                    ("Legion", "a repeat"),
                    ("aqueducts", "too long"),
                    ("ox", "too short"),
                    ("forum", "The forum was the heart of Rome"),
                    ("villa", "A country house"),
                ]))
            },
        )
        .unwrap();
        assert_eq!(
            got.iter().map(|(w, _)| w.as_str()).collect::<Vec<_>>(),
            ["caesar", "legion", "villa"]
        );
        assert!(
            seen_body.contains("Roman Empire") && seen_body.contains("pub quiz"),
            "{seen_body}"
        );
        let b: serde_json::Value = serde_json::from_str(&seen_body).unwrap();
        assert_eq!(b["format"]["required"][0], "entries");
        assert_eq!(b["options"]["num_ctx"], 16_384);
    }

    #[test]
    fn restyle_keeps_the_old_clue_when_the_new_one_fails() {
        let entries = vec![
            ("cargo".to_owned(), "Rust's package registry".to_owned()),
            ("young".to_owned(), "Not old".to_owned()),
            ("ode".to_owned(), "Lyric poem".to_owned()),
        ];
        let got = restyle(
            &cfg(),
            &entries,
            Style::Cryptic,
            "Rust and the Panthers",
            &mut |_, _, body| {
                assert!(body.contains("CARGO: Rust's package registry"));
                assert!(body.contains("themed on: Rust and the Panthers"));
                Ok(reply(&[
                    ("CARGO", "Car go: freight, in a word (5)"),
                    ("young", "The young Young is young"),
                ]))
            },
        )
        .unwrap();
        assert_eq!(got[0].1, "Car go: freight, in a word (5)");
        assert_eq!(
            got[1].1, "Not old",
            "a clue that names its answer is refused"
        );
        assert_eq!(
            got[2].1, "Lyric poem",
            "an entry the model skipped keeps its clue"
        );
        assert_eq!(
            restyle(&cfg(), &entries, Style::Plain, "", &mut |_, _, _| panic!(
                "plain never asks"
            ))
            .unwrap(),
            entries
        );
        assert!(
            restyle(&cfg(), &entries, Style::Kid, "", &mut |_, _, _| Err(
                "down".into()
            ))
            .is_err()
        );
    }

    #[test]
    fn styles_parse() {
        assert_eq!(Style::parse("Cryptic"), Some(Style::Cryptic));
        assert_eq!(Style::parse(""), Some(Style::Plain));
        assert_eq!(Style::parse("hard"), None);
    }
}
