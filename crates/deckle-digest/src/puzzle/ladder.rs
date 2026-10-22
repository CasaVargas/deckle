// SPDX-License-Identifier: GPL-3.0-or-later
//! The word ladder: one 4-letter word into another, a letter a rung (puzzle design §4).

use std::collections::{HashMap, VecDeque};

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use super::words::Words;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ladder {
    pub from: String,
    pub to: String,
    /// One shortest path, both ends included.
    pub path: Vec<String>,
}

/// Opposites and near-pairs. A pair the word list can't make in 3 to 5 rungs is skipped.
pub const PAIRS: &[(&str, &str)] = &[
    ("cold", "warm"),
    ("love", "hate"),
    ("head", "tail"),
    ("wild", "tame"),
    ("lose", "find"),
    ("give", "take"),
    ("rise", "fall"),
    ("east", "west"),
    ("boat", "ship"),
    ("push", "pull"),
    ("dark", "pale"),
    ("lead", "gold"),
    ("ride", "walk"),
    ("bird", "nest"),
    ("sand", "rock"),
    ("wind", "gale"),
    ("lamp", "dark"),
    ("hand", "foot"),
    ("nose", "face"),
    ("ball", "game"),
    ("bean", "soup"),
    ("rose", "bush"),
    ("kind", "mean"),
    ("calm", "wild"),
    ("tall", "wide"),
    ("bark", "bite"),
    ("loud", "soft"),
    ("life", "rest"),
    ("mint", "leaf"),
    ("dawn", "dusk"),
    ("sole", "mate"),
    ("hope", "wish"),
    ("pine", "cone"),
    ("mole", "hill"),
    ("wash", "dirt"),
    ("corn", "crop"),
    ("wave", "hand"),
    ("dusk", "dark"),
    ("fork", "lift"),
    ("weak", "bold"),
    ("mind", "body"),
    ("post", "card"),
    ("ship", "port"),
    ("port", "dock"),
    ("worm", "bird"),
    ("duck", "pond"),
    ("nail", "tack"),
    ("cord", "rope"),
    ("bird", "wing"),
    ("fire", "cool"),
    ("kite", "wind"),
    ("tent", "camp"),
    ("camp", "fire"),
    ("gold", "ring"),
    ("card", "game"),
    ("lock", "door"),
    ("king", "lord"),
    ("home", "work"),
    ("work", "hard"),
    ("moon", "glow"),
    ("dark", "moon"),
    ("hook", "line"),
    ("kick", "ball"),
    ("race", "fast"),
    ("wire", "cord"),
    ("wick", "lamp"),
    ("wash", "face"),
    ("dirt", "dust"),
    ("bowl", "soup"),
    ("food", "meal"),
    ("bold", "meek"),
    ("warm", "cool"),
    ("calm", "rage"),
    ("vase", "rose"),
    ("sand", "dune"),
    ("pole", "mast"),
    ("wood", "barn"),
    ("barn", "farm"),
    ("chop", "wood"),
    ("cart", "road"),
    ("date", "palm"),
    ("wine", "cork"),
    ("cash", "coin"),
    ("coin", "gold"),
    ("meat", "loaf"),
    ("wise", "sage"),
    ("dull", "keen"),
    ("hard", "easy"),
    ("neat", "mess"),
    ("bold", "fear"),
    ("best", "last"),
];

/// One shortest path from `from` to `to` through the list's words of that length, changing a
/// letter at a time; the same path every time for the same list.
pub fn shortest(words: &Words, from: &str, to: &str) -> Option<Vec<String>> {
    let n = from.len();
    if to.len() != n || !words.contains(from) || !words.contains(to) {
        return None;
    }
    // "c_ld" → cold, cord, …: neighbours share a bucket
    let mut buckets: HashMap<String, Vec<&str>> = HashMap::new();
    for w in words.of_len(n) {
        for i in 0..n {
            buckets.entry(wildcard(w, i)).or_default().push(w);
        }
    }
    let start = words.of_len(n).iter().find(|w| *w == from)?.as_str();
    let mut prev: HashMap<&str, &str> = HashMap::from([(start, start)]);
    let mut queue = VecDeque::from([start]);
    while let Some(w) = queue.pop_front() {
        if w == to {
            let mut path = vec![w.to_owned()];
            let mut at = w;
            while at != from {
                at = prev[at];
                path.push(at.to_owned());
            }
            path.reverse();
            return Some(path);
        }
        for i in 0..n {
            for &next in buckets.get(&wildcard(w, i)).into_iter().flatten() {
                if !prev.contains_key(next) {
                    prev.insert(next, w);
                    queue.push_back(next);
                }
            }
        }
    }
    None
}

fn wildcard(w: &str, i: usize) -> String {
    let mut s = w.to_owned();
    s.replace_range(i..=i, "_");
    s
}

/// Today's ladder: the first pair, in an order shuffled by `rng`, that isn't in `recent` and
/// takes 3 to 5 rungs.
pub fn today(words: &Words, rng: &mut StdRng, recent: &[(String, String)]) -> Option<Ladder> {
    let mut pairs: Vec<&(&str, &str)> = PAIRS.iter().collect();
    pairs.shuffle(rng);
    pairs.into_iter().find_map(|&(from, to)| {
        if recent.iter().any(|(f, t)| f == from && t == to) {
            return None;
        }
        let path = shortest(words, from, to)?;
        (3..=5).contains(&(path.len() - 1)).then(|| Ladder {
            from: from.to_owned(),
            to: to.to_owned(),
            path,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn one_apart(a: &str, b: &str) -> bool {
        a.len() == b.len() && a.bytes().zip(b.bytes()).filter(|(x, y)| x != y).count() == 1
    }

    #[test]
    fn cold_to_warm() {
        let words = Words::embedded();
        let path = shortest(words, "cold", "warm").unwrap();
        assert_eq!(path.first().map(String::as_str), Some("cold"));
        assert_eq!(path.last().map(String::as_str), Some("warm"));
        for pair in path.windows(2) {
            assert!(one_apart(&pair[0], &pair[1]), "{path:?}");
        }
        assert!(path.iter().all(|w| words.contains(w)), "{path:?}");
        assert_eq!(path.len() - 1, 4);
        assert_eq!(shortest(words, "cold", "cold").unwrap(), ["cold"]);
        assert_eq!(shortest(words, "cold", "zzzz"), None);
    }

    #[test]
    fn today_respects_length_and_recent() {
        let words = Words::embedded();
        let mut recent: Vec<(String, String)> = Vec::new();
        for day in 0..60 {
            let mut rng = StdRng::seed_from_u64(739_900 + day);
            let l = today(words, &mut rng, &recent).unwrap_or_else(|| panic!("day {day}"));
            assert!((3..=5).contains(&(l.path.len() - 1)), "{l:?}");
            assert_eq!(
                (l.path[0].as_str(), l.path.last().unwrap().as_str()),
                (l.from.as_str(), l.to.as_str())
            );
            assert!(
                !recent.contains(&(l.from.clone(), l.to.clone())),
                "day {day}: {l:?} repeats"
            );
            recent.push((l.from, l.to));
        }
    }

    #[test]
    fn unknown_pair_skipped() {
        // only cold/warm can be made from this list
        let words =
            Words::from_tsv("cold\tx\ncord\tx\nword\tx\nward\tx\nwarm\tx\nlove\tx\nhate\tx\n");
        for day in 0..10 {
            let l = today(&words, &mut StdRng::seed_from_u64(day), &[]).unwrap();
            assert_eq!((l.from.as_str(), l.to.as_str()), ("cold", "warm"));
            assert_eq!(l.path, ["cold", "cord", "word", "ward", "warm"]);
        }
        let recent = [("cold".to_owned(), "warm".to_owned())];
        assert_eq!(today(&words, &mut StdRng::seed_from_u64(1), &recent), None);
    }

    #[test]
    fn pairs_are_four_letters() {
        assert!(PAIRS.len() >= 60);
        for (a, b) in PAIRS {
            assert!(a.len() == 4 && b.len() == 4 && a != b, "{a}/{b}");
        }
    }
}
