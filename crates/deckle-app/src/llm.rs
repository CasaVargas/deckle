// SPDX-License-Identifier: GPL-3.0-or-later
//! The AI page's "On this computer" checklist (local-model design §3): is Ollama running, is
//! the model installed, does it answer; and the download, with progress, behind its button.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub use deckle_xray::llm::Check;
use deckle_xray::llm::{self, Progress, Recommended, recommended_model, total_memory};
use serde::{Deserialize, Serialize};

use crate::events::Events;

/// The model Deckle recommends for this machine, as the page shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recommendation {
    pub model: String,
    /// Bytes to download.
    pub download: u64,
    /// This machine's memory in bytes, when known: the page says why this model.
    pub memory: Option<u64>,
    /// Where to get Ollama.
    pub ollama_url: String,
}

impl From<(Recommended, Option<u64>)> for Recommendation {
    fn from((r, memory): (Recommended, Option<u64>)) -> Self {
        Recommendation {
            model: r.name.to_owned(),
            download: r.download,
            memory,
            ollama_url: llm::OLLAMA_DOWNLOAD.to_owned(),
        }
    }
}

pub fn llm_recommended() -> Recommendation {
    let memory = total_memory();
    (recommended_model(memory), memory).into()
}

/// The three checks against the Ollama at `endpoint` (empty: the default) with `model`
/// (empty: the recommended one). Each is a network call, so it runs off the main thread.
pub fn llm_check(endpoint: String, model: String) -> Check {
    llm::check(&endpoint, &model, &mut llm::http_get, &mut llm::http_post)
}

static PULLING: AtomicBool = AtomicBool::new(false);
static STOP_PULLING: AtomicBool = AtomicBool::new(false);

/// Lets go of [`PULLING`] however the download ends.
struct Pulling;

impl Drop for Pulling {
    fn drop(&mut self) {
        PULLING.store(false, Ordering::SeqCst);
    }
}

/// One `llm-pull` event: where the download of `model` stands, or how it ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullEvent {
    pub model: String,
    pub status: String,
    pub completed: u64,
    pub total: u64,
    /// Set on the last event: the download is over, well or not.
    pub done: bool,
    pub error: Option<String>,
}

/// Downloads `model` (empty: the recommended one) into the Ollama at `endpoint`, emitting
/// `llm-pull` ([`PullEvent`]) as it goes and once more when it's over. Returns when it's over;
/// one download at a time. `llm_pull_stop` ends it.
pub fn llm_pull(events: Arc<dyn Events>, endpoint: String, model: String) -> Result<(), String> {
    let model = if model.trim().is_empty() {
        recommended_model(total_memory()).name.to_owned()
    } else {
        model.trim().to_owned()
    };
    if PULLING.swap(true, Ordering::SeqCst) {
        return Err("A download is already running".into());
    }
    let _guard = Pulling;
    STOP_PULLING.store(false, Ordering::SeqCst);
    let emit = |e: PullEvent| {
        events.emit(
            "llm-pull",
            serde_json::to_value(e).unwrap_or(serde_json::Value::Null),
        );
    };
    let result = llm::pull(
        &endpoint,
        &model,
        &mut llm::http_post_lines,
        &mut |p: &Progress| {
            emit(PullEvent {
                model: model.clone(),
                status: p.status.clone(),
                completed: p.completed,
                total: p.total,
                done: false,
                error: None,
            });
            !STOP_PULLING.load(Ordering::SeqCst)
        },
    );
    emit(PullEvent {
        model: model.clone(),
        status: match &result {
            Ok(()) => "success".into(),
            Err(e) if e == llm::PULL_CANCELLED => "stopped".into(),
            Err(_) => "failed".into(),
        },
        completed: 0,
        total: 0,
        done: true,
        error: result.as_ref().err().cloned(),
    });
    result
}

/// Ends a `llm_pull` at its next progress line.
pub fn llm_pull_stop() {
    STOP_PULLING.store(true, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_recommendation_names_the_model_and_where_ollama_comes_from() {
        let r: Recommendation = (recommended_model(None), None).into();
        assert_eq!(r.model, "qwen3:4b");
        assert!(r.download > 1_000_000_000);
        assert!(r.ollama_url.starts_with("https://ollama.com/"));
        let r = llm_recommended();
        assert!(r.model.starts_with("qwen3:"));
    }
}
