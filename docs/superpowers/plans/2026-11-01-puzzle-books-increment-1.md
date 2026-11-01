# Puzzle books, increment 1: word-search book — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `deckle puzzles search` builds a word-search book as a PDF for the Kindle Scribe and
writes it to a folder or a connected Kindle.

**Architecture:**
- **Word search generator:** `puzzle::search` takes its size, allowed directions and word
  counts as options. The Daily keeps calling the old `fill`, which passes its old values, so
  its output doesn't change.
- **New `puzzle::book` module:**
  - makes the book model (title page, puzzles, answers);
  - renders it with a fixed-geometry `pdf-writer` renderer, using font helpers lifted from the
    Daily's `pdf.rs`.
- **CLI:** a new `puzzles` subcommand delivers the PDF with `deliver_named_on`.

**Tech Stack:** Rust, `pdf-writer` 0.15, `ab_glyph`, `rand` `StdRng`, clap, PDFium (tests
only, via `deckle-pdf`).

**Spec:** `docs/superpowers/specs/2026-11-01-puzzle-books-design.md` (§3.1, §3.2, §4, §5, §6.1,
§7).

## Global Constraints

- **The Daily stays unchanged:**
  - For any seed, `search::fill(words, themes, avoid, rng)` returns exactly what it did
    before.
  - The Daily's PDF bytes are unchanged.
- **Page:** the default screen is `kindle-scribe`, 446.4×595.2 pt, with 28 pt margins.
- **Smallest word-search cell:** 18 pt. No grays and no link annotations on puzzle pages.
- **Sizes:**
  - `--size` from 8 to 20, default 12;
  - `--count` from 1 to 200, default 10;
  - `--difficulty` `easy|medium|hard`, default `medium`.
- **Words per puzzle:** `clamp(size² / 9, 8, 40)`. The first try needs at least 80% of that;
  the fallback needs at least 8.
- **Directions by difficulty:**
  - `easy`: across and down;
  - `medium`: across, down, ↘ and ↗;
  - `hard`: all 8.
- **File name:** `<Title> <YYYY-MM-DD> <seed4>.pdf`, sanitized to 120 characters. An existing
  file is refused unless `--replace` is passed.
- **Reproducibility:** the default seed is a hash of the options and the date (FNV-1a 64, not
  `DefaultHasher`). The same seed and date give identical bytes.
- **Code style:** SPDX header, doc comments that cite spec sections as `(puzzle books §N)`.

## Review Focus

1. **Several user words fold to the same letters,** e.g. "Wi-Fi" and "wifi". Dedupe them
   after folding, so the grid never hides two copies. Test in Task 2.
2. **A user word that contains another,** e.g. "SUN" and "SUNSET". The existing
   inside-another rule skips one, and the book must still reach its minimum word count.
   Test in Task 2.
3. **A `--words` file with non-ASCII words** ("café", "naïve"). Fold them with `deunicode`
   and keep only A–Z. Test in Task 2.
4. **Size 20 at `hard`** has the most words and the most directions. It must still fill and
   fit on the page within the 18 pt minimum. Tests in Tasks 1 and 3.
5. **`--to device` with no Kindle connected.** Give the existing "no Kindle connected" error,
   with no partial file left behind. Test in Task 4 (the folder path; the no-device message
   comes from `resolve_target`).

---

### Task 1: Search generator takes options

**Files:**
- Modify: `crates/deckle-digest/src/puzzle/search.rs`
- Modify: `crates/deckle-digest/src/bundle.rs:246-249` (new `Way` variants map to `"left"`,
  `"up"`, `"up-left"` and `"down-left"`; the Daily never emits them)

**Interfaces — produces:**
```rust
pub enum Way { Across, Down, DownRight, UpRight, Left, Up, UpLeft, DownLeft }
impl Way { pub const ALL: [Way; 4]; /* forward, unchanged */ pub const EVERY: [Way; 8]; }
pub struct Options<'a> { pub size: usize, pub ways: &'a [Way], pub max: usize, pub min: usize }
impl Options<'_> { pub const DAILY: Options<'static>; /* 9, &Way::ALL, 10, 8 */ }
/// `first` placed before `pool` (callers have validated both); `None` below `min`.
pub fn fill_with(first: &[String], pool: &[String], opts: &Options, rng: &mut StdRng) -> Option<Search>
```
`fill` builds `first` and `pool` exactly as today, then calls `fill_with(.., &Options::DAILY, rng)`.

- [ ] **Step 1: Freeze the Daily's output before changing anything.** Add the test
  `daily_output_is_unchanged`. It hashes the `Debug` output of `fill(embedded, &[], &{}, day_rng(d))`
  for d in 0..60, plus one themed fill, and asserts the result equals a constant captured
  from the current code. Run it once to print the hash, paste the hash in, then confirm it
  passes on the unchanged code.
- [ ] **Step 2: Write the failing tests.**
  - `sizes_and_ways_fill`: for every size in 8..=20, every difficulty's ways, and seeds 0..10:
    - `fill_with` returns `Some`;
    - the rows are `size×size` upper case;
    - each word reads once from its start along its way, counted over that difficulty's ways;
    - no 3+ letter blocklisted word appears along any of the 8 directions;
    - the word count is at least `min`.
  - `easy_uses_only_across_and_down`.
- [ ] **Step 3: Implement.**
  - The grid becomes a `size×size` `Vec<u8>` (index `r*size+c`).
  - `spots` enumerates `r, c, way in opts.ways`, in that order, so the Daily's RNG draws are
    unchanged.
  - `reads` counts over `opts.ways`. `lens` filters `<= size`.
  - Keep `pool.shuffle` and the stable length sort inside the attempts loop, as today.
  - `Search::cells` works for every `Way`, since it uses `step()`.
- [ ] **Step 4: Run the tests.**
  `cargo test -p deckle-digest --lib puzzle::search`. Expect all to pass, including
  `daily_output_is_unchanged` and the existing tests.
- [ ] **Step 5: Commit.** `feat(puzzle): word search takes a size and directions`

### Task 2: Book model

**Files:**
- Create: `crates/deckle-digest/src/puzzle/book/mod.rs`
- Modify: `crates/deckle-digest/src/puzzle/mod.rs` (`pub mod book;`)

**Interfaces — consumes:** Task 1. **Produces:**
```rust
pub enum Kind { Search }
pub enum Difficulty { Easy, Medium, Hard }          // clap ValueEnum lives in the CLI
pub struct BookOptions { pub kind: Kind, pub count: usize, pub size: usize, pub difficulty: Difficulty,
    pub words: Option<Vec<String>>, pub title: Option<String>, pub seed: Option<u64>, pub date: NaiveDate }
pub struct Book { pub title: String, pub subtitle: String /* "12×12 · medium · 10 puzzles" */,
    pub date: NaiveDate, pub seed: u64, pub puzzles: Vec<search::Search>, pub warnings: Vec<String> }
pub enum BookError { Size { min: usize, max: usize }, Count, TooFewWords(usize) }  // Display = the CLI message
pub fn words_from_file(text: &str) -> Vec<String>   // fold with deunicode, keep a–z, drop # comments, dedupe, keep order
pub fn default_seed(o: &BookOptions) -> u64          // FNV-1a over kind, count, size, difficulty, words, title, date
pub fn make(o: &BookOptions, words: &Words) -> Result<Book, BookError>
pub fn file_name(b: &Book) -> String                 // "<title> <date> <seed4>.pdf", via deliver::document-style sanitizing
```

- **Per puzzle `i`:**
  - The RNG is `StdRng::seed_from_u64(seed ^ (i as u64) << 20)`.
  - `want = clamp(size²/9, 8, 40)`.
- **`first`:**
  - With user words: `want` words, taken from the user list shuffled once per book, starting
    at offset `i*want` and wrapping. Only words of length 3..=size, with no repeats within a
    puzzle.
  - Without: empty.
- **`pool`:** the built-in 4–5-letter words, minus the user words.
- **Calls:**
  1. `fill_with` with `min = ceil(0.8*want)`, `max = want`.
  2. On `None`, again with `min = 8`.
  3. On `None` again, retry with seed `+1` up to 8 times, then return an error.
- **Warning:** "puzzle N is mostly built-in words" when fewer than half its words are user
  words.
- **Default title:** "Word Search".

- [ ] **Step 1: Write the failing tests** (`#[cfg(test)]` in `book/mod.rs`):
  - `same_options_same_book`;
  - `count_and_size_respected` (count 3, size 15: 3 puzzles of 15 rows);
  - `user_words_lead_and_repeat_only_across_puzzles` (12 words, size 12, count 3: each
    puzzle's theme words are distinct and drawn from the list);
  - `words_from_file_folds_and_dedupes`:
    - `"Café\nWi-Fi\nwifi\n# x\n\nnaïve"` → `["cafe","wifi","naive"]`;
  - `nested_user_words_still_fill`:
    - list `["sun","sunset","moon","star","comet","orbit","venus","mars","pluto"]`;
  - `size_out_of_range_is_refused` (7 and 21);
  - `file_name_is_dated_and_seeded`:
    - title "Word Search", date 2026-11-01, seed `0x7f3a…` → `"Word Search 2026-11-01 7f3a.pdf"`.
- [ ] **Step 2: Run** `cargo test -p deckle-digest --lib puzzle::book`. Expect it to fail to
  compile.
- [ ] **Step 3: Implement** to the interfaces above.
- [ ] **Step 4: Run the tests.** They pass.
- [ ] **Step 5: Commit.** `feat(puzzle): a word-search book model`

### Task 3: Book PDF

**Files:**
- Create: `crates/deckle-digest/src/puzzle/book/pdf.rs`
- Modify: `crates/deckle-digest/src/pdf.rs`:
  - lift `shape(font, text)` out of `Fonts::shape`;
  - lift the per-face font embedding out of `write` into
    `pub(crate) fn embed_font(pdf, alloc: &mut dyn FnMut() -> Ref, face: EmbedFace)`;
  - make `REGULAR`, `BOLD`, `deflate` and `glyph_bytes` `pub(crate)`. The Daily's allocation
    order stays the same.
- Test: `crates/deckle-digest/tests/it/puzzle_book.rs` (+ `mod` line in `tests/it/main.rs`)

**Interfaces — consumes:** Task 2's `Book`. **Produces:**
```rust
pub enum PageSize { KindleScribe, Screen(crate::pdf::Screen) }   // serde/clap name "kindle-scribe"
impl PageSize { pub fn page(self) -> (f32, f32) }                // KindleScribe = (446.4, 595.2)
pub fn render(b: &Book, page: PageSize) -> Result<Vec<u8>, BookError>  // BookError::Size when cells < 18 pt
```

**Layout (the coordinates are the builder's):**
- **Title page:** the title in bold at 28 pt, then the subtitle, the date ("1 November 2026")
  and "Made with Deckle".
- **Puzzle page:**
  - "No. N" in bold at 18 pt;
  - the grid, full content width, unless the word list needs the height;
  - the word list in 3–4 columns at 11 pt;
  - "Answers on page P" at 9 pt at the foot, as plain text.
- **Answer pages:** 2×2 per page. Each answer is "No. N" plus the grid. A capsule outline
  (0.75 pt) runs from the first to the last cell centre of each word, with rounded ends of
  radius 0.42 × cell. Non-answer letters print at 60% size.
- **Outline:** "Title", "No. 1…N", "Answers". The Info dictionary title is the book title.
  There are no annotations.
- **Size check:** if `cell = min(grid_w, grid_h_available)/size < 18`, return
  `BookError::Size` with the largest size that fits.

- [ ] **Step 1: Write the failing tests** (PDFium read-back, as in `tests/it/build.rs:1185`):
  - `book_reads_back`:
    - count 4, size 12;
    - the page is 446.4×595.2 ±1;
    - pages = 1 + 4 + 1;
    - page 2's text contains "No. 1" and every word of puzzle 1 in upper case;
    - bookmarks = 6.
  - `same_book_same_bytes`.
  - `no_links_on_any_page`: no annotations on any page (`page.annotations().len() == 0`).
  - `size_20_hard_fits_the_scribe`.
  - `a_small_screen_refuses_a_big_grid`: `Screen(RemarkablePaperProMove)` at size 20 gives
    `Err(Size{..})`, or `Ok` if it fits. Assert that it never produces a cell under 18 pt.
- [ ] **Step 2: Run** `cargo test -p deckle-digest --test it puzzle_book`. Expect it to fail.
- [ ] **Step 3: Implement** the lifts in `pdf.rs`, then `book/pdf.rs`.
- [ ] **Step 4: Run** the `puzzle_book` tests, plus
  `cargo test -p deckle-digest --test it the_pdf_edition` and `--lib pdf`. All pass.
- [ ] **Step 5: Commit.** `feat(puzzle): render a puzzle book as a PDF for the Scribe`

### Task 4: `deckle puzzles` CLI and docs

**Files:**
- Modify: `crates/deckle-cli/src/cli.rs` (the `Puzzles { action: PuzzlesCommand }` enum,
  with `Search { … }`)
- Modify: `crates/deckle-cli/src/lib.rs` (dispatch)
- Create: `crates/deckle-cli/tests/it/puzzle_book.rs` (+ `mod` line in `tests/it/main.rs`)
- Create: `docs/puzzle-books.md`
- Modify: `README.md` / the docs index, if it lists commands

**Flags:**
- `--count`, `--size`, `--difficulty`, `--words FILE`, `--title`, `--seed`,
  `--date YYYY-MM-DD`;
- `--screen` (default `kindle-scribe`);
- `-o DIR` or `--to device|<folder>`, exactly one required;
- `--replace`;
- the global `--json`.

**Behavior:**
- The name comes from `file_name`.
- **Folder targets:** if the file exists and `--replace` isn't given, the command refuses with
  "<name> is already there; pass --replace to overwrite it (its ink may stay)".
- `--to` goes through `resolve_target` + `deliver_named_on`.
- **Text output:** "wrote <path> (N pages, M puzzles, seed S)", then "remake it with:
  deckle puzzles search … --seed S --date D".
- **JSON output:** `{path, pages, puzzles, seed, date, warnings}`.

- [ ] **Step 1: Write the failing tests:**
  - `puzzles_search_writes_a_pdf` (`-o tmp --count 2 --json`): the file exists, starts
    `%PDF-`, and the JSON has `pages == 4`.
  - `a_second_run_is_refused_without_replace`.
  - `to_a_folder_delivers`.
  - `bad_size_says_the_range`.
- [ ] **Step 2: Run**
  `cargo test -p deckle-cli --test it puzzle_book`. Expect it to fail.
- [ ] **Step 3: Implement**, then write `docs/puzzle-books.md`. It covers the command table,
  the Scribe how-to, the naming and ink note, and what comes next.
- [ ] **Step 4: Run** the tests, then:
  - `cargo clippy -p deckle-digest -p deckle-cli --all-targets -- -D warnings`;
  - `cargo fmt --check`.
- [ ] **Step 5: Commit.** `feat(cli): deckle puzzles search makes a write-on book`

### Task 5: Device check and PR

- [ ] **Step 1:** Build with `CARGO_TARGET_DIR=<scratch>/target` and run
  `deckle puzzles search --count 10 --to device` on the Scribe. Write in ink, turn the page
  and reopen the book. Record the result in the PR. If the Scribe isn't connected, say so in
  the PR and leave it for the owner.
- [ ] **Step 2:** Push the branch and open the PR with
  `gh pr create --repo CasaVargas/deckle --head <branch>`.
