// SPDX-License-Identifier: GPL-3.0-or-later
//! The puzzle's word list: `word<TAB>clue` lines, built offline by `data/wordwise` and
//! embedded gzipped (puzzle design §4).

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::OnceLock;

static EMBEDDED: &[u8] = include_bytes!("../../assets/puzzle/words.tsv.gz");
/// The words the list is built without, also kept out of the word search's filler letters.
static BLOCKLIST: &str = include_str!("../../../../data/wordwise/puzzle-blocklist.txt");

/// A blocklist line naming a term no clue may hold, not a word: `clue:lascivious`.
const CLUE_PREFIX: &str = "clue:";

/// The blocklist's lines, lowercase, comments and blanks skipped.
fn blocklist_lines() -> impl Iterator<Item = String> {
    BLOCKLIST
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim().to_lowercase())
        .filter(|l| !l.is_empty())
}

/// The puzzle blocklist's words (lowercase; `#` lines, blanks and `clue:` terms skipped),
/// parsed once.
pub fn blocklist() -> &'static HashSet<String> {
    static SET: OnceLock<HashSet<String>> = OnceLock::new();
    SET.get_or_init(|| {
        blocklist_lines()
            .filter(|l| !l.starts_with(CLUE_PREFIX))
            .collect()
    })
}

/// The blocklist's clue terms — its `clue:` lines and every blocklisted word, since a word the
/// grid never takes is no word for its clue either ("dang": "Damn") — each as a padded run of
/// lowercase words (" sexual desire "), parsed once.
fn clue_terms() -> &'static [String] {
    static TERMS: OnceLock<Vec<String>> = OnceLock::new();
    TERMS.get_or_init(|| {
        blocklist_lines()
            .map(|l| padded_words(l.strip_prefix(CLUE_PREFIX).unwrap_or(&l)))
            .filter(|t| !t.trim().is_empty())
            .collect()
    })
}

/// Text as its lowercase words, one space apart and padded: "Lascivious, rude" → " lascivious rude ".
fn padded_words(s: &str) -> String {
    let w: Vec<String> = s
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    format!(" {} ", w.join(" "))
}

/// Whether a clue holds a blocklisted word or clue term as whole words, ignoring case and punctuation —
/// the rule `data/wordwise` builds the list by (its `adult_clue`), kept here too so a list
/// built before a term was added still loads clean.
pub fn adult_clue(clue: &str) -> bool {
    let clue = padded_words(clue);
    clue_terms().iter().any(|t| clue.contains(t.as_str()))
}

#[derive(Clone, Debug, Default)]
pub struct Words {
    /// Words by length (3 to 5 are filled), each sorted.
    pub by_len: [Vec<String>; 6],
    pub clues: HashMap<String, String>,
}

impl Words {
    /// The list built into the app, unpacked once.
    pub fn embedded() -> &'static Words {
        static WORDS: OnceLock<Words> = OnceLock::new();
        WORDS.get_or_init(|| {
            let mut text = String::new();
            // the bytes are checked in and tested: a bad file is a build error, not a runtime one
            flate2::read::GzDecoder::new(EMBEDDED)
                .read_to_string(&mut text)
                .expect("the embedded puzzle word list");
            Words::from_tsv(&text)
        })
    }

    /// `word<TAB>clue` lines; anything else (blank, no tab, not 3–5 lowercase letters) is skipped,
    /// and so is a blocklisted word or a clue holding a blocklist clue term: the daily mini is a
    /// family paper's, whatever the list was built with.
    pub fn from_tsv(text: &str) -> Words {
        let mut w = Words::default();
        for line in text.lines() {
            let Some((word, clue)) = line.split_once('\t') else {
                continue;
            };
            if !(3..=5).contains(&word.len())
                || !word.bytes().all(|b| b.is_ascii_lowercase())
                || clue.trim().is_empty()
                || w.clues.contains_key(word)
                || blocklist().contains(word)
                || adult_clue(clue)
            {
                continue;
            }
            w.by_len[word.len()].push(word.to_owned());
            w.clues.insert(word.to_owned(), clue.trim().to_owned());
        }
        for list in &mut w.by_len {
            list.sort_unstable();
        }
        w
    }

    pub fn contains(&self, w: &str) -> bool {
        self.clues.contains_key(w)
    }

    pub fn clue(&self, w: &str) -> Option<&str> {
        self.clues.get(w).map(String::as_str)
    }

    pub fn of_len(&self, n: usize) -> &[String] {
        self.by_len.get(n).map(Vec::as_slice).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_list_is_sane() {
        let w = Words::embedded();
        let total: usize = (3..=5).map(|n| w.of_len(n).len()).sum();
        assert!((3_000..=30_000).contains(&total), "{total}");
        for n in 3..=5 {
            for word in w.of_len(n) {
                assert_eq!(word.len(), n);
                assert!(word.bytes().all(|b| b.is_ascii_lowercase()), "{word}");
                let clue = w.clue(word).unwrap().to_lowercase();
                assert!(!clue.contains(word.as_str()), "{word}: {clue}");
            }
        }
        assert!(w.contains("card"));
        assert!(!w.contains("cards"));
    }

    #[test]
    fn from_tsv_skips_bad_lines() {
        let w = Words::from_tsv(
            "card\tA piece of stiff paper\n\nnotab\nLEAF\tUpper case\nab\tToo short\nlonger\tToo long\nleaf\tA green organ\n",
        );
        assert!(w.contains("card") && w.contains("leaf"));
        assert!(!w.contains("notab") && !w.contains("leaf ") && !w.contains("ab"));
        assert!(!w.contains("LEAF") && !w.contains("longer"));
        assert_eq!(w.clue("leaf"), Some("A green organ"));
        assert_eq!(w.of_len(4), ["card", "leaf"]);
        assert!(w.of_len(3).is_empty() && w.of_len(9).is_empty());
    }

    #[test]
    fn from_tsv_leaves_out_blocked_words_and_adult_clues() {
        let w = Words::from_tsv(
            "lewd\tLascivious, sexually promiscuous, rude\n\
             busty\tHaving large breasts\n\
             punk\tA passive or effeminate homosexual man\n\
             rut\tSexual desire in any of many mammals\n\
             fudge\tA soft candy made of SEXY sugar\n\
             puppy\tA young dog, especially before sexual maturity\n\
             hen\tA female chicken, especially a sexually mature one\n\
             dang\tDamn\n\
             prog\tTo prick\n\
             pubic\tOf the area adjacent to the PUBES\n\
             leaf\tA green organ\n",
        );
        // a blocklisted answer, whatever its clue
        assert!(!w.contains("lewd") && !w.contains("busty"));
        // a clean answer with an adult clue: terms match whole words in any case
        // a blocklisted word inside a clue is an adult clue too
        for word in ["punk", "rut", "fudge", "dang", "prog", "pubic"] {
            assert!(!w.contains(word), "{word}");
        }
        // a term is whole words: "sexual maturity" is not "sexual desire"
        assert!(w.contains("puppy") && w.contains("hen") && w.contains("leaf"));
        assert_eq!(w.of_len(5), ["puppy"]);
    }

    #[test]
    fn blocklist_keeps_clue_terms_apart() {
        assert!(blocklist().contains("lewd"));
        assert!(blocklist().iter().all(|b| !b.starts_with(CLUE_PREFIX)));
        assert!(adult_clue("Lascivious, sexually promiscuous, rude"));
        assert!(adult_clue("To act as a procurer of prostitutes"));
        assert!(!adult_clue(
            "A young dog, especially before sexual maturity"
        ));
        assert!(!adult_clue("Cheap and vulgar tastelessness"));
        // a blocklisted word is a clue term too, as a whole word
        assert!(adult_clue("Damn") && adult_clue("Synonym of booty"));
        assert!(!adult_clue("Damnation") && !adult_clue("A hellebore"));
    }

    #[test]
    fn embedded_list_has_no_adult_entries() {
        let w = Words::embedded();
        // the 2026-11-24 report (LEWD in a paper's mini) and its neighbours in the built list
        // and the second skim: adult answers, and clean answers clued with blocked words
        for word in [
            "lewd", "busty", "pimp", "punk", "lust", "rut", "molly", "date", "pubic", "randy",
            "poo", "fecal", "fart", "sperm", "dang", "darn", "prog", "loot",
        ] {
            assert!(!w.contains(word), "{word}");
        }
        // terms checked apart from the blocklist, so a word dropped from it shows up here
        const NEVER_IN_A_CLUE: [&str; 18] = [
            "damn",
            "prick",
            "booty",
            "pubes",
            "pubis",
            "feces",
            "sperm",
            "semen",
            "penis",
            "vagina",
            "anus",
            "breasts",
            "lustful",
            "lascivious",
            "prostitute",
            "whore",
            "urine",
            "vomit",
        ];
        for n in 3..=5 {
            for word in w.of_len(n) {
                let clue = w.clue(word).unwrap();
                assert!(!blocklist().contains(word), "{word}");
                assert!(!adult_clue(clue), "{word}: {clue}");
                let clue = padded_words(clue);
                for t in NEVER_IN_A_CLUE {
                    assert!(!clue.contains(&format!(" {t} ")), "{word}: {clue}");
                }
            }
        }
    }
}
