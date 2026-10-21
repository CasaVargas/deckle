// SPDX-License-Identifier: GPL-3.0-or-later
//! Stage 5 (enrich): the X-Ray sidecar for an EBOK book. Heuristic entities always; an LLM
//! rewrites descriptions, kinds and aliases when configured; Wikipedia fills in terms when
//! opted in; the first-mention sentence is the fallback description.

use serde::{Deserialize, Serialize};

use deckle_kfx::text::Run;
use deckle_xray::asc::{DescriptionSource, Entity, EntityKind, XRayBook, write_asc};
use deckle_xray::extract::{Candidate, heuristic};
use deckle_xray::llm::{self, Provider};
use deckle_xray::wiki;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: Provider,
    /// Empty: the provider's default model.
    pub model: String,
    /// Empty: the provider's default endpoint (OpenAI-compatible servers set their own).
    pub endpoint: String,
    /// Empty: read from `ANTHROPIC_API_KEY` / `OPENAI_API_KEY`.
    pub api_key: String,
}

impl std::fmt::Debug for LlmConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmConfig")
            .field("provider", &self.provider)
            .field("model", &self.model)
            .field("endpoint", &self.endpoint)
            .field(
                "api_key",
                &if self.api_key.is_empty() {
                    ""
                } else {
                    "<redacted>"
                },
            )
            .finish()
    }
}

pub type Post<'a> = dyn FnMut(&str, &[(String, String)], &str) -> Result<String, String> + 'a;
pub type Fetch<'a> = dyn FnMut(&str) -> Result<String, String> + 'a;

#[derive(Clone, Debug, Default)]
pub struct XRayOptions {
    pub llm: Option<LlmConfig>,
    pub wikipedia: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct XRaySummary {
    pub people: usize,
    pub terms: usize,
    /// `llm` | `wikipedia` | `basic`
    pub source: String,
    /// [`deckle_xray::ENGINE`] at build time.
    pub engine: u32,
}

#[derive(Debug)]
pub struct XRayOutput {
    pub file_name: String,
    pub bytes: Vec<u8>,
    pub summary: XRaySummary,
    /// Non-fatal problems (LLM or Wikipedia failures that fell back).
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum XRayError {
    #[error("cannot write X-Ray database: {0}")]
    Write(#[from] deckle_xray::asc::AscError),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
}

pub fn sidecar_name(asin: &str) -> String {
    format!("XRAY.entities.{asin}.asc")
}

/// Builds the sidecar bytes; `None` when the text has no recurring names. Real HTTP.
pub fn build(
    asin: &str,
    title: &str,
    author: &str,
    lang: &str,
    runs: &[Run],
    erl: i64,
    opts: &XRayOptions,
) -> Result<Option<XRayOutput>, XRayError> {
    build_with(
        asin,
        title,
        author,
        lang,
        runs,
        erl,
        opts,
        &mut llm::http_post,
        &mut wiki::http_get,
    )
}

/// [`build`] with injected network calls (tests).
#[allow(clippy::too_many_arguments)]
pub fn build_with(
    asin: &str,
    title: &str,
    author: &str,
    lang: &str,
    runs: &[Run],
    erl: i64,
    opts: &XRayOptions,
    post: &mut Post<'_>,
    fetch: &mut Fetch<'_>,
) -> Result<Option<XRayOutput>, XRayError> {
    // "en-US" → "en": Wikipedia hosts and the strings table use the primary subtag.
    let lang = lang
        .split(['-', '_'])
        .next()
        .unwrap_or("en")
        .to_ascii_lowercase();
    let lang = if lang.is_empty() { "en" } else { lang.as_str() };
    let cands = heuristic(runs, lang);
    if cands.is_empty() {
        return Ok(None);
    }
    let mut warnings = Vec::new();
    let mut entities: Vec<Entity> = cands
        .iter()
        .map(|c| Entity {
            label: c.label.clone(),
            kind: c.kind,
            description: c.quote.clone(),
            source: DescriptionSource::Deckle,
            occurrences: c.occurrences.clone(),
        })
        .collect();
    let mut source = "basic";
    if let Some(cfg) = &opts.llm {
        let model = if cfg.model.is_empty() {
            cfg.provider.default_model()
        } else {
            &cfg.model
        };
        let key = if cfg.api_key.is_empty() {
            std::env::var(cfg.provider.env_key()).unwrap_or_default()
        } else {
            cfg.api_key.clone()
        };
        let answer = llm::describe_all(
            cfg.provider,
            model,
            &key,
            &cfg.endpoint,
            title,
            author,
            &cands,
            lang,
            post,
        );
        if answer.failures.len() < answer.batches {
            apply_llm(&mut entities, &cands, &answer.items);
            source = "llm";
        }
        if let Some(e) = answer.failures.first() {
            warnings.push(if answer.failures.len() == answer.batches {
                format!("X-Ray: LLM descriptions skipped ({e}); using basic ones")
            } else {
                format!(
                    "X-Ray: {} of {} LLM batches failed ({e}); their names keep basic descriptions",
                    answer.failures.len(),
                    answer.batches
                )
            });
        }
    }
    if opts.wikipedia {
        let mut hits = 0;
        for e in entities
            .iter_mut()
            .filter(|e| e.kind == EntityKind::Term && e.source == DescriptionSource::Deckle)
        {
            if let Some(s) = wiki::summary(lang, &e.label, fetch) {
                e.description = s;
                e.source = DescriptionSource::Wikipedia;
                hits += 1;
            }
        }
        if hits > 0 && source == "basic" {
            source = "wikipedia";
        }
    }
    let people = entities
        .iter()
        .filter(|e| e.kind == EntityKind::Person)
        .count();
    let terms = entities.len() - people;
    let book = XRayBook {
        asin: asin.to_owned(),
        lang: lang.to_owned(),
        erl,
        entities,
    };
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join(sidecar_name(asin));
    write_asc(&path, &book)?;
    let bytes = std::fs::read(&path)?;
    Ok(Some(XRayOutput {
        file_name: sidecar_name(asin),
        bytes,
        summary: XRaySummary {
            people,
            terms,
            source: source.to_owned(),
            engine: deckle_xray::ENGINE,
        },
        warnings,
    }))
}

/// Merges the LLM answer: descriptions and kinds by name, empty description = drop, aliases
/// fold their occurrences into the named entity. Symmetric or chained aliases resolve to one
/// survivor; an alias whose target is unknown is left alone.
fn apply_llm(entities: &mut Vec<Entity>, cands: &[Candidate], items: &[llm::Described]) {
    use std::collections::HashMap;
    let mut drop: Vec<String> = Vec::new();
    let mut into: HashMap<String, String> = HashMap::new(); // alias -> canonical
    for it in items {
        let Some(e) = entities.iter_mut().find(|e| e.label == it.name) else {
            continue;
        };
        if it.description.is_empty() {
            drop.push(it.name.clone());
            continue;
        }
        if let Some(k) = it.kind {
            e.kind = k;
        }
        e.description = it.description.clone();
        for a in &it.aliases {
            if a != &it.name
                && cands.iter().any(|c| &c.label == a)
                && !into.contains_key(a)
                && into.get(&it.name) != Some(a)
            {
                into.insert(a.clone(), it.name.clone());
            }
        }
    }
    // resolve chains; a cycle keeps the first-listed name
    let resolve = |mut name: String| {
        for _ in 0..8 {
            match into.get(&name) {
                Some(next) if next != &name => name = next.clone(),
                _ => break,
            }
        }
        name
    };
    let folds: Vec<(String, String)> = into
        .keys()
        .map(|a| (a.clone(), resolve(a.clone())))
        .filter(|(a, t)| a != t && !drop.contains(t))
        .collect();
    for (alias, target) in folds {
        if !entities.iter().any(|e| e.label == target) {
            continue;
        }
        if let Some(pos) = entities.iter().position(|e| e.label == alias) {
            let occ = entities.remove(pos).occurrences;
            if let Some(t) = entities.iter_mut().find(|e| e.label == target) {
                t.occurrences.extend(occ);
                t.occurrences.sort();
                t.occurrences.dedup();
            }
        }
    }
    entities.retain(|e| !drop.contains(&e.label));
}
