// SPDX-License-Identifier: GPL-3.0-or-later
//! The puzzle's word list: `word<TAB>clue` lines, built offline by `data/wordwise` and
//! embedded gzipped (puzzle design §4).

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::OnceLock;

static EMBEDDED: &[u8] = include_bytes!("../../assets/puzzle/words.tsv.gz");
/// The words the list is built without, also kept out of the word search's filler letters.
static BLOCKLIST: &str = include_str!("../../../../data/wordwise/puzzle-blocklist.txt");

/// The puzzle blocklist (lowercase; `#` lines and blanks skipped), parsed once.
pub fn blocklist() -> &'static HashSet<String> {
    static SET: OnceLock<HashSet<String>> = OnceLock::new();
    SET.get_or_init(|| {
        BLOCKLIST
            .lines()
            .map(|l| l.split('#').next().unwrap_or("").trim().to_lowercase())
            .filter(|l| !l.is_empty())
            .collect()
    })
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

    /// `word<TAB>clue` lines; anything else (blank, no tab, not 3–5 lowercase letters) is skipped.
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
}
