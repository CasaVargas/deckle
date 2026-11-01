# Puzzle books, increment 2: crossword book — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `deckle puzzles crossword` builds a crossword book (5×5 to 15×15) as a write-on PDF
for the Kindle Scribe.

**Architecture:**
- **Word list:** the offline builder (`data/wordwise`) gains a `crossword` mode. It writes
  `words-long.tsv.gz`, with entries of 3–15 letters, each with its frequency (zipf) and a clue.
- **New `puzzle::cross` module:** makes symmetric block patterns, then fills them with a
  bitset constraint search that runs on a step budget, not a clock.
- **Book:** the book model gains a `Puzzle` enum. The PDF renderer gets a crossword page,
  where clues that don't fit continue on a following page, and crossword answer pages.
- **What stays as it is:** the Daily's mini (`grid.rs`), `words.tsv.gz`, and the
  word-search code.

**Spec:** `docs/superpowers/specs/2026-11-01-puzzle-books-design.md` §3.3, §4, §5, §7.
Increment 1's plan has the book/PDF/CLI conventions.

## Global Constraints

- **Sizes:** `--size` from 5 to 15, default 11. `--words` is refused for crosswords: user
  words have no clues until increment 4.
- **Cells:** at least 22 pt on puzzle pages.
- **Determinism:** same options, seed and date give identical bytes. Search budgets count
  steps, never time.
- **Grid rules:**
  - 180° rotational symmetry;
  - every entry is at least 3 letters;
  - the white squares are connected;
  - no word appears twice in a grid.
- **Clues:** a clue never contains its answer.
- **The Daily stays byte-identical.** Its mini, its word list and the word-search hash are
  unchanged.
- **Difficulty:**
  - `easy`: list words at zipf ≥ 3.5;
  - `medium`: zipf ≥ 3.0;
  - `hard`: the whole list.

  When a size can't fill at a difficulty, the book falls back to the next harder list and
  warns *(builder's call)*.

## Review Focus

1. **A 15×15 at `easy`.** That list is the smallest. The fallback has to produce a book, not
   an `Unfilled` error.
2. **A 15×15 with about 78 clues.** The clues must continue onto the next page with nothing
   clipped, and the "Answers on page N" numbers must still be right.
3. **Long clues** (up to 90 characters) in a narrow column. They wrap; they never overrun the
   column or the page.
4. **A 5×5 book.** It uses mini-style patterns, and its numbering must match `grid::slots_in`.
5. **Determinism under load.** No wall clock anywhere in `cross`.

---

### Task 1: Word list for crosswords

**Files:**
- `data/wordwise/src/lib.rs`: generalize `puzzle_words` into
  `word_list(jsonl, freq, blocklist, &ListRules)`. `puzzle_words` keeps its output.
- `data/wordwise/src/main.rs`: add the `crossword` subcommand. It writes `word\tzipf\tclue`
  lines, with zipf to 2 decimals.
- `data/wordwise/tests/build.rs`
- `data/wordwise/README.md`: a new table row.
- New `crates/deckle-digest/assets/puzzle/words-long.tsv.gz`.

**Interfaces — produces:**
```rust
pub struct ListRules { pub lens: RangeInclusive<usize>, pub min_zipf: fn(usize) -> f64 }
pub fn crossword_words(jsonl, freq, blocklist) -> Vec<(String, f64, String)>
```

**Steps:**
- [ ] **Tests:** `crossword_words_keeps_longer_words`: "bucket" is in the crossword list but
  not in `puzzle_words`. The puzzle list's existing tests stay green.
- [ ] **Implement.** The length thresholds come from the real counts: aim for at least 25k
  entries in all, and at least 1k at each length from 3 to 10.
- [ ] **Build the list** from the downloads. Skim 50 random lines and add any bad ones to the
  blocklist. Record the counts in the README.
- [ ] **Commit:** `feat(wordwise): a 3–15-letter word list for crosswords`

### Task 2: Lexicon, patterns and fill (`puzzle::cross`)

**Files:**
- Create `crates/deckle-digest/src/puzzle/cross.rs`.
- Add `pub mod cross;` to `puzzle/mod.rs`.

**Interfaces — produces:**
```rust
pub struct Lexicon { /* by_len: Vec<Vec<String>> (sorted), zipf, clues */ }
impl Lexicon { pub fn embedded() -> &'static Lexicon; pub fn from_tsv(&str) -> Lexicon;
               pub fn at_least(&self, zipf: f64) -> Lexicon; pub fn clue(&self, w) -> Option<&str> }
pub fn slots(rows: &[String]) -> Vec<grid::Slot>         // any N×N, grid::slots_in's numbering
pub fn pattern(size: usize, rng: &mut StdRng) -> Vec<String>   // '#' black, '.' white
pub struct Crossword { pub rows: Vec<String> /* '#', A–Z */, pub clues: Vec<store::Clue> }
pub fn fill(lex: &Lexicon, size: usize, rng: &mut StdRng) -> Option<Crossword>
```

**Pattern rules:**
- Sizes 5–7 start from the mini templates where the size is 5.
- Otherwise a black-square density target of 10% at 7 and 16% at 15. Blacks are added in
  symmetric pairs from the seed, rejecting any pair that:
  - leaves an entry of 1–2 letters, or
  - disconnects the white squares.
- Entries are capped at 11 letters at sizes up to 13, and at 15 at size 15
  *(builder's call; tuned against the fill rate)*.

**Fill:**
- A bitset index by (length, position, letter).
- It picks the most-constrained slot first and checks ahead that no crossing slot is left
  empty. Words are tried in an order shuffled from the seed, and none is used twice.
- Budget: 200k placements per pattern, then up to 12 fresh patterns.

**Steps:**
- [ ] **Tests:**
  - `slots_match_the_minis` (every template);
  - `patterns_are_valid` (sizes 5–15 × 20 seeds: symmetric, entries ≥3, connected);
  - `fills_every_size` (sizes 5–15 × 5 seeds at the whole list; every entry a list word,
    none twice, every clue present without its answer);
  - `deterministic`;
  - `a_busy_machine_makes_the_same_fill`.
- [ ] **Implement.**
- [ ] **Commit:** `feat(puzzle): crossword patterns and fill at any size`

### Task 3: Crossword books (model + PDF)

**Files:**
- `puzzle/book/mod.rs`:
  - `Kind::Crossword`;
  - `pub enum Puzzle { Search(Search), Crossword(Crossword) }`;
  - `Book.puzzles: Vec<Puzzle>`;
  - `CROSS_SIZES = 5..=15`;
  - the difficulty fallback and its warning.
- `puzzle/book/pdf.rs`: the crossword page and crossword answers. `pages()` counts the
  continuation pages.
- Tests in `book/mod.rs` and `tests/it/puzzle_book.rs`.

**Puzzle page layout:**
- "No. N", then the grid at the content width, capped so it takes at most 62% of the page
  height.
- Square numbers at 25% of the cell, top-left.
- Below the grid, two columns, "Across" then "Down". Each clue is "N. text" at 10 pt, with a
  hanging indent, wrapped to the column width.
- When the columns fill, the clues continue on a following page with two full-height
  columns. That page has no outline entry, and its heading is "No. N (continued)".

**Answers:** filled grids, four per page.

**Steps:**
- [ ] **Tests:**
  - `crossword_book_reads_back`: count 3, size 11. The page count equals `pages()`, the first
    puzzle page shows "No. 1", "Across", "Down" and every clue's number, and there are 1 + 3
    outline entries plus Answers.
  - `fifteen_spills_its_clues_onto_a_second_page`.
  - `no_links`.
  - `same_bytes`.
  - `easy_fifteen_still_builds` (it may warn).
  - `size_out_of_range_is_refused` (4 and 16).
- [ ] **Implement.**
- [ ] **Commit:** `feat(puzzle): crossword books`

### Task 4: CLI, docs, review, PR

**Steps:**
- [ ] **CLI:** `PuzzlesCommand::Crossword { size (default 11), difficulty, book }`.
  - `--words` isn't offered.
  - The remake line names `crossword`.
- [ ] **CLI tests:**
  - `puzzles_crossword_writes_a_pdf` (`--json`, pages, puzzles);
  - `crossword_bad_size_says_the_range`.
- [ ] **Docs:** update `docs/puzzle-books.md` (the crossword section and its options).
- [ ] **Checks:** clippy, fmt, the deckle-digest tests and the CLI puzzle tests.
- [ ] **Review:** a final reviewer on Opus.
- [ ] **PR:** stacked on #284 (base `claude/bold-aryabhata-fedb21`).
- [ ] **Device:** send a crossword book to the Scribe with `--to device`.
