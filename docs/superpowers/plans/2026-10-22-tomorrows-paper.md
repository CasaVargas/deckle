# Tomorrow's Paper, So Far Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the Daily tab around the next edition: a press clock with Stop the presses, a
live front page with reason lines, an editable page plan with a page estimate, and this week's
reads, with CLI parity.

**Architecture:**
- **Three new modules in `crates/deckle-digest`:**
  - `press`: the next press, the stop state in `press.json`, and the countdown copy;
  - `front`: the lead rule, shared by the cover and the preview;
  - `next`: the feed-headline cache with its 30-minute throttle, and `plan`, which composes the
    whole view from plain inputs.
- **`deckle-readership`** turns `readership.json` into the inputs `next` needs: section sizes
  and the week. It already depends on `deckle-digest`.
- **The CLI and the desktop both call `next::plan`.** The desktop gets three Tauri commands and
  three new components. Config edits keep going through `daily_save`.

**Tech Stack:**
- Rust 2024 workspace: `chrono` and `chrono-tz` (through `deckle_digest`), `serde_json`,
  `feed-rs` (already used by `sources::feeds`).
- Tauri 2, React 19/TS, vitest. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-10-22-tomorrows-paper-design.md`

## Global Constraints

- **Nothing new in `digest.toml`.**
  - `press.json` sits beside `digest.toml`, via `Config::press_state()` with a
    `#[serde(skip)] press_path` override.
  - `next.json` sits in `deckle_core::wordwise::default_cache_dir().join("digest")`.
  - Both are written through a temp file and a rename. A missing or bad file reads as empty.
- **Edition date:** the local date of the next occurrence of the schedule's `at`. Number:
  `date.ordinal()`.
- **Feed fetches:** RSS/Atom of `[[feeds]].url`, the `[local] google_news` search
  (`google_news_url`) and `[local] feeds` only. At most once per URL per 30 minutes
  (`REFRESH_EVERY`), failures included. Sequential. Never article pages or photos.
- **The view is recomputed** on tab open, on focus, every 60 s while visible, and after any save
  or run. Refresh runs on open and focus only.
- **The lead rule** (`front::lead`): Local first; else the first section in paper order with a
  story among Your Reading and the feeds; within the section, the first story with an opening
  paragraph, else the first.
- **Budgets:**
  - editable: `[reading] max_items`, `[local] max_items`, `[[feeds]] max_items`, from 1 to 20;
  - fixed labels: "1 page" (weather, lottery, On This Day), "{n} signs" (horoscope);
  - the last row, when fixed, reads "back page".
- **The estimate:** `POSITIONS_PER_PAGE = 1800.0`, using 14 layouts. Defaults in pages:
  - front 2, weather 2, lottery 1, On This Day 2;
  - Your Reading 3 per article;
  - a story 1.5 per item with `full_text`, else 0.3;
  - horoscope 0.25 per sign.
- **Exact copy** (spec §4–§7):
  - the press bar:
    - "Goes to press at {HH:MM} · in {h} h {m} m", or "in {m} m" under an hour;
    - "Presses stopped: no paper {Weekday} · next at {HH:MM} {Weekday}";
    - "No press time set";
  - buttons: **Send now**, **Preview**, **Stop the presses**, **Restart the presses**;
  - card titles: **Tomorrow's paper, so far** with the badge "Live", **Page plan** with "Drag to
    reorder · about {n} Kindle pages", **This week** with **All editions**;
  - "Weather · fills in at press time", "{Section} · {n} slots", "{Section} · fills at press
    time", and "Headlines checked {HH:MM}" or "Headlines not checked yet";
  - the run message: "Presses stopped for {Weekday, Month D}: no paper today (deckle digest stop
    --undo restarts them)".
  - The reason lines are in spec §5.3, verbatim.
- `digest run` checks the stop **before** locating Kindle Previewer or touching anything.
- The pre-push hook runs `cargo fmt --check` and clippy `-D warnings`. Keep both clean.

**Plan rulings** (left open by the spec; flagged to the owner):
- **`digest stop --date YYYY-MM-DD`** sets a stop for any date. Without `--date` it uses the next
  scheduled edition. This makes the CLI testable without a LaunchAgent.
- **"Sent" in the week** is inferred from `deckle-daily-YYYY-MM-DD.epub` in the digest cache
  folder (the files `digest run` writes). A day with neither a layout nor a file is "no paper".
- **Stale cache entries:** `next.json` drops URLs no longer in the config on each save.

## Review Focus

1. **A press time inside a spring-forward gap** (02:30 on 2026-03-08, America/New_York). The
   next press is the first valid instant after the gap. No panic and no skipped day. Test in
   Task 1.
2. **Stop pressed after today's press time has passed** (06:00 with the press at 05:30). It
   stops *tomorrow's* paper, not today's, which already ran. Test in Task 1.
3. **A Mac asleep through press time.** launchd runs the job at 09:00 on wake; the stop is by
   date, so it still skips. Test in Task 2.
4. **A feed renamed or removed after it was fetched.** The cache is keyed by URL: a renamed
   feed keeps its headlines, and a removed one vanishes and is pruned. Test in Task 5.
5. **A saved page with no title and no URL** (an inbox `.html` file). The tile shows the file
   name and the kicker "YOUR READING" with no host. Test in Task 6.

---

### Task 1: The press clock and the stop state

**Files:**
- Create: `crates/deckle-digest/src/press.rs` (unit tests inside). Register `pub mod press;` in
  `lib.rs`.
- Modify: `crates/deckle-digest/src/config.rs`. Add `#[serde(skip)] pub press_path:
  Option<PathBuf>` and `Config::press_state() -> PathBuf` (default
  `default_path().with_file_name("press.json")`), next to `almanac_cache`.

**Interfaces:**
- Produces:
  - `pub fn next_press(at: (u8, u8), now: DateTime<Tz>) -> DateTime<Tz>`. That's today's `at`
    if `now` is before it, else tomorrow's. A nonexistent local time resolves to the first valid
    instant after it; an ambiguous one to the earlier.
  - `#[derive(Clone, Debug, PartialEq, Serialize)] pub struct Press`, fields:
    - `at: Option<String>`;
    - `next: Option<DateTime<FixedOffset>>`, the press that will print;
    - `edition: NaiveDate`, the date of the paper the preview shows;
    - `stopped: Option<NaiveDate>`;
    - `since: DateTime<FixedOffset>`, the last press instant, or local midnight with no
      schedule.
  - `pub fn press(at: Option<(u8, u8)>, stopped: Option<NaiveDate>, now: DateTime<Tz>) ->
    Press`:
    - When the next press's date equals `stopped`, `next` and `edition` move one day on.
    - With `at = None`: `edition = now.date_naive()` and `next = None`.
  - `pub fn countdown(from: DateTime<Utc>, to: DateTime<Utc>) -> String`: "in 14 h 22 m", "in 38
    m", or "now" at ≤ 0. Minutes round down.
  - `pub fn read_stop(path: &Path, today: NaiveDate) -> Option<NaiveDate>`. It ignores stops
    before `today`.
  - `pub fn write_stop(path: &Path, stopped: Option<NaiveDate>) -> Result<(), String>`. `None`
    removes the file.
  - The file format is `{"stopped": "YYYY-MM-DD"}`.

- [ ] **Step 1: Write failing tests:**
  - `next_press_before_and_after`: at 15:08 on 2026-10-22 with (5, 30), the result is
    2026-10-23 05:30. At 04:00 it's 2026-10-22 05:30.
  - `countdown_copy`: 14 h 22 m 59 s gives "in 14 h 22 m"; 38 m gives "in 38 m"; −1 s gives
    "now".
  - `clock_change_is_absolute`: Europe/London, 2026-10-24 23:00 to the next 05:30 gives "in 7 h
    30 m".
  - `spring_gap` (Review Focus 1): America/New_York 2026-03-07 23:00 with (2, 30) gives the
    first valid instant on 2026-03-08, 03:00 EDT, so the instant equals
    `2026-03-08T07:00:00Z`.
  - `stop_after_todays_press_stops_tomorrow` (Review Focus 2): at 06:00 on the 22nd with (5,
    30), `press(..).next.date_naive()` is the 23rd. That's the date `digest stop` must write.
  - `stopped_edition_moves_on`: with `stopped = 2026-10-23`, `edition` is the 24th and `next` is
    2026-10-24 05:30.
  - `stop_file_round_trip_and_expiry`: write the 23rd, then read it with today = 22 →
    `Some(23)`, and with today = 24 → `None`. `write_stop(None)` removes the file. A garbage
    file reads `None`.
  - `no_schedule`: `press(None, None, now)` has `edition == today`, `next == None`, and `since`
    at local midnight.

- [ ] **Step 2: Run** `cargo test -p deckle-digest --lib press`. Expected: FAIL to compile.

- [ ] **Step 3: Implement.** Use `Tz::from_local_datetime(..)`. For `LocalResult::None`, step
  forward minute by minute until it resolves (at most 120 steps).

- [ ] **Step 4: Run** the same command. Expected: PASS.

- [ ] **Step 5: Commit** `feat(digest): the press clock and the stop state`.

### Task 2: Stop the presses in the CLI and on Send now

**Files:**
- Modify: `crates/deckle-cli/src/cli.rs`:
  - `DigestCommand::Stop { #[arg(long)] undo: bool, #[arg(long)] date: Option<String> }`;
  - `#[arg(long)] force: bool` on `DigestCommand::Run`.
- Modify: `crates/deckle-cli/src/lib.rs`:
  - In `Run`, right after `parse_date`: when `!force` and `press::read_stop(&cfg.press_state(),
    today) == Some(date)`, emit the run message (JSON: `{"stopped": "<date>"}`) and return
    `ExitCode::SUCCESS`.
  - `Stop`:
    - The date is `--date`, or `press(schedule_at(), None, now).next`'s date.
    - With no schedule and no `--date`, it errors with "nothing is scheduled; there's no press
      to stop".
    - `--undo` writes `None`.
- Modify: `apps/desktop/src-tauri/src/daily.rs`. `run_args(true, _)` appends `--force`.
- Test: create `crates/deckle-cli/tests/press.rs` (harness copied from `tests/readership.rs`:
  temp `HOME` and `XDG_CONFIG_HOME`). Update `runs_use_the_cli_with_json_and_read_its_report`
  in `daily.rs`.

**Interfaces:**
- Consumes: `press::{read_stop, write_stop, press}`, `Config::press_state`,
  `schedule::{status, parse_at}`.
- Produces: `fn schedule_at() -> Option<(u8, u8)>` in the CLI. It's `status().at` through
  `parse_at`. Task 7 reuses it.

- [ ] **Step 1: Write failing tests:**
  - `stop_then_run_builds_nothing`:
    - `digest stop --date 2026-10-23`, then `digest run --date 2026-10-23 --to <tmp>` with no
      Previewer on `PATH`;
    - it exits 0, stdout contains "Presses stopped for Friday, October 23", and `<tmp>` is
      empty;
    - `reading.jsonl` is byte-identical before and after.
  - `force_runs_anyway`: the same, with `--force --previewer <tmp>/missing`. It gets past the
    stop and fails on the missing Previewer (exit 1, the error names Kindle Previewer), which
    proves the stop didn't apply. (Pass the missing path so a Previewer installed on the Mac
    never starts a real build.)
  - `asleep_through_press` (Review Focus 3): a stop for today's date, then `digest run` with no
    `--date`. It's skipped.
  - `undo_clears`: `stop --date …` then `stop --undo`, and `press.json` is gone.
  - `stop_without_schedule_errors`: `digest stop` exits 1 with the message (the CLI's
    `EXIT_FAILURE` for an `Err`).
  - In `daily.rs`: `run_args(true, …)` ends with `"--force"`, and `run_args(false, …)` doesn't
    contain it.

- [ ] **Step 2: Run** `cargo test -p deckle-cli --test press` and `cargo test -p deckle-desktop
  daily`. Expected: FAIL.

- [ ] **Step 3: Implement** as in Files. Text output for `stop`: "Presses stopped for {long
  date}." or "Presses restarted."

- [ ] **Step 4: Run** `cargo test --workspace`. Expected: PASS.

- [ ] **Step 5: Commit** `feat(digest): stop the presses for one morning`.

### Task 3: One lead rule for the press and the preview

**Files:**
- Create: `crates/deckle-digest/src/front.rs` (unit tests inside). Register it in `lib.rs`.
- Modify: `crates/deckle-digest/src/cover.rs`. The lead comes from `front::lead`. The saved
  grid spots skip the lead.

**Interfaces:**
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Tier { Local, Reading, Feed }`.
  - `pub struct Candidate { pub tier: Tier, pub section: usize, pub article: usize, pub teaser:
    bool }`.
  - `pub fn lead(c: &[Candidate]) -> Option<usize>`. `c` is in paper order (all articles of each
    section, in order); it returns an index into `c`.
  - `pub fn candidates(g: &Gathered) -> Vec<Candidate>`, built from `g.news` (Local by
    `Section.key == "local"`, the rest Feed) and `g.reading`.

- [ ] **Step 1: Write failing tests** (`front.rs`):
  - `local_beats_saved`: [Reading, Local(no teaser), Local(teaser)] gives the Local with a
    teaser.
  - `local_without_teaser_still_leads`.
  - `no_local_plan_order_decides`: [Reading, Feed] gives Reading; [Feed, Reading] gives Feed.
  - `teaser_preferred_within_section`: [Feed s1 a0 (no teaser), Feed s1 a1 (teaser), Feed s2
    a0 (teaser)] gives s1 a1.
  - `empty_is_none`.
  - `cover_lead_matches_rule`: a fixture `Gathered` with Your Reading first and Local empty.
    The cover's chosen lead (expose `cover::lead_of(g) -> Option<(usize, usize)>` for the test)
    is the saved article. With one Local story it's that story.

- [ ] **Step 2: Run** `cargo test -p deckle-digest --lib front cover`. Expected: FAIL.

- [ ] **Step 3: Implement.** Replace the `heads.iter().position(..)` lead in `cover::render`
  with `lead_of`. When the lead is a saved article, the remaining `heads` all go to the grid and
  the saved spots skip it.

- [ ] **Step 4: Run** `cargo test -p deckle-digest`. Expected: PASS. Note the intended change
  (saved articles can now lead) in the commit body.

- [ ] **Step 5: Commit** `feat(digest): Local leads, then the paper's order`.

### Task 4: Section sizes and the week from Readership

**Files:**
- Modify: `crates/deckle-readership/src/layout.rs`. `SectionStart` gains
  `#[serde(default, skip_serializing_if = "Option::is_none")] pub items: Option<u32>`.
  `layout_from_kfx` sets `None`.
- Modify: `crates/deckle-cli/src/lib.rs` (`record_layout`). After `layout_from_kfx`, set each
  `items` from `built.sections[i].items`, matched by key. The front page stays `None`.
- Create: `crates/deckle-digest/src/next/mod.rs` with the shared types below, and
  `pub mod next;` in `lib.rs`.
- Create: `crates/deckle-readership/src/sizes.rs`, exported from `lib.rs`.
- Test: `crates/deckle-readership/tests/sizes.rs`.

**Interfaces:**
- Defines, in `deckle_digest::next` (Tasks 6–8 use them):
  - `#[derive(Clone, Debug, Default, PartialEq, Serialize)] pub struct History { pub per_item:
    BTreeMap<String, f64>, pub whole: BTreeMap<String, f64> }`, in KFX positions;
  - `#[derive(Clone, Debug, PartialEq, Serialize)] #[serde(tag = "state", content = "percent",
    rename_all = "camelCase")] pub enum WeekState { Read(u8), NotOpened, Sent, NoPaper }`;
  - `pub struct WeekDay { pub date: NaiveDate, pub state: WeekState }`.
- Produces:
  - `pub fn history(state: &State) -> History`, over the newest 14 layouts:
    - `whole[key]` is the median section length;
    - `per_item[key]` is the median of length / items, where `items > 0`.
  - `pub fn recent(state: &State, before: NaiveDate, days: usize, built: &dyn Fn(NaiveDate) ->
    bool) -> Vec<WeekDay>`, oldest first:
    - `Read(percent)`, with the furthest taken as the max over devices;
    - `NotOpened`;
    - `Sent` when `built(date)`;
    - otherwise `NoPaper`.

- [ ] **Step 1: Write failing tests:**
  - `lengths_from_starts`: a layout with starts 0, 1000 and 4000 and end 10000 gives lengths
    1000, 3000 and 6000 (front, a, b).
  - `per_item_median`: three layouts where `local` is 3000/3 items, 4000/2 and 900/3 give
    `per_item["local"] == 1000.0` (the median of 1000, 2000 and 300). `whole["local"] ==
    3000.0`.
  - `items_none_skips_per_item`.
  - `recent_states`: over six dates, a read at 9000/10000 on two devices (6000 and 9000) gives
    `Read(90)`; a layout with no read gives `NotOpened`; `built` true with no layout gives
    `Sent`; the rest `NoPaper`. Dates are oldest first and exactly `before − 6 … before − 1`.
  - `old_json_still_loads`: a `readership.json` without `items` loads, with `items == None`.
  - In `deckle-cli/tests/readership.rs`, extend an existing layout test or add
    `layout_records_items`, asserting the saved layout's `items` for a feed equal the build
    report's count. If no CLI test reaches `record_layout` without Previewer, unit-test the
    item-matching helper `fn with_items(layout, &BuildReport) -> Layout` instead.

- [ ] **Step 2: Run** `cargo test -p deckle-readership --test sizes`. Expected: FAIL.

- [ ] **Step 3: Implement.** Lengths come from consecutive starts in each layout's own order.
  Use the existing lower-middle median.

- [ ] **Step 4: Run** `cargo test --workspace`. Expected: PASS.

- [ ] **Step 5: Commit** `feat(readership): section sizes and the week for the next paper`.

### Task 5: Live headlines, fetched politely

**Files:**
- Create: `crates/deckle-digest/src/next/live.rs`, declared in `next/mod.rs`.
- Test: `crates/deckle-digest/tests/next.rs`, with fixtures in
  `tests/fixtures/next/{science.xml,gnews.xml}`.

**Interfaces:**
- Consumes: `sources::feeds::{parse, as_briefs, google_news_url}`, `Net`.
- Produces:
  - `pub const REFRESH_EVERY_MIN: i64 = 30`.
  - `#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)] pub struct LiveCache {
    pub feeds: BTreeMap<String, Fetched> }`, keyed by URL.
  - `pub struct Fetched { pub at: DateTime<Utc>, pub title: Option<String>, pub items:
    Vec<LiveItem>, pub error: Option<String> }`.
  - `pub struct LiveItem { pub title: String, pub source: Option<String>, pub published:
    Option<DateTime<Utc>> }`.
  - `LiveCache::load(path: &Path) -> LiveCache` and `save(&self, path: &Path) -> Result<(),
    String>`.
  - `pub fn live_urls(cfg: &Config) -> Vec<String>`, covering the Google News URL (when set),
    `[local] feeds`, and the `[[feeds]]` URLs, in that order.
  - `pub fn refresh(cfg: &Config, cache: &mut LiveCache, now: DateTime<Utc>, net: &dyn Net) ->
    usize`. It returns the requests made, and prunes URLs not in `live_urls`.
  - `pub fn checked(cache: &LiveCache) -> Option<DateTime<Utc>>`: the newest `at`.

- [ ] **Step 1: Write failing tests:**
  - `throttles_per_url`:
    - a counting closure `Net` serves the fixtures;
    - refreshes at t = 0, t + 10 min and t + 31 min make 2, 0 and 2 requests for two feeds.
  - `failure_waits_too`: an erroring URL is fetched once at t = 0, not at t + 10 min, and again
    at t + 31 min. `Fetched.error` is set and `items` is empty.
  - `only_feed_urls`: the `Net` panics on any URL not in `live_urls(cfg)`. A config with weather,
    lottery and On This Day on still refreshes cleanly.
  - `briefs_and_age_window`: Google News items are split by `as_briefs` (source set), and items
    older than `max_age_hours` (`local.max_age_hours` for Local) are dropped.
  - `renamed_feed_keeps_headlines_removed_is_pruned` (Review Focus 4): rename a feed (same
    URL) → no new request within 30 min, and its items are kept. Remove it → the next `refresh`
    drops its URL.
  - `cache_round_trip`, and a garbage file loads empty.

- [ ] **Step 2: Run** `cargo test -p deckle-digest --test next live`. Expected: FAIL.

- [ ] **Step 3: Implement.** Keep the feed's channel title from `parse`'s first value. Fetches
  are sequential.

- [ ] **Step 4: Run.** Expected: PASS.

- [ ] **Step 5: Commit** `feat(digest): live headlines for the next paper, at most twice an
  hour`.

### Task 6: Composing the next paper

**Files:**
- Create: `crates/deckle-digest/src/next/plan.rs`, re-exported from `next/mod.rs`.
- Test: `crates/deckle-digest/tests/next.rs`.

**Interfaces:**
- Consumes: Task 1 (`Press`, `press`), Task 3 (`front::{lead, Candidate, Tier}`), Task 4's
  types, Task 5 (`LiveCache`), `reading::Entry`, and `tz::clock`.
- Produces:
  - `pub struct Inputs<'a>`, fields:
    - `cfg: &'a Config`, `now: DateTime<Tz>`, `at: Option<(u8, u8)>`, `stopped:
      Option<NaiveDate>`;
    - `queue: &'a [Entry]`, the whole queue;
    - `live: &'a LiveCache`, `history: &'a History`;
    - `shares: &'a BTreeMap<String, u8>`, readership shares by key;
    - `week: Vec<WeekDay>`.
  - `#[derive(Serialize)] #[serde(rename_all = "camelCase")] pub struct NextPaper`, fields:
    - `press: Press`, `paper: String`, `place: String`, `date: NaiveDate`, `number: u32`;
    - `weather: bool`;
    - `lead: Option<Tile>`, `tiles: Vec<Tile>`;
    - `checked: Option<DateTime<Utc>>`;
    - `plan: Vec<PlanRow>`, `off: Vec<PlanRow>`;
    - `pages: u32`;
    - `week: Vec<WeekDay>`.
  - `pub struct Tile { pub key: String, pub kicker: String, pub title: Option<String>, pub
    reason: Option<String>, pub dashed: bool }`.
  - `pub struct PlanRow { pub key: String, pub kind: SectionKind, pub feed: Option<usize>, pub
    name: String, pub detail: String, pub budget: String, pub items: Option<usize> }`. `items`
    is the editable value and is `None` for fixed rows; `feed` is the `[[feeds]]` index.
  - `pub const POSITIONS_PER_PAGE: f64 = 1800.0`.
  - `pub fn plan(i: &Inputs) -> NextPaper`.
  - `pub fn saved_line(saved: DateTime<Tz>, now: DateTime<Tz>, units: Units) -> String`.
  - `pub fn estimate(rows: &[PlanRow], planned: &[usize], history: &History, cfg: &Config) ->
    u32`.

- [ ] **Step 1: Write failing tests**, using a fixture config with `title = "The Morning Deckle"`,
  `place = "Linden Falls, NC"`, sections `reading, weather, local, feeds, horoscope, onthisday`,
  and one feed "Science". `now` is 2026-10-22 15:08 America/New_York and `at` is (5, 30).
  - `masthead_is_the_next_edition`: `date == 2026-10-23` and `number == 296`.
  - `saved_article_leads_without_local`:
    - two pending entries saved at 09:14 and 11:02 today, and Local not fetched;
    - the lead's kicker is "YOUR READING · THEATLANTIC.COM";
    - its reason is "Saved at 9:14 this morning. Leads the paper because nothing local outranks
      it yet.";
    - the tiles in order are: the second saved article ("Saved at 11:02 this morning."), "Local
      · 12 slots" (dashed), "Science · 10 slots" (dashed), "Horoscope · fills at press time",
      "On This Day · fills at press time".
  - `local_leads_when_fetched`: one fresh Local item → the lead's reason is "Leads the paper:
    local news always leads.", and the saved articles become tiles.
  - `feed_leads_line`: no saved articles and no Local → "Leads the paper: nothing local yet, and
    Science comes first in your page plan."
  - `new_since_last_press`: Science items at 05:00, 06:10 and 14:00 today (with `since` =
    05:30) give the kicker "SCIENCE · 2 NEW SINCE 05:30" and the title of the 14:00 item.
  - `saved_line_table`: 11:59 "this morning"; 12:00 "this afternoon"; 17:00 "this evening";
    yesterday "Saved yesterday at 9:14."; 6 days back "Saved Friday."; 7 days back "Saved Oct
    15."; metric gives "09:14".
  - `untitled_saved_page` (Review Focus 5): an entry with `title: None`, `url: None` and `file:
    Some("reading/x/page.html")` gives the tile title "page.html" and the kicker "YOUR READING".
  - `plan_rows_and_budgets`:
    - Your Reading with 3 pending and max 10 → detail "3 saved today", budget "all", `items:
      Some(10)`;
    - with 12 pending → "10 of 12 fit; the rest wait", budget "10";
    - Local → "3 stories" with `items: Some(3)` when `max_items = 3`;
    - Science (`feed: Some(0)`) → detail from the cached channel title, else the URL host;
    - Weather → "Linden Falls, NC · 5-day box and almanac", plus " · Your Day" with a calendar;
    - the last fixed row's budget is "back page";
    - `off` lists Lottery.
  - `readership_note_at_threshold`: share 20 → the detail ends with " · you get to it about 20%
    of days"; share 21 → no note.
  - `estimate_defaults_and_history`:
    - With an empty `History`, 3 pending and nothing fetched, the fixture estimates (2 + 3×3 +
      2 + 1.5×12 + 1.5×10 + 3 + 2) pages, rounded. Compute the expectation in the test from the constants, not a literal.
    - With `History { per_item: {"local": 900.0}, whole: {"front": 3600.0} }`, Local
      contributes 900 × planned / 1800 and the front 2.
    - Fetched Science with 2 fresh items and `max_items = 10` plans 2.
  - `stopped_shows_the_paper_after`: `stopped = 2026-10-23` gives `date == 2026-10-24`.

- [ ] **Step 2: Run** `cargo test -p deckle-digest --test next plan`. Expected: FAIL.

- [ ] **Step 3: Implement.**
  - Pending entries come from the queue the way `for_edition` filters them (`Pending`, or
    `printed_in == Some(date)`), oldest first. Those beyond `max_items` only count toward the
    detail line.
  - Candidates for `front::lead`: every fitting saved article (`Tier::Reading`), fetched Local
    items (`Tier::Local`), and fetched feed items (`Tier::Feed`), in plan order, all with
    `teaser: true`.
  - Fresh items are counted against `press.since`.

- [ ] **Step 4: Run** `cargo test -p deckle-digest`. Expected: PASS.

- [ ] **Step 5: Commit** `feat(digest): compose tomorrow's paper from what's known`.

### Task 7: `deckle digest next`

**Files:**
- Modify: `crates/deckle-cli/src/{cli.rs,lib.rs}`. Add `DigestCommand::Next { #[arg(long)]
  fetch: bool }`, using the global `--json`.
- Test: `crates/deckle-cli/tests/press.rs`.

**Interfaces:**
- Consumes: `next::{plan, Inputs, LiveCache, refresh}`, `deckle_readership::{history, recent,
  report}`, Task 2's `schedule_at`, `reading_queue(&cfg).load()`.
- Produces: `fn next_inputs(cfg, now) -> (LiveCache, History, BTreeMap<String, u8>,
  Vec<WeekDay>)` in the CLI. The shares come from `report(..).sections` (`share` where `Some`).

- [ ] **Step 1: Write failing tests:**
  - `next_json_has_the_view`:
    - a temp config and a queue with one pending entry;
    - `digest --json next` exits 0, and its JSON has `.lead.kicker` starting with "YOUR
      READING", `.plan` as an array whose first `key` is `"reading"`, and `.pages` ≥ 1.
  - `next_text_lines`: the text contains "about", "Kindle pages", and "Weather · fills in at
    press time".
  - `next_without_fetch_makes_no_request`: with `next.json` absent and no network (an
    unroutable feed URL), it finishes quickly and shows "{Section} · {n} slots".

- [ ] **Step 2: Run** `cargo test -p deckle-cli --test press next`. Expected: FAIL.

- [ ] **Step 3: Implement.**
  - **Text layout:** the press line, then "{paper} · No. {n} · {long date}", then "Lead: {kicker}
    — {title}" with the reason indented, then one line per tile.
  - **The plan:** "Page plan (about {n} Kindle pages):", with numbered rows "{name} — {detail}
    [{budget}]".
  - **The week:** "This week: Sat read 90% · …".
  - `--fetch` calls `refresh` with `Http` and saves `next.json`.

- [ ] **Step 4: Run** `cargo test --workspace`. Expected: PASS.

- [ ] **Step 5: Commit** `feat(cli): digest next shows tomorrow's paper`.

### Task 8: Tauri commands

**Files:**
- Create: `apps/desktop/src-tauri/src/next.rs`, with `_at` functions for tests as in
  `readership.rs`. Register the commands in `lib.rs`.

**Interfaces:**
- Produces:
  - `pub fn next_at(digest: &Path, readership: &Path, cache: &Path, now: DateTime<Tz>, at:
    Option<(u8, u8)>) -> Result<NextPaper, String>`.
  - `pub fn stop_at(digest: &Path, on: bool, at: Option<(u8, u8)>, now: DateTime<Tz>) ->
    Result<(), String>`, which `press_stop` wraps.
  - `#[tauri::command] pub fn daily_next() -> Result<NextPaper, String>`.
  - `#[tauri::command] pub async fn daily_next_refresh() -> Result<NextPaper, String>`. It runs
    `refresh` on a blocking thread, saves, then returns `daily_next()`.
  - `#[tauri::command] pub fn press_stop(on: bool) -> Result<NextPaper, String>`. With `on`, it
    writes the next press's date (no schedule → `Err("nothing is scheduled; there's no press to
    stop")`). Without, it clears the stop.

- [ ] **Step 1: Write failing tests:**
  - `next_at_reads_files_only`: temp files, `at = Some((5, 30))` → `Ok`, `date` is tomorrow, and
    no network type is touched (`next_at` takes no `Net`).
  - `press_stop_round_trip`: through `stop_at(digest, on, at, now)` → `press.stopped` is set,
    then cleared.
  - `press_stop_without_schedule_errors`.

- [ ] **Step 2: Run** `cargo test -p deckle-desktop next`. Expected: FAIL.

- [ ] **Step 3: Implement.** Paths:
  - `Config::press_state()` beside the digest path;
  - `deckle_readership::state_path()`;
  - `digest_cache().join("next.json")`.

  "Sent" is inferred from `digest_cache()` holding `deckle-daily-{date}.epub`.

- [ ] **Step 4: Run** `cargo test --workspace`. Expected: PASS.

- [ ] **Step 5: Commit** `feat(desktop): commands for tomorrow's paper`.

### Task 9: Desktop types and pure helpers

**Files:**
- Modify: `apps/desktop/src/types.ts`. Add `NextPaper`, `Tile`, `PlanRow`, `Press`, `WeekDay`,
  mirroring Task 6's serde output (camelCase).
- Modify: `apps/desktop/src/api.ts`. Add `dailyNext`, `dailyNextRefresh`, `pressStop(on)`.
- Create: `apps/desktop/src/pagePlan.ts` and `apps/desktop/src/pagePlan.test.ts`.

**Interfaces:**
- Produces:
  - `export function countdown(nextIso: string, nowMs: number): string`, the same copy as
    `press::countdown`.
  - `export function pressLine(p: Press, nowMs: number): string`, giving the three bar states.
  - `export function reorder(cfg: DailyConfig, rows: PlanRow[], from: number, to: number):
    DailyConfig`, implementing spec §6.2.
  - `export function setBudget(cfg: DailyConfig, row: PlanRow, n: number): DailyConfig`.
    `n` is clamped to 1–20 and lands on `reading.max_items`, `local.max_items` or
    `feeds[row.feed].max_items`.
  - `export function weekLabel(d: WeekDay): string`: "read 90%", "not opened", "sent", "no
    paper".

- [ ] **Step 1: Write failing vitest cases:**
  - `countdown` gives "in 14 h 22 m", "in 38 m" and "now".
  - `pressLine` stopped gives "Presses stopped: no paper Friday · next at 05:30 Saturday".
  - Reorder, with sections `[reading, weather, local, feeds, horoscope]` and feeds `[A, B]`
    (rows: reading, weather, local, A, B, horoscope):
    - Moving horoscope to index 1 gives sections `[reading, horoscope, weather, local, feeds]`.
    - Moving weather onto the row of B (inside the block) snaps it after the block: sections
      `[reading, local, feeds, weather, horoscope]`.
    - Moving B to index 3 gives feeds `[B, A]` with sections unchanged.
    - Moving A to index 0 (outside) snaps it to the block's start, so feeds are unchanged.
    - Two feeds with the same name are both kept.
  - `setBudget` writes the right key, and clamps 0 to 1 and 25 to 20.

- [ ] **Step 2: Run** `pnpm --dir apps/desktop test`. Expected: FAIL.

- [ ] **Step 3: Implement.**

- [ ] **Step 4: Run** `pnpm --dir apps/desktop test` and `pnpm --dir apps/desktop build`.
  Expected: PASS.

- [ ] **Step 5: Commit** `feat(desktop): page plan edits and press copy`.

### Task 10: The Daily tab rebuilt, docs

**Files:**
- Create: `apps/desktop/src/PressBar.tsx`, `apps/desktop/src/TomorrowsPaper.tsx`,
  `apps/desktop/src/PagePlan.tsx`, `apps/desktop/src/ThisWeek.tsx`.
- Modify: `apps/desktop/src/DailyView.tsx`:
  - Remove the paper card, the **Every morning** card and the "Sections, in order" list.
  - Render, in order: `PressBar`; a `.daily-columns` wrapper holding `TomorrowsPaper` and
    `PagePlan` + `ThisWeek`; `Readership`; Your Reading; Email to Kindle; The paper (without
    the section list); `YourDay`.
  - Keep `run`, `result` and the schedule state, now passed to `PressBar` and `TomorrowsPaper`.
- Modify: `apps/desktop/src/Readership.tsx`. Add an `onApplied` prop, called after apply, which
  reloads the config and the next paper.
- Modify: `apps/desktop/src/styles.css`:
  - the masthead, the tiles (dashed for `tile.dashed`), and the plan rows with a drag handle;
  - two columns over 900 px, stacked under.
- Modify: `docs/daily.md` (a "Tomorrow's paper" section: live, page plan, Stop the presses),
  `README.md` (the `digest next` and `digest stop` lines), and `CHANGELOG.md`.

**Interfaces:**
- Consumes: Task 8's commands, Task 9's helpers, and the existing `api.dailyRun`,
  `scheduleSet`, `dailySave` and `editionPreview`.
- Produces: the card markup has `id="tomorrows-paper"`. `ThisWeek` renders **All editions**
  only when `document.getElementById("every-edition")` exists, and clicking it calls
  `scrollIntoView`.

- [ ] **Step 1: Wire the data flow:**
  - `DailyView` holds `next: NextPaper | null`.
  - It loads with `api.dailyNext()` on mount, on `focus`, every 60 s while
    `document.visibilityState === "visible"`, and after save, reading add/remove and
    `daily-finished`.
  - It calls `api.dailyNextRefresh()` on mount and on `focus`.
  - Page plan edits apply `reorder`/`setBudget` to `draft` and call `save(c)` at once (as
    `YourDay` does), then reload `next`.

- [ ] **Step 2: Build the components:**
  - `PressBar`: `pressLine` with a 60 s tick, the time input (`scheduleSet`), the switch, and
    **Restart the presses** (`pressStop(false)`) when stopped.
  - `TomorrowsPaper`, from top to bottom:
    - the masthead and the "Weather · fills in at press time" line;
    - the lead, with its kicker, title and reason;
    - the tiles grid, then the explainer line;
    - **Send now**, **Preview**, and **Stop the presses** (only when `press.next` is set);
    - the run result block moved from the old card;
    - "Last built {date} · Show file" from `editionPreview`;
    - "Live" with "Headlines checked {HH:MM}".
  - `PagePlan`: rows with `draggable`, `onDragStart`/`onDrop` calling `reorder`, the up/down
    buttons, a switch, a `<select>` 1–20 for rows with `items`, and the fixed label otherwise.
    Off rows sit below, muted. The head reads "Drag to reorder · about {pages} Kindle pages".
  - `ThisWeek`: six cells of `weekday short` and `weekLabel`, plus the All editions link.

- [ ] **Step 3: Run** `pnpm --dir apps/desktop test` and `pnpm --dir apps/desktop build`, then
  `cargo test --workspace`. Expected: PASS.

- [ ] **Step 4: Verify visually** in the running app (`pnpm --dir apps/desktop tauri dev`), with
  a real `digest.toml`:
  - save a link and see it drop in as the lead within a second;
  - drag a feed and a section, and check that `digest.toml` keeps its comments;
  - change a budget, and check that the estimate and Local's slot count change;
  - stop and restart the presses (and check `deckle digest next` agrees);
  - narrow the window below 900 px and check the columns stack;
  - check the "Live" timestamp moves after a focus once 30 minutes have passed.

  Screenshot the tab for the PR.

- [ ] **Step 5: Write the docs** (`docs/daily.md`, `README.md`, `CHANGELOG.md`), then commit
  `feat(desktop): the Daily tab shows tomorrow's paper, so far`.

- [ ] **Step 6: Tune the estimate with the owner.** Send one edition to the Paperwhite over USB
  and count its page turns at the default font. If it's more than 15% off the estimate for that
  edition's sizes, adjust `POSITIONS_PER_PAGE`, update the Global Constraints value, and commit
  `fix(digest): page estimate tuned on the Paperwhite`.
