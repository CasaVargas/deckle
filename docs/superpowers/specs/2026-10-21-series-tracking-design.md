# Series tracking and reading order — design

**Status:** approved by owner (2026-10-21) for [#40](https://github.com/prjoni99/deckle/issues/40) ("Series tracking and reading order") ·
**Date:** 2026-10-21 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-21-library-and-device-view-design.md` (the folder library, `deckle.json`)

Deckle's library already records each book's series name and number. This issue adds a command
that groups the library by series, lists each series in reading order, and says which books are
missing ("Hainish Cycle: missing 3").

## 1. Decision

- **Computed from the folder library only.** Series come from `metadata.series` in each
  `deckle.json`, which is refreshed from the source whenever the source changes. Nothing new
  is stored: there's no series index or cache file, so the "it's just folders" promise holds.
- **CLI only for now.** The app has no library screen yet, and building one just for series
  would make this a GUI project. The app gets series when a library view exists (#41 search is
  the natural place for one).
- **Gaps cover only missing whole numbers inside the series' known range.** Deckle doesn't know
  how long a series is, so it never reports "book 6 not owned" past the highest number you
  have.
- Rejected: looking up the full series list online (Open Library has no reliable series data,
  and #28's fill is opt-in). Rejected: letting a series be edited by hand in Deckle. You edit
  the source's metadata, and a scan picks it up.

## 2. Rules

**Grouping.** A book belongs to a series when `metadata.series.name` isn't empty. Names are
matched by a key: NFC-normalised, case-folded, internal whitespace collapsed, and leading and
trailing punctuation trimmed. "The Expanse" and "the  expanse" are one series. A leading "The"
counts, so "Expanse" and "The Expanse" stay separate. The name shown is the most common
spelling in the group, with ties going to the first added.

**Reading order.**
1. Numbered books, by `index` ascending. Equal numbers are sorted by `added`.
2. Then books with no number, by title.

**Gap rules.**

| Case | Treatment |
|---|---|
| Whole numbers between 1 and the highest number owned that no book has | **missing** |
| Highest number owned is 4, nothing past it | not reported (length unknown) |
| Lowest number owned is 3 | 1 and 2 reported **missing** |
| `0` (a prequel) | counts as owned, never reported missing |
| A fraction (`2.5`, a novella) | owned and placed in order. Never creates or fills a gap: owning 2.5 does not fill 2 or 3 |
| Two books with the same number (different `id`) | both listed, marked **2 editions**, not a gap |
| A book with no number | listed as **unnumbered** and ignored by gap detection |
| A negative number or above 1000 | treated as no number (bad metadata) |
| Omnibus editions (books 1–3 in one) | can't be detected: the metadata holds one number. See §5 |

## 3. Surface

```
deckle library series [NAME] [--missing] [--json]
```

- With no `NAME`, prints every series, sorted by name: `Hainish Cycle — 4 books, missing 3`.
- With `NAME` (matched by the same key, or by a unique prefix of it), prints that series in
  reading order. Each line shows the number, title, author, and the Kindles it was last sent to
  (from `sends`).

  ```
  Hainish Cycle (Ursula K. Le Guin)
    1    Rocannon's World          Kindle Scribe
    2    Planet of Exile
    3    — missing —
    4    The Left Hand of Darkness Kindle Scribe, Paperwhite
    5    The Dispossessed          Kindle Scribe
         The Birthday of the World (unnumbered)
  ```
- `--missing` lists only series that have gaps, each with its missing numbers.
- `--json`: `[{ "name", "key", "books": [{ "index", "id", "title", "authors", "folder",
  "sends": [...] }], "missing": [3], "duplicates": [2], "unnumbered": n }]`.
- `deckle library list` also gets a `--series` flag that groups its output under series
  headings, with books outside a series last. The default output doesn't change.

## 4. Where it sits

New module `crates/deckle-library/src/series.rs`. It's a pure function of `&[Book]`, so it
needs no filesystem access in tests:

```rust
pub struct Series { pub name: String, pub key: String, pub entries: Vec<Entry>,
                    pub missing: Vec<u32>, pub duplicates: Vec<f32> }
pub enum Entry { Book { index: Option<f32>, book: Book }, Missing(u32) }
pub fn series(books: &[Book]) -> Vec<Series>;
pub fn series_key(name: &str) -> String;
```

`deckle-cli` calls `Library::list()` and then `series::series`. Nothing outside these two
crates changes.

## 5. Known limits

- An omnibus counts only under the number in its metadata. If that's 1, books 2 and 3 are
  reported missing.
- The gap check runs only from 1 to the highest number owned. A series where you have only
  book 7 reports 1–6 missing. That's correct, but it's noisy for long series you've
  deliberately started partway through.
- The series comes from what the publisher wrote in the source. Books with no
  `belongs-to-collection` or Calibre series metadata won't group until that metadata is
  filled in (by #28's fill or by hand).

## 6. Acceptance

- `series()` unit tests cover every row of the gap table, plus key matching (case, whitespace,
  NFC, "The"), the shown-name choice, and reading-order ties.
- CLI test: a temp library with four books across two series and one standalone prints the
  expected text, the expected `--json` and the expected `--missing` output. `library list`
  without `--series` is byte-identical to today's output.
- `cargo test -p deckle-library -p deckle-cli` passes.
