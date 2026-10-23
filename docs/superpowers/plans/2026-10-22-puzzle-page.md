# The Puzzle Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `puzzle` section to the Deckle Daily: a 5×5 mini crossword themed on yesterday's
headlines, a word ladder, yesterday's answers, and "You solved N of M" from Kindle notes.

**Architecture:**
- A new `puzzle` module tree in `crates/deckle-digest/src/puzzle/`: `words`, `grid`, `theme`,
  `ladder`, `store`, `check`, and `mod.rs` (the section). It runs on an embedded, gzipped word
  list built offline by `data/wordwise`.
- Each date's puzzle is made once and stored in `puzzles.json`.
- On connect, the existing clippings read (`deckle_core::sync`) also runs when the puzzle is
  on. The CLI and the Kindle tab score yesterday's notes into the store.

**Tech Stack:**
- Rust 2024 workspace, `flate2` (new in `deckle-digest`), `rand` 0.9 (seeded `StdRng`),
  `deckle-highlights` (new dependency of `deckle-digest`).
- Tauri/React for one label.

**Spec:** `docs/superpowers/specs/2026-10-22-puzzle-page-design.md`

## Global Constraints

- Section config name and readership key: `puzzle`. In `STARTER` only, placed after
  `horoscope` and before `onthisday`. Existing configs are never changed on their own.
- No network for the puzzle. The word list is embedded gzipped, under 1 MB.
- Words: lowercase a–z, 3 to 5 letters, zipf ≥ 3.0, no archaic, rare, slang, vulgar,
  derogatory or obsolete senses, no proper nouns or abbreviations, no inflections whose lemma
  is in, and nothing on `data/wordwise/puzzle-blocklist.txt`. A clue never contains its answer.
- State: `puzzles.json` next to `digest.toml` (a `#[serde(skip)] puzzle_path` override for
  tests, like `queue_path`). It keeps the last 60 days and is written through a temp file and
  a rename.
- Deterministic per date: `StdRng::seed_from_u64(date.num_days_from_ce() as u64)`.
- Fill time cap: 500 ms per template.
- Exact copy:
  - kicker "Answers in tomorrow's paper";
  - article titles "The Mini · No. {n}", "Word ladder", "Yesterday's answers";
  - the hint "Highlight a clue and add your answer as a note; tomorrow's paper checks it.";
  - the score "You solved {n} of {m}";
  - the credit "Words and definitions from Wiktionary and wordfreq (CC BY-SA 4.0)";
  - theme clues "{headline with ____} ({Section}, yesterday)".
- Headlines over 90 characters are cut at a word boundary with "…", keeping the blank.
- The ladder's shortest path is 3 to 5 rungs. No pair repeats within 60 days.
- A puzzle failure never fails the build: the mini or the ladder is left out, or the section
  shows `unavailable`.
- The pre-push hook runs `cargo fmt --check` and clippy `-D warnings`. Keep both clean on every
  commit.

**Plan rulings** (left to the builder by the spec; flagged to the owner):
- **The notes hint line** prints unless the last Kindle that connected had no readable
  `My Clippings.txt` (MTP Scribe). `store.notes_readable: Option<bool>` records it on every
  connection that read clippings. `None` (never connected) prints the line.
- **Puzzle numbers** count the stored puzzles ever made: `No. {n}` = 1 + the highest stored
  number, or 1.

## Review Focus

1. **The same day built twice after the word list changed** (an app update mid-day) must still
   print the stored puzzle, not a new one. Test in Task 6.
2. **A headline word that's also a clue word.** "Rain" is blanked in the headline, but the
   dictionary clue of another entry says "rain". That's fine. The theme clue itself must hold
   no unblanked form of its word, such as "Rains" or "RAIN". Test in Task 4.
3. **Clippings from an edition whose paper was renamed.** The title's paper part changes, but
   the date part still matches. Test in Task 8.
4. **A note typed with punctuation or a trailing space** ("card.", " Card ") still counts.
   Test in Task 8.
5. **A missed day.** With no puzzle stored for the previous date, the paper prints no
   "Yesterday's answers" article: no stale answers from an older day. Test in Task 7.

---

### Task 1: The puzzle word list (builder)

**Files:**
- Modify: `data/wordwise/src/lib.rs`. Add `puzzle_words`.
- Modify: `data/wordwise/src/main.rs`. Add the `puzzle` subcommand.
- Create: `data/wordwise/puzzle-blocklist.txt`. Slurs, profanity and sensitive words, one per
  line, seeded from a standard English profanity list.
- Modify: `data/wordwise/tests/sample.jsonl`, adding entries as needed. Test in
  `data/wordwise/tests/build.rs`.
- Create: `crates/deckle-digest/assets/puzzle/words.tsv.gz`, built from the real data in
  Step 6.
- Modify: `data/wordwise/README.md`.

**Interfaces:**
- Produces:
  - `deckle_wordwise_build::puzzle_words(jsonl: impl BufRead, freq: &HashMap<String, f64>,
    blocklist: &HashSet<String>) -> Vec<(String, String)>`, as (word, clue), sorted by word.
  - The file format: UTF-8 lines `word\tclue\n`, gzip -9 -n.

- [ ] **Step 1: Write failing tests** in `tests/build.rs`:
  - `puzzle_words_keeps_common_short_words`: `card` and `area` (add sample entries with
    ordinary senses) are in, with their first everyday short definition as the clue.
  - `puzzle_words_filters`: these are all out:
    - `cards` (inflection of `card`);
    - a 6-letter word;
    - a capitalised proper noun;
    - a word with a hyphen;
    - a word under zipf 3.0;
    - a word whose only sense is tagged `slang`;
    - a word in the blocklist passed in.
  - `puzzle_clue_never_contains_answer`: a word whose only definitions contain the word itself
    (sample: "rose: a rose-coloured …") is dropped.

- [ ] **Step 2: Run** `cargo test -p deckle-wordwise-build puzzle`. Expected: FAIL to compile.

- [ ] **Step 3: Implement** `puzzle_words`, reusing the existing sense-picking:
  - Reuse `sense_penalty` and the definition cleaning from the database build, and `zipf`.
  - A "plain inflection" is any entry whose `form_of`/`inflection of` points at a lemma that
    is itself kept.
  - "Contains the answer" is a case-insensitive whole-word or prefix match of the answer in
    the clue.
  - `main` takes `puzzle <kaikki> <wordfreq> <blocklist> <out.tsv>`.

- [ ] **Step 4: Run** the same command. Expected: PASS.

- [ ] **Step 5: Commit** `feat(wordwise-build): a word list for the puzzle page`.

- [ ] **Step 6: Build the real list.** Download the sources as the README shows, run `cargo
  run --release -p deckle-wordwise-build -- puzzle …`, then `gzip -9 -n` into
  `crates/deckle-digest/assets/puzzle/words.tsv.gz`.
  - Expected: 3,000 to 30,000 words, under 1 MB gzipped.
  - Skim 50 random lines and add anything objectionable to the blocklist, then rebuild.
  - Record the counts, size and source dates in the README.
  - Commit `chore(puzzle): the embedded word list`.

### Task 2: Loading the word list

**Files:**
- Modify: `crates/deckle-digest/Cargo.toml`. Add `flate2 = "1"`.
- Create: `crates/deckle-digest/src/puzzle/mod.rs`, which starts with the module declarations,
  and `crates/deckle-digest/src/puzzle/words.rs`.
- Register `pub mod puzzle;` in `lib.rs`.

**Interfaces:**
- Produces:
  - `puzzle::words::Words { by_len: [Vec<String>; 6], clues: HashMap<String, String> }`.
  - `Words::embedded() -> &'static Words`, through a `OnceLock`.
  - `Words::from_tsv(text: &str) -> Words`.
  - `contains(&self, w: &str) -> bool`.
  - `clue(&self, w: &str) -> Option<&str>`.
  - `of_len(&self, n: usize) -> &[String]`.

- [ ] **Step 1: Write failing unit tests:**
  - `embedded_list_is_sane`: total size 3,000–30,000. Every word is 3 to 5 letters of `a..=z`.
    No clue contains its word (case-insensitive). The list contains `card`.
  - `from_tsv_skips_bad_lines`: blank lines, lines with no tab, or upper-case words are
    skipped.

- [ ] **Step 2: Run** `cargo test -p deckle-digest --lib puzzle::words`. Expected: FAIL.

- [ ] **Step 3: Implement.** Use `flate2::read::GzDecoder` over
  `include_bytes!("../../assets/puzzle/words.tsv.gz")`.

- [ ] **Step 4: Run.** Expected: PASS.

- [ ] **Step 5: Commit** `feat(puzzle): load the embedded word list`.

### Task 3: Grid templates and the fill

**Files:**
- Create: `crates/deckle-digest/src/puzzle/grid.rs`, with unit tests.

**Interfaces:**
- Consumes: `Words`.
- Produces:
  - `pub struct Template(pub [&'static str; 5])`, where `'#'` is black and `'.'` is white.
  - `pub const TEMPLATES: &[Template]`: at least 4, all rotationally symmetric, with no white
    run of length 1–2.
  - `#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)] pub enum Dir {
    Across, Down }`.
  - `pub struct Slot { pub number: u8, pub dir: Dir, pub row: usize, pub col: usize, pub len:
    usize }`.
  - `pub fn slots(t: &Template) -> Vec<Slot>`, numbered in standard crossword order.
  - `pub struct Fill { pub template: usize, pub rows: [String; 5], pub answers: Vec<(Slot,
    String)> }`.
    - `rows` uses `#` for black squares and uppercase letters for answers.
  - `pub fn fill(words: &Words, seeds: &[String], rng: &mut StdRng, cap: Duration) ->
    Option<Fill>`.
    - It tries the templates in an order shuffled by `rng`.
    - It places seeds into the longest matching slots first.

- [ ] **Step 1: Write failing tests:**
  - `templates_are_symmetric_and_have_no_short_runs`.
  - `slots_number_like_a_crossword`: for the open template, Across is 1, 6, 7, 8, 9 and Down
    is 1–5, matching the spec's mockup.
  - `fills_thirty_days_within_cap`: for seeds `[]`, each of 30 consecutive date seeds returns
    `Some` within 500 ms, and:
    - every answer is `words.contains`;
    - no word repeats;
    - each crossing cell agrees.
  - `deterministic`: the same seed and inputs give an equal `Fill`.
  - `seed_is_placed`: seeds `["leaf"]` give an answer equal to `leaf`.
  - `impossible_seed_still_fills`: seeds `["zzzzz"]` (not in the list) are skipped, and the
    result is `Some` without it.

- [ ] **Step 2: Run** `cargo test -p deckle-digest --lib puzzle::grid`. Expected: FAIL.

- [ ] **Step 3: Implement** a backtracking search:
  - Pick the most constrained unfilled slot first, by fewest candidates matching its pattern.
  - Shuffle the candidates with `rng`.
  - Check the deadline every 1,000 nodes, and on timeout move to the next template.
  - Index candidates by (length, position, letter) for pattern matching.

- [ ] **Step 4: Run.** Expected: PASS. Then run `cargo test --release` to confirm the timing.

- [ ] **Step 5: Commit** `feat(puzzle): fill a 5x5 mini`.

### Task 4: Theme words from headlines

**Files:**
- Create: `crates/deckle-digest/src/puzzle/theme.rs`, with unit tests.

**Interfaces:**
- Consumes: `Words`, `crate::Edition`.
- Produces:
  - `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] pub struct ThemeWord { pub
    word: String, pub headline: String, pub section: String }`.
  - `pub fn harvest(e: &Edition, news: &[usize], words: &Words) -> Vec<ThemeWord>`. It covers
    the headlines (`Article::title`) of the sections in `news` (`Gathered::news`) plus
    `reading`.
  - `pub fn pick(found: &[ThemeWord], max: usize) -> Vec<ThemeWord>`: longer words first, at
    most one per section, ties broken in headline order.
  - `pub fn clue(t: &ThemeWord) -> String`.
  - `pub const STOP: &[&str]`: function words and newsroom verbs ("the", "and", "says",
    "said", "new", "will", …, about 80).

- [ ] **Step 1: Write failing tests:**
  - `harvest_takes_list_words_from_news_headlines`:
    - fixture edition, Local "Leaf season peaks on the ridge this weekend" → `leaf` is found;
    - stop words are not;
    - the place name's words ("Candor") are not;
    - Weather and Lottery headlines are ignored.
  - `pick_prefers_long_words_one_per_section`.
  - `clue_blanks_every_form`: the word `rain` in the headline "Rain, rain and more RAIN for
    Rains County" gives the clue "____, ____ and more ____ for Rains County (Local,
    yesterday)". "Rains" is left alone because it's a different word, but no standalone
    case-insensitive `rain` remains (Review Focus 2).
  - `clue_cuts_long_headlines_keeping_the_blank`: in a 140-character headline with the word
    near the end, the result is ≤ 90 characters before " (", ends with "…", and contains
    `____`.

- [ ] **Step 2: Run** `cargo test -p deckle-digest --lib puzzle::theme`. Expected: FAIL.

- [ ] **Step 3: Implement.**
  - Tokenize on non-letters, lowercase, length 3 to 5, `words.contains`, not in `STOP`, not in
    the place name.
  - The blank replaces whole-word, case-insensitive matches.
  - The cut keeps a window around the first blank.

- [ ] **Step 4: Run.** Expected: PASS.

- [ ] **Step 5: Commit** `feat(puzzle): theme answers from yesterday's headlines`.

### Task 5: Word ladder

**Files:**
- Create: `crates/deckle-digest/src/puzzle/ladder.rs`, with unit tests.

**Interfaces:**
- Consumes: `Words`.
- Produces:
  - `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] pub struct Ladder { pub from:
    String, pub to: String, pub path: Vec<String> }`. `path` includes both ends.
  - `pub const PAIRS: &[(&str, &str)]`: about 60 pairs of common 4-letter words (cold/warm,
    love/hate, head/tail, …). A pair with either word missing from the list is skipped.
  - `pub fn shortest(words: &Words, from: &str, to: &str) -> Option<Vec<String>>`.
  - `pub fn today(words: &Words, rng: &mut StdRng, recent: &[(String, String)]) ->
    Option<Ladder>`.

- [ ] **Step 1: Write failing tests:**
  - `cold_to_warm`: `shortest` gives a path with each step differing by exactly one letter and
    every rung in the list.
  - `today_respects_length_and_recent`:
    - over 60 seeds, every ladder takes 3 to 5 steps (`path.len() - 1` in `3..=5`; a "rung" is
      one letter change, as in the mockup LEAF → LEAD → LOAD → ROAD);
    - no pair in `recent` is returned.
  - `unknown_pair_skipped`: a pair not in the list is never returned.

- [ ] **Step 2: Run** `cargo test -p deckle-digest --lib puzzle::ladder`. Expected: FAIL.

- [ ] **Step 3: Implement** breadth-first search over `of_len(4)`, with neighbours by wildcard
  buckets ("c_ld").

- [ ] **Step 4: Run.** Expected: PASS.

- [ ] **Step 5: Commit** `feat(puzzle): the word ladder`.

### Task 6: The store, config and making today's puzzle

**Files:**
- Create: `crates/deckle-digest/src/puzzle/store.rs`. Add the `make` function in
  `puzzle/mod.rs`.
- Modify: `crates/deckle-digest/src/config.rs`:
  - `SectionKind::Puzzle`, with `section_key` returning `"puzzle"`;
  - `PuzzleConfig { ladder: bool = true, theme: bool = true }` as `pub puzzle`, with
    `skip_serializing_if = "PuzzleConfig::is_default"`;
  - `#[serde(skip)] pub puzzle_path: Option<PathBuf>` and `Config::puzzle_store() -> PathBuf`;
  - the `STARTER` sections list gets `"puzzle"` between `"horoscope"` and `"onthisday"`, and
    `Config::default().sections` too.
- Test: `crates/deckle-digest/tests/puzzle.rs`.

**Interfaces:**
- Consumes: Tasks 2–5.
- Produces:
  - `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] pub struct Clue { pub number:
    u8, pub dir: grid::Dir, pub answer: String, pub clue: String, pub theme: bool }`.
  - `pub struct Puzzle { pub number: u32, pub rows: Option<[String; 5]>, pub clues: Vec<Clue>,
    pub ladder: Option<Ladder> }`. `rows: None` means there's no mini.
  - `pub struct Solve { pub solved: usize, pub of: usize, pub ladder: bool }`.
  - `pub struct Day { pub puzzle: Puzzle, pub headlines: Vec<ThemeWord>, pub solve:
    Option<Solve> }`.
  - `pub struct Store { pub days: BTreeMap<NaiveDate, Day>, pub notes_readable: Option<bool>
    }`.
  - `Store::load(path: &Path) -> Store` (a missing or bad file loads empty), and
    `save(&self, path: &Path) -> io::Result<()>`.
  - `Store::prune(&mut self, today: NaiveDate)`, keeping 60 days.
  - `pub fn make(date: NaiveDate, cfg: &PuzzleConfig, words: &Words, store: &Store) ->
    Puzzle`.
    - Yesterday's theme words come from `store.days[date - 1].headlines`, or none.
    - The recent ladder pairs are the last 60 days.
    - It is seeded by the date.
  - `pub fn today<'s>(date: NaiveDate, cfg: &PuzzleConfig, words: &Words, store: &'s mut
    Store) -> &'s Day`: the stored day, or one made and inserted.

- [ ] **Step 1: Write failing tests** in `tests/puzzle.rs`:
  - `same_day_twice_is_identical`: two `today` calls give equal days.
  - `stored_puzzle_survives_a_new_word_list` (Review Focus 1): store a day made with
    `Words::from_tsv(small_list)`, then call `today` with `Words::embedded()` and get the
    stored puzzle unchanged.
  - `lost_store_regenerates_the_same`: `make` twice with equal inputs gives equal output.
  - `theme_clues_come_from_yesterday`: with yesterday's `headlines = [leaf …]`, today's clues
    contain one with `theme: true` and the blanked headline.
  - `no_theme_when_off`: with `theme = false`, no clue is themed.
  - `no_ladder_when_off`.
  - `numbers_count_up`: day 1 is No. 1 and the next day is No. 2.
  - `prune_keeps_sixty_days`.
  - `starter_has_puzzle_before_onthisday`, and `untouched_config_saves_no_puzzle_table`.

- [ ] **Step 2: Run** `cargo test -p deckle-digest --test puzzle`. Expected: FAIL to compile.

- [ ] **Step 3: Implement.**
  - `make` calls `theme::pick(&yesterday, 2)` when `cfg.theme`.
  - It calls `grid::fill` with those words as seeds. A seed that's placed keeps
    `theme::clue` as its clue; the other answers take `words.clue`.
  - It calls `ladder::today` when `cfg.ladder`.
  - Fix up any `match` on `SectionKind` across the workspace (gather and the CLI's section
    lists) so it compiles. The section itself arrives in Task 7: here `gather` pushes
    `sources::unavailable("Puzzle", "not built yet")` for it.

- [ ] **Step 4: Run** `cargo test --workspace`. Expected: PASS.

- [ ] **Step 5: Commit** `feat(puzzle): make and keep each day's puzzle`.

### Task 7: The section on the page

**Files:**
- Modify: `crates/deckle-digest/src/puzzle/mod.rs`. Add `section` and `render_*`.
- Modify: `crates/deckle-digest/src/lib.rs` (gather) and `crates/deckle-digest/src/render.rs`
  (CSS).
- Test: `crates/deckle-digest/tests/puzzle.rs`. Golden file:
  `crates/deckle-digest/tests/fixtures/puzzle/golden-page.xhtml`. Add
  `fixtures/puzzle/* -text` to `.gitattributes`.

**Interfaces:**
- Consumes: `Store`, `today`, `Words::embedded`.
- Produces:
  - `pub fn section(date: NaiveDate, cfg: &Config, store: &mut Store) -> Section`, with title
    "Puzzle" and kicker "Answers in tomorrow's paper". It has the articles from the Global
    Constraints, in order:
    - "The Mini · No. N": a grid table `<table class="grid">` with `<td class="b"></td>` for
      black squares and `<td><span class="n">1</span></td>` for numbered cells; then
      `<h3 class="subhead">Across</h3>` with numbered clues, Down the same, and the hint line
      when `store.notes_readable != Some(false)`;
    - "Word ladder": FROM, as many blank rungs as `path.len() - 2`, then TO;
    - "Yesterday's answers", only when a day before `date` exists: the filled small grid
      `<table class="grid small">`, "Ladder: A, B, C, D", the score line when `solve` is
      `Some`, then the credit.
  - `gather`:
    - Loads the store from `cfg.puzzle_store()`, calls `section`, and saves.
    - After all sections are gathered, records
      `store.days[date].headlines = theme::harvest(&edition, &news, words)` when the puzzle
      section is on, and saves.
    - A store write error becomes a `notes` entry, never a failure.

- [ ] **Step 1: Write failing tests:**
  - `golden_page`:
    - A fixed date, `Words::from_tsv(fixture)`, and a store holding yesterday with a solve of
      4 of 6.
    - The page renders through `render::render` to `golden-page.xhtml`. `UPDATE_GOLDEN=1`
      writes it; read it before committing, as in `tests/weather_page.rs` on the almanac
      branch.
    - It must contain the exact strings from the Global Constraints, and pass a well-formed XML
      parse with `roxmltree`.
  - `no_yesterday_no_answers` (Review Focus 5): with nothing stored for `date - 1`, even with
    `date - 2` stored, there's no "Yesterday's answers" article. (The spec's goal 4 says "when
    Deckle built a puzzle yesterday".)
  - `hint_hidden_when_notes_unreadable`.
  - `fill_failure_keeps_the_ladder`: a word list too small to fill gives no mini article, but
    "Word ladder" is present.
  - `everything_fails_unavailable`: an empty word list gives `section.unavailable.is_some()`,
    and `build` still writes the EPUB.
  - `gather_records_headlines`: after `gather` with fixture feeds, the store has
    `days[date].headlines` non-empty.
  - `store_write_error_is_a_note`: a `puzzle_path` in a read-only directory gives a `notes`
    entry and the paper builds.

- [ ] **Step 2: Run** `cargo test -p deckle-digest --test puzzle`. Expected: FAIL.

- [ ] **Step 3: Implement, with the CSS:**
  - `table.grid` uses fixed square cells (`width: 2em; height: 2em; border: 1px solid black`).
  - `td.b` has `background-color: black`.
  - `span.n` is about 0.5em, at the top left.
  - `table.grid.small td` is about 1.1em.
  - No colour beyond black.

- [ ] **Step 4: Run** `cargo test --workspace`. Expected: PASS, with any test asserting the
  old section list updated and ledgered.

- [ ] **Step 5: Commit** `feat(puzzle): the puzzle page in the paper`.

### Task 8: Checking answers from Kindle notes

**Files:**
- Modify: `crates/deckle-digest/Cargo.toml`. Add `deckle-highlights.workspace = true`, after
  checking that `deckle-highlights` doesn't depend on `deckle-digest`.
- Create: `crates/deckle-digest/src/puzzle/check.rs`.
- Test: `crates/deckle-digest/tests/puzzle.rs`, with fixture
  `tests/fixtures/puzzle/clippings.txt`.

**Interfaces:**
- Consumes: `deckle_highlights::parse::parse`, `deckle_highlights::book::{group, Item,
  edition_date}`, `crate::long_date`.
- Produces:
  - `pub fn score(puzzle: &Puzzle, date: NaiveDate, entries: &[deckle_highlights::parse::Entry])
    -> Option<Solve>`. It gives `None` when there are no notes on that edition's clues or
    ladder.
  - `pub fn record(store: &mut Store, clippings: Option<&[u8]>, today: NaiveDate)`.
    - It scores each of the last 7 stored days that have clues, and keeps the best (latest)
      result.
    - It sets `notes_readable = Some(clippings.is_some())`.

- [ ] **Step 1: Write failing tests:**
  - `correct_and_wrong_notes`: on the 2026-10-22 edition, "CARD" on the clue "1 …" and "AREX"
    on "6 …" give `Solve { solved: 1, of: <clue count>, .. }`.
  - `case_spaces_punctuation_ignored` (Review Focus 4): notes " card. " and "c a r d" both
    count.
  - `latest_note_wins`: two notes on one clue, wrong then right, count as solved.
  - `non_clue_highlight_ignored`, and `other_edition_ignored` (a note on the 2026-10-21
    edition's text doesn't count for 2026-10-22).
  - `renamed_paper_still_matches` (Review Focus 3): the title "The Morning Deckle — Thursday,
    October 22, 2026" matches by the date part.
  - `ladder_note_spells_the_path`: a note "COLD CORD WORD WARD WARM" on the ladder line sets
    `ladder: true`.
  - `record_marks_unreadable`: `record(store, None, …)` sets `notes_readable = Some(false)`
    and leaves solves alone.

- [ ] **Step 2: Run** `cargo test -p deckle-digest --test puzzle`. Expected: FAIL.

- [ ] **Step 3: Implement.**
  - Matching normalises both texts to lowercase letters and spaces only. A highlight matches a
    clue when it contains the clue's normalised text without its number, or the clue text
    contains the highlight and the highlight is at least 12 characters.
  - The ladder line matches by its `FROM` … `TO` words.

- [ ] **Step 4: Run.** Expected: PASS.

- [ ] **Step 5: Commit** `feat(puzzle): check answers written as Kindle notes`.

### Task 9: Reading notes on connect (CLI watcher and push, Kindle tab)

**Files:**
- Modify: `crates/deckle-cli/src/lib.rs`:
  - `sync_actions`: `clippings: a.clippings || puzzle_on(cfg)`.
  - `sync_to`:
    - highlights export only when `cfg.sync.for_device(id).clippings`;
    - when `puzzle_on(cfg)`, call `puzzle::check::record(&mut store, flush.clippings.as_deref(),
      today)` and save to `cfg.puzzle_store()`;
    - a clippings read that ran with no file found counts as `None`.
  - `push_plan`: with an empty outbox, run when `actions.readership || actions.clippings`,
    as `Plan { push: false, prune: false, clippings: actions.clippings, readership:
    actions.readership }`, and leave the Kindle alone otherwise.
- Modify: `apps/desktop/src-tauri/src/kindle.rs` (`shelf_on`). After `highlights_on`, when the
  digest config has the puzzle section, call `puzzle::check::record` with the clippings bytes
  the shelf read already holds. Store errors are logged, not fatal.
- Test: `crates/deckle-cli/tests/puzzle.rs`, using the readership test's harness: a temp
  `HOME` and a folder target with `documents/My Clippings.txt`.

**Interfaces:**
- Consumes: `check::record`, `Store`, `Config::puzzle_store`.
- Produces: `fn puzzle_on(cfg: &deckle_digest::Config) -> bool`, true when `sections` contains
  `SectionKind::Puzzle`.

- [ ] **Step 1: Write failing tests:**
  - `push_scores_yesterday`:
    - a stored puzzle for yesterday, a folder Kindle with a clippings file answering one clue,
      and `[sync] clippings = false`;
    - run `deckle digest push --to <folder>` with an empty outbox;
    - `puzzles.json` then has a `solve`, and no highlights folder was written.
  - `puzzle_off_reads_no_clippings`: with `sections` lacking `puzzle` and clippings off, the
    store is untouched.
  - `no_clippings_file_marks_unreadable`: a folder Kindle without the file gives
    `notes_readable: false`.

- [ ] **Step 2: Run** `cargo test -p deckle-cli --test puzzle`. Expected: FAIL.

- [ ] **Step 3: Implement** as described in Files.

- [ ] **Step 4: Run** `cargo test --workspace`. Expected: PASS, including the existing
  readership and highlights CLI tests.

- [ ] **Step 5: Commit** `feat(puzzle): read answers whenever the Kindle connects`.

### Task 10: CLI `digest puzzle`, Daily tab, docs

**Files:**
- Modify: `crates/deckle-cli/src/{cli.rs,lib.rs}`. Add `DigestCommand::Puzzle { json: bool }`.
  It prints today's stored puzzle, or makes one when there's none, and yesterday's answers.
- Modify: `apps/desktop/src/types.ts`. Add `"puzzle"` to `SectionKind`, `SECTION_LABEL.puzzle =
  "Puzzle"`, and put it in `ALL_SECTIONS` between `horoscope` and `onthisday`.
- Modify: `docs/daily.md` (a "Puzzle page" section), `CHANGELOG.md`, and `README.md` if it
  lists sections.
- Test: `crates/deckle-cli/tests/puzzle.rs` and the vitest in `settingsKeys.test.ts` or a new
  `types.test.ts`.

- [ ] **Step 1: Write failing tests:**
  - `digest_puzzle_prints_the_mini`: the output contains "The Mini · No." and "Across", and
    `--json` parses with `.clues` as an array.
  - vitest `puzzle_is_a_section`: `ALL_SECTIONS` includes `"puzzle"` before `"onthisday"`, and
    its label is "Puzzle".

- [ ] **Step 2: Run** `cargo test -p deckle-cli --test puzzle` and `pnpm --dir apps/desktop
  test`. Expected: FAIL.

- [ ] **Step 3: Implement.** Text output: the grid as five lines (`#` for black, `.` for
  white), the clues, the ladder ends, then "Yesterday:" with the answers and score.

- [ ] **Step 4: Run** `cargo test --workspace`, `pnpm --dir apps/desktop test` and `pnpm
  --dir apps/desktop build`. Expected: PASS.

- [ ] **Step 5: Verify visually.**
  - Build an edition with `deckle digest build -o <tmp>` and open the puzzle XHTML from the
    EPUB in the browser pane. Check that the grid is square and the clues are readable.
  - Check that "Puzzle" appears in the Daily tab's section list (browser pane over the IPC
    mock, as in the almanac work).

- [ ] **Step 6: Commit** `feat(puzzle): digest puzzle command, Daily tab, docs`.

- [ ] **Step 7: Device check, with the owner.** Send an edition to the Scribe and the
  Paperwhite and check the grid on e-ink. On the Paperwhite, highlight two clues, add notes,
  plug in, build tomorrow's paper (or `deckle digest puzzle`) and check the score.
