# Interests, increment 2: Learning Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The paper notices what you read: stories you jumped back from, text you highlighted in the paper, and authors on your shelf become candidates; a recurring one becomes a "Deckle noticed" suggestion on the Daily tab and in the CLI, accepted or dismissed in one click; and renaming an interest keeps its history.

**Architecture:** `deckle-readership` grows: the `.yjf` parser returns the page-turn history, layouts record where each story starts, and a new `learn` module keeps `interests.json` (candidates, hits, dismissed, accepted) and turns it into suggestions. The connection pass (CLI `watch`, desktop Kindle tab) feeds it from the readership state, the clippings and the shelf store. The desktop's Readership card and the CLI's `digest interests` show and act on suggestions.

**Tech Stack:** Rust 2024 (`deckle-readership`, `deckle-digest`, `deckle-core` sync, `deckle-cli`, `deckle-desktop`), Tauri 2 + React/TypeScript + vitest.

**Spec:** `docs/superpowers/specs/2026-10-25-interests-design.md` §5 (learning), §5.4 (suggestions, rename), §5.5 (CLI), §8, §10, §11 item 2, with the §5.2 amendment in Global Constraints below.

## Global Constraints

- **The history records page starts, not elements** (checked against `fixtures/yjf/daily-2026-10-21.yjf`: records at 821, 4 and 1015, all on the front page, `fpr` 103303 deep in the book; `daily-2026-10-20.yjf`: one record at 4). So a **hit** is the story whose range holds a history position: `story.start <= pos < next story's start` within the position's section. No link ranges. A position before a section's first story (its front) is no hit. Spec §5.2 is amended to this in Task 7.
- All learning is local, per paper, in `interests.json` beside that paper's `readership.json` (`cfg.state_file("interests.json")`, else beside `deckle_readership::state_path()`). Nothing is sent anywhere.
- Signals (spec §5.1): taps (history), paper highlights (clippings entries with author `Deckle` and an edition title), shelf authors (books in progress). Not `fpr`, not book highlights, not look-ups.
- Phrase extraction (spec §5.3): runs of capitalised words, allowing `of the and de for` inside; single capitalised words of 4+ letters; key = lowercase; display = most common spelling. Stoplist: weekday and month names, the paper's title and place words, section titles, every interest's `name`/`also`/`not`, dismissed keys (90 days), and `Breaking Opinion Live Watch Photos Video Update Report News Today`.
- A suggestion needs `min_stories` distinct `(date, title)` hits across at least 2 dates within `window_days`; top 5 by hit count then latest date. Shelf candidates: one hit per edition date the author's book was in progress; "You've been reading <author> for <n> days."
- Accept writes `[[interests]] name = <display>` (shelf: `query = "\"<author>\" author OR book"`), runs `ensure_section(Interests, Feeds)`, records `accepted[key] = today`. Dismiss records `dismissed[key] = today` (90 days). Nothing changes the paper without a click.
- Clippings are read on a connection when `[learning] enabled` and the paper has readership on for that Kindle (`learning_on(cfg, id)`), in addition to today's reasons. Taps need `[sync] readership`; highlights need the clippings read.
- Rename (spec call 11): the desktop save migrates `interests:<old>` → `interests:<new>` in `readership.json` (layout section keys, dismissed ids) and `interests.json` in the same save; the CLI has no rename.
- Existing files keep loading: every new struct field has `#[serde(default)]`.
- Tests offline, no personal data. `cargo fmt --check`, `cargo clippy --workspace -D warnings`, `cargo test` on touched crates (the full suite on CI), `pnpm build && pnpm test`. Commit per task on `claude/daily-interests-learning`; never push to `main`.

## Review Focus

- A `.yjf` whose `page.history.store` is empty or absent, or holds a position past the edition's end: no hit, no panic. (Task 1, Task 3)
- Two Kindles read the same edition: hits are per `(date, title)`, so the same story tapped on both counts once. (Task 3)
- A candidate that is already an interest, an `also`, a `not`, or was dismissed last week: never suggested. (Task 3)
- A clippings file with highlights from books and from two editions of two papers: only this paper's edition highlights count. (Task 3)
- Renaming an interest to a name that differs only in case: keys migrate, no duplicate error. (Task 6)

---

### Task 1: Page history in `Positions` and `Read`

**Files:**
- Modify: `crates/deckle-readership/src/krds.rs` (`Positions.history`, `parse_yjf`)
- Modify: `crates/deckle-readership/src/state.rs` (`Read.history`, `record_reads`)
- Test: `crates/deckle-readership/tests/krds.rs`, `crates/deckle-readership/tests/state.rs`

**Interfaces:**
- Produces: `Positions { …, pub history: Vec<i64> }` in file order; `Read { …, #[serde(default)] pub history: Vec<i64> }` filled by `record_reads`.

- [ ] **Step 1: Failing tests.** In `tests/krds.rs`:
```rust
#[test]
fn page_history_lists_the_jumps_origins_in_order() {
    assert_eq!(parse_yjf(&fixture("daily-2026-10-21.yjf")).unwrap().history, [821, 4, 1015]);
    assert_eq!(parse_yjf(&fixture("daily-2026-10-20.yjf")).unwrap().history, [4]);
    assert!(parse_yjf(&yjf(5)).unwrap().history.is_empty());
}
```
In `tests/state.rs` (use the file's edition/record helpers): record a `.yjf` built with `synthetic(&[obj("fpr", …), obj("page.history.store", &[int(1), obj("page.history.record", &[utf("AQ:821"), long(1)])])])` and assert `state.reads[&date]["K"].history == [821]`; then `serde_json` round-trip a `Read` without the field (`{"opened":true,"seen":"x"}`) → `history` empty.
- [ ] **Step 2: Run** `cargo test -p deckle-readership --test krds --test state` → FAIL (no field `history`).
- [ ] **Step 3: Implement.** In `parse_yjf`: arm `("page.history.store", values)` → for each `Value::Object(name, vs)` with `name == "page.history.record"` and `vs[0]` a `Str(pos)`, push `position_id(pos)`. `Read.history` set from `p.history` in `record_reads`.
- [ ] **Step 4: Run** the same → PASS.
- [ ] **Step 5: Commit** `feat(readership): page-turn history from the .yjf, kept per read`.

---

### Task 2: Story starts in the layout

**Files:**
- Modify: `crates/deckle-readership/src/layout.rs`
- Modify: `crates/deckle-digest/src/lib.rs` (`SectionSummary.titles: Vec<String>`, filled in `build_gathered` where summaries are made)
- Modify: `crates/deckle-cli/src/lib.rs:4010-4054` (`record_layout` adds stories)
- Test: `crates/deckle-readership/tests/layout.rs` (new)

**Interfaces:**
```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoryStart { pub title: String, pub start: i64 }
pub struct SectionStart { …, #[serde(default, skip_serializing_if = "Vec::is_empty")] pub stories: Vec<StoryStart> }
/// Each section's stories from the runs inside its range: the first run equal to each title (whitespace-collapsed), in order; a title not found is skipped.
pub fn stories_from_runs(runs: &[deckle_kfx::text::Run], layout: &Layout, titles: &[(String, Vec<String>)]) -> Vec<(String, Vec<StoryStart>)>;   // (section key, stories)
pub fn add_stories(layout: &mut Layout, kfx: &[u8], titles: &[(String, Vec<String>)]) -> Result<(), String>;  // content_runs + stories_from_runs, written into layout.sections
/// The section and story a position falls in: the section as `section_at`, the story the last one starting at or before `pos`; None before the first story.
pub fn story_at(layout: &Layout, pos: i64) -> Option<(&SectionStart, &StoryStart)>;
```
`SectionSummary.titles` are the section's article titles in order; `record_layout` passes `(key, titles)` for every section into `add_stories` after `layout_from_kfx` (a failure logs and keeps the layout without stories).

- [ ] **Step 1: Failing tests** in `tests/layout.rs`: build a `Layout` with sections front@0, `interests:Rust`@100, `feeds:HN`@500, end 1000, and runs `[(100,"Rust"),(110,"Rust 2.0 lands"),(300,"Rust jobs grow"),(500,"HN"),(520,"Rust 2.0 lands")]`; `stories_from_runs(…, &[("interests:Rust", ["Rust 2.0 lands","Rust jobs grow","Missing"]), ("feeds:HN", ["Rust 2.0 lands"])])` → Rust: starts 110 and 300 (Missing skipped), HN: 520 (the one inside its own range, not 110). `story_at` on the filled layout: 105 → None, 110 → Rust 2.0 lands, 299 → Rust 2.0 lands, 300 → Rust jobs grow, 700 → HN's story, 1000 → None. A layout JSON without `stories` loads with empty vectors.
- [ ] **Step 2: Run** `cargo test -p deckle-readership --test layout` → FAIL.
- [ ] **Step 3: Implement** the three functions and the summary titles (`SectionSummary { titles: s.articles.iter().map(|a| a.title.clone()).collect() }`), and the CLI call.
- [ ] **Step 4: Run** `cargo test -p deckle-readership -p deckle-digest --lib --tests` → PASS (the existing `build.rs` tests that construct `SectionSummary`, if any, get the new field).
- [ ] **Step 5: Commit** `feat(readership): where each story starts, recorded with the layout`.

---

### Task 3: `learn`: candidates, hits, suggestions

**Files:**
- Create: `crates/deckle-readership/src/learn.rs`; export in `lib.rs`
- Modify: `crates/deckle-readership/Cargo.toml` (add `deckle-highlights.workspace = true`)
- Test: `crates/deckle-readership/tests/learn.rs` (new)

**Interfaces:**
```rust
pub mod learn;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "lowercase")]
pub enum Signal { Tap, Highlight, Shelf }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hit { pub date: NaiveDate, pub section: String, pub title: String, pub signal: Signal }
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)] #[serde(default)]
pub struct Candidate { pub display: String, pub hits: Vec<Hit>, pub named: Option<Named> }   // Named { name, query } is for increment 3; parsed now
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)] #[serde(default)]
pub struct Learning { pub candidates: BTreeMap<String, Candidate>, pub dismissed: BTreeMap<String, NaiveDate>, pub accepted: BTreeMap<String, NaiveDate> }
impl Learning { pub fn load(path: &Path) -> Learning; pub fn save(&self, path: &Path) -> Result<(), String>; pub fn path_for(cfg: &Config) -> PathBuf; }
/// Candidate phrases in `text` (spec §5.3), as (key, display).
pub fn phrases(text: &str) -> Vec<(String, String)>;
/// Words never suggested for this paper: the built-in list, the paper's title and place, section titles, every interest's name/also/not, and the config's own keys.
pub fn stoplist(cfg: &Config) -> HashSet<String>;
/// Adds a hit under every phrase of `title` not on the stoplist; a hit already present (same date, section, title, signal) is not added twice.
pub fn note(learning: &mut Learning, stop: &HashSet<String>, text: &str, hit: Hit);
/// Taps: every history position of every read of every edition in `state`, resolved through the layout to a story. Returns hits added.
pub fn record_taps(learning: &mut Learning, state: &State, stop: &HashSet<String>) -> usize;
/// Paper highlights: clippings entries that are this paper's editions (author "Deckle", title "<paper> — <long date>"), the clipped text as the hit's title (first 80 chars). Returns hits added.
pub fn record_highlights(learning: &mut Learning, clippings: &[u8], paper_title: &str, stop: &HashSet<String>) -> usize;
/// Shelf authors: one Shelf hit per author of each in-progress book of every fresh snapshot, dated the snapshot's seen day.
pub fn record_shelf(learning: &mut Learning, shelf: &deckle_digest::shelf::store::Shelf, now: DateTime<Utc>, stop: &HashSet<String>) -> usize;
/// Drops hits older than `window_days` before `today`, dismissals older than 90 days, and candidates left with no hits.
pub fn prune(learning: &mut Learning, today: NaiveDate, window_days: u32);
#[derive(Clone, Debug, PartialEq, Serialize)] #[serde(rename_all = "camelCase")]
pub struct Noticed { pub key: String, pub name: String, pub why: String, pub stories: usize, pub days: u32, pub signal: Signal, pub query: Option<String> }
pub fn suggest(learning: &Learning, cfg: &Config, today: NaiveDate) -> Vec<Noticed>;
/// Writes the entry into `cfg` (name, query for shelf), ensures the section, records accepted. Err when the key is unknown.
pub fn accept(learning: &mut Learning, cfg: &mut Config, key: &str, today: NaiveDate) -> Result<InterestConfig, String>;
pub fn dismiss(learning: &mut Learning, key: &str, today: NaiveDate) -> Result<(), String>;
pub const DISMISSED_DAYS: u64 = 90;
```
Date parsing for highlights: the clippings title's long date (`Tuesday, October 20, 2026`) via `NaiveDate::parse_from_str(d, "%A, %B %-d, %Y")` (try `%d` too). `why` copy: Tap/Highlight: `You read <n> stories about <name> in the last <days> days.` where `<days>` is `window_days`; Shelf: `You've been reading <name> for <n> days.`

- [ ] **Step 1: Failing tests** in `tests/learn.rs` (one `cfg()` from a TOML with `title = "The Deckle Daily"`, `place = "Carthage, NC"`, one interest `Rust` with `not = ["Rust Belt"]`, `[learning] min_stories = 3, window_days = 14`):
  - `phrases_find_capitalised_runs_and_long_words`: `"Bryce Young benched as Carolina Panthers fall; Rust 2.0 lands in Moore County Schools"` → keys include `bryce young`, `carolina panthers`, `rust`, `moore county schools`; not `lands`, not `in`. `"The Trial of Alec Baldwin"` → `alec baldwin` and `trial` (4+ letters) but `the` and `of` only inside runs.
  - `stoplist_holds_config_and_builtins`: contains `rust`, `rust belt`, `deckle daily`, `carthage`, `breaking`, `monday`, `october`; not `panthers`.
  - `note_dedupes_the_same_hit`: two `note` calls with the same hit → one hit under `bryce young`.
  - `taps_resolve_through_the_layout`: a `State` with one edition whose `interests:Panthers` section has stories at 110 ("Bryce Young benched") and 300 ("Panthers trade talk"), reads `K` with history `[4, 150, 310]` and `K2` with history `[150]` → `record_taps` adds 2 hits (`bryce young` ← 150 once, `panthers trade talk` phrases ← 310); position 4 (front) adds none; a position beyond `end` adds none.
  - `highlights_count_only_this_papers_editions`: a `My Clippings.txt` fixture string with a book highlight, an entry for `The Deckle Daily — Tuesday, October 20, 2026` by `Deckle` with text "Bryce Young looked lost against the Bucs", and one for `The Kid Courier — Tuesday, October 20, 2026` → one Highlight hit dated 2026-10-20 under `bryce young` (and `bucs`); the Kid Courier's and the book's add nothing.
  - `shelf_authors_get_one_hit_per_snapshot_day`: a `Shelf` with one Kindle seen `2026-10-24…`, a book by `Brandon Sanderson` in progress (last_read within 14 days) → one Shelf hit dated 2026-10-24 under `brandon sanderson`; recording again the same day adds nothing.
  - `suggest_needs_three_stories_on_two_days_and_ranks`: hits for `bryce young` on 3 titles over 2 dates → suggested with `stories: 3`, why `You read 3 stories about Bryce Young in the last 14 days.`; `cargo` with 3 titles on 1 date → not; `rust` (an interest) → not; a dismissed key dated 10 days ago → not, dated 100 days ago → yes after `prune`; 7 qualifying candidates → 5 returned, ordered by stories then latest date.
  - `accept_writes_the_entry_and_dismiss_records`: `accept` → `cfg.interests` gains `Bryce Young` and `sections` has `interests` before `feeds`; a shelf candidate's entry has `query == "\"Brandon Sanderson\" author OR book"`; `accepted[key]` set; unknown key → Err; `dismiss` sets `dismissed[key]`.
  - `prune_drops_old_hits_and_empty_candidates`.
  - `learning_file_round_trips`: save/load in a tempdir; a missing file loads empty; `path_for` is beside `readership.json` for a paper (`papers/<slug>/interests.json`) and beside `state_path()` for the main paper.
- [ ] **Step 2: Run** `cargo test -p deckle-readership --test learn` → FAIL to compile.
- [ ] **Step 3: Implement** `learn.rs`. `phrases`: tokenize on whitespace, strip punctuation at token edges, a run = maximal sequence of tokens that start uppercase (joiners `of the and de for` allowed between two capitalised tokens); emit the run when it has 2+ capitalised tokens, else the single token when it has 4+ letters. `record_taps`: for each `(date, reads)` in `state.reads`, layout = `state.editions[date]`, for each read's history position → `story_at` → `note(…, &story.title, Hit{date, section.key, story.title, Tap})`. `record_highlights`: `deckle_highlights::parse` entries, `author == "Deckle"`, title `== format!("{paper_title} — {date}")` for a parseable date. `record_shelf`: `shelf.fresh(today, tz)`-style over `kindles` whose `seen` day is within the window, books with `last_read` within 14 days of now, each author.
- [ ] **Step 4: Run** the tests → PASS.
- [ ] **Step 5: Commit** `feat(readership): learning — candidates from taps, paper highlights and shelf authors; suggestions`.

---

### Task 4: Learning on the connection (CLI watch and the desktop)

**Files:**
- Modify: `crates/deckle-cli/src/lib.rs` (`sync_actions`, the connection pass after `record_reads` at ~3900; new `fn learn_on_connection(cfg, flush, state_path, states)`)
- Modify: `apps/desktop/src-tauri/src/kindle.rs:300-330` and `readership.rs` (`record_on` returns whether reads were recorded; a sibling `learn_on(conn_report…)`)
- Modify: `crates/deckle-readership/src/learn.rs` (one entry point for both callers)
- Test: `crates/deckle-cli/tests/learn.rs` (new, modelled on `tests/papers.rs` with a fake Kindle mount) 

**Interfaces:**
```rust
/// Everything one connection teaches a paper: taps from `state`, highlights from `clippings` (when read), shelf authors from `shelf`. Loads, updates, prunes and saves `interests.json`; returns hits added. Never fails the caller: a save error is returned as Err for logging.
pub fn learn::on_connection(cfg: &Config, state: &State, clippings: Option<&[u8]>, shelf: Option<&Shelf>, now: DateTime<Utc>) -> Result<usize, String>;
fn learning_on(cfg: &Config, id: &str) -> bool;   // cfg.learning.enabled && cfg.sync.for_device(id).readership   (CLI and desktop each have one; keep them identical)
```
`sync_actions`: `clippings: a.clippings || puzzle_on(cfg) || shelf || learning_on(cfg, id)`. The CLI pass calls `on_connection` for every paper in `states` after `record_reads` (and after `record_shelf`), passing `flush.clippings` and the shelf store loaded from `cfg.shelf_store()`. The desktop's Kindle session calls it after `record_on` with the same inputs.

- [ ] **Step 1: Failing test** `tests/learn.rs` in `deckle-cli`: a fake Kindle (as `papers.rs` builds one) holding `documents/deckle-daily-2026-10-20.sdr/deckle-daily-2026-10-20.yjf` = `fixtures/yjf/daily-2026-10-21.yjf`'s bytes and a `documents/My Clippings.txt` with one edition highlight; a `readership.json` prepared with a layout for that stem whose `interests:Panthers` section spans 0..200_000 with stories at 800 ("Bryce Young benched") and 1010 ("Panthers trade talk"); `[learning]` on, `[sync] readership = true`. Run `deckle digest watch --once` (or whatever single-pass flag `papers.rs` uses) and assert `interests.json` exists with hits under `bryce young` (Tap, from 821) and from the clipping (Highlight). With `[learning] enabled = false`, no `interests.json`.
- [ ] **Step 2: Run** `cargo test -p deckle-cli --test learn` → FAIL.
- [ ] **Step 3: Implement** `on_connection`, `learning_on`, the two call sites, the `sync_actions` clause.
- [ ] **Step 4: Run** `cargo test -p deckle-cli --test learn --test papers` and `cargo clippy -p deckle-desktop --all-targets -- -D warnings` → PASS.
- [ ] **Step 5: Commit** `feat: learn from each Kindle connection — taps, paper highlights, shelf authors`.

---

### Task 5: "Deckle noticed" in the app and the CLI; the learning switch

**Files:**
- Modify: `apps/desktop/src-tauri/src/readership.rs` (`ReadershipView.noticed: Vec<Noticed>`; commands `interests_accept(key, paper)`, `interests_dismiss(key, paper)` returning `ReadershipView`), registered in the Tauri builder
- Modify: `apps/desktop/src/types.ts` (`Noticed`, `ReadershipView.noticed`), `api.ts`, `Readership.tsx` (the list), `DailyView.tsx` (the `[learning] enabled` checkbox in The paper, next to the Interests card)
- Modify: `crates/deckle-cli/src/cli.rs` (`InterestsCommand::Accept { phrase }`, `Dismiss { phrase }`, `Why { name }`), `lib.rs` (`digest_interests`)
- Test: `apps/desktop/src/readershipView.test.ts` (a pure `noticedLine(n)` helper if copy is built in TS; otherwise none), `crates/deckle-cli/tests/interests.rs`

**Interfaces:**
- `ReadershipView { enabled, report, #[serde(default)] noticed: Vec<Noticed> }`, filled by `learn::suggest(&Learning::load(&Learning::path_for(&cfg)), &cfg, today)`.
- `interests_accept` → `learn::accept` + `papers::save` + `Learning.save` → `view_at`. `interests_dismiss` likewise.
- Readership.tsx: under the suggestions, `<h4>Deckle noticed</h4>` and one row per `noticed`: `<strong>{n.name}</strong>`, `n.why`, buttons **Add as interest** (`api.interestsAccept(n.key, paper)` then `onApplied`) and **Not interested**.
- DailyView: in The paper, under Your interests: `<label className="check"><input type="checkbox" checked={draft.learning?.enabled ?? true} …/> Deckle keeps track of what you tap and highlight in the paper, on this Mac, to suggest new interests.</label>`; unticking writes `learning = { …defaults, enabled: false }`.
- CLI: `digest interests` (list) prints pending suggestions after the interests as `noticed: Bryce Young — You read 3 stories … (accept with: deckle digest interests accept "Bryce Young")`; `accept "<phrase>"` matches `key` or `display` case-insensitively; `why "<name>"` prints the hits (date, section, title) behind a candidate or an accepted interest's key.

- [ ] **Step 1: Failing tests.** CLI (`tests/interests.rs`): seed `interests.json` with a qualifying `bryce young` candidate (3 hits, 2 dates, Tap) next to the main `digest.toml`; `--json digest interests` → `noticed[0].name == "Bryce Young"`; `interests accept "Bryce Young"` → config has the entry and `--json digest interests` shows it under `interests` and no longer under `noticed`; `interests dismiss "Cargo"` on an unknown key → failure "no suggestion"; `interests why "Bryce Young"` lists 3 lines. Desktop: `readershipView.test.ts` gets `noticedLabel(n)` → `"Add as interest"` for taps/highlights and `"Add Brandon Sanderson"`… keep it simple: no new TS test unless a helper is introduced.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement** all of the above.
- [ ] **Step 4: Run** `cargo test -p deckle-cli --test interests`, `pnpm -C apps/desktop build && pnpm -C apps/desktop test`, `cargo clippy -p deckle-desktop --all-targets -- -D warnings` → PASS. Check the card in the browser with the stubbed bridge (runbook in Tolaria `deckle/deckle-desktop-ui-check-without-tauri-window.md`) with a `readership_report` answer that includes `noticed`.
- [ ] **Step 5: Commit** `feat: Deckle noticed — suggestions on the Daily tab and in the CLI, the learning switch`.

---

### Task 6: Rename keeps the history

**Files:**
- Modify: `crates/deckle-readership/src/state.rs` (`State::rename_section_key(old: &str, new: &str) -> usize`), `learn.rs` (`Learning::rename(old_key, new_key)` for accepted)
- Modify: `apps/desktop/src-tauri/src/daily.rs` (`daily_save`: before saving, load the old config, pair old and new interests by index, and for each name change call both renames and save both files; `papers_save` likewise if the app saves papers through another command)
- Test: `crates/deckle-readership/tests/state.rs`, `apps/desktop/src-tauri/src/daily.rs` unit test

- [ ] **Step 1: Failing tests**: `rename_section_key` rewrites every layout's `interests:Rust` to `interests:Rust lang` and `dismissed` ids `drop:interests:Rust`/`up:interests:Rust`, returning the count; a desktop test calling the save path on a temp config with a renamed interest asserts the readership file's keys moved; renaming only by case (`rust` → `Rust`) saves without a duplicate error.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p deckle-readership -p deckle-desktop` → PASS.
- [ ] **Step 5: Commit** `feat(desktop): renaming an interest keeps its readership and learning history`.

---

### Task 7: Docs, spec notes, verification, PR

- [ ] **Step 1:** `docs/daily.md`: `### Deckle noticed` after `### How far you read` (what's a signal, what isn't, where it's stored, the switch, the CLI lines, the 90-day dismissal). Spec §14 gains "Implementation notes, PR 2": the page-start finding and the simplified hit rule replacing §5.2's link ranges; highlights match on the clipped text rather than story bodies; what moved to increment 3 (naming).
- [ ] **Step 2:** `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p deckle-readership -p deckle-digest -p deckle-cli -p deckle-desktop` and `pnpm -C apps/desktop build && pnpm -C apps/desktop test` → all pass. The full workspace suite runs on CI.
- [ ] **Step 3:** Open the issue "Interests, part 2: learning", push, `gh pr create` with `Closes #N`, bind the PR.
