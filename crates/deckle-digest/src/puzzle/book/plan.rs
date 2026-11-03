// SPDX-License-Identifier: GPL-3.0-or-later
//! A book's plan (Puzzles tab design §2): where its words come from and how its clues read,
//! resolved into the engine's options before the book is made, and applied to the clues
//! after. The app and `deckle puzzles` share this, so a book made either way is the same.

use std::path::Path;

use chrono::NaiveDate;
use deckle_xray::llm::Post;
use serde::{Deserialize, Serialize};

use super::theme::{self, Source, Themed};
use super::{Book, BookOptions, Kind, Puzzles};
use crate::editor::AppAi;
use crate::puzzle::ai::{self, AiConfig, Style};
use crate::puzzle::cross;

/// Where a book's theme words come from.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Theme {
    /// Deckle's list only.
    #[default]
    None,
    /// The words (and clues) typed or pasted: already in `BookOptions.words` / `clued`.
    Own,
    /// Something on this machine (§2).
    Source(Source),
    /// The AI picks the words and writes the clues.
    Subject { text: String },
}

/// What to do about the words and the clues.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub theme: Theme,
    /// The clues' style; anything but `Plain` needs the AI.
    pub style: Style,
}

/// What `prepare` found out, for the title page and the report.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Prepared {
    /// The theme, as a line: "41 words from the last 30 days of Rust & Panthers".
    pub note: String,
    /// A title when the options gave none.
    pub title: String,
    /// What the AI is told the book is about, when it restyles the clues.
    pub about: String,
    pub warnings: Vec<String>,
}

impl AppAi {
    /// The app's AI setting as the puzzles' client config; `None` when it's off or unknown.
    pub fn ai_config(&self) -> Option<AiConfig> {
        match self {
            AppAi::On {
                provider,
                model,
                endpoint,
                api_key,
            } => {
                let env = std::env::var(provider.env_key()).unwrap_or_default();
                Some(AiConfig {
                    provider: *provider,
                    model: model.clone(),
                    endpoint: endpoint.clone(),
                    api_key: if env.is_empty() { api_key.clone() } else { env },
                })
            }
            _ => None,
        }
    }
}

/// How many words a subject should give: enough to seed every puzzle in the book.
fn subject_count(o: &BookOptions) -> usize {
    (o.count * 6).clamp(12, 60)
}

/// Fills `o.words` / `o.clued` from the plan's theme. `main` is the main paper's config path
/// (the sources live beside it); `ai` is needed for a subject, and `post` is its HTTP.
pub fn prepare(
    o: &mut BookOptions,
    plan: &Plan,
    main: &Path,
    today: NaiveDate,
    ai: Option<&AiConfig>,
    post: &mut Post<'_>,
) -> Result<Prepared, String> {
    let mut out = Prepared::default();
    if plan.style != Style::Plain && ai.is_none() {
        return Err("Styled clues need the AI: choose where it runs in Settings → AI".into());
    }
    let themed: Option<Themed> = match &plan.theme {
        Theme::None | Theme::Own => None,
        Theme::Source(src) => Some(theme::resolve(main, src, today)?),
        Theme::Subject { text } => {
            let text = text.trim();
            if text.is_empty() {
                return Err("Name a subject for the AI to write about".into());
            }
            let cfg = ai.ok_or("A subject needs the AI: choose where it runs in Settings → AI")?;
            let (min, max) = match o.kind {
                Kind::Crossword => (3, cross::LONGEST.min(o.size)),
                _ => (3, o.size.max(5)),
            };
            let words = ai::subject_words(cfg, text, plan.style, subject_count(o), min, max, post)?;
            if words.is_empty() {
                return Err(
                    "The AI gave no usable words for that subject; try another, or a wider grid"
                        .into(),
                );
            }
            Some(Themed {
                note: format!("{} words from the AI about {text}", words.len()),
                title: text.to_owned(),
                words,
            })
        }
    };
    if let Some(t) = themed {
        match o.kind {
            Kind::Crossword => o.clued = Some(t.words.clone()),
            Kind::Search => o.words = Some(t.words.iter().map(|(w, _)| w.clone()).collect()),
            other => {
                return Err(format!(
                    "a theme is for crosswords and word searches, not a {}",
                    other.name()
                ));
            }
        }
        if o.title.is_none() {
            o.title = Some(t.title.clone());
        }
        out.about = t.title.clone();
        out.note = t.note;
        out.title = t.title;
    } else if let Theme::Own = plan.theme {
        out.about = o.title.clone().unwrap_or_default();
    }
    if !matches!(plan.theme, Theme::None | Theme::Own) && o.kind == Kind::Search {
        // a search needs at least 8 words that fit: fewer falls back to the list with a note
        let fit = o.words.as_ref().map_or(0, |w| {
            w.iter().filter(|x| (3..=o.size).contains(&x.len())).count()
        });
        if fit < 8 {
            out.warnings.push(format!(
                "only {fit} theme words fit the grid, so the list fills in"
            ));
            o.words = None;
        }
    }
    if !matches!(plan.theme, Theme::None | Theme::Own) && o.kind == Kind::Crossword {
        let fit = o.clued.as_ref().map_or(0, |c| {
            c.iter()
                .filter(|(w, _)| (3..=cross::LONGEST.min(o.size)).contains(&w.len()))
                .count()
        });
        if fit < 3 {
            out.warnings.push(format!(
                "only {fit} theme words fit a crossword, so the list fills in"
            ));
            o.clued = None;
        }
    }
    Ok(out)
}

/// Rewrites a crossword book's clues in the plan's style, one request a puzzle; a puzzle
/// whose request fails keeps its clues and adds a warning. Other kinds are left alone.
pub fn restyle(b: &mut Book, plan: &Plan, about: &str, ai: Option<&AiConfig>, post: &mut Post<'_>) {
    if plan.style == Style::Plain {
        return;
    }
    let Some(cfg) = ai else {
        b.warnings
            .push("styled clues need the AI, which is off".into());
        return;
    };
    let Puzzles::Crossword(xs) = &mut b.puzzles else {
        return;
    };
    for (i, x) in xs.iter_mut().enumerate() {
        let entries: Vec<(String, String)> = x
            .clues
            .iter()
            .map(|c| (c.answer.to_lowercase(), c.clue.clone()))
            .collect();
        match ai::restyle(cfg, &entries, plan.style, about, post) {
            Ok(new) => {
                for (c, (_, clue)) in x.clues.iter_mut().zip(new) {
                    c.clue = clue;
                }
            }
            Err(e) => b
                .warnings
                .push(format!("puzzle {}: the clues stayed plain ({e})", i + 1)),
        }
    }
    b.subtitle = format!("{} · {} clues", b.subtitle, plan.style.name());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::puzzle::words::Words;
    use deckle_xray::llm::Provider;

    const DAY: NaiveDate = match NaiveDate::from_ymd_opt(2026, 11, 2) {
        Some(d) => d,
        None => unreachable!(),
    };

    fn opts(kind: Kind) -> BookOptions {
        BookOptions {
            seed: Some(3),
            ..BookOptions::new(kind, NaiveDate::from_ymd_opt(2026, 11, 2).unwrap())
        }
        .with_count(2)
    }

    fn ai() -> AiConfig {
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
        serde_json::json!({"message": {"content": serde_json::json!({"entries": list}).to_string()}}).to_string()
    }

    #[test]
    fn a_subject_seeds_the_book_and_a_style_restyles_its_clues() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("digest.toml");
        let mut o = opts(Kind::Crossword);
        let plan = Plan {
            theme: Theme::Subject {
                text: "Rome".into(),
            },
            style: Style::Trivia,
        };
        let words = [
            "caesar", "legion", "villa", "forum", "toga", "senate", "nero", "lira", "arena",
            "ovid", "tiber", "pax",
        ];
        let mut calls = 0;
        let prepared = prepare(&mut o, &plan, &main, DAY, Some(&ai()), &mut |_, _, body| {
            calls += 1;
            assert!(body.contains("Subject: Rome"));
            Ok(reply(
                &words
                    .iter()
                    .map(|w| (*w, "a fact about it"))
                    .collect::<Vec<_>>(),
            ))
        })
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(o.title.as_deref(), Some("Rome"));
        assert!(prepared.note.starts_with("12 words from the AI about Rome"));
        assert_eq!(o.clued.as_ref().unwrap().len(), 12);
        let mut b = super::super::make(&o, Words::embedded()).unwrap();
        assert!(
            b.crosswords()
                .iter()
                .any(|x| x.clues.iter().any(|c| c.theme)),
            "the subject's words are in"
        );
        let mut asked = 0;
        restyle(
            &mut b,
            &plan,
            &prepared.about,
            Some(&ai()),
            &mut |_, _, body| {
                asked += 1;
                assert!(body.contains("themed on: Rome"));
                // rewrite every clue the request lists
                let b: serde_json::Value = serde_json::from_str(body).unwrap();
                let user = b["messages"][1]["content"].as_str().unwrap();
                let answers: Vec<&str> = user
                    .lines()
                    .filter_map(|l| l.strip_prefix("- "))
                    .filter_map(|l| l.split(':').next())
                    .collect();
                Ok(reply(
                    &answers
                        .iter()
                        .map(|a| (*a, "A trivia fact"))
                        .collect::<Vec<_>>(),
                ))
            },
        );
        assert_eq!(asked, 2, "one request a puzzle");
        assert!(
            b.crosswords()
                .iter()
                .all(|x| x.clues.iter().all(|c| c.clue == "A trivia fact"))
        );
        assert!(b.subtitle.ends_with("trivia clues"), "{}", b.subtitle);
        // a failed request keeps the clues
        let mut b2 = super::super::make(&o, Words::embedded()).unwrap();
        restyle(&mut b2, &plan, "Rome", Some(&ai()), &mut |_, _, _| {
            Err("down".into())
        });
        assert_eq!(b2.warnings.len(), 2);
        assert_ne!(b2.crosswords()[0].clues[0].clue, "A trivia fact");
    }

    #[test]
    fn styles_and_subjects_need_the_ai_and_themes_fit_their_kind() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("digest.toml");
        let mut o = opts(Kind::Crossword);
        let plan = Plan {
            theme: Theme::None,
            style: Style::Kid,
        };
        let err = prepare(&mut o, &plan, &main, DAY, None, &mut |_, _, _| panic!()).unwrap_err();
        assert!(err.contains("Settings → AI"), "{err}");
        let plan = Plan {
            theme: Theme::Subject { text: " ".into() },
            style: Style::Plain,
        };
        assert!(
            prepare(
                &mut o,
                &plan,
                &main,
                DAY,
                Some(&ai()),
                &mut |_, _, _| panic!()
            )
            .unwrap_err()
            .contains("Name a subject")
        );
        let mut m = opts(Kind::Maze);
        let plan = Plan {
            theme: Theme::Subject {
                text: "Rome".into(),
            },
            style: Style::Plain,
        };
        let err = prepare(&mut m, &plan, &main, DAY, Some(&ai()), &mut |_, _, _| {
            Ok(reply(&[("forum", "x"), ("villa", "y"), ("nero", "z")]))
        })
        .unwrap_err();
        assert!(err.contains("not a maze"), "{err}");
        // no AI words that fit: the list fills in, with a warning, for a search
        let mut s = opts(Kind::Search);
        let p = prepare(&mut s, &plan, &main, DAY, Some(&ai()), &mut |_, _, _| {
            Ok(reply(&[("forum", "x"), ("villa", "y")]))
        })
        .unwrap();
        assert!(s.words.is_none());
        assert!(
            p.warnings[0].contains("only 2 theme words"),
            "{:?}",
            p.warnings
        );
    }

    #[test]
    fn the_apps_setting_becomes_a_client_config() {
        assert!(AppAi::Off.ai_config().is_none());
        let on = AppAi::On {
            provider: Provider::Ollama,
            model: "qwen3:8b".into(),
            endpoint: String::new(),
            api_key: "k".into(),
        };
        let c = on.ai_config().unwrap();
        assert_eq!(
            (c.provider, c.model.as_str()),
            (Provider::Ollama, "qwen3:8b")
        );
    }
}
