// SPDX-License-Identifier: GPL-3.0-or-later
//! Highlights out as Markdown (highlights export design): the glue between a Kindle's
//! `My Clippings.txt` and the files in the chosen folder, shared by `deckle highlights` and
//! `digest watch`.

use std::path::{Path, PathBuf};

use deckle_highlights::{ExportReport, ParseError, State};

/// `highlights.json`, next to `library.json` and `digest.toml`.
pub fn state_path() -> PathBuf {
    deckle_library::config::default_path().with_file_name("highlights.json")
}

#[derive(Debug, Default, serde::Serialize)]
pub struct Outcome {
    #[serde(flatten)]
    pub report: ExportReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_error: Option<ParseError>,
}

impl Outcome {
    pub fn describe(&self) -> String {
        let new: usize = self.report.written.iter().map(|w| w.new).sum();
        let mut lines = vec![match new {
            0 => "highlights: nothing new".to_owned(),
            n => format!(
                "highlights: {n} new in {} file(s)",
                self.report.written.len()
            ),
        }];
        lines.extend(
            self.report
                .failed
                .iter()
                .map(|(_, e)| format!("highlights: error: {e}")),
        );
        if let Some(e) = &self.parse_error {
            lines.push(format!(
                "highlights: warning: My Clippings.txt line {}: {}",
                e.line, e.message
            ));
        }
        lines.join("\n")
    }
}

/// Exports `clippings` into `to` (remembered for next time), else the remembered folder.
/// `Err` when no folder is known yet or it can't be created.
pub fn export(clippings: &[u8], state_path: &Path, to: Option<&Path>) -> Result<Outcome, String> {
    let mut state = State::load(state_path);
    if let Some(to) = to {
        std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
        state.folder = Some(to.canonicalize().unwrap_or_else(|_| to.to_owned()));
    }
    let Some(folder) = state.folder.clone() else {
        return Err(
            "highlights: no folder set; run `deckle highlights export --to <dir>` once".into(),
        );
    };
    let parsed = deckle_highlights::parse(clippings);
    let books = deckle_highlights::group(parsed.entries);
    let report = deckle_highlights::export(&books, &folder, &mut state);
    if let Some(dir) = state_path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    state
        .save(state_path)
        .map_err(|e| format!("{}: {e}", state_path.display()))?;
    Ok(Outcome {
        report,
        parse_error: parsed.error,
    })
}
