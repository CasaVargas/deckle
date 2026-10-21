// SPDX-License-Identifier: GPL-3.0-or-later
//! Series and reading order (series design §2): the library grouped by `metadata.series`,
//! each series in reading order with the whole numbers it's missing. Computed from the books
//! every time; nothing is stored.

use unicode_normalization::UnicodeNormalization;

use crate::Book;

/// One series: its books in reading order, with gaps as [`Entry::Missing`].
#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    /// The most common spelling in the group; ties go to the first added.
    pub name: String,
    /// [`series_key`] of the name: what groups the books.
    pub key: String,
    pub entries: Vec<Entry>,
    /// Whole numbers from 1 to the highest whole number owned that no book has.
    pub missing: Vec<u32>,
    /// Numbers held by more than one book (editions).
    pub duplicates: Vec<f32>,
}

/// A gap's few bytes don't matter beside a series' handful of books.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    /// `index` is `None` for an unnumbered book (or a number outside 0–1000).
    Book {
        index: Option<f32>,
        book: Book,
    },
    Missing(u32),
}

impl Series {
    pub fn books(&self) -> impl Iterator<Item = (Option<f32>, &Book)> {
        self.entries.iter().filter_map(|e| match e {
            Entry::Book { index, book } => Some((*index, book)),
            Entry::Missing(_) => None,
        })
    }

    pub fn unnumbered(&self) -> usize {
        self.books().filter(|(i, _)| i.is_none()).count()
    }
}

/// What groups series names: NFC, case-folded, whitespace collapsed, leading and trailing
/// punctuation trimmed. A leading "The" stays.
pub fn series_key(name: &str) -> String {
    let folded = name.nfc().collect::<String>().to_lowercase();
    let collapsed = folded.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_owned()
}

/// Above this a series number is taken for bad metadata.
const MAX_INDEX: f32 = 1000.0;

/// A usable series number: 0 to [`MAX_INDEX`], else none.
fn usable(index: Option<f32>) -> Option<f32> {
    index.filter(|i| (0.0..=MAX_INDEX).contains(i))
}

fn whole(i: f32) -> Option<u32> {
    (i.fract() == 0.0).then_some(i as u32)
}

/// Every series in `books`, sorted by key.
pub fn series(books: &[Book]) -> Vec<Series> {
    let mut groups: Vec<(String, Vec<&Book>)> = Vec::new();
    for b in books {
        let Some(s) = &b.sidecar.metadata.series else {
            continue;
        };
        let key = series_key(&s.name);
        if key.is_empty() {
            continue;
        }
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, g)) => g.push(b),
            None => groups.push((key, vec![b])),
        }
    }
    let mut out: Vec<Series> = groups.into_iter().map(|(k, g)| one_series(k, g)).collect();
    out.sort_by(|a, b| a.key.cmp(&b.key));
    out
}

fn one_series(key: String, books: Vec<&Book>) -> Series {
    let index = |b: &Book| usable(b.sidecar.metadata.series.as_ref().and_then(|s| s.index));
    let name = shown_name(&books);

    let mut numbered: Vec<(f32, &Book)> = books
        .iter()
        .filter_map(|b| index(b).map(|i| (i, *b)))
        .collect();
    numbered.sort_by(|(i, a), (j, b)| {
        i.total_cmp(j)
            .then_with(|| a.sidecar.added.cmp(&b.sidecar.added))
            .then_with(|| a.sidecar.metadata.title.cmp(&b.sidecar.metadata.title))
            .then_with(|| a.sidecar.id.cmp(&b.sidecar.id))
    });
    let mut unnumbered: Vec<&Book> = books
        .iter()
        .copied()
        .filter(|b| index(b).is_none())
        .collect();
    unnumbered.sort_by_cached_key(|b| {
        let t = &b.sidecar.metadata.title;
        (t.to_lowercase(), t.clone(), b.sidecar.id.clone())
    });

    let owned: Vec<u32> = numbered.iter().filter_map(|(i, _)| whole(*i)).collect();
    let top = owned.iter().copied().max().unwrap_or(0);
    let missing: Vec<u32> = (1..=top).filter(|n| !owned.contains(n)).collect();

    let mut duplicates: Vec<f32> = Vec::new();
    for (i, _) in &numbered {
        let mut ids: Vec<&str> = numbered
            .iter()
            .filter(|(j, _)| j == i)
            .map(|(_, b)| b.sidecar.id.as_str())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        if ids.len() > 1 && !duplicates.contains(i) {
            duplicates.push(*i);
        }
    }

    let mut entries = Vec::with_capacity(books.len() + missing.len());
    let mut gaps = missing.iter().copied().peekable();
    for (i, b) in numbered {
        while let Some(n) = gaps.next_if(|&n| (n as f32) < i) {
            entries.push(Entry::Missing(n));
        }
        entries.push(Entry::Book {
            index: Some(i),
            book: b.clone(),
        });
    }
    entries.extend(gaps.map(Entry::Missing));
    entries.extend(unnumbered.into_iter().map(|b| Entry::Book {
        index: None,
        book: b.clone(),
    }));

    Series {
        name,
        key,
        entries,
        missing,
        duplicates,
    }
}

/// The most common spelling (trimmed); ties go to the first added.
fn shown_name(books: &[&Book]) -> String {
    let mut spellings: Vec<(&str, usize, &str)> = Vec::new();
    for b in books {
        let name = b
            .sidecar
            .metadata
            .series
            .as_ref()
            .map_or("", |s| s.name.trim());
        let added = b.sidecar.added.as_str();
        match spellings.iter_mut().find(|(n, _, _)| *n == name) {
            Some((_, count, first)) => {
                *count += 1;
                *first = (*first).min(added);
            }
            None => spellings.push((name, 1, added)),
        }
    }
    spellings
        .into_iter()
        .min_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.cmp(b.2)))
        .map(|(n, _, _)| n.to_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Metadata, SendRecord, SeriesRef, Sidecar};
    use std::path::PathBuf;

    fn book(id: &str, title: &str, series: Option<(&str, Option<f32>)>, added: &str) -> Book {
        Book {
            dir: PathBuf::from(format!("/lib/{id}")),
            sidecar: Sidecar {
                schema: 1,
                id: id.into(),
                added: added.into(),
                metadata: Metadata {
                    title: title.into(),
                    authors: vec!["Ursula K. Le Guin".into()],
                    series: series.map(|(name, index)| SeriesRef {
                        name: name.into(),
                        index,
                    }),
                    ..Default::default()
                },
                ..Default::default()
            },
        }
    }

    fn numbered(id: &str, index: f32) -> Book {
        book(id, id, Some(("Hainish", Some(index))), "2026-01-01")
    }

    fn one(books: &[Book]) -> Series {
        let mut all = series(books);
        assert_eq!(all.len(), 1, "{all:?}");
        all.remove(0)
    }

    /// The entries as `1`, `2.5`, `-3` (missing), `u:<title>` (unnumbered).
    fn shape(s: &Series) -> Vec<String> {
        s.entries
            .iter()
            .map(|e| match e {
                Entry::Book { index: Some(i), .. } => format!("{i}"),
                Entry::Book { index: None, book } => format!("u:{}", book.sidecar.metadata.title),
                Entry::Missing(n) => format!("-{n}"),
            })
            .collect()
    }

    #[test]
    fn key_folds_case_whitespace_and_edge_punctuation() {
        assert_eq!(series_key("The Expanse"), series_key("the  expanse"));
        assert_eq!(series_key("  The\tExpanse "), "the expanse");
        assert_eq!(series_key("Discworld."), "discworld");
        assert_eq!(series_key("\"Discworld\""), "discworld");
        assert_eq!(series_key("Mr. Men & Little Miss"), "mr. men & little miss");
    }

    #[test]
    fn key_is_nfc() {
        // "é" precomposed vs "e" + combining acute.
        assert_eq!(
            series_key("Les Rougon-Macquart \u{e9}"),
            series_key("Les Rougon-Macquart e\u{301}")
        );
    }

    #[test]
    fn key_keeps_a_leading_the() {
        assert_ne!(series_key("Expanse"), series_key("The Expanse"));
    }

    #[test]
    fn books_group_by_key_and_others_are_left_out() {
        let books = vec![
            book("a", "A", Some(("The Expanse", Some(1.0))), "2026-01-01"),
            book("b", "B", Some(("the  expanse", Some(2.0))), "2026-01-02"),
            book("c", "C", Some(("Expanse", Some(1.0))), "2026-01-03"),
            book("d", "D", None, "2026-01-04"),
            book("e", "E", Some(("  ", Some(1.0))), "2026-01-05"),
        ];
        let all = series(&books);
        let keys: Vec<_> = all.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(keys, ["expanse", "the expanse"]);
        assert_eq!(all[1].books().count(), 2);
    }

    #[test]
    fn shown_name_is_the_most_common_spelling() {
        let books = vec![
            book("a", "A", Some(("the expanse", Some(1.0))), "2026-01-01"),
            book("b", "B", Some(("The Expanse", Some(2.0))), "2026-01-02"),
            book("c", "C", Some(("The Expanse", Some(3.0))), "2026-01-03"),
        ];
        assert_eq!(one(&books).name, "The Expanse");
    }

    #[test]
    fn shown_name_tie_goes_to_the_first_added() {
        let books = vec![
            book("a", "A", Some(("The Expanse", Some(1.0))), "2026-03-01"),
            book("b", "B", Some(("the expanse", Some(2.0))), "2026-01-01"),
        ];
        assert_eq!(one(&books).name, "the expanse");
    }

    #[test]
    fn gap_between_one_and_the_highest_is_missing() {
        let s = one(&[numbered("a", 1.0), numbered("b", 2.0), numbered("d", 4.0)]);
        assert_eq!(s.missing, [3]);
        assert_eq!(shape(&s), ["1", "2", "-3", "4"]);
    }

    #[test]
    fn nothing_is_missing_past_the_highest() {
        let s = one(&[
            numbered("a", 1.0),
            numbered("b", 2.0),
            numbered("c", 3.0),
            numbered("d", 4.0),
        ]);
        assert!(s.missing.is_empty());
        assert_eq!(shape(&s), ["1", "2", "3", "4"]);
    }

    #[test]
    fn numbers_below_the_lowest_are_missing() {
        let s = one(&[numbered("c", 3.0), numbered("d", 4.0)]);
        assert_eq!(s.missing, [1, 2]);
        assert_eq!(shape(&s), ["-1", "-2", "3", "4"]);
    }

    #[test]
    fn a_prequel_zero_is_owned_and_never_missing() {
        let s = one(&[numbered("z", 0.0), numbered("b", 2.0)]);
        assert_eq!(s.missing, [1]);
        assert_eq!(shape(&s), ["0", "-1", "2"]);
        let s = one(&[numbered("a", 1.0)]);
        assert!(s.missing.is_empty(), "0 isn't missing without it");
    }

    #[test]
    fn a_fraction_is_placed_but_neither_creates_nor_fills_a_gap() {
        let s = one(&[numbered("a", 1.0), numbered("n", 2.5), numbered("d", 4.0)]);
        assert_eq!(s.missing, [2, 3]);
        assert_eq!(shape(&s), ["1", "-2", "2.5", "-3", "4"]);
        let s = one(&[numbered("a", 1.0), numbered("n", 3.5)]);
        assert!(
            s.missing.is_empty(),
            "3.5 doesn't extend the range: {:?}",
            s.missing
        );
        assert_eq!(shape(&s), ["1", "3.5"]);
    }

    #[test]
    fn two_books_with_one_number_are_editions_not_a_gap() {
        let s = one(&[numbered("a", 1.0), numbered("b1", 2.0), numbered("b2", 2.0)]);
        assert!(s.missing.is_empty());
        assert_eq!(s.duplicates, [2.0]);
        assert_eq!(shape(&s), ["1", "2", "2"]);
    }

    #[test]
    fn the_same_book_twice_is_not_two_editions() {
        let s = one(&[numbered("a", 1.0), numbered("a", 1.0)]);
        assert!(s.duplicates.is_empty());
    }

    #[test]
    fn unnumbered_books_come_last_and_are_ignored_by_gaps() {
        let books = vec![
            book("u2", "Zebra", Some(("Hainish", None)), "2026-01-01"),
            book("u1", "Aardvark", Some(("Hainish", None)), "2026-01-01"),
            numbered("a", 1.0),
        ];
        let s = one(&books);
        assert!(s.missing.is_empty());
        assert_eq!(s.unnumbered(), 2);
        assert_eq!(shape(&s), ["1", "u:Aardvark", "u:Zebra"]);
    }

    #[test]
    fn negative_or_huge_numbers_are_no_number() {
        let s = one(&[
            numbered("a", 1.0),
            numbered("neg", -1.0),
            numbered("big", 1001.0),
            numbered("nan", f32::NAN),
        ]);
        assert!(s.missing.is_empty());
        assert_eq!(s.unnumbered(), 3);
        assert_eq!(shape(&s), ["1", "u:big", "u:nan", "u:neg"]);
        let s = one(&[numbered("top", 1000.0)]);
        assert_eq!(s.missing.len(), 999, "1000 itself is a number");
    }

    #[test]
    fn equal_numbers_order_by_added() {
        let books = vec![
            book("late", "Late", Some(("Hainish", Some(1.0))), "2026-05-01"),
            book("early", "Early", Some(("Hainish", Some(1.0))), "2026-02-01"),
        ];
        let s = one(&books);
        let ids: Vec<_> = s.books().map(|(_, b)| b.sidecar.id.as_str()).collect();
        assert_eq!(ids, ["early", "late"]);
    }

    #[test]
    fn series_sort_by_key() {
        let books = vec![
            book("a", "A", Some(("Zamonia", Some(1.0))), "2026-01-01"),
            book(
                "b",
                "B",
                Some(("anne of green gables", Some(1.0))),
                "2026-01-01",
            ),
            book("c", "C", Some(("Discworld", Some(1.0))), "2026-01-01"),
        ];
        let names: Vec<_> = series(&books).into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["anne of green gables", "Discworld", "Zamonia"]);
    }

    #[test]
    fn books_keep_their_sends() {
        let mut b = numbered("a", 1.0);
        b.sidecar.sends.push(SendRecord {
            device_name: "Kindle Scribe".into(),
            ..Default::default()
        });
        let s = one(&[b.clone()]);
        assert_eq!(s.books().next().unwrap().1, &b);
    }
}
