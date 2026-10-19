// SPDX-License-Identifier: GPL-3.0-or-later
//! Builds Deckle's Word Wise database (see `deckle_wordwise::lexicon`) from kaikki.org's
//! English Wiktionary extract and wordfreq's `large_en` list. Rules are in
//! `docs/formats/wordwise.md`; the selection follows Proficiency (GPL-3) where it applies.

use std::collections::HashMap;
use std::io::{BufRead, Read};
use std::path::Path;

use anyhow::{Context, Result, bail};
use deckle_wordwise::lexicon::{create, normalize_form};
use rusqlite::params;
use serde_json::Value;

/// Zipf at or above this is too common to hint (and disables a surface form outright).
pub const TOO_COMMON: f64 = 5.0;

/// Word → Zipf frequency from wordfreq's cBpack msgpack (`[header, bucket0, bucket1, …]`,
/// bucket *i* holds words at −*i* centibels, Zipf = 9 − *i*/100).
pub fn load_wordfreq(gz: impl Read) -> Result<HashMap<String, f64>> {
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(gz).read_to_end(&mut bytes)?;
    let v = rmpv::decode::read_value(&mut bytes.as_slice()).context("wordfreq msgpack")?;
    let Some(arr) = v.as_array() else {
        bail!("wordfreq: not an array")
    };
    let header = arr
        .first()
        .and_then(|h| h.as_map())
        .context("wordfreq header")?;
    let format = header
        .iter()
        .find(|(k, _)| k.as_str() == Some("format"))
        .and_then(|(_, v)| v.as_str());
    if format != Some("cB") {
        bail!("wordfreq: unexpected format {format:?}");
    }
    let mut out = HashMap::new();
    for (i, bucket) in arr.iter().skip(1).enumerate() {
        let zipf = 9.0 - i as f64 / 100.0;
        for w in bucket.as_array().into_iter().flatten() {
            if let Some(w) = w.as_str() {
                out.entry(w.to_owned()).or_insert(zipf);
            }
        }
    }
    Ok(out)
}

/// Zipf of a word or phrase; a phrase combines its words like wordfreq does (1/f = Σ 1/fᵢ).
pub fn zipf(freq: &HashMap<String, f64>, text: &str) -> Option<f64> {
    let mut inv = 0.0;
    for w in text.split(' ') {
        let z = *freq.get(w)?;
        inv += 1.0 / 10f64.powf(z - 9.0);
    }
    Some((1.0 / inv).log10() + 9.0)
}

/// Kindle difficulty (1 rarest … 5 commonest) or `None` when the word is not hinted.
/// Bands are spread so each level holds a similar share of the lexicon (Amazon's own list is
/// roughly even across 1–5).
pub fn difficulty(zipf: Option<f64>) -> Option<i64> {
    let z = zipf?;
    if z >= TOO_COMMON {
        return None;
    }
    Some(match z {
        z if z >= 4.25 => 5,
        z if z >= 3.5 => 4,
        z if z >= 2.75 => 3,
        z if z >= 2.0 => 2,
        _ => 1,
    })
}

/// An archaic, dated, literary or contracted spelling ("connexion", "o'er") is hinted with the
/// modern word it stands for.
fn dated_target(sense: &Value) -> Option<String> {
    let t = tags(sense);
    let gloss = sense
        .get("glosses")
        .and_then(Value::as_array)
        .and_then(|g| g.last())
        .and_then(Value::as_str)
        .unwrap_or("");
    let dated_gloss = ["Dated ", "Archaic ", "Literary ", "Poetic "]
        .iter()
        .any(|p| gloss.starts_with(p));
    if t.iter()
        .any(|t| matches!(*t, "misspelling" | "obsolete" | "nonstandard" | "form-of"))
        || !(dated_gloss
            || t.iter().any(|t| {
                matches!(
                    *t,
                    "archaic" | "dated" | "literary" | "poetic" | "contraction"
                )
            }))
    {
        return None;
    }
    // "Dated spelling of connection." names the word in the gloss when there is no alt_of
    let from_gloss = || {
        let (_, rest) = gloss.split_once(" of ")?;
        Some(trim_stop(rest))
    };
    let target = sense
        .get("alt_of")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|w| w.get("word"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| dated_gloss.then(from_gloss).flatten())?;
    (!target.is_empty() && target.chars().count() <= 40).then_some(target)
}

const SKIP_SENSE_TAGS: &[&str] = &[
    "obsolete",
    "form-of",
    "alt-of",
    "alternative",
    "misspelling",
    "abbreviation",
    "initialism",
    "acronym",
    "nonstandard",
];
const SKIP_FORM_TAGS: &[&str] = &[
    "table-tags",
    "inflection-template",
    "obsolete",
    "archaic",
    "alternative",
    "class",
    "auxiliary",
    "misspelling",
    "nonstandard",
];

fn kindle_pos(pos: &str) -> Option<&'static str> {
    Some(match pos {
        "noun" => "noun",
        "verb" => "verb",
        "adj" => "adjective",
        "adv" => "adverb",
        "phrase" => "other",
        _ => return None,
    })
}

fn tags(v: &Value) -> Vec<&str> {
    v.get("tags")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// A lemma Deckle can hint: lowercase letters, inner `'`/`-`/space, 1–3 words, 3+ letters.
fn usable_word(w: &str) -> bool {
    let words = w.split(' ').count();
    w.chars().filter(|c| c.is_alphabetic()).count() >= 3
        && words <= 3
        && w.chars().next().is_some_and(|c| c.is_lowercase())
        && w.chars().last().is_some_and(char::is_alphabetic)
        && w.chars()
            .all(|c| c.is_lowercase() || matches!(c, '\'' | '-' | ' '))
        && !w.contains("  ")
}

fn strip_parens(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' if depth > 0 => depth -= 1,
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn trim_stop(s: &str) -> String {
    s.trim().trim_end_matches('.').trim().to_owned()
}

/// Proficiency's short gloss: no parentheticals, the shortest `;` part, the first `,` part,
/// the shortest `/` alternative.
pub fn short_def(gloss: &str) -> String {
    let g = trim_stop(&strip_parens(gloss));
    let g = g
        .split(';')
        .map(str::trim)
        .min_by_key(|s| s.len())
        .unwrap_or("");
    // first comma part, unless it is a dangling "An open" of "An open, horse-drawn carriage"
    let mut parts = g.split(',').map(str::trim);
    let mut acc = parts.next().unwrap_or("").to_owned();
    for next in parts {
        let words: Vec<&str> = acc.split_whitespace().collect();
        let dangling = (words.len() <= 2
            && words
                .first()
                .is_some_and(|w| matches!(w.to_lowercase().as_str(), "a" | "an" | "the")))
            || words
                .last()
                .is_some_and(|w| matches!(*w, "and" | "or" | "of" | "to"));
        if !dangling {
            break;
        }
        acc = format!("{acc}, {next}");
    }
    let g = acc.as_str();
    let g = g
        .split('/')
        .map(str::trim)
        .min_by_key(|s| s.len())
        .unwrap_or("");
    trim_stop(g)
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuiltSense {
    pub lemma: String,
    pub pos: &'static str,
    pub short_def: String,
    pub full_def: String,
    pub example: Option<String>,
    pub difficulty: i64,
    pub forms: Vec<String>,
    /// Usable senses in the entry: the main etymology of a word has the most.
    pub weight: usize,
}

/// The sense Deckle keeps for one kaikki entry, or `None` when the entry is not hinted.
pub fn sense_from_entry(entry: &Value, freq: &HashMap<String, f64>) -> Option<BuiltSense> {
    if entry.get("lang_code").and_then(Value::as_str) != Some("en") {
        return None;
    }
    let word = entry.get("word")?.as_str()?;
    let pos = kindle_pos(entry.get("pos")?.as_str()?)?;
    let word = normalize_form(word);
    if !usable_word(&word) || tags(entry).iter().any(|t| SKIP_SENSE_TAGS.contains(t)) {
        return None;
    }
    let diff = difficulty(zipf(freq, &word))?;
    let good: Vec<&Value> = entry
        .get("senses")?
        .as_array()?
        .iter()
        .filter(|s| {
            s.get("glosses")
                .and_then(Value::as_array)
                .is_some_and(|g| !g.is_empty())
                && (dated_target(s).is_some()
                    || (s.get("form_of").is_none()
                        && s.get("alt_of").is_none()
                        && !tags(s).iter().any(|t| SKIP_SENSE_TAGS.contains(t))))
        })
        .collect();
    let sense = *good.first()?;
    let gloss = sense["glosses"].as_array()?.last()?.as_str()?;
    let full = trim_stop(gloss);
    let short = dated_target(sense).unwrap_or_else(|| short_def(gloss));
    if short.is_empty() || full.is_empty() {
        return None;
    }
    let example = sense
        .get("examples")
        .and_then(Value::as_array)
        .and_then(|ex| {
            ex.iter()
                .filter_map(|e| e.get("text").and_then(Value::as_str))
                .find(|t| t.chars().count() <= 200)
        })
        .map(trim_stop);
    let mut forms = vec![word.clone()];
    for f in entry
        .get("forms")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(form) = f.get("form").and_then(Value::as_str) else {
            continue;
        };
        let form = normalize_form(form);
        if tags(f).iter().any(|t| SKIP_FORM_TAGS.contains(t))
            || !usable_word(&form)
            || form.split(' ').count() != word.split(' ').count()
            || forms.contains(&form)
        {
            continue;
        }
        forms.push(form);
    }
    Some(BuiltSense {
        lemma: word,
        pos,
        short_def: short,
        full_def: full,
        example,
        difficulty: diff,
        forms,
        weight: good.len(),
    })
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct BuildStats {
    pub entries: usize,
    pub senses: usize,
    pub forms: usize,
}

/// Streams kaikki JSONL into a new database at `out`.
pub fn build(
    jsonl: impl BufRead,
    freq: &HashMap<String, f64>,
    out: &Path,
    version: &str,
) -> Result<BuildStats> {
    let mut stats = BuildStats::default();
    let mut senses: Vec<BuiltSense> = Vec::new();
    let mut seen: HashMap<(String, &'static str), usize> = HashMap::new();
    // word → its first entry is a function word ("thee" pron, "whilst" conj): never hinted,
    // whatever later entries ("thee" verb) say.
    let mut function_word: HashMap<String, bool> = HashMap::new();
    for line in jsonl.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        stats.entries += 1;
        let Ok(entry) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let (Some(w), Some(p)) = (
            entry.get("word").and_then(Value::as_str),
            entry.get("pos").and_then(Value::as_str),
        ) {
            function_word.entry(normalize_form(w)).or_insert(matches!(
                p,
                "pron" | "prep" | "conj" | "det" | "article" | "particle" | "num" | "intj"
            ));
        }
        if let Some(s) = sense_from_entry(&entry, freq) {
            // one entry per lemma + POS: the etymology with the most senses, else the first
            match seen.get(&(s.lemma.clone(), s.pos)) {
                Some(&i) if senses[i].weight >= s.weight => {}
                Some(&i) => senses[i] = s,
                None => {
                    seen.insert((s.lemma.clone(), s.pos), senses.len());
                    senses.push(s);
                }
            }
        }
    }
    let senses: Vec<BuiltSense> = senses
        .into_iter()
        .filter(|s| !function_word.get(&s.lemma).copied().unwrap_or(false))
        .collect();
    // form → (rank, sense index): the form being the lemma itself beats an inflection, then the
    // entry with more senses, then the earlier one.
    type Rank = (u8, std::cmp::Reverse<usize>, usize);
    let mut best: HashMap<String, (Rank, usize)> = HashMap::new();
    for (i, s) in senses.iter().enumerate() {
        for f in &s.forms {
            if zipf(freq, f).is_some_and(|z| z >= TOO_COMMON) {
                continue;
            }
            let rank = (u8::from(f != &s.lemma), std::cmp::Reverse(s.weight), i);
            best.entry(f.clone())
                .and_modify(|e| {
                    if rank < e.0 {
                        *e = (rank, i)
                    }
                })
                .or_insert((rank, i));
        }
    }
    let mut used: Vec<bool> = vec![false; senses.len()];
    for (_, i) in best.values() {
        used[*i] = true;
    }
    let mut conn = create(out, version)?;
    let tx = conn.transaction()?;
    {
        let mut ins = tx.prepare("INSERT INTO senses VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)")?;
        let mut ids = vec![0i64; senses.len()];
        for (next, (i, s)) in (1..).zip(senses.iter().enumerate().filter(|(i, _)| used[*i])) {
            ins.execute(params![
                next,
                s.lemma,
                s.pos,
                s.short_def,
                s.full_def,
                s.example,
                s.difficulty
            ])?;
            ids[i] = next;
            stats.senses += 1;
        }
        let mut forms: Vec<(&String, usize)> = best.iter().map(|(f, (_, i))| (f, *i)).collect();
        forms.sort();
        let mut ins = tx.prepare("INSERT INTO forms VALUES (?1, ?2)")?;
        for (f, i) in forms {
            ins.execute(params![f, ids[i]])?;
            stats.forms += 1;
        }
    }
    tx.commit()?;
    conn.execute_batch("VACUUM; PRAGMA optimize;")?;
    Ok(stats)
}
