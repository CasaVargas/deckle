// SPDX-License-Identifier: GPL-3.0-or-later
//! Builds Deckle's Word Wise database (see `deckle_wordwise::lexicon`) from kaikki.org's
//! English Wiktionary extract and wordfreq's `large_en` list. Rules are in
//! `docs/formats/wordwise.md`; the selection follows Proficiency (GPL-3) where it applies.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{BufRead, Read};
use std::path::Path;

use anyhow::{Context, Result, bail};
use deckle_wordwise::lexicon::{create, normalize_form};
use rusqlite::params;
use serde_json::Value;
use sha2::Digest;

/// Zipf at or above this is too common to hint (and disables a surface form outright).
pub const TOO_COMMON: f64 = 4.25;

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
        z if z >= 3.75 => 5,
        z if z >= 3.25 => 4,
        z if z >= 2.75 => 3,
        z if z >= 2.0 => 2,
        _ => 1,
    })
}

/// Tags that keep a sense out of the puzzle list (puzzle design §4).
const PUZZLE_MARKED: &[&str] = &[
    "archaic",
    "dated",
    "rare",
    "historical",
    "dialectal",
    "slang",
    "vulgar",
    "derogatory",
    "offensive",
    "obsolete",
    "nonstandard",
    "colloquial",
    "informal",
];

/// Longer definitions don't fit beside a 5×5 grid.
const MAX_CLUE: usize = 90;

/// The word list for the puzzle page (puzzle design §4): lowercase a–z words of 3 to 5 letters
/// at zipf 3.0 or more, each with the short definition of its first everyday sense as the clue,
/// sorted by word. Proper nouns, abbreviations, marked senses, plain inflections of a kept
/// word, blocklisted words and clues that give the answer away are left out.
pub fn puzzle_words(
    jsonl: impl BufRead,
    freq: &HashMap<String, f64>,
    blocklist: &HashSet<String>,
) -> Vec<(String, String)> {
    // word → its clue, from the first entry and sense that give one; word → the lemmas it
    // inflects; word → whether its lowercase entries are all nouns; names ("Goa", "Beth")
    let mut best: BTreeMap<String, String> = BTreeMap::new();
    let mut inflects: HashMap<String, Vec<String>> = HashMap::new();
    let mut nouns_only: HashMap<String, bool> = HashMap::new();
    let mut names: HashSet<String> = HashSet::new();
    let mut function_word: HashMap<String, bool> = HashMap::new();
    for line in jsonl.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => continue,
            Err(e) => panic!("reading kaikki: {e}"),
        };
        let Ok(entry) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let (Some(word), Some(pos)) = (
            entry.get("word").and_then(Value::as_str),
            entry.get("pos").and_then(Value::as_str),
        ) else {
            continue;
        };
        if entry.get("lang_code").and_then(Value::as_str) != Some("en")
            || !(3..=5).contains(&word.len())
            || !word.bytes().all(|b| b.is_ascii_alphabetic())
        {
            continue;
        }
        if !word.bytes().all(|b| b.is_ascii_lowercase()) {
            if pos == "name" {
                names.insert(word.to_ascii_lowercase());
            }
            continue;
        }
        function_word.entry(word.to_owned()).or_insert(matches!(
            pos,
            "pron" | "prep" | "conj" | "det" | "article" | "particle" | "num" | "intj"
        ));
        let senses = entry
            .get("senses")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        for s in senses {
            for lemma in s
                .get("form_of")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(l) = lemma.get("word").and_then(Value::as_str) {
                    inflects
                        .entry(word.to_owned())
                        .or_default()
                        .push(l.to_owned());
                }
            }
        }
        if kindle_pos(pos).is_none_or(|p| p == "other")
            || tags(&entry).iter().any(|t| SKIP_SENSE_TAGS.contains(t))
            || blocklist.contains(word)
            || zipf(freq, word).is_none_or(|z| z < 3.0)
        {
            continue;
        }
        // Wiktionary lists the main entry and its everyday senses first
        let clues: Vec<String> = senses
            .iter()
            .filter(|s| {
                s.get("form_of").is_none()
                    && s.get("alt_of").is_none()
                    && !tags(s)
                        .iter()
                        .any(|t| SKIP_SENSE_TAGS.contains(t) || PUZZLE_MARKED.contains(t))
            })
            .filter_map(|s| {
                let g = s.get("glosses")?.as_array()?.last()?.as_str()?;
                (!g.contains("spelling of")).then(|| puzzle_clue(g))
            })
            .collect();
        if !clues.is_empty() {
            *nouns_only.entry(word.to_owned()).or_insert(true) &= pos == "noun";
        }
        let clue = clues.into_iter().find(|c| {
            c.chars().next().is_some_and(char::is_uppercase)
                && c.chars().count() <= MAX_CLUE
                && !gives_away(word, c)
        });
        if let Some(c) = clue {
            best.entry(word.to_owned()).or_insert(c);
        }
    }
    best.iter()
        .filter(|(w, _)| !function_word.get(*w).copied().unwrap_or(false))
        // wordfreq counts "goa" for Goa: a name whose lowercase spelling is only a rare noun.
        // "bath" (a verb too) is a word whatever Bath is.
        .filter(|(w, _)| !(names.contains(*w) && nouns_only.get(*w).copied().unwrap_or(true)))
        .filter(|(w, _)| {
            // "cards" (of "card"), "baked" (of "bake"): a regular inflection of a kept word.
            // Irregular forms with their own meaning ("bore", "left") stay.
            !inflects.get(*w).into_iter().flatten().any(|l| {
                let stem = l.trim_end_matches(['e', 'y']);
                best.contains_key(l) && !stem.is_empty() && w.starts_with(stem)
            })
        })
        .map(|(w, c)| (w.clone(), c.clone()))
        .collect()
}

/// A definition as a clue: its first clause whole when it fits, else the Word Wise short
/// definition.
fn puzzle_clue(gloss: &str) -> String {
    let g = trim_stop(&strip_parens(gloss));
    // "Specifically, a flexible tube" → "A flexible tube"
    let g = match ["Specifically, ", "Especially, ", "In particular, "]
        .iter()
        .find_map(|p| g.strip_prefix(p))
    {
        Some(rest) => {
            let mut c = rest.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default()
        }
        None => g,
    };
    let first = trim_stop(g.split(';').next().unwrap_or(""));
    if first.chars().count() <= MAX_CLUE {
        first
    } else {
        short_def(&g)
    }
}

/// A clue gives its answer away when the answer is anywhere in it, ignoring case: "roses",
/// "rose-coloured", "Firewood".
pub fn gives_away(answer: &str, clue: &str) -> bool {
    clue.to_lowercase().contains(answer)
}

/// Ordering key for picking a sense: 0 for an everyday sense, more for marked ones.
fn sense_penalty(sense: &Value) -> u8 {
    const MARKED: &[&str] = &[
        "archaic",
        "dated",
        "rare",
        "historical",
        "dialectal",
        "slang",
        "vulgar",
        "derogatory",
        "obsolete",
        "nonstandard",
    ];
    const SPECIALIST: &[&str] = &[
        "grammar",
        "linguistics",
        "physics",
        "chemistry",
        "mathematics",
        "biology",
        "medicine",
        "law",
        "computing",
        "nautical",
        "military",
        "heraldry",
        "music",
        "card-games",
        "sports",
        "finance",
        "business",
        "botany",
        "zoology",
        "anatomy",
        "geology",
        "astronomy",
    ];
    let topics: Vec<&str> = sense
        .get("topics")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    u8::from(tags(sense).iter().any(|t| MARKED.contains(t)))
        + u8::from(topics.iter().any(|t| SPECIALIST.contains(t)))
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
    // the first `;` part when it is short enough, else the shortest one that says something
    // ("also" alone does not)
    let parts: Vec<&str> = g
        .split(';')
        .map(str::trim)
        .filter(|p| p.chars().filter(|c| c.is_alphabetic()).count() >= 4)
        .collect();
    let g = match parts.first() {
        Some(first) if first.chars().count() <= 50 => *first,
        _ => parts.iter().copied().min_by_key(|s| s.len()).unwrap_or(""),
    };
    // first comma part, unless it dangles: "An open" of "An open, horse-drawn carriage", or the
    // adjective list of "Any well-bred, well-mannered, kind-hearted man"
    let mut parts = g.split(',').map(str::trim);
    let mut acc = parts.next().unwrap_or("").to_owned();
    let starts_with_determiner = acc.split_whitespace().next().is_some_and(|w| {
        matches!(
            w.to_lowercase().as_str(),
            "a" | "an" | "the" | "any" | "some" | "one" | "each" | "every"
        )
    });
    let mut last_words = acc.split_whitespace().count();
    for next in parts {
        let dangling = (starts_with_determiner && last_words <= 2)
            || acc
                .split_whitespace()
                .last()
                .is_some_and(|w| matches!(w, "and" | "or" | "of" | "to"));
        if !dangling {
            break;
        }
        acc = format!("{acc}, {next}");
        last_words = next.split_whitespace().count();
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
    // the first everyday sense: archaic, rare and specialist senses only when nothing else fits
    let sense = *good.iter().min_by_key(|s| sense_penalty(s))?;
    let gloss = sense["glosses"].as_array()?.last()?.as_str()?;
    if dated_target(sense).is_none() && gloss.contains("spelling of") {
        return None; // "British standard spelling of behavior": a variant, not a meaning
    }
    let full = trim_stop(gloss);
    let short = dated_target(sense).unwrap_or_else(|| short_def(gloss));
    if short.is_empty() || full.is_empty() {
        return None;
    }
    // examples are not shown in Word Wise hints; leaving them out halves the dictionary
    let example = None;
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
    // form → (rank, sense index): the entry with more senses ("lady" over the poker term
    // "ladies", "pass" over the adjective "passed"), then the form being the lemma itself,
    // then the earlier entry.
    type Rank = (std::cmp::Reverse<usize>, u8, usize);
    let mut best: HashMap<String, (Rank, usize)> = HashMap::new();
    for (i, s) in senses.iter().enumerate() {
        for f in &s.forms {
            if zipf(freq, f).is_some_and(|z| z >= TOO_COMMON) {
                continue;
            }
            let rank = (std::cmp::Reverse(s.weight), u8::from(f != &s.lemma), i);
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
    // a fingerprint of the rows: the app keys its cached dictionary and the device's installed
    // one on it, since sense ids are only meaningful for this exact build
    let build_id = {
        let mut h = sha2::Sha256::new();
        let mut st =
            tx.prepare("SELECT id, lemma, pos, short_def, difficulty FROM senses ORDER BY id")?;
        let mut rows = st.query([])?;
        while let Some(r) = rows.next()? {
            let row: (i64, String, String, String, i64) =
                (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?);
            h.update(format!("{row:?}\n").as_bytes());
        }
        let d = h.finalize();
        d[..6]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    tx.execute(
        "INSERT INTO metadata VALUES ('build', ?1)",
        params![build_id],
    )?;
    tx.commit()?;
    conn.execute_batch("VACUUM; PRAGMA optimize;")?;
    Ok(stats)
}
