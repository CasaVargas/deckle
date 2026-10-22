# Readership Report — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Record where each Deckle Daily section starts when the edition is built, read the Kindle's `.yjf` reading position back on every connect, and show "how far you read" per section on the Daily tab and in the CLI, with one-click Drop / Move up suggestions.

**Architecture:** A new crate, `deckle-readership`, holds everything that isn't a device: the KRDS `.yjf` decoder, the edition layout taken from the finished KFX, `readership.json`, the report and its suggestions (which edit a `deckle_digest::Config`). `deckle-core` gets one function that reads each edition's `.yjf` over an existing `Conn`, and the sync watcher learns a `readership` action that runs first in the same session as push, prune and clippings. `deckle-digest` gains section keys, the `[sync] readership` key and a comment-keeping `[[feeds]]` merge. The CLI and the desktop app glue them together; the app's Daily run already goes through the CLI.

**Tech Stack:** Rust 2024 workspace (serde, serde_json, chrono, toml_edit from `[workspace.dependencies]`), `deckle-kfx` text runs, Tauri 2 commands, React/TypeScript Daily tab, vitest.

**Spec:** `docs/superpowers/specs/2026-10-22-readership-report-design.md`

## Global Constraints

- Nothing leaves the Mac; no network calls.
- One connection: the `.yjf` read rides in the `Conn` an action already holds, and runs **before** push and prune (prune deletes the `.sdr`).
- A failed `.yjf` read never fails a delivery: it's reported beside it (same rule as clippings).
- `readership.json` lives next to `highlights.json`: `deckle_library::config::default_path().with_file_name("readership.json")`. Temp file + rename on save; missing or corrupt starts empty.
- `[sync] readership` defaults to **true**, with per-device overrides under `[sync.devices."<id>"]`, exactly like `clippings`.
- Positions are KFX position ids (the number after `:` in a `.yjf` position string). Percent through = position / `end`.
- The report window is the last **14** editions with a layout; keep the last **60** editions in state; suggestions need **7** opened editions; a new section needs **5** appearances before it gets a share.
- Suggestion thresholds: share **≤ 20%**; Drop needs a median start in the **last third** (`start/end ≥ 2/3`); "where you stop" shows when the median furthest point is **< 75%**. At most **3** suggestions. Dismiss lasts **14** editions.
- Never suggest for `front` or `reading`.
- `digest.toml` edits keep comments, including inside `[[feeds]]`.
- The KRDS decoder is ported from `krds.py` (K-R-D-S/KRDS, © 2019 John Howell, GPL-3.0): keep that attribution in the module doc.
- New source files start with `// SPDX-License-Identifier: GPL-3.0-or-later` and a `//!` doc line.
- CLI parity with the app; `--json` on the new command.

## Review Focus

1. **An edition that isn't on this Kindle** (pruned already, delivered to the other Kindle, or shipped under an older name such as `Deckle - The Deckle Daily -- Tuesday, October 20, 2026.kfx`): record nothing for it on this device. It must not count as "not opened". Test: Task 6 `an_edition_not_on_the_kindle_is_absent_not_unopened`.
2. **A front-page line that equals a later section's title** (the lead headline is "Weather", or "Inside today" lists a section by name): the layout still picks the section's own heading, not the front-page line. Test: Task 3 `section_starts_come_from_section_boundaries_not_text_search`.
3. **A truncated or foreign `.yjf`** (half-written file, a `.yjr` passed by mistake, unknown firmware objects): that one edition gets an error or no positions; the other editions on the Kindle are still recorded. Tests: Task 2 `truncated_input_is_an_error_not_a_panic`, Task 4 `one_bad_yjf_does_not_stop_the_others`.
4. **`digest push` with an empty outbox** while a Kindle is connected: it still records positions (today it returns early). Test: Task 7 `push_with_an_empty_outbox_still_reads_positions` (unit test of `push_plan`).
5. **Two Kindles, then one of them goes backwards** (an older `.yjf` from a reset device reports a smaller `fpr`): the report uses the furthest per date across devices, and a device's own entry is replaced by its latest read. Test: Task 5 `two_devices_take_the_furthest`.

---

### Task 1: Section keys, `[sync] readership`, and a `[[feeds]]` merge that keeps comments

**Files:**
- Modify: `crates/deckle-digest/src/lib.rs` (`Section`, `SectionSummary`, `gather`, `report`)
- Modify: `crates/deckle-digest/src/sources/*.rs` where a `Section` is built (set `key`)
- Modify: `crates/deckle-digest/src/config.rs` (`SyncConfig`, `DeviceSync`, `SyncActions`, `for_device`, `STARTER`, `merge_table`)
- Test: `crates/deckle-digest/src/config.rs` (unit tests), `crates/deckle-digest/tests/` (gather keys)

**Interfaces:**
- Produces: `Section.key: String` and `SectionSummary.key: String`, values `"reading"`, `"weather"`, `"local"`, `"lottery"`, `"horoscope"`, `"onthisday"`, `"feeds:<feed name>"`. (The front page isn't a `Section`; later tasks use the key `"front"` for it.)
- Produces: `SyncConfig.readership: bool` (default `true`), `DeviceSync.readership: Option<bool>`, `SyncActions.readership: bool`.
- Produces: `pub fn section_key(kind: SectionKind, feed: Option<&str>) -> String` in `config.rs`, the one place keys are spelled.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn gathered_sections_carry_their_keys() {
    // offline Net stub as in the existing gather tests; feeds "Ars Science" and "HN"
    let g = gather(&cfg_with_feeds(&["Ars Science", "HN"]), date(), now(), &Offline);
    let keys: Vec<_> = g.edition.sections.iter().map(|s| s.key.as_str()).collect();
    assert_eq!(keys, ["weather", "local", "feeds:Ars Science", "feeds:HN", "lottery", "horoscope", "onthisday"]);
}

#[test]
fn readership_defaults_on_and_follows_device_overrides() {
    let c = Config::parse("[sync.devices.\"G000\"]\nreadership = false\n").unwrap();
    assert!(c.sync.readership);
    assert!(!c.sync.for_device("G000").readership);
    assert!(c.sync.for_device("other").readership);
}

#[test]
fn saving_keeps_comments_inside_feeds_when_one_is_dropped_or_moved() {
    // file: [[feeds]] A (comment "# morning tech"), B (comment "# keep"), C
    let mut cfg = Config::load(&path).unwrap();
    cfg.feeds.retain(|f| f.name != "A");
    cfg.feeds.swap(0, 1); // C, B
    save(&path, &cfg).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("morning tech"));
    assert!(text.contains("# keep"));
    assert!(text.find("name = \"C\"").unwrap() < text.find("name = \"B\"").unwrap());
}
```

(The starter template line is pinned too: `readership = true                # note how far you read the paper (Daily tab)`.)

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p deckle-digest readership saving_keeps gathered_sections 2>&1 | tail -20`
Expected: compile errors (`no field key`, `no field readership`), then the feeds test failing on the lost `# keep`.

- [ ] **Step 3: Implement**

Add the fields and `section_key`; set `key` where each section is pushed in `gather` (`Feeds` uses the feed's `name`). In `merge_table`, handle `(Some(Item::ArrayOfTables(o)), Item::ArrayOfTables(n))`: build the result in `n`'s order, taking each element from `o` with the same `name` and merging it with `merge_table` (keeps its decor and comments); new names come from `n` whole; names only in `o` are dropped.

- [ ] **Step 4: Run the crate's tests**

Run: `cargo test -p deckle-digest 2>&1 | tail -5`
Expected: all pass, including the existing save tests.

- [ ] **Step 5: Commit**

```bash
git add crates/deckle-digest
git commit -m "feat(digest): section keys, [sync] readership, keep comments in [[feeds]] on save"
```

---

### Task 2: `deckle-readership` crate — KRDS `.yjf` decoding

**Files:**
- Create: `crates/deckle-readership/Cargo.toml` (deps: serde, serde_json, chrono, deckle-kfx, deckle-digest; dev: tempfile)
- Create: `crates/deckle-readership/src/lib.rs`, `src/krds.rs`
- Create: `fixtures/yjf/daily-2026-10-21.yjf`, `fixtures/yjf/daily-2026-10-20.yjf`, `fixtures/yjf/book.yjf`, `fixtures/yjf/README.md` (copied from the session scratchpad `yjr/` pull: the three `.yjf` files; README says where they came from and that they hold reading positions and speeds only)
- Test: `crates/deckle-readership/tests/krds.rs`

**Interfaces:**
- Produces: `pub struct Positions { pub furthest: Option<i64>, pub last: Option<i64>, pub last_read: Option<String> }` (`last_read` as `YYYY-MM-DDTHH:MM:SS`, local time as the Kindle wrote it, `None` when the time is `-1`).
- Produces: `pub fn parse_yjf(bytes: &[u8]) -> Result<Positions, KrdsError>`; `KrdsError` is a `thiserror` enum with `BadSignature`, `Truncated { at: usize }`, `BadType { byte: u8, at: usize }`.
- Produces: `pub fn position_id(s: &str) -> Option<i64>` (the number after the last `:`).

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn reads_furthest_and_last_from_a_real_scribe_yjf() {
    let p = parse_yjf(&fixture("daily-2026-10-21.yjf")).unwrap();
    assert_eq!(p.furthest, Some(103303));
    assert_eq!(p.last, Some(103303));
    assert_eq!(p.last_read.as_deref(), Some("2026-10-21T22:06:30"));
}

#[test]
fn furthest_can_be_ahead_of_last() {
    let p = parse_yjf(&fixture("daily-2026-10-20.yjf")).unwrap();
    assert_eq!((p.furthest, p.last), (Some(195894), Some(153130)));
}

#[test]
fn unknown_objects_and_trailing_fields_are_skipped() {
    // signature, then an unknown object "x.new" holding an int and a string, then a real "lpr"
    let bytes = synthetic(&[obj("x.new", &[int(7), utf("hi")]), obj("lpr", &[int(2), utf("AQ:42"), long(-1), int(9)])]);
    assert_eq!(parse_yjf(&bytes).unwrap().last, Some(42));
}

#[test]
fn truncated_input_is_an_error_not_a_panic() {
    let b = fixture("book.yjf");
    for n in [0, 4, 8, 20, b.len() - 1] {
        assert!(parse_yjf(&b[..n]).is_err(), "len {n}");
    }
}

#[test]
fn position_ids() {
    assert_eq!(position_id("AQULAAB1AgAA:54797"), Some(54797));
    assert_eq!(position_id("nonsense"), None);
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p deckle-readership --test krds 2>&1 | tail -10`
Expected: compile error, `parse_yjf` not found.

- [ ] **Step 3: Implement `parse_yjf` in `src/krds.rs`**

Signature `00 00 00 00 00 1A B1 26`, then a value stream. Type bytes (signed): `0` bool (1 byte), `1` int (i32 BE), `2` long (i64 BE), `3` utf (bool "is empty" flag, then u16 BE length + UTF-8 when the flag is 0), `4` double, `5` short, `6` float, `7` byte, `9` char (2 bytes), `-2` object begin (utf name, values until `-1`), `-1` object end. Decode every top-level object generically into a small value tree; then read `fpr` (first value: position string) and `lpr` (first value an int version ≤ 2 followed by position string and a long ms time; or, in old files, a bare position string). Times: ms since epoch → local naive time via `chrono::Local`. Any read past the end is `Truncated`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p deckle-readership --test krds 2>&1 | tail -5`
Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/deckle-readership fixtures/yjf Cargo.lock
git commit -m "feat(readership): decode the Kindle's .yjf reading position (KRDS)"
```

---

### Task 3: An edition's layout from its KFX

**Files:**
- Modify: `crates/deckle-kfx/src/text.rs` (section boundaries)
- Create: `crates/deckle-readership/src/layout.rs`
- Test: `crates/deckle-kfx/tests/text_runs.rs`, `crates/deckle-readership/tests/layout.rs`

**Interfaces:**
- Consumes: `deckle_kfx::text::{content_runs, Run}`; `SectionSummary.key` (Task 1).
- Produces: `pub fn section_first_runs(kfx: &[u8]) -> Result<Vec<Run>, TextError>` in `deckle-kfx`: the first text run of each reading-order section, in order (sections with no text, like the cover, are left out).
- Produces: `#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)] pub struct Layout { pub date: NaiveDate, pub title: String, pub file: String, pub end: i64, pub sections: Vec<SectionStart> }` and `pub struct SectionStart { pub key: String, pub name: String, pub start: i64 }`.
- Produces: `pub fn layout_from_kfx(kfx: &[u8], date: NaiveDate, title: &str, file: &str, sections: &[(String, String)]) -> Result<Layout, String>`, where `sections` is `(key, printed title)` in paper order, **without** the front page; the result starts with `front` (`name` "Front page", `start` = first run's position).
- Produces: `pub fn section_at(layout: &Layout, position: i64) -> Option<&SectionStart>` (last section starting at or before it).

- [ ] **Step 1: Write the failing tests**

```rust
// deckle-kfx/tests/text_runs.rs
#[test]
fn section_first_runs_of_the_mini_kfx_are_its_chapter_headings() {
    let kfx = std::fs::read(deckle_kfx::fixtures::path("deckle-mini-pdoc.golden.kfx")).unwrap();
    let firsts = section_first_runs(&kfx).unwrap();
    assert_eq!(firsts[0].text, "Chapter One");
    assert_eq!(firsts[0].pid, 3);
    assert_eq!(firsts.len(), 3);
}

// deckle-readership/tests/layout.rs (mini KFX: the "front page" is chapter one)
#[test]
fn section_starts_come_from_section_boundaries_not_text_search() {
    let names = chapter_names(); // [("b", "Chapter Two"), ("c", "Chapter Three")] as read from section_first_runs
    let l = layout_from_kfx(&mini(), d("2026-10-21"), "Mini", "deckle-daily-2026-10-21", &names).unwrap();
    assert_eq!(l.sections.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), ["front", "b", "c"]);
    assert_eq!(l.sections[0].start, 3);
    assert!(l.sections.windows(2).all(|w| w[0].start < w[1].start));
    assert_eq!(l.end, content_runs(&mini()).unwrap().1);
}

#[test]
fn a_section_missing_from_the_kfx_is_an_error() {
    let names = vec![("x".to_owned(), "No Such Heading".to_owned())];
    assert!(layout_from_kfx(&mini(), d("2026-10-21"), "Mini", "f", &names).is_err());
}

#[test]
fn positions_map_to_the_section_starting_at_or_before_them() {
    let l = layout_fixture(); // starts 4, 1204, 20511
    assert_eq!(section_at(&l, 1204).unwrap().key, "reading");
    assert_eq!(section_at(&l, 1203).unwrap().key, "front");
    assert_eq!(section_at(&l, 999_999).unwrap().key, "weather");
    assert!(section_at(&l, 0).is_none());
}
```

If the mini KFX's sections aren't three chapters, correct the counts from what `section_first_runs` really returns and ledger it; the assertions that matter are "first heading, positions increasing, matched by section, not by text anywhere".

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p deckle-kfx --test text_runs section_first && cargo test -p deckle-readership --test layout 2>&1 | tail -10`
Expected: compile errors.

- [ ] **Step 3: Implement**

`section_first_runs`: walk the reading order's sections (`$169` → section names), take each section's first content position from the position map, and return the first `content_runs` run at or after it that belongs to that section. `layout_from_kfx`: section 0 of `section_first_runs` is the front page; match each `(key, title)` to the **next** first-run whose trimmed text equals the title (in order, never reusing one). A title with no match is `Err("section \"<title>\" not found in the edition")`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p deckle-kfx && cargo test -p deckle-readership 2>&1 | tail -5`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/deckle-kfx crates/deckle-readership
git commit -m "feat(readership): record where each section of an edition starts"
```

---

### Task 4: `readership.json` — layouts and reads

**Files:**
- Create: `crates/deckle-readership/src/state.rs`
- Test: `crates/deckle-readership/tests/state.rs`

**Interfaces:**
- Consumes: `Layout` (Task 3), `parse_yjf`, `Positions` (Task 2).
- Produces: `pub enum OnDevice { Absent, Unopened, Opened(Vec<u8>) }` (what the device read returns per edition stem; defined here so `deckle-core` can stay free of this crate: Task 6 returns its own `deckle_core::yjf::OnDevice` with the same three variants, and the CLI maps one to the other).
- Produces: `pub struct Read { pub opened: bool, pub furthest: Option<i64>, pub last: Option<i64>, pub last_read: Option<String>, pub seen: String }`.
- Produces: `#[derive(Default, Serialize, Deserialize)] pub struct State { pub editions: BTreeMap<NaiveDate, Layout>, pub reads: BTreeMap<NaiveDate, BTreeMap<String, Read>>, pub dismissed: BTreeMap<String, NaiveDate> }` with
  - `pub fn load(path: &Path) -> State`, `pub fn save(&self, path: &Path) -> Result<(), String>`
  - `pub fn record_layout(&mut self, layout: Layout)` (replaces; drops that date's `reads` when the section starts differ; trims to 60 editions)
  - `pub fn stems(&self) -> Vec<String>` (the `file` of every edition, newest first)
  - `pub fn record_reads(&mut self, device: &str, found: &[(String, OnDevice)], now: &str) -> Recorded`
  - `pub struct Recorded { pub opened: usize, pub unopened: usize, pub errors: Vec<(String, String)> }`
  - `pub fn state_path() -> PathBuf`

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn missing_or_corrupt_file_starts_empty() { /* load of nothing, load of "{" */ }
#[test] fn save_then_load_round_trips() { /* one layout, one read */ }

#[test]
fn rebuilding_a_date_with_different_sections_drops_its_reads() {
    let mut s = State::default();
    s.record_layout(layout("2026-10-21", &[4, 1204]));
    s.record_reads("K", &[("deckle-daily-2026-10-21".into(), OnDevice::Opened(yjf(1300)))], NOW);
    s.record_layout(layout("2026-10-21", &[4, 1204]));      // same starts: reads stay
    assert!(s.reads.contains_key(&d("2026-10-21")));
    s.record_layout(layout("2026-10-21", &[4, 900]));       // different: reads go
    assert!(!s.reads.contains_key(&d("2026-10-21")));
}

#[test]
fn keeps_the_last_sixty_editions() { /* record 61 dates, oldest gone from editions and reads */ }

#[test]
fn absent_editions_record_nothing_and_unopened_record_unopened() {
    let r = s.record_reads("K", &[("a".into(), OnDevice::Absent), ("b".into(), OnDevice::Unopened)], NOW);
    assert_eq!((r.opened, r.unopened), (0, 1));
    assert!(s.reads[&date_of_a].get("K").is_none());
    assert!(!s.reads[&date_of_b]["K"].opened);
}

#[test]
fn one_bad_yjf_does_not_stop_the_others() {
    let r = s.record_reads("K", &[("a".into(), OnDevice::Opened(b"junk".to_vec())), ("b".into(), OnDevice::Opened(yjf(50)))], NOW);
    assert_eq!(r.opened, 1);
    assert_eq!(r.errors.len(), 1);
    assert_eq!(r.errors[0].0, "a");
}
```

`yjf(n)` builds a minimal valid `.yjf` with `fpr` and `lpr` at position `n` (a test helper next to the Task 2 synthetic builder; share it from `tests/common/mod.rs`).

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p deckle-readership --test state 2>&1 | tail -10`
Expected: compile errors.

- [ ] **Step 3: Implement `state.rs`**

Stems map to dates through `editions[*].file`. A device's entry for a date is replaced by the new read. `state_path()` = `deckle_library::config::default_path().with_file_name("readership.json")` (add the `deckle-library` dependency).

- [ ] **Step 4: Run the tests**

Run: `cargo test -p deckle-readership 2>&1 | tail -5`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/deckle-readership
git commit -m "feat(readership): readership.json with edition layouts and reads per Kindle"
```

---

### Task 5: The report, suggestions, apply and dismiss

**Files:**
- Create: `crates/deckle-readership/src/report.rs`
- Test: `crates/deckle-readership/tests/report.rs`

**Interfaces:**
- Consumes: `State`, `Layout`, `section_at`; `deckle_digest::Config`, `config::section_key`.
- Produces (all `Serialize`, `camelCase` for the app):
  - `pub struct Report { pub window: usize, pub opened: usize, pub median_percent: Option<u8>, pub stop_section: Option<String>, pub sections: Vec<SectionRow>, pub suggestions: Vec<Suggestion> }`
  - `pub struct SectionRow { pub key: String, pub name: String, pub share: Option<u8>, pub seen_in: usize }` (`share` is `None` until the section appeared in 5 editions)
  - `pub struct Suggestion { pub id: String, pub kind: SuggestionKind, pub key: String, pub title: String, pub why: String }`, `pub enum SuggestionKind { Drop, MoveUp, Stop }` (`id` = `"drop:<key>"`, `"up:<key>"`, `"stop"`)
  - `pub fn report(state: &State, cfg: &deckle_digest::Config) -> Report`
  - `pub fn apply(s: &Suggestion, cfg: &mut deckle_digest::Config) -> Result<(), String>` (`Stop` → `Err("nothing to apply")`)
  - `pub fn dismiss(state: &mut State, s: &Suggestion)` (hidden until 14 newer editions have a layout)

Copy, exactly (`N` reached, `M` opened-with-the-section, `P` percent):
- Drop: title `Drop <Name>`, why `You got to it on N of M days. It's about P% of the paper.` (P = the section's median share of `end`)
- Move up: title `Move <Name> up`, why `You got to it on N of M days, about P% in. Move it ahead of <Previous name>?`
- Stop: title `Where you stop`, why `You usually stop about P% in, in <Section name>.`

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn opened_count_and_median_percent() { /* 14 layouts, 10 opened at 40..60% → opened 10, median 50 */ }
#[test] fn sections_follow_the_current_config_order_and_drop_removed_ones() {}
#[test] fn a_new_section_has_no_share_until_five_editions() {}
#[test] fn share_counts_only_editions_that_had_the_section() {}

#[test]
fn two_devices_take_the_furthest() {
    // same date: K1 furthest 30% , K2 furthest 80% → counted at 80%, opened once
}

#[test]
fn fewer_than_seven_opened_means_no_suggestions() {}

#[test]
fn a_rarely_reached_section_early_in_the_paper_is_not_dropped() {
    // "local" reached 1 of 10 but starts at 20% → no Drop
}

#[test]
fn a_late_rarely_reached_section_gets_drop_with_the_exact_copy() {
    let s = &report(&state, &cfg).suggestions[0];
    assert_eq!(s.id, "drop:lottery");
    assert_eq!(s.why, "You got to it on 2 of 14 days. It's about 6% of the paper.");
}

#[test]
fn a_feed_that_meets_both_rules_gets_move_up_only() {}
#[test] fn front_and_reading_never_get_a_suggestion() {}
#[test] fn stop_shows_below_75_percent() {}
#[test] fn at_most_three_rule_order_then_lowest_share() {}

#[test]
fn apply_drop_and_move_up_edit_the_config() {
    // Drop lottery → removed from sections; Drop feeds:HN → gone from feeds;
    // Move up feeds:HN (not first) → swaps with previous feed;
    // Move up first feed → `feeds` moves one place earlier in sections
}

#[test]
fn dismissed_suggestions_return_after_fourteen_editions() {}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p deckle-readership --test report 2>&1 | tail -10`
Expected: compile errors.

- [ ] **Step 3: Implement `report.rs`**

Window = the 14 newest dates in `editions`. Per date, the reading is the max `furthest` over devices with `opened`. "Reached" = `furthest >= start`. Medians use the lower middle for even counts. A section's position in the paper is its median `start/end` over the window.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p deckle-readership 2>&1 | tail -5`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/deckle-readership
git commit -m "feat(readership): the report and its Drop / Move up suggestions"
```

---

### Task 6: Read each edition's `.yjf` over any transport, first in the sync session

**Files:**
- Create: `crates/deckle-core/src/yjf.rs`
- Modify: `crates/deckle-core/src/lib.rs` (module), `crates/deckle-core/src/sync.rs` (`Actions`, `Plan`, `Watcher`, `run`, `run_with`), `crates/deckle-core/src/queue.rs` (`FlushReport`), `crates/deckle-core/src/fake_mtp.rs` (if nested `put_bytes` needs it)
- Test: unit tests in `yjf.rs` and `sync.rs`

**Interfaces:**
- Produces: `pub enum OnDevice { Absent, Unopened, Opened(Vec<u8>) }` and `pub fn read_on(conn: &mut Conn<'_>, stems: &[String]) -> Result<Vec<(String, OnDevice)>, DeliverError>` in `deckle_core::yjf`. `Absent` = no `documents/<stem>.kfx`; `Unopened` = the KFX is there but no `<stem>.sdr/` or no `*.yjf` in it; `Opened` = the bytes of the one `*.yjf` (ignore `*.yjr`, `*.bad_file`).
- Produces: `Actions.readership: bool`, `Plan.readership: bool`; `run_with(queue, conn, plan, prune, finishing, progress, stems: &[String])` and `run(queue, conn, plan, prune, stems: &[String])`.
- Produces: `FlushReport.positions: Vec<(String, OnDevice)>` (`#[serde(skip)]`) and `FlushReport.positions_error: Option<String>`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn reads_yjf_from_a_folder_and_mass_storage() { /* documents/x.kfx + x.sdr/x9d6e.yjf → Opened */ }
#[test] fn an_edition_not_on_the_kindle_is_absent_not_unopened() { /* no x.kfx → Absent even if x.sdr exists */ }
#[test] fn a_kfx_without_a_sidecar_is_unopened() {}
#[test] fn only_the_yjf_is_taken() { /* .yjr, .yjr.bad_file, .yjf all present → the .yjf bytes */ }
#[test] fn reads_over_mtp_in_the_same_session() { /* fake_mtp: one OpenSession for read + push */ }

#[test]
fn positions_are_read_before_prune_removes_the_edition() {
    // fake_mtp with deckle-daily-2026-10-19.kfx (+ .sdr/.yjf) and keep = 1, plan push+prune+readership
    // → report.positions has Opened for the 19th, and the 19th is gone afterwards
}

#[test] fn readership_rides_along_with_a_push_once_per_connection() {}
#[test] fn readership_runs_alone_on_an_idle_plug_in() { /* IDLE_SESSION_OK */ }
#[test] fn a_failed_yjf_read_keeps_the_delivery() { /* fail_get_object → positions_error, delivered non-empty */ }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p deckle-core yjf readership positions_are_read 2>&1 | tail -10`
Expected: compile errors.

- [ ] **Step 3: Implement**

`yjf::read_on` mirrors `clippings::read_on` (folder = `documents/`, mass storage = `<root>/documents`, MTP = `find_documents` + `find_child` + list the `.sdr` + `get_object`). In the watcher, `readership` rides along exactly like `clippings` (its own `read` flag per connection; alone on an idle plug-in when `IDLE_SESSION_OK`). In `run_with`, read positions **first**, then push/prune, then clippings; an error goes to `positions_error`.

- [ ] **Step 4: Run the crate's tests**

Run: `cargo test -p deckle-core 2>&1 | tail -5`
Expected: all pass (update the existing `Plan { … }` literals with `readership: false`).

- [ ] **Step 5: Commit**

```bash
git add crates/deckle-core
git commit -m "feat(sync): read each edition's reading position first in the plug-in session"
```

---

### Task 7: CLI — record layouts and reads, `deckle digest readership`

**Files:**
- Modify: `crates/deckle-cli/Cargo.toml` (dep `deckle-readership`), `crates/deckle-cli/src/cli.rs` (`DigestCommand::Readership`), `crates/deckle-cli/src/lib.rs` (`digest run`, `push_queued`, `sync_actions`, `sync_to`, `DigestOutcome`, `describe`)
- Test: `crates/deckle-cli/tests/readership.rs`

**Interfaces:**
- Consumes: Tasks 3–6.
- Produces: `DigestCommand::Readership { action: Option<ReadershipCommand> }`, `enum ReadershipCommand { Apply { n: usize }, Dismiss { n: usize } }` (`n` is 1-based, as printed).
- Produces: `DigestOutcome.readership: Option<deckle_readership::Recorded>`; `describe` prints `      readership: N editions read` (and each error as `      readership: <stem>: <error>`; `positions_error` as `      readership: <error>`).

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn readership_with_no_state_says_how_to_start() {
    // stdout: "No editions recorded yet. Build one with `deckle digest run --to device`."
}
#[test] fn readership_prints_the_report_and_json() { /* seeded readership.json + digest.toml */ }
#[test] fn apply_drops_the_section_from_digest_toml_keeping_comments() {}
#[test] fn apply_out_of_range_exits_2() {}
```

And a unit test in `crates/deckle-cli/src/lib.rs` (push only ever targets a real Kindle, so this is tested at the plan level):

```rust
#[test]
fn push_with_an_empty_outbox_still_reads_positions() {
    let on = deckle_core::sync::Actions { push: true, prune: true, clippings: false, readership: true };
    assert_eq!(push_plan(on, true).map(|p| (p.push, p.readership)), Some((false, true)));
    assert_eq!(push_plan(Actions { readership: false, ..on }, true), None);
    assert_eq!(push_plan(on, false).map(|p| p.push), Some(true));
}
```

`fn push_plan(actions: Actions, outbox_empty: bool) -> Option<Plan>` decides what `push_queued` runs; `None` keeps today's "nothing queued" answer without touching the Kindle.

Use the HOME / XDG_CONFIG_HOME / APPDATA temp-dir pattern from `crates/deckle-cli/tests/highlights.rs`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p deckle-cli --test readership 2>&1 | tail -10`
Expected: compile errors.

- [ ] **Step 3: Implement**

- `digest run`: after the KFX is built (the `kfx` path), call `layout_from_kfx` with the KFX bytes, `date`, `built.title`, the edition's stem (`edition_filename(date)` without `.kfx`), and `built.sections` keys/titles; `record_layout` + save. A layout failure prints `readership: <error>` and never fails the run.
- `sync_actions` passes `readership`; `push_queued` sets `plan.readership` and no longer returns early on an empty outbox when a Kindle is connected and readership is on; `sync_to` passes `state.stems()` and afterwards maps `flush.positions` into `record_reads(device_id, …, now)` and saves.
- Text output of `digest readership`: `Opened N of W editions · usually about P% in`, then one line per section `  <Name padded>  <share>%` (or `not enough editions yet`), then numbered suggestions `  1. <title>: <why>`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p deckle-cli 2>&1 | tail -5`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/deckle-cli Cargo.lock
git commit -m "feat(cli): deckle digest readership, and record layouts and reading positions"
```

---

### Task 8: Daily tab panel and the Settings checkbox

**Files:**
- Modify: `apps/desktop/src-tauri/Cargo.toml`, `apps/desktop/src-tauri/src/daily.rs` (commands), `apps/desktop/src-tauri/src/lib.rs` (register), `apps/desktop/src-tauri/src/kindle.rs` (`shelf_on` records reads in its session)
- Create: `apps/desktop/src/Readership.tsx`, `apps/desktop/src/readershipView.ts`, `apps/desktop/src/readershipView.test.ts`
- Modify: `apps/desktop/src/DailyView.tsx`, `apps/desktop/src/api.ts`, `apps/desktop/src/types.ts`, `apps/desktop/src/settings/Kindles.tsx`, `apps/desktop/src/styles.css`

**Interfaces:**
- Consumes: `deckle_readership::{report, apply, dismiss, State}`, `deckle_core::yjf::read_on`.
- Produces Tauri commands: `readership_report() -> Report`, `readership_apply(id: String) -> Result<Report, String>`, `readership_dismiss(id: String) -> Result<Report, String>`, `readership_set(on: bool) -> Result<bool, String>` (writes `[sync] readership` through `config::save`).
- Produces: `readershipView.ts`: `barTone(share: number | null): "strong" | "muted" | "none"` (muted below 30) and `headline(r: Report): string` (`"Opened 12 of 14"`).

- [ ] **Step 1: Write the failing tests**

```ts
test("bars under 30% are muted, missing shares have none", () => {
  expect(barTone(64)).toBe("strong"); expect(barTone(21)).toBe("muted"); expect(barTone(null)).toBe("none");
});
test("headline", () => { expect(headline(report({ opened: 12, window: 14 }))).toBe("Opened 12 of 14"); });
```

And in `daily.rs`: `#[test] fn apply_by_id_writes_digest_toml()` with a temp config.

- [ ] **Step 2: Run them to see them fail**

Run: `cd apps/desktop && pnpm vitest run readershipView 2>&1 | tail -5; cargo test -p deckle-desktop apply_by_id 2>&1 | tail -5`
Expected: FAIL (module / function missing).

- [ ] **Step 3: Implement**

Panel follows board 2 in the app's existing styles: heading **How far you read**, the line "Each time the Kindle connects, Deckle notes how far you got in each edition, from the Kindle's own reading position. It stays on this Mac.", the two numbers (`Opened N of W`, `About P% in`), section bars, then **Suggested edits** with **Drop it** / **Move up** / **Dismiss**. Empty state (fewer than 3 opened): "Plug your Kindle in after reading and this fills in." Off: "Readership is off." with a link to Settings → Kindles. Settings → Kindles gets the checkbox *Note how far I read the paper*.

- [ ] **Step 4: Run the tests and build**

Run: `cd apps/desktop && pnpm vitest run && pnpm build 2>&1 | tail -3; cargo test -p deckle-desktop 2>&1 | tail -3`
Expected: all pass; build succeeds.

- [ ] **Step 5: Check it visually**

With the mocked Tauri backend in the browser pane (as for highlights): a seeded report with 14 editions, one muted bar and two suggestions; the empty state; the off state. Screenshots before claiming done.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop
git commit -m "feat(desktop): How far you read on the Daily tab, and its setting"
```

---

### Task 9: Docs and the real-device pass

**Files:**
- Modify: `docs/daily.md`, `README.md`, `CHANGELOG.md`, `docs/devices.md`

- [ ] **Step 1: Docs**

`docs/daily.md`: a "How far you read" section (what's read, when, that it stays local, `[sync] readership`, the "got to or past" caveat for contents jumps, email-only editions not tracked). README: the `deckle digest readership` lines. CHANGELOG Unreleased: one entry. `docs/devices.md`: where the `.yjf` is and what it holds (Scribe, verified 2026-10-22).

- [ ] **Step 2: Scribe**

Build and deliver an edition (`deckle digest run --to device`), read partway, go Home, replug, run `~/.cache/cargo-target/deckle/debug/deckle digest readership`. Expected: the edition counted as opened, the stop section matching where reading stopped.

- [ ] **Step 3: Paperwhite**

Same on the Paperwhite (mass storage). Record in `docs/devices.md` whether its editions get a `.yjf` at `documents/<stem>.sdr/`; if not, say so and leave the feature reporting "not opened" correctly there (a follow-up issue, not a fix in this branch).

- [ ] **Step 4: Full suite**

Run: `cargo test --workspace 2>&1 | tail -3 && cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -3 && cd apps/desktop && pnpm vitest run && pnpm build`
Expected: all green.

- [ ] **Step 5: Commit**

```bash
git add docs README.md CHANGELOG.md
git commit -m "docs: readership report"
```
