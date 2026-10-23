# From Your Shelf Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `shelf` section to the Deckle Daily, *From Your Shelf*: the books in progress on
your Kindle (from `.yjf` reading positions, works on the Scribe), plus a past highlight and this
week's looked-up words on Kindles that share them.

**Architecture:**
- **Device read** in `deckle-core`: the inventory walk optionally fetches each book's `.yjf`;
  a new `vocab` reader copies `system/vocabulary/vocab.db`. Both ride in the sync's `Conn`, once
  per connection, as a new `shelf` action next to readership and clippings.
- **Snapshot** in a new crate `crates/deckle-shelf`: turns the read into one `KindleSnapshot`
  per Kindle (titles, percent, highlights, words) and saves `shelf.json`.
- **Page** in `deckle_digest::shelf`: the `shelf.json` types, the selection rules and the
  section, read by `gather`. `deckle-digest` stays free of other Deckle crates.

**Tech Stack:** Rust 2024 workspace; `rusqlite` (workspace, bundled), `rand` 0.9 seeded
`StdRng`, `chrono`; existing crates `deckle-readership` (KRDS), `deckle-highlights`,
`deckle-library`; Tauri 2 / React / vitest for the Daily tab.

**Spec:** `docs/superpowers/specs/2026-10-22-from-your-shelf-design.md`

## Global Constraints

- Section config name and readership key `shelf`; title "From Your Shelf". In `STARTER` and
  `Config::default().sections` only, after `feeds` and before `lottery`. Existing configs are
  never changed on their own.
- State: `shelf.json` next to `digest.toml`; `#[serde(skip)] pub shelf_path: Option<PathBuf>`
  override for tests (like `almanac_path`); temp file and rename; missing or corrupt loads
  empty. One snapshot per Kindle (keyed by `deckle_core::sync::device_id`), replaced whole.
  `shown` keeps 90 days. Snapshots older than 30 days are ignored by the page.
- Gating: snapshot taken when `SectionKind::Shelf` is in `sections` **and**
  `cfg.sync.for_device(id).shelf` (new key, default `true`). The clippings read runs whenever
  the shelf is due, even with `clippings = false`.
- Only `.yjf` beside `kfx` books; files starting with `deckle_digest::EDITION_PREFIX` are
  skipped. Over MTP at most `MAX_YJF = 300` `.yjf` reads per connection; the rest are counted.
- Percent rules (spec §4.2), in order: `last / end` from the library send's `end`; `last / end`
  from the mass-storage details cache; `kindle_fraction` as "about N%" only if
  `KINDLE_FRACTION_OK` (a `const bool` set in Task 1 from the hardware check); else none.
  Rounded down, capped at 99 while `last < end`.
- In progress: `lpr` time within 60 days of the edition date; percent (when known) in 1..=97;
  not an edition; title not matching `[shelf] hide` (case-insensitive substring). Same book on
  two Kindles: later `lpr` wins. Newest first, at most 3.
- Line states, first that applies: percent ≥ 90 "nearly done"; ≥ 14 days "not opened in N
  days"; same day "last read today"; day before "last read yesterday"; ≤ 6 days "last read
  <Weekday>"; else "last read <Mon D>". Local time zone (`deckle_digest::tz::local_tz`).
- Highlight: 40–600 chars with a parsed date; ≥ 180 days old, else ≥ 30, else none; with a
  note first; not shown in the last 90 days; `StdRng::seed_from_u64(date.num_days_from_ce() as
  u64)`.
- Words: `lang = "en"`, `category != 100`; snapshot keeps 14 days; page shows 7 days, one per
  stem, newest first, at most 3; `usage` trimmed to 160 chars at a word boundary with "…".
- Exact copy:
  - kicker "Taken from your Kindle ({name}) when it was last plugged in, {Weekday}" (or "{n} days
    ago" past 6 days);
  - "On your nightstand", "You highlighted this {age} ago", "Words you looked up this week";
  - nudge "Pick up *{Title}* tonight? You left off {p}% in." / "Pick up *{Title}* tonight?" /
    "*{Title}*: one more evening should finish it.";
  - "Your note: {text}"; source line "{Title}, {Author} · page {p} · {Month D, YYYY}";
  - parts from another Kindle append " (from your {name})" to their heading;
  - Daily tab email line "Emailed papers include this page, so it passes through Amazon."
- Never fails a build or a delivery: read errors go in the report, store errors become build
  `notes`.
- The pre-push hook runs `cargo fmt --check` and clippy `-D warnings`. Keep both clean.

**Plan rulings** (left open by the spec; flagged to the owner):
- `KINDLE_FRACTION_OK` and `MAX_YJF` live in `deckle-shelf` and `deckle-core` as constants with
  the Task 1 measurement in their doc comment.
- The Word Wise definition is looked up at snapshot time and stored, so the build never opens
  the database.

## Review Focus

1. **A Kindle whose clippings file is missing this time** (the Scribe after the Paperwhite) must
   not erase the Paperwhite's highlights: snapshots are per Kindle. Test in Task 6.
2. **The edition itself, and long reads.** `deckle-daily-*.kfx` never appears on the nightstand;
   a long read (`deckle-…` but not the edition prefix) does. Test in Task 3.
3. **A `.yjf` with `lpr` but no time** (old firmware, bare position string) has no "last read"
   and must not count as in progress, rather than panicking or showing "today". Test in Task 5.
4. **`vocab.db` held with a `-wal` file** must show the newest lookups (copy both, open the
   copy). Test in Task 3.
5. **A rebuild the same day** prints the same highlight even though the first build recorded it
   in `shown`. Test in Task 5.

---

### Task 1: KRDS `book.info.store` and pace samples, then the Scribe check

**Files:**
- Modify: `crates/deckle-readership/src/krds.rs`
- Create: `crates/deckle-readership/examples/yjf.rs` (prints `Positions` for each path given)
- Test: `crates/deckle-readership/tests/krds.rs`

**Interfaces:**
- Produces: `Positions` gains `words_read: Option<i64>`, `kindle_fraction: Option<f64>`,
  `wpm: Vec<f64>`. `Positions` drops `Eq` (keeps `PartialEq`); fix any `Eq` bounds that break.
  `Value` gains `Double(f64)` for type 4 (big-endian `f64`); type 6 stays skipped.

- [ ] **Step 1: Write failing tests:**
  - `book_info_store_from_a_book`: `book.yjf` → `words_read == Some(9519)`,
    `(kindle_fraction.unwrap() * 10_000.0).round() == 1505.0`, `wpm.len() == 4`,
    `wpm[0].round() == 322.0`.
  - `book_info_store_from_an_edition`: `daily-2026-10-21.yjf` → `Some(4611)` and fraction
    rounds to 2984 (×10,000); `furthest == Some(103303)` still.
  - `wpm_samples_flatten_all_groups`: `timer.average.calculator` has groups of (int count, that
    many doubles); `daily-2026-10-21.yjf` yields 13 samples (7 + 6).
  - The existing `unknown_objects_and_trailing_fields_are_skipped` still passes.
- [ ] **Step 2: Run** `cargo test -p deckle-readership --test krds`. Expected: FAIL.
- [ ] **Step 3: Implement.** Match `("book.info.store", [Value::Int(w), Value::Double(f), ..])`
  and `("timer.average.calculator", values)` collecting every `Double`. Example:
  `cargo run -p deckle-readership --example yjf -- <file.yjf>…` prints one line per file.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `feat(readership): decode book.info.store and pace samples from .yjf`.
- [ ] **Step 6: Hardware check, with the owner (blocks Tasks 2–10's percent rule only):**
  - On the Scribe, for two books in progress, copy their `.yjf` off with Android File Transfer,
    run the example, and note the Kindle's own progress (library bar or "x%" in the book).
    `kindle_fraction` within 5 points on both → `KINDLE_FRACTION_OK = true` in Task 6.
  - Note whether store books appear in `documents/` or one folder down (the inventory walk's
    depth) and record it in `docs/devices.md`.
  - On the Paperwhite (mass storage): confirm `.yjf` files exist beside KFX books and that
    `system/vocabulary/vocab.db` (and any `-wal`) is present.
  - Record the findings in `fixtures/yjf/README.md` and `docs/devices.md`; commit
    `docs(devices): .yjf progress and vocab.db on the Scribe and Paperwhite`.

### Task 2: Record each book's end position

**Files:**
- Modify: `crates/deckle-core/src/pipeline.rs` (`RunReport.end_position`), set after
  `kpf_to_kfx` from `out.bytes`.
- Modify: `crates/deckle-library/src/sidecar.rs` (`SendRecord.end`) and `lib.rs`
  (`send_record`).
- Modify: `crates/deckle-core/src/inventory.rs` (`DeviceBook.end`, `Cached.end`).
- Modify: `apps/desktop/src-tauri/src/kindle.rs` only if the `SendRecord { … }` literal there
  stops compiling (it uses `..Default::default()`, so it shouldn't).
- Test: `crates/deckle-library/tests/` (existing send-record tests file) and inventory unit
  tests.

**Interfaces:**
- Produces:
  - `RunReport.end_position: Option<i64>` = `deckle_kfx::text::content_runs(&out.bytes).ok()
    .map(|(_, e)| e)`.
  - `SendRecord.end: Option<i64>` with `#[serde(default, skip_serializing_if =
    "Option::is_none")]`; `send_record` copies `report.end_position`.
  - `DeviceBook.end: Option<i64>` (`#[serde(skip)]`), filled by `details_cached` on mass
    storage and folders; the cache JSON gains `end` (old cache files without it re-read once:
    treat a missing `end` as a cache miss).

- [ ] **Step 1: Write failing tests:**
  - `send_record_carries_end`: a `RunReport` with `end_position: Some(1234)` → record `end ==
    Some(1234)`; a sidecar without `end` deserialises with `None` and re-serialises without the
    key.
  - `pipeline_reports_end`: converting `deckle_kfx::fixtures::path("deckle-mini.epub")` gives
    `end_position == Some(content_runs(&kfx_bytes).unwrap().1)`.
  - `listing_reads_end_from_kfx`: a folder target holding the mini KFX → `books[0].end` is
    `Some(_)` and equals `content_runs`; second listing hits the cache (same value, no re-read:
    assert via the cache file containing `"end"`).
- [ ] **Step 2: Run** `cargo test -p deckle-core -p deckle-library`. Expected: FAIL.
- [ ] **Step 3: Implement** as in Interfaces.
- [ ] **Step 4: Run** `cargo test --workspace`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(library): record each sent book's end position`.

### Task 3: Read books' positions and the vocabulary over one connection

**Files:**
- Modify: `crates/deckle-core/src/inventory.rs` (`list_with_positions`, `DeviceBook.yjf`).
- Create: `crates/deckle-core/src/vocab.rs` (`read_vocab_on`); register in `lib.rs`.
- Test: unit tests in both, with `fake_mtp`.

**Interfaces:**
- Produces:
  - `DeviceBook.yjf: Option<Vec<u8>>` (`#[serde(skip)]`).
  - `pub const MAX_YJF: usize = 300;`
  - `pub fn list_with_positions(conn: &mut Conn<'_>, cache: Option<&Path>, skip_prefix: &str)
    -> Result<(Inventory, usize), DeliverError>`: the inventory, with each `kfx` book's `.yjf`
    bytes when its `.sdr` holds one; books whose file starts with `skip_prefix` (callers pass
    `deckle_digest::EDITION_PREFIX`; `deckle-core` doesn't depend on `deckle-digest`) get no
    `.yjf`. The `usize` counts `.yjf` left unread past `MAX_YJF`.
  - `pub struct VocabFiles { pub db: Vec<u8>, pub wal: Option<Vec<u8>> }`
  - `pub fn read_vocab_on(conn: &mut Conn<'_>) -> Result<Option<VocabFiles>, DeliverError>`:
    mass storage `<root>/system/vocabulary/vocab.db` (+ `vocab.db-wal`); folder targets return
    `Ok(None)`; MTP tries `find_path(storage, content_root, "system/vocabulary/vocab.db")`
    once and returns `Ok(None)` when absent.

- [ ] **Step 1: Write failing tests:**
  - `positions_ride_in_the_listing`: folder with `Book.kfx` + `Book.sdr/Book9d6e.yjf`,
    `Other.kfx` without `.sdr`, `deckle-daily-2026-10-21.kfx` + `.sdr/…yjf`,
    `deckle-longread-walden.kfx` + `.sdr/…yjf`, `Sub/Inner.kfx` + `Sub/Inner.sdr/…yjf` →
    `Book.yjf` and the long read and `Sub/Inner` have bytes, `Other` has `None`, the edition is
    either absent from `books` or has `yjf: None` (assert `yjf.is_none()`), and `.yjr`/
    `.bad_file` are never returned (Review Focus 2).
  - `positions_over_mtp_in_one_session`: the same layout through `fake_mtp` → same result and
    `fake::opens() == 1`.
  - `cap_counts_the_rest`: 302 books with `.yjf` over fake MTP → 300 read, returned count 2.
  - `vocab_from_mass_storage_with_wal`: a `Device` root with `system/vocabulary/vocab.db` and
    `vocab.db-wal` → both bytes (Review Focus 4, first half).
  - `vocab_absent_is_none`: mass storage without it and fake MTP without it → `Ok(None)`.
- [ ] **Step 2: Run** `cargo test -p deckle-core --lib inventory vocab`. Expected: FAIL.
- [ ] **Step 3: Implement.** Share the walk with `list_on` (a private `walk(conn, cache, opts)`
  used by both); over MTP, take the `.yjf` handle from the `.sdr` listing the walk already does
  and `get_object` it.
- [ ] **Step 4: Run** `cargo test -p deckle-core`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(core): read book positions and vocab.db in the sync connection`.

### Task 4: Config and the `shelf.json` types

**Files:**
- Modify: `crates/deckle-digest/src/config.rs`:
  - `SectionKind::Shelf`, `section_key` → `"shelf"`;
  - `ShelfConfig { nightstand: bool = true, highlights: bool = true, words: bool = true, hide:
    Vec<String> = [] }` as `pub shelf`, `skip_serializing_if = "ShelfConfig::is_default"`;
  - `SyncConfig.shelf: bool = true`, `DeviceSync.shelf: Option<bool>`, `SyncActions.shelf`;
  - `#[serde(skip)] pub shelf_path: Option<PathBuf>`, `Config::shelf_store() -> PathBuf`
    (`shelf.json` beside `digest.toml`, as `almanac_path` resolves);
  - `STARTER`: `sections = [… "feeds", "shelf", "lottery", …]`, `shelf = true  # From Your
    Shelf reads this Kindle (Daily tab)` under `[sync]`, and a commented `# [shelf]` block with
    the four keys.
- Create: `crates/deckle-digest/src/shelf/mod.rs` and `shelf/store.rs`; `pub mod shelf;` in
  `lib.rs`.
- Fix every exhaustive `match` on `SectionKind` (gather: push nothing for `Shelf` yet; the
  CLI's section lists).
- Test: `crates/deckle-digest/tests/shelf.rs`.

**Interfaces:**
- Produces (`deckle_digest::shelf::store`, all `Clone, Debug, PartialEq, Serialize,
  Deserialize`, `#[serde(default)]`):
  - `BookSnap { file: String, title: String, authors: Vec<String>, asin: Option<String>, last:
    Option<i64>, furthest: Option<i64>, last_read: Option<String>, end: Option<i64>,
    kindle_fraction: Option<f64>, words_read: Option<i64>, wpm: Vec<f64> }`
  - `HighlightSnap { id: String, title: String, author: String, text: String, notes:
    Vec<String>, page: Option<String>, added: Option<NaiveDate> }`
  - `WordSnap { word: String, stem: String, usage: String, book: String, at: String,
    definition: Option<String> }`
  - `KindleSnapshot { name: String, seen: String, books: Vec<BookSnap>, skipped: usize,
    highlights_shared: bool, highlights: Vec<HighlightSnap>, words_shared: bool, words:
    Vec<WordSnap> }`
  - `Shelf { kindles: BTreeMap<String, KindleSnapshot>, shown: BTreeMap<String, NaiveDate> }`
  - `Shelf::load(path: &Path) -> Shelf`, `save(&self, path: &Path) -> Result<(), String>`,
    `put(&mut self, id: &str, snap: KindleSnapshot)`, `forget(path: &Path) ->
    Result<(), String>` (removes the file; missing is `Ok`), `prune_shown(&mut self, today:
    NaiveDate)` (keeps 90 days).
  - `Shelf::fresh(&self, today: NaiveDate) -> Vec<(&str, &KindleSnapshot)>`: snapshots seen
    within 30 days, newest `seen` first.

- [ ] **Step 1: Write failing tests:**
  - `starter_has_shelf_between_feeds_and_lottery`; `default_sections_too`;
    `untouched_config_saves_no_shelf_table`; `sync_shelf_defaults_on_and_follows_devices`
    (`[sync.devices."G0"] shelf = false` → `for_device("G0").shelf == false`, others true);
    `section_key(SectionKind::Shelf, None) == "shelf"`.
  - `round_trip_and_corrupt_file`: save/load equal; garbage bytes load empty.
  - `put_replaces_one_kindle_only`: put A, put B, put A again → B untouched.
  - `fresh_ignores_old_snapshots`: seen 31 days before → excluded; 30 → included.
  - `prune_shown_keeps_ninety_days`.
- [ ] **Step 2: Run** `cargo test -p deckle-digest --test shelf`. Expected: FAIL to compile.
- [ ] **Step 3: Implement** as in Interfaces.
- [ ] **Step 4: Run** `cargo test --workspace`. Expected: PASS (update tests asserting the old
  starter section list).
- [ ] **Step 5: Commit** `feat(digest): shelf section key, settings and shelf.json`.

### Task 5: Selection rules

**Files:**
- Create: `crates/deckle-digest/src/shelf/select.rs`.
- Test: `crates/deckle-digest/tests/shelf.rs`.

**Interfaces:**
- Consumes: Task 4 types.
- Produces:
  - `pub struct Nightstand { pub books: Vec<Line>, pub nudge: Option<String>, pub from: String
    }` and `pub struct Line { pub title: String, pub percent: Option<u8>, pub about: bool, pub
    state: String }`.
  - `pub fn percent(b: &BookSnap, fraction_ok: bool) -> Option<(u8, bool)>` (`bool` = "about").
  - `pub fn nightstand(shelf: &Shelf, cfg: &ShelfConfig, date: NaiveDate, tz: Tz, fraction_ok:
    bool) -> Option<Nightstand>`.
  - `pub struct Pick { pub highlight: HighlightSnap, pub age: String, pub from: String }` and
    `pub fn highlight(shelf: &Shelf, date: NaiveDate) -> Option<Pick>`: ignores `shown`
    entries dated `date` itself (Review Focus 5).
  - `pub fn words(shelf: &Shelf, date: NaiveDate, tz: Tz) -> Vec<(WordSnap, String)>` (word,
    "<Book>, <Weekday>"), plus the `from` name via the snapshot it came from.
  - `pub fn age(added: NaiveDate, date: NaiveDate) -> String`: "N weeks" under 2 months, "N
    months" under 24, else "N years".
- `fraction_ok` is passed in (from `deckle_shelf::KINDLE_FRACTION_OK` by the caller in Task 7)
  so `deckle-digest` stays standalone.

- [ ] **Step 1: Write failing tests** (date 2026-10-22, `America/New_York`):
  - `percent_by_source`: `end` set → `last*100/end` floored, `about=false`; 664520/412004 → 61;
    `last == end` → 100; `last = end - 1` → 99; no `end`, fraction 0.613 and `fraction_ok` →
    `(61, true)`; same with `fraction_ok = false` → `None`.
  - `in_progress_window`: `last_read` 60 days before → in; 61 → out; percent 0 and 98 → out; 1
    and 97 → in; no percent → in; `last_read: None` → out (Review Focus 3); a `hide = ["mush"]`
    title out.
  - `states`: 91% → "nearly done"; 23 days → "not opened in 23 days"; same day "last read
    today"; 1 day "last read yesterday"; 2026-10-20 → "last read Tuesday"; 9 days → "last read
    Oct 13".
  - `newest_three_and_two_kindles`: four books → newest three; the same ASIN on two snapshots →
    one line, the later `last_read`.
  - `nudge_rules`: newest read 2 days ago at 62% → "Pick up *The Count of Monte Cristo*
    tonight? You left off 62% in."; at 91% → "one more evening" copy; 7 days old → `None`; no
    percent → short copy.
  - `highlight_age_and_preference`: candidates 200 days (no note), 200 days (note), 40 days →
    the noted 200-day one; only 40-day → that one; only 10-day → `None`; 39-char and 601-char →
    never.
  - `highlight_no_repeat_and_stable`: shown 2026-09-01 → skipped; two calls same date → equal;
    `shown` holding today's pick dated 2026-10-22 → still picked (Review Focus 5).
  - `age_wording`: 30 days "4 weeks"; 19 months "19 months"; 3 years "3 years".
  - `words_rules`: lookups 1, 3, 8 days ago, two of one stem, one `category` already filtered
    at snapshot (not here) → the 7-day ones, one per stem, newest first, ≤ 3.
- [ ] **Step 2: Run** `cargo test -p deckle-digest --test shelf`. Expected: FAIL.
- [ ] **Step 3: Implement.** Highlight pick: filter, partition by age tier, prefer
  `!notes.is_empty()`, then `choose` with the date-seeded `StdRng` over the remaining list
  sorted by `id` (sorting makes it stable across file orders).
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `feat(digest): From Your Shelf selection rules`.

### Task 6: The `deckle-shelf` crate: taking a snapshot

**Files:**
- Create: `crates/deckle-shelf/{Cargo.toml,src/lib.rs,src/books.rs,src/highlights.rs,
  src/words.rs}`; add to the workspace `members` and `[workspace.dependencies]`.
- Test: `crates/deckle-shelf/tests/snapshot.rs`; fixture `vocab.db` built in the test.

**Interfaces:**
- Consumes: `deckle_core::inventory::{DeviceBook, Inventory}`, `deckle_core::vocab::VocabFiles`,
  `deckle_library::shelf::shelve`, `deckle_readership::parse_yjf`,
  `deckle_highlights::{parse::parse, book::{group, Item, entry_id}}`,
  `deckle_core::wordwise::{default_cache_dir, DB_FILE}` (and its env override),
  `deckle_digest::shelf::store::*`.
- Produces:
  - `pub const KINDLE_FRACTION_OK: bool` (value and evidence from Task 1 step 6).
  - `pub struct Read { pub books: Vec<DeviceBook>, pub skipped: usize, pub clippings:
    Option<Vec<u8>>, pub vocab: Option<VocabFiles> }`.
  - `pub fn snapshot(read: Read, name: &str, device_key: &str, library: Option<&Library>, now:
    DateTime<Utc>) -> (KindleSnapshot, Vec<String>)` (the `Vec` is per-book errors).
  - `pub fn record(path: &Path, id: &str, snap: KindleSnapshot) -> Result<Summary, String>`
    (load, `put`, save) and `pub struct Summary { pub in_progress: usize, pub highlights:
    bool, pub words: usize }` with `describe() -> String` = "Shelf: 3 books in progress, 1
    highlight source, 0 words".
  - `books::title_from_file(file: &str) -> Option<String>`: strips the extension, a trailing
    `_EBOK`/`_PDOC`, a trailing `-B0XXXXXXXX`/`_B0XXXXXXXX` ASIN; empty → `None`.
  - `words::read(files: &VocabFiles, since: DateTime<Utc>, define: &dyn Fn(&str) ->
    Option<String>) -> Result<Vec<WordSnap>, String>`.

- [ ] **Step 1: Write failing tests:**
  - `books_get_titles_and_ends`: a `DeviceBook` with KFX title → kept; one with no title but a
    library match (`shelve` by send record) → library title and the send's `end`; one titled
    only by file `Walden_B0ABCDEFGH.kfx` → "Walden"; `yjf: None` → not in `books`; a corrupt
    `.yjf` → an error string, book left out. Use `fixtures/yjf/book.yjf` bytes for `yjf`.
  - `highlights_snapshot`: fixture clippings from `crates/deckle-highlights` tests (copy the
    small fixture) → highlights with notes attached, editions dropped, `highlights_shared ==
    true`; `clippings: None` → empty, `false`.
  - `scribe_shaped_read`: books only → `highlights_shared == false`, `words_shared == false`,
    `books.len() > 0`.
  - `vocab_read_filters_and_uses_wal`: build `vocab.db` with `WORDS`, `LOOKUPS`, `BOOK_INFO`
    in WAL mode, insert one lookup only into the WAL (keep the connection open while copying
    both files) → it appears (Review Focus 4); a `lang = "de"` row and a `category = 100` row
    are absent; a lookup 15 days old is absent; `define` returning `Some("hopeful")` fills
    `definition`.
  - `record_keeps_other_kindles` (Review Focus 1): record the Paperwhite with highlights, then
    the Scribe with none → the Paperwhite's highlights are still in the file.
- [ ] **Step 2: Run** `cargo test -p deckle-shelf`. Expected: FAIL to compile.
- [ ] **Step 3: Implement.** Vocab: write the bytes to a `tempfile::tempdir()` as `vocab.db`
  (and `vocab.db-wal`), open read-only, query
  `SELECT w.word, w.stem, l.usage, b.title, l.timestamp FROM LOOKUPS l JOIN WORDS w ON
  l.word_key = w.id LEFT JOIN BOOK_INFO b ON l.book_key = b.id WHERE w.lang = 'en' AND
  w.category != 100 AND l.timestamp >= ?1 ORDER BY l.timestamp DESC`. Definitions: open the
  Word Wise database only if the file exists; query `short_def` by `forms.form = stem`.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `feat(shelf): take a snapshot of a Kindle's shelf`.

### Task 7: The section on the page

**Files:**
- Modify: `crates/deckle-digest/src/shelf/mod.rs` (`section`), `lib.rs` (`gather`),
  `render.rs` (CSS).
- Test: `crates/deckle-digest/tests/shelf.rs`; golden
  `crates/deckle-digest/tests/fixtures/shelf/golden-page.xhtml` (`-text` in `.gitattributes`).

**Interfaces:**
- Consumes: Task 5.
- Produces:
  - `pub fn section(shelf: &mut Shelf, cfg: &Config, date: NaiveDate, tz: Tz, fraction_ok:
    bool) -> Option<Section>`: `None` when no part has content. Articles in order: "On your
    nightstand" (each line `<p class="shelf-book"><i>Title</i></p><div class="bar"><div
    style="width:62%"></div></div><p class="meta">62% · last read Tuesday</p>`, then the nudge
    `<p class="nudge">`), "You highlighted this … ago" (`<blockquote>`, source line, "Your
    note:"), "Words you looked up this week". It records the picked highlight in `shelf.shown`
    under `date`. Parts off in `[shelf]` are skipped.
  - The rule-3 flag reaches `gather` through a new `pub fn gather_with(cfg, date, now, net,
    opts: &GatherOptions)` with `GatherOptions {
    shelf_fraction_ok: bool }`; `gather` calls it with `false`. The CLI and app pass
    `deckle_shelf::KINDLE_FRACTION_OK`.
  - In `gather_with`: for `SectionKind::Shelf`, load `cfg.shelf_store()`, call `section`,
    `prune_shown`, save; `None` → skip and push the note "From Your Shelf left out: plug your
    Kindle in once to fill it" (only when no fresh snapshot exists); save errors → a note.

- [ ] **Step 1: Write failing tests:**
  - `golden_page`: fixed `Shelf` (Scribe books + Paperwhite highlights and words, both fresh),
    date 2026-10-22 → `render::render` output equals the golden file (`UPDATE_GOLDEN=1` writes
    it; read it before committing) and parses with `roxmltree`; it contains "On your
    nightstand", "You highlighted this 19 months ago", " (from your Kindle Paperwhite)",
    "Your note: read this again before the cabin trip", "Words you looked up this week".
  - `scribe_only_prints_nightstand`: books only → one article, no highlight or words headings.
  - `nothing_leaves_section_out`: empty shelf → `section` is `None`; through `gather_with` the
    edition has no `shelf` key and `notes` has the plug-in line; with a 40-day-old snapshot of
    finished books only → `None` and no plug-in note.
  - `parts_switch_off`: `[shelf] words = false` → no words article.
  - `rebuild_same_day_same_highlight`: two `gather_with` calls on one date → equal shelf
    articles (Review Focus 5, end to end).
  - `readership_key`: the section's `key == "shelf"` and
    `deckle_readership::layout::layout_from_kfx` finds "From Your Shelf" in a built KFX (put
    this test in `crates/deckle-readership/tests/layout.rs`, reusing its build helper).
- [ ] **Step 2: Run** `cargo test -p deckle-digest --test shelf`. Expected: FAIL.
- [ ] **Step 3: Implement**, CSS: `.bar { border: 1px solid black; height: 0.5em; width: 60%
  }`, `.bar > div { background-color: black; height: 100% }`, `.nudge { font-style: italic }`.
- [ ] **Step 4: Run** `cargo test --workspace`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(digest): the From Your Shelf page`.

### Task 8: The shelf read on connect (watcher, push)

**Files:**
- Modify: `crates/deckle-core/src/sync.rs` (`Actions.shelf`, `Plan.shelf`, `Connected.shelved`,
  `run_with`), `crates/deckle-core/src/queue.rs` (`FlushReport.shelf`, `shelf_error`).
- Modify: `crates/deckle-cli/src/lib.rs`: `sync_actions`, `push_plan`, `sync_to`,
  `DigestOutcome` (+ `describe`), every `gather` call → `gather_with`.
- Add `deckle-shelf.workspace = true` to `crates/deckle-cli/Cargo.toml`.
- Test: `crates/deckle-core/src/sync.rs` unit tests; `crates/deckle-cli/tests/shelf.rs` using
  the readership test harness (temp `HOME`, folder Kindle).

**Interfaces:**
- Produces:
  - `pub struct ShelfRead { pub books: Vec<DeviceBook>, pub skipped: usize, pub vocab:
    Option<VocabFiles> }` in `deckle_core::sync`; `FlushReport.shelf: Option<ShelfRead>`
    (`#[serde(skip)]`), `FlushReport.shelf_error: Option<String>`.
  - `run_with(…, stems, skip_prefix: &str)`: after prune and before clippings, when
    `plan.shelf`: `list_with_positions(conn, None, skip_prefix)` then `read_vocab_on(conn)`; an
    error sets `shelf_error`, never fails.
  - The watcher's shelf is once per connection; `clippings` is also due when `shelf` is.
  - CLI: `fn shelf_on(cfg: &Config, id: &str) -> bool` = `sections` contains `Shelf` &&
    `cfg.sync.for_device(id).shelf`. `sync_actions` sets `shelf` and `clippings: a.clippings
    || shelf`. `push_plan` with an empty outbox runs when `readership || shelf ||` (puzzle's
    `clippings` if merged). In `sync_to`, the highlights export runs only when
    `cfg.sync.for_device(id).clippings`; the snapshot gets the clippings bytes regardless and
    is recorded with `deckle_shelf::record` after the session closes.
  - `DigestOutcome.shelf: Option<deckle_shelf::Summary>`; describe adds `"      {summary}"`
    and `"      shelf: {error}"`.

- [ ] **Step 1: Write failing tests:**
  - core `shelf_rides_with_push_once_per_connection`: tick with pending → `plan.shelf` and
    `plan.clippings` true; next tick same id → false; new connection → true again.
  - core `shelf_read_in_the_same_session`: fake MTP push + shelf → one open,
    `report.shelf.unwrap().books` has the fixture book.
  - core `shelf_error_keeps_the_delivery`: a walk failure (fake storage error after the push) →
    `delivered` non-empty and `shelf_error` set.
  - cli `push_records_the_shelf`: folder Kindle with a KFX + `.sdr/x.yjf` (copy
    `fixtures/yjf/book.yjf`) and a `My Clippings.txt`, `[sync] clippings = false`, empty
    outbox → `deckle digest push --to <folder>` writes `shelf.json` with one book and
    highlights, no highlights Markdown written, stdout contains "Shelf: ".
  - cli `shelf_off_for_device_reads_nothing`: `[sync.devices."<id>"] shelf = false` →
    `shelf.json` absent.
  - cli `shelf_not_in_sections_reads_nothing`.
- [ ] **Step 2: Run** `cargo test -p deckle-core --lib sync` and `cargo test -p deckle-cli
  --test shelf`. Expected: FAIL.
- [ ] **Step 3: Implement** as in Interfaces.
- [ ] **Step 4: Run** `cargo test --workspace`. Expected: PASS, including the existing
  readership and highlights CLI tests.
- [ ] **Step 5: Commit** `feat(shelf): take the shelf snapshot whenever the Kindle connects`.

### Task 9: CLI `digest shelf`, Kindle tab, Daily tab, docs

**Files:**
- Modify: `crates/deckle-cli/src/{cli.rs,lib.rs}`: `DigestCommand::Shelf { json: bool, forget:
  bool }`.
- Modify: `apps/desktop/src-tauri/src/kindle.rs` (`shelf_on`): after the readership record,
  when the shelf is due for this Kindle, `list_with_positions` is **not** called again: use
  `inv.books` from a `list_with_positions` call that replaces `list_on` at the top of
  `shelf_on` (one walk), plus `read_vocab_on` and the clippings bytes `highlights_on` read;
  then `deckle_shelf::snapshot` + `record`. Errors are logged.
- Create: `apps/desktop/src-tauri/src/shelf.rs` with Tauri commands `shelf_status() ->
  ShelfStatus` and `shelf_forget() -> ShelfStatus`; register in `lib.rs`.
- Modify: `apps/desktop/src/types.ts` (`"shelf"` in `SectionKind`, `SECTION_LABEL.shelf = "From
  Your Shelf"`, `ALL_SECTIONS` between `"feeds"` and `"lottery"`), the Daily tab's section list
  component (status line, Forget button, email line), the Settings → Kindles per-device
  checkbox *Use this Kindle for From Your Shelf* (writes `[sync.devices."<id>"] shelf`, like
  the readership checkbox).
- Modify: `docs/daily.md`, `docs/devices.md`, `README.md` (CLI lines), `CHANGELOG.md`.
- Test: `crates/deckle-cli/tests/shelf.rs`, `apps/desktop/src-tauri` unit test for
  `shelf_status`, vitest.

**Interfaces:**
- Produces: `ShelfStatus { kindle: Option<String>, seen: Option<String>, in_progress: usize,
  highlights_shared: bool, words_shared: bool, email_on: bool }` (camelCase over IPC). The
  status line: "From your {kindle}, {Weekday}: {n} books in progress." plus "Highlights and
  words aren't shared by this Kindle." when both are false.
- CLI text: the kindle and seen time, then the page as text (titles with percent and state, the
  nudge, the highlight, the words); `--json` gives `{snapshot summary, page}`; `--forget` calls
  `Shelf::forget` and prints "Forgot the shelf snapshot."

- [ ] **Step 1: Write failing tests:**
  - cli `digest_shelf_prints_the_page`: with a fixture `shelf.json`, output contains "On your
    nightstand" and the book title; `--json` parses with `.page.nightstand.books` an array;
    `--forget` removes the file.
  - tauri `status_says_what_the_kindle_shares`: a Scribe-shaped snapshot → `highlights_shared
    == false` and `in_progress == 1`.
  - vitest `shelf_is_a_section`: `ALL_SECTIONS` has `"shelf"` right after `"feeds"`; label "From
    Your Shelf"; the status line renders the "aren't shared" sentence for a Scribe status.
- [ ] **Step 2: Run** `cargo test -p deckle-cli --test shelf`, `cargo test -p deckle-desktop`, `pnpm --dir apps/desktop test`. Expected: FAIL.
- [ ] **Step 3: Implement** as in Files and Interfaces.
- [ ] **Step 4: Run** `cargo test --workspace`, `pnpm --dir apps/desktop test`, `pnpm --dir
  apps/desktop build`. Expected: PASS.
- [ ] **Step 5: Verify visually.** `deckle digest build -o <tmp>` with the golden fixture as
  `shelf.json`; open the shelf XHTML from the EPUB in the browser pane (bars, quote, words). The
  Daily tab section list and status line in the browser pane over the IPC mock.
- [ ] **Step 6: Commit** `feat(shelf): digest shelf command, Kindle and Daily tabs, docs`.

### Task 10: Device check, with the owner

- [ ] **Step 1:** Scribe over MTP: plug in with `deckle digest watch` running; time the
  connection with and without the shelf (`[sync] shelf = false`). Record both in
  `docs/devices.md`; if the shelf adds over 10 s, lower `MAX_YJF` and note why.
- [ ] **Step 2:** Build tomorrow's paper (`deckle digest build`), send it, and check the page on
  the Scribe: the books and percents match what the Kindle shows (exact for Deckle-sent books;
  "about" only if `KINDLE_FRACTION_OK`), no highlight or words parts, the kicker names the
  Scribe.
- [ ] **Step 3:** Paperwhite over mass storage: plug in, then build and check the highlight
  ("from your Kindle Paperwhite" when the Scribe's snapshot is newer) and the words.
- [ ] **Step 4:** Commit any doc updates `docs(devices): From Your Shelf on the Scribe and
  Paperwhite`.
