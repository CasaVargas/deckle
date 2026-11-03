// SPDX-License-Identifier: GPL-3.0-or-later
//! `deckle llm`: the AI model on this computer, through Ollama (local-model design §3–4).
//! `check` runs the three checks the app's setting shows; `pull` downloads the model.

use std::io::Write;
use std::process::ExitCode;

use deckle_xray::llm::{self, Check, OLLAMA_DOWNLOAD, Progress, Recommended, recommended_model};

use crate::cli::LlmCommand;
use crate::emit;

pub(crate) fn run(json: bool, action: LlmCommand) -> Result<ExitCode, String> {
    match action {
        LlmCommand::Check { endpoint, model } => {
            let check = llm::check(
                endpoint.as_deref().unwrap_or_default(),
                model.as_deref().unwrap_or_default(),
                &mut llm::http_get,
                &mut llm::http_post,
            );
            let ok = check.ok();
            emit(json, &check, || {
                report(&check, recommended_model(llm::total_memory()))
            })?;
            Ok(if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(crate::EXIT_FAILURE)
            })
        }
        LlmCommand::Pull { model, endpoint } => {
            let model =
                llm::resolve_model(llm::Provider::Ollama, model.as_deref().unwrap_or_default());
            let endpoint = endpoint.unwrap_or_default();
            let mut last = String::new();
            let mut progress = |p: &Progress| {
                if json {
                    return true;
                }
                let mut out = std::io::stdout().lock();
                if let Some(pct) = (p.completed * 100).checked_div(p.total) {
                    let _ = write!(out, "\r{} {:>3}% of {}", p.status, pct, gb(p.total));
                } else if p.status != last {
                    let _ = write!(out, "\n{}", p.status);
                }
                last = p.status.clone();
                let _ = out.flush();
                true
            };
            llm::pull(&endpoint, &model, &mut llm::http_post_lines, &mut progress)?;
            if !json {
                println!();
            }
            emit(
                json,
                &serde_json::json!({"model": model, "pulled": true}),
                || format!("{model} is installed. `deckle llm check` tries it out."),
            )?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9)
}

/// The checklist as the app's setting shows it, with what to do about the first thing wrong.
fn report(c: &Check, rec: Recommended) -> String {
    let mut lines = Vec::new();
    match &c.server {
        Ok(s) => lines.push(format!(
            "✓ Ollama {} is running at {}",
            s.version, c.endpoint
        )),
        Err(e) => {
            lines.push(format!("✗ {e}"));
            lines.push(format!(
                "  Get Ollama at {OLLAMA_DOWNLOAD}, open it, then run this again."
            ));
            return lines.join("\n");
        }
    }
    match c.installed {
        Some(true) => lines.push(format!("✓ {} is installed", c.model)),
        _ => {
            lines.push(format!("✗ {} isn't installed", c.model));
            let size = if c.model == rec.name {
                format!(" ({} download)", gb(rec.download))
            } else {
                String::new()
            };
            lines.push(format!("  deckle llm pull {}{size}", c.model));
            if let Ok(s) = &c.server
                && !s.models.is_empty()
            {
                let have: Vec<_> = s.models.iter().map(|m| m.name.as_str()).collect();
                lines.push(format!(
                    "  Installed already: {}. Pick one with --model or --llm-model.",
                    have.join(", ")
                ));
            }
            return lines.join("\n");
        }
    }
    match &c.answered {
        Some(Ok(ms)) => {
            lines.push(format!(
                "✓ Answered a test prompt in {:.1} s",
                *ms as f64 / 1000.0
            ));
            lines.push(format!(
                "Ready. Use it with --llm {} (and --llm-model {} if it isn't the default).",
                "ollama", c.model
            ));
        }
        Some(Err(e)) => lines.push(format!("✗ The model didn't answer: {e}")),
        None => {}
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use deckle_xray::llm::Server;

    fn checked(
        server: Result<Server, String>,
        installed: Option<bool>,
        answered: Option<Result<u64, String>>,
    ) -> Check {
        Check {
            endpoint: "http://localhost:11434".into(),
            model: "qwen3:4b".into(),
            server,
            installed,
            answered,
        }
    }

    #[test]
    fn the_report_names_the_first_thing_wrong_and_its_fix() {
        let rec = recommended_model(None);
        let down = checked(
            Err("Ollama isn't answering at http://localhost:11434 (refused)".into()),
            None,
            None,
        );
        let r = report(&down, rec);
        assert!(r.starts_with("✗ Ollama isn't answering"), "{r}");
        assert!(r.contains(OLLAMA_DOWNLOAD));

        let server = Server {
            version: "0.12.0".into(),
            models: vec![llm::InstalledModel {
                name: "llama3.2:latest".into(),
                size: 2_000_000_000,
            }],
        };
        let missing = checked(Ok(server.clone()), Some(false), None);
        let r = report(&missing, rec);
        assert!(r.contains("✓ Ollama 0.12.0 is running"), "{r}");
        assert!(
            r.contains("deckle llm pull qwen3:4b (2.6 GB download)"),
            "{r}"
        );
        assert!(r.contains("Installed already: llama3.2:latest"), "{r}");

        let ready = checked(Ok(server), Some(true), Some(Ok(1_400)));
        let r = report(&ready, rec);
        assert!(r.contains("✓ Answered a test prompt in 1.4 s"), "{r}");
        assert!(r.ends_with("Ready. Use it with --llm ollama (and --llm-model qwen3:4b if it isn't the default)."), "{r}");
    }
}
