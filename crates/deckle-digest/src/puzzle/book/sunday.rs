// SPDX-License-Identifier: GPL-3.0-or-later
//! Sunday's book (Puzzles tab design §1, puzzle book bundle design §5): when a paper's
//! `[puzzles] sunday = true`, the morning run of a Sunday edition makes the book the recipe
//! asks for, through the same `plan` the tab uses, and writes it as a bundle beside the edition
//! in every export folder whose `format = "bundle"`, with `paper_id` and `sunday_of` set so the
//! reader shelves it with that edition. The book is never pruned by `keep`, and one already
//! there is left as it is: the reader may hold ink on it.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, NaiveDate, Utc, Weekday};
use deckle_xray::llm::Post;
use serde::Serialize;

use super::bundle::{self, Extras};
use super::plan::{self, Plan, Theme};
use super::theme::Source;
use super::{self as book, BookOptions, Difficulty, Kind};
use crate::config::{Config, ExportFormat, PuzzlesConfig};
use crate::export::ExportReport;
use crate::puzzle::ai::{AiConfig, Style};
use crate::puzzle::words::Words;

/// What one bundle export got, or why it didn't.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SundayReport {
    /// The export's `name`.
    pub name: String,
    /// The bundle beside the edition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// The bundle was already there (its ink may be on it) and was left alone.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub kept: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Whether this morning's run should write Sunday's book: the paper asks for one, the edition
/// is a Sunday's, and some export takes bundles.
pub fn is_due(cfg: &Config, date: NaiveDate) -> bool {
    cfg.puzzles.sunday
        && date.weekday() == Weekday::Sun
        && cfg.export.iter().any(|e| e.format == ExportFormat::Bundle)
}

/// The recipe as the engine's options and the plan that themes it, dated `date`. The seed is
/// the options' default, so the same recipe and date make the same book on every machine.
pub fn options(p: &PuzzlesConfig, date: NaiveDate) -> Result<(BookOptions, Plan), String> {
    let kind = Kind::ALL
        .iter()
        .copied()
        .find(|k| k.key() == p.kind.trim())
        .ok_or_else(|| format!("[puzzles] kind: no puzzle kind {:?}", p.kind))?;
    let difficulty = match p.difficulty.trim() {
        "" | "medium" => Difficulty::Medium,
        "easy" => Difficulty::Easy,
        "hard" => Difficulty::Hard,
        other => {
            return Err(format!(
                "[puzzles] difficulty: {other:?} isn't easy, medium or hard"
            ));
        }
    };
    let style = Style::parse(&p.clues)
        .ok_or_else(|| format!("[puzzles] clues: no clue style {:?}", p.clues))?;
    let theme = match p.theme.trim() {
        "" | "none" => Theme::None,
        "interests" => Theme::Source(Source::Interests {
            names: Vec::new(),
            days: 30,
        }),
        "week" => Theme::Source(Source::Week { days: 7 }),
        "vocab" => Theme::Source(Source::Vocab),
        "cookbook" => Theme::Source(Source::Cookbook),
        other => {
            return Err(format!(
                "[puzzles] theme: {other:?} isn't none, interests, week, vocab or cookbook"
            ));
        }
    };
    let o = BookOptions {
        count: p.count.clamp(1, 200),
        size: if p.size == 0 {
            kind.default_size()
        } else {
            p.size
        },
        difficulty,
        ..BookOptions::new(kind, date)
    };
    Ok((o, Plan { theme, style }))
}

/// Makes Sunday's book for `cfg` (the paper's config, read from `main`) and writes it beside
/// the edition in every export of `exports` that took a bundle: one report per such export.
/// A book that can't be made is one report with the error, under the first export's name.
pub fn write_beside(
    cfg: &Config,
    main: &Path,
    date: NaiveDate,
    now: DateTime<Utc>,
    exports: &[ExportReport],
    ai: Option<&AiConfig>,
    post: &mut Post<'_>,
) -> Vec<SundayReport> {
    let targets: Vec<&ExportReport> = cfg
        .export
        .iter()
        .filter(|e| e.format == ExportFormat::Bundle)
        .filter_map(|e| {
            exports
                .iter()
                .find(|x| x.name == e.name && x.error.is_none())
        })
        .collect();
    if targets.is_empty() {
        return Vec::new();
    }
    let made = (|| {
        let (mut o, plan) = options(&cfg.puzzles, date)?;
        let found = plan::prepare(&mut o, &plan, main, date, ai, post)?;
        let mut b = book::make(&o, Words::embedded()).map_err(|e| e.to_string())?;
        plan::restyle(&mut b, &plan, &found.about, ai, post);
        let mut warnings = found.warnings.clone();
        if !found.note.is_empty() {
            warnings.insert(0, found.note.clone());
        }
        warnings.append(&mut b.warnings);
        b.warnings = warnings;
        Ok::<_, String>((b, o, plan))
    })();
    let (b, o, plan) = match made {
        Ok(m) => m,
        Err(e) => {
            return vec![SundayReport {
                name: targets[0].name.clone(),
                error: Some(format!("Sunday's book wasn't made: {e}")),
                ..Default::default()
            }];
        }
    };
    let extras = Extras {
        theme: plan.theme,
        style: plan.style,
        paper_id: cfg.paper_id,
        sunday_of: Some(date),
        built_at: now,
        replace: false,
    };
    let name = bundle::bundle_name(&b);
    targets
        .into_iter()
        .map(|x| {
            let there = x.dir.join(&name);
            if there.exists() {
                return SundayReport {
                    name: x.name.clone(),
                    path: Some(there),
                    kept: true,
                    error: None,
                };
            }
            match bundle::write_with(&b, &o, &extras, &x.dir) {
                Ok(p) => SundayReport {
                    name: x.name.clone(),
                    path: Some(p),
                    ..Default::default()
                },
                Err(e) => SundayReport {
                    name: x.name.clone(),
                    error: Some(e),
                    ..Default::default()
                },
            }
        })
        .collect()
}

/// The report lines for the morning's text output, one per export.
pub fn lines(reports: &[SundayReport]) -> String {
    reports
        .iter()
        .map(|r| match (&r.error, &r.path) {
            (Some(e), _) => format!("      Sunday's book, {}: {e}", r.name),
            (None, Some(p)) if r.kept => format!(
                "      Sunday's book, {}: already there, {}",
                r.name,
                p.display()
            ),
            (None, Some(p)) => format!("      Sunday's book -> {}", p.display()),
            (None, None) => String::new(),
        })
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_recipe_becomes_options_and_a_plan() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 8).unwrap();
        let (o, plan) = options(&PuzzlesConfig::default(), date).unwrap();
        assert_eq!(
            (o.kind, o.count, o.size, o.difficulty),
            (Kind::Crossword, 10, 11, Difficulty::Medium)
        );
        assert_eq!(plan.theme, Theme::Source(Source::Week { days: 7 }));
        assert_eq!(plan.style, Style::Plain);
        let p = PuzzlesConfig {
            kind: "search".into(),
            size: 15,
            count: 500,
            difficulty: "hard".into(),
            theme: "none".into(),
            clues: "kid".into(),
            ..Default::default()
        };
        let (o, plan) = options(&p, date).unwrap();
        assert_eq!(
            (o.kind, o.count, o.size, o.difficulty),
            (Kind::Search, 200, 15, Difficulty::Hard)
        );
        assert_eq!((plan.theme, plan.style), (Theme::None, Style::Kid));
        for (field, bad) in [
            (
                "kind",
                PuzzlesConfig {
                    kind: "riddle".into(),
                    ..Default::default()
                },
            ),
            (
                "difficulty",
                PuzzlesConfig {
                    difficulty: "brutal".into(),
                    ..Default::default()
                },
            ),
            (
                "theme",
                PuzzlesConfig {
                    theme: "subject".into(),
                    ..Default::default()
                },
            ),
            (
                "clues",
                PuzzlesConfig {
                    clues: "loud".into(),
                    ..Default::default()
                },
            ),
        ] {
            assert!(options(&bad, date).unwrap_err().contains(field), "{field}");
        }
    }

    #[test]
    fn due_only_on_a_sunday_with_the_switch_on_and_a_bundle_export() {
        let sunday = NaiveDate::from_ymd_opt(2026, 11, 8).unwrap();
        let monday = NaiveDate::from_ymd_opt(2026, 11, 9).unwrap();
        let mut cfg = Config::default();
        assert!(!is_due(&cfg, sunday));
        cfg.puzzles.sunday = true;
        assert!(!is_due(&cfg, sunday), "no bundle export");
        cfg.export.push(crate::config::ExportConfig {
            name: "iPad".into(),
            dir: "/tmp/x".into(),
            format: ExportFormat::Bundle,
            ..Default::default()
        });
        assert!(is_due(&cfg, sunday));
        assert!(!is_due(&cfg, monday));
    }
}
