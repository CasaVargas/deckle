# Word Search (PR 1: Deckle generator, paper, CLI, bundle) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a daily 9×9 word search to Deckle's puzzle page. It's made once per date, stored in
`puzzles.json`, printed in the EPUB and PDF paper, shown by `deckle digest puzzle`, and
carried in the edition bundle as `puzzle.search`.

**Architecture:** A new `puzzle::search` module fills the grid: words first, then filler letters
checked square by square against an embedded blocklist. `puzzle::make` calls it with its own
date-seeded RNG, after the mini and ladder, so existing days' minis and ladders don't change. It
avoids the mini's words and any search grid from the last 60 days. The section, bundle and CLI
read `store::Puzzle.search`.

**Tech Stack:** Rust (crate `deckle-digest`, `deckle-cli`), `rand::StdRng`, serde, XHTML/CSS
for EPUB, the existing `pdf.rs` table block, the JSON Schema in `docs/formats/`.

**Spec:** `docs/superpowers/specs/2026-10-31-word-search-design.md` (§7 step 1 is this plan;
step 2, the Scribe PDF email, and step 3, Deckle Reader, get their own plans).

**Base:** branch `feat/word-search` from `main` **after CasaVargas/deckle#263 is merged**. It adds
the 60-day no-repeat loop to `puzzle::make` that Task 2 mirrors.

## Global Constraints

- Grid 9×9 (`SIZE = 9`), 8–10 hidden words of 4–5 letters. Each word appears once, all upper
  case in output.
- Directions are forward only: across (→), down (↓), down-right (↘), up-right (↗). Bundle names:
  `"across" | "down" | "down-right" | "up-right"`.
- At most 3 theme words, taken from yesterday's headlines (`theme::pick`). They're never one
  of the mini's answers that day, and they're 4–5 letters long (shorter theme words are skipped).
- Filler letters never spell a blocklisted word (`data/wordwise/puzzle-blocklist.txt`) along any
  line in any of the 8 directions, checked as each square is filled. Accidental ordinary words
  are allowed.
- The fill cap is 200 ms (`SEARCH_CAP`). On failure it drops one word and retries, down to 8
  words, then gives up: `search` is `None` and the rest of the page prints.
- Deterministic per date and inputs, and never equal to a search grid from the last 60 days
  (`store::KEEP_DAYS`).
- Copy is exact:
  - The article title is `Word search · No. {number}` (the mini's number).
  - The instruction is `Find these words; they run across, down, or diagonally, never backwards.`
  - The theme suffix is ` (from yesterday's paper)`.
  - Yesterday's masked grid uses `·` (U+00B7) for squares that aren't part of a hidden word.
- The word search is never scored from Kindle notes, and no notes hint is printed for it.
- Bundle: `puzzle.search` and `puzzle.yesterday.search` are optional and additive, and the
  bundle is version `"1.5"` whenever any section carries `search`.
- `puzzles.json` from before this ship must still load (`#[serde(default)]`), and days stored
  without a search print none.

## Review Focus

1. **A day stored before the upgrade** (`puzzles.json` already has today without `search`): the
   page prints the mini and ladder, with no word search and no "Word search" in yesterday's
   answers. Nothing panics. Covered in Task 3 (`a_day_stored_before_the_search_prints_without_it`).
2. **The mini already took the theme words** (yesterday's only 4–5-letter headline words are
   the mini's answers): the search uses list words only and still fills. Covered in Task 2
   (`search_never_reuses_the_minis_words`).
3. **Theme words of 3 letters** (`harvest` keeps 3–5): they're skipped for the search, never
   placed as a 3-letter hidden word. Covered in Task 2.
4. **Yesterday had no search but today does** (the first day after the upgrade): the bundle's
   `yesterday.search` is `null`, the schema validates, and the reader-facing JSON keeps every
   required key. Covered in Task 4.
5. **Every fill attempt fails** (a tiny word list): `search` is `None`, the section still has
   the mini and ladder, and the bundle omits `search`. Covered in Task 2
   (`a_list_too_small_leaves_the_search_out`).

---

### Task 1: The generator, `puzzle::search`

**Files:**
- Create: `crates/deckle-digest/src/puzzle/search.rs`
- Modify: `crates/deckle-digest/src/puzzle/mod.rs` (add `pub mod search;`)
- Modify: `crates/deckle-digest/src/puzzle/words.rs` (embedded blocklist)
- Test: unit tests in `search.rs` (`#[cfg(test)] mod tests`, the style of `grid.rs`)

**Interfaces:**
- Produces:
  - `pub const SIZE: usize = 9;`
  - `pub const SEARCH_CAP: Duration = Duration::from_millis(200);`
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "kebab-case")] pub enum Way { Across, Down, DownRight, UpRight }`, with `pub fn step(self) -> (isize, isize)`, giving (0,1) (1,0) (1,1) (-1,1).
  - `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] pub struct Hidden { pub word: String /* lower case */, pub row: u8, pub col: u8, pub way: Way, pub theme: bool }`
  - `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] pub struct Search { pub rows: Vec<String> /* SIZE strings of SIZE upper-case letters */, pub words: Vec<Hidden> /* sorted by word */ }`, with `impl Search { pub fn cells(&self, h: &Hidden) -> Vec<(usize, usize)> }`.
  - `pub fn fill(words: &Words, themes: &[String], avoid: &HashSet<String>, rng: &mut StdRng, cap: Duration) -> Option<Search>`
  - In `words.rs`: `pub fn blocklist() -> &'static HashSet<String>`, which reads `include_str!("../../../../data/wordwise/puzzle-blocklist.txt")`: lower case, `#` comments and blank lines skipped, held in a `LazyLock`.

- [ ] **Step 1: Write the failing tests** in `search.rs`. Use `Words::embedded()` and `StdRng::seed_from_u64(739_900 + day)`, as `grid.rs` tests do.
  - `placed_words_read_back`: for 60 consecutive days `fill(words, &[], &HashSet::new(), …, SEARCH_CAP)` is `Some`. For each `Hidden`, reading `rows` along `way` from `(row, col)` gives `word.to_uppercase()`. `8 <= words.len() <= 10`, every word is 4–5 letters and `words.contains`, there are no duplicates, and `rows.len() == SIZE` with every row `SIZE` ASCII uppercase letters.
  - `no_blocklisted_word_in_any_line`: for the same 60 grids, no window of length 3–5 along any of the 8 directions (the 4 `Way`s and their reverses) is in `blocklist()`, unless that window is entirely the cells of one hidden word (hidden words are list words, so this never happens in practice; assert anyway).
  - `themes_go_in_and_avoid_stays_out`: `fill(words, &["court".into(), "state".into()], &avoid, …)`, where `avoid = {"totem","anime"}`, contains `court` and `state` with `theme: true`, and no hidden word is in `avoid`.
  - `deterministic`: two calls with the same seed and inputs are equal.
  - `fills_within_the_cap`: each of the 60 fills takes under `SEARCH_CAP * 2` (wall clock, generous for CI).
- [ ] **Step 2: Run them and see them fail.** Run: `cargo test -p deckle-digest --lib puzzle::search`. Expected: fails to compile (`search` is missing).
- [ ] **Step 3: Implement `fill`**:
  - Choose the words: the themes that are 4–5 letters and in `words`, then list words of length 4–5 from `words.of_len(4)` and `of_len(5)`, shuffled by `rng`, skipping `avoid`. Aim for 10.
  - Place the longest first, trying start squares and `Way`s in `rng` order. A placement fits when every square is empty or already holds the same letter.
  - If a word won't place, drop it. If fewer than 8 are placed by the deadline, return `None`.
  - Fill the empty squares in row-major order with a letter drawn from English frequency weights (builder picks the table). Reject a letter that completes a blocklisted window through that square, trying the next weighted letter.
  - Sort `words` by `word` before returning.
  - `Hidden.theme` is true for words that came from `themes`.
- [ ] **Step 4: Run them and see them pass.** Run: `cargo test -p deckle-digest --lib puzzle::search`. Expected: 5 passed.
- [ ] **Step 5: Commit.**
  ```bash
  git add crates/deckle-digest/src/puzzle/search.rs crates/deckle-digest/src/puzzle/mod.rs crates/deckle-digest/src/puzzle/words.rs
  git commit -m "feat(puzzle): the word search generator, forward-only, blocklist-checked filler"
  ```

### Task 2: Store it with the day's puzzle

**Files:**
- Modify: `crates/deckle-digest/src/puzzle/store.rs` (`Puzzle`)
- Modify: `crates/deckle-digest/src/puzzle/mod.rs` (`make`)
- Modify: every `store::Puzzle { … }` literal (`tests/it/puzzle.rs::checked_puzzle`, plus any `git grep -n 'Puzzle {'` finds) to add `search: None`
- Test: `crates/deckle-digest/tests/it/puzzle.rs`

**Interfaces:**
- Consumes: `search::{fill, Search, SEARCH_CAP}` (Task 1).
- Produces: `store::Puzzle { …, #[serde(default)] pub search: Option<search::Search> }`. `make(date, cfg, words, store)` fills `search`.

- [ ] **Step 1: Write the failing tests** in `tests/it/puzzle.rs`:
  - `a_day_has_a_word_search`: `today(date(), …)` has `search: Some(s)` with `8..=10` words, and calling `today` again returns the identical puzzle.
  - `old_puzzles_json_loads_without_search`: `serde_json::from_str::<Store>` on a one-day store JSON with no `search` key gives `days[d].puzzle.search == None`.
  - `search_never_reuses_the_minis_words`: for `store_with_yesterday(vec![leaf()])` (the existing helper), no `Hidden.word` equals any `clue.answer` of the same puzzle, and `leaf` (the mini's theme) is not hidden.
  - `three_letter_themes_are_skipped`: with yesterday's headlines only `ThemeWord { word: "oak", … }`, `make` gives a search where no hidden word is `"oak"` and every word is 4–5 letters.
  - `a_search_is_never_a_recent_one`: build a store whose yesterday's `puzzle.search` equals what `make(today)` produced on a store without it, then `make(today)` again gives a different `search.rows`.
  - `a_list_too_small_leaves_the_search_out`: with `Words::from_tsv("cold\tx\ncord\tx\ncard\tx\nward\tx\nwarm\tx\n")`, `make(…).search == None` and `make` still returns (the mini may be `None` too).
  - `existing_days_keep_their_mini_and_ladder`: for date 2026-10-22, with an empty store, `make` now gives the same `rows` and `ladder` as before this change. Pin the current values, captured with a one-off run before Step 3.
- [ ] **Step 2: Run them and see them fail.** Run: `cargo test -p deckle-digest --test it puzzle::`. Expected: compile error (`search` field missing).
- [ ] **Step 3: Implement.**
  - Add the `search` field.
  - At the end of `make`, after the ladder (so the main `rng` sequence is untouched), set up the theme words and the words to avoid:
    - `search_themes`: `theme::pick(&y.headlines, THEME_WORDS + 3)`, keeping words of 4–5 letters that are not a mini answer, taking at most 3, and only when `cfg.theme`.
    - `avoid`: the mini's answers, plus the mini's theme words.
  - Fill with a separate RNG, `StdRng::seed_from_u64(seed ^ 0x5EA8_C000)`.
  - Retry like the mini's loop from #263 while `recent_searches` (search rows from the last `KEEP_DAYS` days) contains the result. Use `seed ^ 0x5EA8_C000 ^ (attempt << 32)` for each retry, and drop the themes after `FRESH_TRIES`.
- [ ] **Step 4: Run them and see them pass,** plus the existing puzzle tests. Run: `cargo test -p deckle-digest --test it puzzle::`. Expected: all pass, including the existing `same_day_twice_is_identical` and `a_grid_is_never_yesterdays_again`.
- [ ] **Step 5: Commit.** Message: `feat(puzzle): store a word search with each day's puzzle`.

### Task 3: Print it in the paper (EPUB and PDF)

**Files:**
- Modify: `crates/deckle-digest/src/puzzle/mod.rs` (`section`, plus a new `fn search_table(s: &Search, mask: bool) -> String`)
- Modify: `crates/deckle-digest/src/render.rs:68-74` (CSS)
- Modify: `crates/deckle-digest/tests/fixtures/puzzle/golden-page.xhtml` (regenerate)
- Test: `crates/deckle-digest/tests/it/puzzle.rs`, and the PDF test in `crates/deckle-digest/tests/it/build.rs`

**Interfaces:**
- Consumes: `store::Puzzle.search`, `Search::cells` (Tasks 1–2).
- Produces: the article order mini → `Word ladder` → `Word search · No. N` → `Yesterday's answers`.

- [ ] **Step 1: Write the failing tests:**
  - `the_search_prints_after_the_ladder`: `section(date(), …)` has article titles, in order, `The Mini · No. 1`, `Word ladder`, `Word search · No. 1`, and the third body contains:
    - `<table class="grid search">` with 9 `<tr>` of 9 `<td>`;
    - the instruction copy exactly;
    - the hidden words, upper case and alphabetical, in a two-column `<table class="cols">` (first half of the list in the left `<td>`, rest in the right; one `<p class="clue">` per word), theme words followed by ` (from yesterday's paper)`;
    - no `class="note"`.
  - `yesterdays_answers_show_the_search_masked`: with yesterday's day holding a search, the `Yesterday's answers` body has `<table class="grid search small">` in which every cell outside `cells()` of all hidden words is `·`, and every other cell is its letter. A line `Word search: ` lists the words, comma-separated.
  - `a_day_stored_before_the_search_prints_without_it`: a store whose today and yesterday puzzles both have `search: None` gives no `Word search` article, and no `Word search:` line in yesterday's answers.
  - In `build.rs`, `the_pdf_edition_reads_back_with_its_titles_and_outline` (existing): extend it to assert the PDF text contains `Word search · No.`.
- [ ] **Step 2: Run them and see them fail.** Run: `cargo test -p deckle-digest --test it puzzle:: build::the_pdf_edition_reads_back_with_its_titles`. Expected: the new assertions fail.
- [ ] **Step 3: Implement.**
  - Write `search_table`. Cells are plain letters, with no `class="b"`; masked cells print `·`.
  - Add the article in `section`.
  - Extend yesterday's answers.
  - CSS, after `table.grid.small td`:
    ```css
    table.grid.search td { width: 1.6em; height: 1.6em; text-align: center; vertical-align: middle; font-weight: bold; }
    table.grid.search.small td { width: 1.1em; height: 1.1em; font-weight: normal; }
    ```
  - Regenerate `golden-page.xhtml` the way its test documents. Read the diff: it should only add the search article and lines.
- [ ] **Step 4: Run them and see them pass,** plus the whole crate. Run: `cargo test -p deckle-digest`. Expected: all pass (PDF tests need `target/pdfium`; see `tools/fetch-pdfium.sh`).
- [ ] **Step 5: Look at it.** Render a paper (`deckle digest build` or the build test's EPUB) and screenshot the puzzle section in a browser at phone width (`table.grid.search` must fit 9 columns at 375 px), and the PDF puzzle page via `pdftoppm -r 60`. Fix any overflow.
- [ ] **Step 6: Commit.** Message: `feat(puzzle): print the word search, and yesterday's answers masked`.

### Task 4: Carry it in the edition bundle

**Files:**
- Modify: `crates/deckle-digest/src/bundle.rs` (structs near `PuzzleJson` at :205, the builder at :634, the version at :695)
- Modify: `docs/formats/edition-bundle.schema.json` (`$defs/puzzle`, and its `yesterday`)
- Modify: `docs/superpowers/specs/2026-10-23-edition-bundle-design.md` §6.6 (example and a `search` bullet)
- Test: `crates/deckle-digest/tests/it/build.rs` (the schema tests at :1328 and :1453), and `bundle.rs` unit tests

**Interfaces:**
- Consumes: `store::Puzzle.search` (Task 2).
- Produces: JSON `puzzle.search = { "size": 9, "grid": [9 strings], "words": [{ "word": "COURT", "row": 0, "col": 2, "dir": "across", "theme": true }] }`, and `puzzle.yesterday.search`, the same shape or `null`. `version` is `"1.5"` when any section's `puzzle.search` is present, and otherwise unchanged (`"1.4"` with identity, else `VERSION`).

- [ ] **Step 1: Write the failing tests:**
  - In `build.rs`, extend `the_bundle_validates_against_its_schema_and_holds_the_whole_edition`:
    - the puzzle section's `search.size == 9`;
    - `search.grid` is 9 strings of length 9;
    - every word's letters read back from `grid` along `dir`;
    - `version == "1.5"`.
  - New, in the same file: `a_bundle_whose_yesterday_had_no_search_validates`. It uses a store whose yesterday puzzle has `search: None`; the bundle has `yesterday.search == null` and validates against the schema.
- [ ] **Step 2: Run them and see them fail.** Run: `cargo test -p deckle-digest --test it build::the_bundle_validates build::a_bundle_whose_yesterday`. Expected: FAIL (no `search` key).
- [ ] **Step 3: Implement.**
  - Add `SearchJson { size: usize, grid: Vec<String>, words: Vec<HiddenJson> }` and `HiddenJson { word: String /* upper */, row: u8, col: u8, dir: &'static str, theme: bool }`.
  - Add `#[serde(skip_serializing_if = "Option::is_none")] search: Option<SearchJson>` to `PuzzleJson`, and `search: Option<SearchJson>` to `YesterdayJson` (always serialized, `null` allowed).
  - Map `Way` to its kebab-case name.
  - Version: `"1.5"` if any section's `puzzle.search` is `Some`.
  - Schema: add an optional `search` to `$defs/puzzle`, not listed in `required`, and add `search` to `yesterday`'s properties as `oneOf [null, $defs/search]`. Define `$defs/search`, with `dir` as an `enum` of the four names and the description `1.5`.
  - In §6.6, add `"search": {…}` to the example and one bullet: "`search` (1.5, optional): a 9×9 word search; `words[].dir` is `across`, `down`, `down-right` or `up-right`, forward only; answers are in the bundle like the mini's."
- [ ] **Step 4: Run them and see them pass.** Run: `cargo test -p deckle-digest`. Expected: all pass.
- [ ] **Step 5: Commit.** Message: `feat(bundle): puzzle.search (1.5), the word search for native readers`.

### Task 5: `deckle digest puzzle` shows it

**Files:**
- Modify: `crates/deckle-cli/src/lib.rs` (`puzzle_text` at :5389)
- Test: `crates/deckle-cli/tests/it/puzzle.rs`

**Interfaces:**
- Consumes: `store::Puzzle.search`. `--json` already flattens `Puzzle`, so `search` appears there with no code change. The test pins it.

- [ ] **Step 1: Write the failing tests** in `tests/it/puzzle.rs`:
  - `digest_puzzle_prints_the_word_search`: the text output contains `Word search · No.`, 9 grid lines of 9 space-separated letters, and a `Find:` line listing the words in upper case.
  - `digest_puzzle_json_has_search`: the JSON has `search.rows` (9) and `search.words` (8–10).
- [ ] **Step 2: Run them and see them fail.** Run: `cargo test -p deckle-cli --test it puzzle::digest_puzzle_prints_the_word_search puzzle::digest_puzzle_json_has_search`. Expected: the text test fails, and the JSON test passes (or fails only if `search` is absent).
- [ ] **Step 3: Implement.** In `puzzle_text`, after the ladder line, when `p.search` is `Some(s)`, push `Word search · No. {p.number}`, each row with its letters joined by spaces (indented two), then `Find: {WORDS, comma-separated}`. Mark theme words with `*` and add the line `* from yesterday's paper` when any are present.
- [ ] **Step 4: Run them and see them pass.** Run: `cargo test -p deckle-cli --test it puzzle::`. Expected: all pass.
- [ ] **Step 5: Verify the whole thing and open the PR.**
  - Run: `cargo fmt --all -- --check && cargo clippy -p deckle-digest -p deckle-cli --all-targets -- -D warnings && cargo test -p deckle-digest && cargo test -p deckle-cli --test it puzzle::`. Expected: clean, all pass.
  - Commit with the message `feat(cli): digest puzzle prints the word search`, push `feat/word-search`, and open a PR titled "Word search: generator, paper, CLI and bundle (spec §7 step 1)". The body must not contain `Closes` (there's no issue).
