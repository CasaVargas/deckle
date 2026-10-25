# One Mac, many papers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let one Mac print a separate Deckle Daily for each Kindle. Each paper has its own
name, reader, sections, time and days, delivery (one Kindle by USB, or one email address) and
large-print option, and is managed from **Your papers** on the Daily tab and through
`--paper <slug>`.

**Architecture:**
- **`deckle-digest`** gains `papers.rs`. It loads `digest.toml` (slug `main`) plus
  `papers/<slug>.toml`, rejects household keys in paper files, and copies those keys from the
  main paper.
- **`Config`** gains a `#[serde(skip)] paper: Option<PaperRef>`. It drives the edition stem,
  the state folder, the work folder and whether Your Reading prints.
- **`deckle-core`:**
  - prune and the outbox match `<prefix>YYYY-MM-DD.kfx` exactly;
  - targeted papers queue in `outbox/kindle/<folder>/`;
  - `sync::run_with` flushes several queues and runs several prunes over one connection.
- **`schedule.rs`** installs one job per paper, with days, and reconciles stale jobs.
- **The CLI** threads `--paper` through every command. Push and watch serve every paper.
- **The desktop** passes an optional paper slug to every Daily command and shows the cards.

**Tech Stack:**
- Rust 2024 workspace: `toml`/`toml_edit` (existing), `std::fs::File::lock` (MSRV 1.89),
  chrono.
- Tauri 2, React/TS, vitest.

**Spec:** `docs/superpowers/specs/2026-10-22-many-papers-design.md`

## Refresh (2026-10-25)

The spec was approved on 2026-10-25, with call 6 revised. This plan was refreshed against main
at #175 (Kindle profiles), after the Kobo, PocketBook, reMarkable, PDF edition, edition
bundle, puzzle and Morgue work landed.

- **Routing reuses #37.** No `outbox/kindle/<id>/` folders:
  - a bound paper is queued in the shared outbox with an owner, through a new
    `Queue::add_keeping_for`;
  - `kindle =` takes a name or a sync id, resolved with `SyncConfig::find_device` at queue time;
  - `Queue::for_kindle`, `pending_all` and `device_folder` are dropped from Task 4.
- **Other outputs are per paper.** Export folders, the edition bundle, the reMarkable PDF and
  the e-reader EPUB use the paper's stem and exact replace (Task 2 and Task 6). A bound paper
  skips them. `[export]` and `[remarkable]` are per paper and not inherited.
- **Per-paper state** covers readership, the puzzle store and the Morgue (Task 6).
- **Increments** (spec §11):
  - PR 1 is Tasks 1, 2, 4 and 6;
  - PR 2 is Task 5;
  - PR 3 is Task 3;
  - PR 4 is Task 7.

  Task 8's docs ride with each PR; its device checks run after PR 3 and PR 4.

## Global Constraints

- **Files:**
  - main paper: `default_path()` (`digest.toml`), slug `main`;
  - extra papers: `<dir of digest.toml>/papers/<slug>.toml`;
  - their state: `papers/<slug>/`.
- **Slugs:** `^[a-z][a-z0-9-]{0,31}$`, and `main` is reserved. Other names in `papers/` are
  skipped with a note: "papers/<file>: not a paper name (lowercase letters, digits, dashes)".
- **New keys:**
  - top level: `reader: String`, `kindle: String`, `large_print: bool`;
  - `[schedule] at: Option<String>` and `days: Vec<Day>` (`mon`…`sun`);
  - all are skipped on save at their defaults.
- **Household keys:** `[sync]`, `[reading]`, and `[email]`'s `from`, `smtp_host`, `smtp_port`,
  `username`. They're allowed only in `digest.toml`. In a paper file, the error is exactly
  "papers/<slug>.toml: <key> is set in digest.toml for every paper".
- **Stems:**
  - main `deckle-daily-YYYY-MM-DD`;
  - extra `deckle-daily-<slug>-YYYY-MM-DD`.
- **Matching:** prune and the outbox's replace match `^<prefix>\d{4}-\d{2}-\d{2}\.kfx$`.
- **Outbox:** a bound paper goes to the shared outbox with an owner (`<entry>.for`, Kindle
  profiles design §2), resolved from `kindle` (a name or a sync id) when it's queued.
- **Work folders:** main `<cache>/digest`, extra `<cache>/digest/papers/<slug>`. The run lock is
  `<cache>/digest/run.lock`.
- **Jobs:**
  - LaunchAgent: main `com.casavargas.deckle.digest`, extra `com.casavargas.deckle.digest.<slug>`;
  - Windows: `\Deckle\Digest` and `\Deckle\Digest-<slug>`;
  - each runs `digest [--config C] run --paper <slug> --to device`.
- **The main paper with no `[schedule] at`:** its installed job is never touched.
- **Email:**
  - the main paper's subject and attachment are unchanged ("Deckle Daily — <date>",
    `deckle-daily-<date>.epub`);
  - an extra paper's are "<title> — <date>" and `<stem>.epub`.
- **Large print:** an extra stylesheet block. No `font-size` below 1em. Regular papers'
  XHTML is byte-identical to today.
- **Exact copy:**
  - "for <reader>", "Any Kindle", "USB at 05:30", "USB, Saturdays at 07:00", "Email at
    06:00", "Not scheduled", "Regular", "Large print";
  - "Saved articles go to <main title>.";
  - "<title> goes to whichever Kindle connects first.";
  - "Set the text size on the Kindle itself (Aa). It applies to every book on that Kindle.";
  - the buttons **Edit**, **Preview**, **New paper**, and the card title **Your papers**.
- **Workspace:** `rust-version` goes from 1.85 to 1.89 (`File::lock`).
- The pre-push hook runs fmt and clippy `-D warnings`. Keep them clean.

**Plan rulings** (open in the spec; flagged to the owner):
- **Watch reloads papers every check,** so a new paper's prune and routing apply without a
  restart. A file that fails to load keeps the last good set and prints the error once.
- **Highlights from all papers** stay in the one "Deckle Daily" export book (`EDITIONS_KEY`),
  under date headings.

## Review Focus

1. **A paper bound to a Kindle that connects as mass storage with no serial**
   (`Kindle Paperwhite/Kindle`). `same_kindle` matches it to any Kindle of that model and
   volume name (the #37 limitation). The picker shows the Kindle's name. Test in Task 7.
2. **A paper file renamed by hand** (`Kid Courier.toml`). It's skipped with the note, and is
   never half-loaded or scheduled. Test in Task 1.
3. **A `days` paper built by hand on another day** (`run --paper kid-courier` on Tuesday). It
   builds; `days` only gates the scheduled job. Test in Task 6.
4. **An extra paper's undelivered edition, then a main paper's new edition.** The main paper's
   replace must not drop the extra paper's queued file. This matters for papers on "Any
   Kindle", which share the outbox. Test in Task 4.
5. **Deleting a paper's file while its edition waits for its Kindle.** The owned entry is still
   delivered at that Kindle's next connection (ownership lives on the entry, not the paper),
   and it's then pruned by no one. Test in Task 6.

---

### Task 1: Paper config and the loader (PR 1)

**Files:**
- Create: `crates/deckle-digest/src/papers.rs` (exported from `lib.rs`).
- Modify: `crates/deckle-digest/src/config.rs`:
  - add the new keys, `ScheduleConfig`, `Day` and `PaperRef`;
  - `EmailConfig` keeps its fields;
  - the commented new keys go in `STARTER`, and `PAPER_STARTER` is added.
- Test: `crates/deckle-digest/tests/papers.rs`, and unit tests in `config.rs`.

**Interfaces:**
- Produces:
  - In `config.rs`:
    - `pub reader: String`, `pub kindle: String`, `pub large_print: bool`;
    - `pub schedule: ScheduleConfig { at: Option<String>, days: Vec<Day> }`;
    - `#[serde(rename_all = "lowercase")] pub enum Day { Mon, Tue, Wed, Thu, Fri, Sat, Sun }`;
    - `#[serde(skip)] pub paper: Option<PaperRef>`, with `pub struct PaperRef { pub slug:
      String, pub state_dir: PathBuf }` (`None` = the main paper);
    - `ScheduleConfig::validate(&self) -> Result<(), String>`, which checks `at` with
      `schedule::parse_at`;
    - `pub const PAPER_STARTER: &str`, with `{title}`, `{reader}`, `{place}`, `{latitude}`
      and `{longitude}` placeholders;
    - `Config::slug(&self) -> &str` (`"main"` when `paper` is `None`).
  - In `papers.rs`:
    - `pub const MAIN: &str = "main"`;
    - `pub fn valid_slug(s: &str) -> bool`;
    - `pub fn slug_from_title(title: &str) -> String` ("The Kid Courier" → `kid-courier`,
      dropping a leading "the-");
    - `pub fn papers_dir(main: &Path) -> PathBuf`;
    - `pub struct Loaded { pub papers: Vec<Config>, pub notes: Vec<String> }`;
    - `pub fn load_all(main: &Path) -> Result<Loaded, String>`: main first, then extras by
      slug. A main paper that fails is `Err`. A bad extra is skipped with a note carrying its
      error;
    - `pub fn load_one(main: &Path, slug: &str) -> Result<Config, String>`;
    - `pub fn paper_path(main: &Path, slug: &str) -> PathBuf`;
    - `pub fn create(main: &Path, slug: &str, title: &str, reader: &str) -> Result<PathBuf,
      String>`: it refuses an existing file and a bad slug, and fills `PAPER_STARTER` from
      the main paper's place and location.

- [ ] **Step 1: Write failing tests:**
  - `no_papers_folder_is_just_main`: `load_all` gives one config, equal to `Config::load(main)`,
    with `paper == None`.
  - `extras_load_in_slug_order_with_state_dirs`: `b.toml` and `a.toml` load as `[main, a, b]`;
    `a`'s `state_dir` is `papers/a`.
  - `bad_names_are_skipped_with_a_note` (Review Focus 2): `Kid Courier.toml`, `main.toml` and
    `notes.txt` are skipped. There are two notes for the `.toml` files, and none for `.txt`.
  - `household_keys_are_refused`: `[sync]`, `[reading]` and `[email] smtp_host` in a paper file
    each give the exact error, naming the key.
  - `household_keys_are_inherited`: the extra's `sync`, `reading`, `email.from/smtp_host/
    smtp_port/username` equal main's; its `email.to/enabled/usb_too` are its own.
  - `new_keys_round_trip_and_hide_at_defaults`: `reader`, `kindle`, `large_print = true` and
    `[schedule] at = "07:00", days = ["sat"]` round-trip through `save`. Saving the untouched
    `STARTER` writes no `reader`, `kindle`, `large_print` or `[schedule]` line.
  - `bad_schedule_is_an_error`: `at = "25:00"` and `days = ["saturday"]` are errors.
  - `slug_rules`: `valid_slug` accepts `kid-courier` and refuses `Main`, `main`, `1paper` and a
    33-character slug. `slug_from_title("The Kid Courier") == "kid-courier"`.
  - `create_writes_a_parseable_paper`: it uses main's place, refuses a second create of the
    same slug, and the created file loads.

- [ ] **Step 2: Run** `cargo test -p deckle-digest --test papers` and `cargo test -p
  deckle-digest config`. Expected: FAIL to compile.

- [ ] **Step 3: Implement.** Household keys are detected on the raw `toml::Table` before
  `Config::parse`. Inheritance copies `sync`, `reading` and the four SMTP fields from main.

- [ ] **Step 4: Run** the same tests. Expected: PASS. The existing `config.rs` tests still pass.

- [ ] **Step 5: Commit** `feat(digest): papers folder and per-paper keys`.

### Task 2: Per-paper editions, saved articles and email (PR 1)

**Files:**
- Modify: `crates/deckle-digest/src/lib.rs`: the stem and prefix, `build`'s file name, and
  `gather`'s reading guard.
- Modify: `crates/deckle-digest/src/mail.rs`: `edition_message`'s subject and attachment.
- Test: `crates/deckle-digest/tests/papers.rs` (append) and the `mail.rs` unit tests.

**Interfaces:**
- Consumes: `Config::paper`, `Config::slug()` (Task 1).
- Produces:
  - `pub fn edition_prefix(cfg: &Config) -> String`: `"deckle-daily-"` or
    `"deckle-daily-<slug>-"`.
  - `pub fn edition_stem(cfg: &Config, date: NaiveDate) -> String`.
  - `pub fn edition_file(cfg: &Config, date: NaiveDate) -> String`: `<stem>.kfx`. The existing
    `edition_filename(date)` stays as the main paper's.
  - `pub fn is_edition_name(name: &str, prefix: &str) -> bool`: an exact
    `<prefix>YYYY-MM-DD.<ext>` match (`kfx` or `epub`).
  - `Config::state_file(&self, name: &str) -> Option<PathBuf>`: `Some(state_dir/name)` for an
    extra paper, `None` for main (callers keep today's path).
  - `Config::work_dir(&self, cache: &Path) -> PathBuf`.
  - `build` writes `<stem>.epub`.
  - `edition_message(cfg: &EmailConfig, paper: Option<&str>, date, epub: Vec<u8>, stem: &str)
    -> Outgoing`: `paper = None` keeps today's subject.

- [ ] **Step 1: Write failing tests:**
  - `stems_per_paper`: main gives `deckle-daily-2026-10-24`; `kid-courier` gives
    `deckle-daily-kid-courier-2026-10-24`.
  - `edition_names_match_exactly`: `is_edition_name("deckle-daily-2026-10-24.kfx",
    "deckle-daily-")` is true. It's false for `deckle-daily-kid-courier-2026-10-24.kfx` and
    `deckle-daily-email-test.kfx` with that prefix, and true for the Courier with its own
    prefix.
  - `extra_paper_never_prints_saved_articles`: with one saved entry in a temp queue and
    `sections = ["reading", "weather"]`, an extra paper's `gather` has no Your Reading section,
    no `reading_outcomes`, and the note "Saved articles go to <main title>."; the main paper
    prints it.
  - `build_names_the_epub_by_stem`: an extra paper writes `deckle-daily-<slug>-<date>.epub`.
  - `email_subject_and_attachment`: main gives "Deckle Daily — Saturday, October 24, 2026"
    and `deckle-daily-2026-10-24.epub`; an extra gives "The Kid Courier — Saturday, October
    24, 2026" and `deckle-daily-kid-courier-2026-10-24.epub`.

- [ ] **Step 2: Run** `cargo test -p deckle-digest`. Expected: FAIL.

- [ ] **Step 3: Implement.** `gather` skips the auto-insert of Your Reading, and the section
  itself, when `cfg.paper.is_some()`. Fix the `edition_message` call sites (CLI and tests).

- [ ] **Step 4: Run** `cargo test --workspace`. Expected: PASS.

- [ ] **Step 5: Commit** `feat(digest): per-paper edition names, reading and email`.

### Task 3: Large print (PR 3)

**Files:**
- Modify: `crates/deckle-digest/src/render.rs`: `LARGE_STYLE`, chosen by a new
  `Edition::large_print`.
- Modify: `crates/deckle-digest/src/sources/weather.rs`: the strip renders as a list when large.
- Modify: `crates/deckle-digest/src/lib.rs`:
  - `Edition { large_print: bool }`, set from `cfg.large_print`;
  - `gather` caps `max_photos` at 1 when large.
- Test: `crates/deckle-digest/tests/large_print.rs`, plus the existing golden fixture.

**Interfaces:**
- Produces:
  - `pub(crate) const LARGE_STYLE: &str`, appended to `style.css` after `STYLE` only when
    `large_print`;
  - `pub(crate) fn min_font_em(css: &str) -> f64`, the smallest `font-size: Nem` in effect
    (for the test).

- [ ] **Step 1: Write failing tests:**
  - `large_print_has_nothing_under_1em`: `min_font_em(STYLE + LARGE_STYLE) >= 1.0`, with later
    rules winning per selector.
  - `large_print_strip_is_a_list`: the weather XHTML holds no `table.strip` and five
    `li class="day"` rows.
  - `regular_paper_unchanged`: the weather page golden (`tests/weather_page.rs`) and `STYLE` match byte for byte.
  - `large_print_one_photo`: an article with three photos keeps one.

- [ ] **Step 2: Run** `cargo test -p deckle-digest --test large_print`. Expected: FAIL.

- [ ] **Step 3: Implement.** Headline and section-head sizes are 1.1× their `STYLE` values.
  Rules are 2px.

- [ ] **Step 4: Run** `cargo test -p deckle-digest`. Expected: PASS.

- [ ] **Step 5: Commit** `feat(digest): large-print papers`.

- [ ] **Step 6: Device check, with the owner.**
  - Build a large-print edition and send it to the Paperwhite and the Scribe. Check it reads
    well at the Kindle's large sizes.
  - Then build one with `body { font-size: 1.25em }` added. If the body text is visibly larger
    than a regular paper's at the same Aa setting on both Kindles, add that rule to
    `LARGE_STYLE` in a follow-up commit (`feat(digest): large print scales body text`).
    Otherwise record "not honoured" in the spec's §6.

### Task 4: Exact prune and replace, owned editions, several prunes per connection (PR 1)

**Files:**
- Modify: `crates/deckle-core/src/deliver.rs`: `prunable` matches exactly.
- Modify: `crates/deckle-core/src/queue.rs`:
  - `make_room` matches exactly;
  - add `add_keeping_for`;
  - `Prune` gains nothing, and the flush prunes a list.
- Modify: `crates/deckle-core/src/sync.rs`: `run_with` takes `prunes: &[Prune]`.
- Test: the unit tests in those three files.

**Interfaces:**
- Produces:
  - `prunable(names, prefix, keep)`: same signature, now dated names only (`<prefix>YYYY-MM-DD.<ext>`).
  - `pub fn is_dated(name: &str, prefix: &str) -> bool` in `deliver.rs`, shared by prune and
    replace.
  - `Queue::add_keeping_for(&self, kfx, name, replaces, keep, owner: Option<&str>)`: owner
    written before the entry appears (as `add_for`).
  - `Queue::flush_with(conn, prunes: &[Prune], finishing, progress)`: each prune runs after
    the deliveries.
  - `sync::run_with(queue, conn, plan, prunes: &[Prune], finishing, progress, stems,
    skip_prefix)`.

- [ ] **Step 1: Write failing tests:**
  - `prunable_ignores_other_papers`: with `deckle-daily-2026-10-2{0..3}.kfx`,
    `deckle-daily-kid-courier-2026-10-17.kfx` and `-24`, and `deckle-daily-email-test.kfx`,
    `prunable(.., "deckle-daily-", 2)` is the two oldest main files only.
  - `replace_keeps_other_papers` (Review Focus 4): queue `deckle-daily-kid-courier-2026-10-24.kfx`
    with replace `deckle-daily-kid-courier-`, then `deckle-daily-2026-10-25.kfx` with replace
    `deckle-daily-`. Both are pending.
  - `a_bound_edition_waits_for_its_kindle`: `add_keeping_for` with an owner; a flush to another
    Kindle leaves it, and its own takes it.
  - `two_prunes_keep_their_own`: on a folder target, main (keep 3) and the Courier (keep 2) each
    keep their own `keep` of their own prefix.
  - The existing `sync.rs` and `queue.rs` tests, updated to the new signature, pass.

- [ ] **Step 2: Run** `cargo test -p deckle-core queue sync deliver`. Expected: FAIL.

- [ ] **Step 3: Implement.** The date check is a small hand match (10 chars,
  `dddd-dd-dd`), not a regex dependency in `deckle-core`.

- [ ] **Step 4: Run** `cargo test -p deckle-core`. Expected: PASS.

- [ ] **Step 5: Commit** `feat(core): exact edition matching and per-paper prune`.

### Task 5: A schedule per paper (PR 2)

**Files:**
- Modify: `crates/deckle-digest/src/schedule.rs`.
- Test: the unit tests in `schedule.rs`.

**Interfaces:**
- Consumes: `ScheduleConfig`, `Day`, `Config::slug()` (Task 1).
- Produces:
  - `Schedule { hour, minute, days: Vec<Day>, slug: String, exe, config }`;
    `Schedule::args("run")` adds `--paper <slug>`.
  - `pub fn label(slug: &str) -> String`: `DIGEST_LABEL`, or `DIGEST_LABEL.<slug>`.
  - `pub fn task_name_for(slug: &str) -> String`.
  - `pub fn launchd_digest_plist(s: &Schedule, logs: &Path) -> (String, String)` and
    `pub fn launchd_watch_plist(exe: &Path, config: Option<&Path>, logs: &Path) -> (String,
    String)`, replacing `launchd_plists`.
  - `task_xml(s: &Schedule, job: &str) -> String`, with a `ScheduleByWeek` trigger when `days`
    is set.
  - `pub fn jobs_for(papers: &[Config], exe: &Path, config: Option<&Path>) -> Vec<Schedule>`:
    papers with `schedule.at` only.
  - `pub fn stale_labels(installed: &[String], wanted: &[Schedule]) -> Vec<String>`: installed
    `DIGEST_LABEL.<slug>` labels not wanted. It never includes `DIGEST_LABEL`.
  - `pub fn install_all(jobs: &[Schedule], exe: &Path, config: Option<&Path>) ->
    Result<Vec<String>, String>`: installs the jobs and the watcher, and removes the stale ones.
  - `pub struct PaperStatus { slug, at: Option<String>, days: Vec<Day>, installed: bool }`.
  - `Status { papers: Vec<PaperStatus>, watch, exe }`. Main's `at` comes from its plist or task
    when the file has none.

- [ ] **Step 1: Write failing tests:**
  - `daily_plist_unchanged_for_main`: for main with no days, the plist equals today's output
    (a fixture string captured before the change).
  - `saturday_plist`: `days = [Sat]` gives an array with one dict `Weekday 6, Hour 7, Minute
    0`. `[Mon, Wed]` gives two dicts.
  - `labels_and_args`: label `com.casavargas.deckle.digest.kid-courier`, and the arguments end
    `run --paper kid-courier --to device`.
  - `weekly_task_xml`: holds `<ScheduleByWeek><DaysOfWeek><Saturday/></DaysOfWeek>
    <WeeksInterval>1</WeeksInterval></ScheduleByWeek>`.
  - `stale_labels_keep_main`: given installed `[main, .lakeside, .old]` and wanted `[lakeside]`,
    it returns `[.old]`.
  - `jobs_for_skips_papers_without_at`.

- [ ] **Step 2: Run** `cargo test -p deckle-digest schedule`. Expected: FAIL.

- [ ] **Step 3: Implement.** Installed digest labels come from listing
  `~/Library/LaunchAgents/com.casavargas.deckle.digest*.plist`, or `schtasks /Query /FO CSV`
  under `\Deckle\`.

- [ ] **Step 4: Run.** Expected: PASS.

- [ ] **Step 5: Commit** `feat(digest): a schedule per paper, with days`.

### Task 6: CLI: `--paper`, `papers`, routing, per-paper state, the run lock (PR 1)

**Refresh (2026-10-25):** routing uses owners, not folders.
- `paper_queue` becomes `fn paper_owner(cfg: &Config, main: &Config) -> Result<Option<String>,
  String>`: `kindle` resolved with `main.sync.find_device`, else the value as an id. An
  unknown name is an error naming the paper's file.
- `queue_edition` uses `add_keeping_for`.
- `sync_to` flushes the one shared outbox with `prunes_for(papers, id)`.
- **Per-paper state:**
  - readership: `cfg.state_file("readership.json")`;
  - the puzzle store: `puzzle_store()` under `state_dir` for an extra paper;
  - the Morgue: `morgue.json` and its folder under `state_dir`.

  The main paper keeps today's paths.
- **Other outputs.** `queue_remarkable`, `queue_epubs`, the export folders and the edition
  bundle use `edition_stem` and `edition_prefix` for an unbound paper, and are skipped for a
  bound one.
- **Tests:**
  - `run_routes_targeted_paper_to_its_kindle` checks the entry's owner, not a folder;
  - `push_prunes_each_paper_on_its_kindle` is unchanged;
  - new `bound_paper_skips_other_readers` (no reMarkable or EPUB-outbox entry);
  - new `extra_paper_morgue_and_puzzle_are_its_own`.


**Files:**
- Modify: `Cargo.toml` (`rust-version = "1.89"`).
- Modify: `crates/deckle-cli/src/cli.rs`:
  - `Digest { paper: Option<String> }` as a global `--paper`;
  - `DigestCommand::Papers { action: Option<PapersCommand> }`, with `PapersCommand::New { slug,
    title, for_: Option<String> }` (`--for`);
  - `Schedule { at, off, days: Option<String> }` (`--days sat,sun`).
- Modify: `crates/deckle-cli/src/lib.rs`: `digest()`, `queue_edition`, `digest_prune`,
  `push_queued`, `sync_to`, `record_layout`, `digest_readership`, and the watch loop.
- Test: `crates/deckle-cli/tests/papers.rs`, plus the existing `email_tests`.

**Interfaces:**
- Consumes: Tasks 1, 2, 4 and 5.
- Produces:
  - `fn paper_config(path: &Path, paper: Option<&str>) -> Result<Config, String>`
    (`None`/`"main"` → main).
  - `fn readership_path(cfg: &Config) -> PathBuf`: `cfg.state_file("readership.json")`, or
    `deckle_readership::state_path()`.
  - `fn paper_queue(root: &Queue, cfg: &Config) -> Queue`: `for_kindle(&cfg.kindle)` when it's
    set, else `root.clone()`.
  - `fn prunes_for(papers: &[Config], sync_id: &str) -> Vec<Prune>`: papers whose `kindle` is
    empty or equal, each `Prune { prefix: edition_prefix(p), keep: p.keep.max(1) }`.
  - `fn run_lock(cache: &Path) -> Result<std::fs::File, String>`: blocks on `File::lock`.
  - `sync_to(root: &Queue, papers: &[Config], target, plan)`. It flushes `[root,
    root.for_kindle(id)]` and reads stems from every paper's readership state, recording each
    stem's reads in the state it came from.
  - `digest papers [--json]` prints one line per paper: `slug · title · for reader · kindle or
    "any Kindle" · delivery · Regular|Large print`. `papers new` prints the file path.
  - `schedule --at T [--days …] [--paper s]` writes `[schedule]` into that paper's file with
    `config::save`, then runs `install_all` over all papers. `schedule --off` removes every
    digest job and the watcher.

- [ ] **Step 1: Write failing tests** (temp `--config`, fake Previewer, `Target::Folder`
  Kindles):
  - `papers_lists_main_and_extras`: the `--json` output has slugs `["main", "kid-courier"]`.
  - `papers_new_creates_and_refuses_twice`.
  - `build_with_paper_names_by_stem`: `digest --config C --paper kid-courier build -o OUT`
    writes `OUT/deckle-daily-kid-courier-<date>.epub`. `--paper nope` is an error naming
    `papers/nope.toml`.
  - `run_routes_targeted_paper_to_its_kindle`: with `kindle = "G1"`, the KFX is in
    `outbox/kindle/G1/`. Then `sync_to` with id `G2` doesn't deliver it, and `G1` does.
  - `push_prunes_each_paper_on_its_kindle`: on Kindle `G1`, main (any, keep 3) and the Courier
    (G1, keep 2) are each pruned to their own keep. On `G2`, the Courier isn't pruned.
  - `deleted_paper_edition_still_delivered` (Review Focus 5).
  - `run_ignores_days_by_hand` (Review Focus 3): a `days = ["sat"]` paper run on a Tuesday date
    builds.
  - `readership_per_paper`: two papers' layouts on one date land in two files, and
    `digest readership --paper kid-courier` reports only the Courier.
  - `extra_paper_email_uses_its_to`, in `email_tests`: it goes to the paper's `to`, with the
    main paper's SMTP account.
  - `run_lock_serializes`: two threads take `run_lock`, and the second waits until the first
    drops it (checked by a timestamp order).

- [ ] **Step 2: Run** `cargo test -p deckle-cli`. Expected: FAIL.

- [ ] **Step 3: Implement.**
  - `run` takes the lock before building.
  - It uses `cfg.work_dir(cache)`, emails with the paper's `[email]`, and commits saved
    articles only for main.
  - `watch` calls `load_all` every check and keeps the last good set when it fails. It prints
    the error only when it changes.

- [ ] **Step 4: Run** `cargo test --workspace`, `cargo clippy --workspace -- -D warnings` and
  `cargo fmt --check`. Expected: all pass.

- [ ] **Step 5: Commit** `feat(cli): --paper, digest papers, per-Kindle delivery`.

### Task 7: Desktop: Your papers (PR 4)

**Files:**
- Modify: `apps/desktop/src-tauri/src/daily.rs`:
  - `daily_state`, `daily_save`, `daily_run`, `edition_preview`, `schedule_get`,
    `schedule_set` and the email commands take `paper: Option<String>`;
  - `run_args(send, preview_dir, paper)`;
  - new `papers_list` and `paper_create`.
- Modify: `apps/desktop/src-tauri/src/readership.rs`: the commands take `paper`.
- Modify: `apps/desktop/src-tauri/src/lib.rs`: register the commands.
- Create: `apps/desktop/src/Papers.tsx`, `apps/desktop/src/papersView.ts` and
  `apps/desktop/src/papersView.test.ts`.
- Modify: `apps/desktop/src/DailyView.tsx` (the selected paper, cards and per-paper fields),
  `api.ts`, `types.ts` and `styles.css`.

**Interfaces:**
- Consumes: `papers::load_all`, `papers::create`, `papers::slug_from_title`, and
  `schedule::Status.papers`.
- Produces:
  - Rust: `#[derive(Serialize)] pub struct PaperCard { slug, title, reader, sections:
    Vec<String>, kindle: String, email_to: Option<String>, at: Option<String>, days:
    Vec<String>, large_print: bool, installed: bool }`.
  - `papers_list() -> Result<Vec<PaperCard>, String>`.
  - `paper_create(title: String, reader: String) -> Result<String, String>`, returning the
    slug.
  - TS:
    - `deliveryLine(card): string`;
    - `kindleLine(card, devices: {syncId: string; name: string}[]): string`;
    - `typeLine(card): string`;
    - `anyKindleHint(cards): string | null`;
    - `slugFromTitle(title): string`, mirroring Rust.

- [ ] **Step 1: Write failing vitests** in `papersView.test.ts`:
  - `deliveryLine` gives "USB at 05:30" (no days), "USB, Saturdays at 07:00" (`["sat"]`),
    "USB, Mon and Wed at 06:00", "Email at 06:00" (email on, `usb_too` off), and "Not
    scheduled".
  - `kindleLine` gives the device's name for a known id, "Any Kindle" for an empty one, and
    "any Kindle Paperwhite" for an unknown mass-storage id with a `/` (Review Focus 1).
  - `typeLine` gives "Regular" and "Large print".
  - `anyKindleHint` is `null` with one paper, and "The Morning Deckle goes to whichever Kindle
    connects first." with two papers when main is on any.
  - `slugFromTitle("The Kid Courier") === "kid-courier"`.

- [ ] **Step 2: Run** `npm --prefix apps/desktop test -- papersView`. Expected: FAIL.

- [ ] **Step 3: Implement.**
  - The cards are laid out as in the mockup.
  - **Edit** sets the selected slug, and every Daily command call passes it.
  - **Preview** runs `daily_run(send=false, paper)`.
  - **New paper** is an inline form (name, reader).
  - The Your Reading card is replaced by "Saved articles go to <main title>." on an extra
    paper.
  - The Email card shows only Send to and Also by USB on an extra paper.
  - Large print shows the Aa line.
  - The Kindle picker lists `list_devices` with each device's sync id.

- [ ] **Step 4: Run** the vitests, `cargo test -p deckle-desktop` and `npm --prefix apps/desktop run build`. Expected: PASS.

- [ ] **Step 5: Visual check.** Open the Daily tab in the browser pane over the IPC mock with
  three papers matching the mockup. Check:
  - the cards and their lines;
  - that Edit switches every card below it;
  - that New paper adds a card;
  - phone width.

  Take a screenshot for the PR.

- [ ] **Step 6: Commit** `feat(desktop): Your papers on the Daily tab`.

### Task 8: Docs, changelog, device check

**Files:**
- Modify: `docs/daily.md`. Add "Many papers": files and slugs, household keys, Kindle and
  email delivery, days, large print and the Aa note, and removing a paper (delete the file,
  run `deckle digest schedule`).
- Modify: `CHANGELOG.md` and the `STARTER` comments (from Task 1; check the wording).

- [ ] **Step 1: Write the docs.** Every key in Global Constraints appears once with an example.

- [ ] **Step 2: Run** `cargo test --workspace` (the starter tests). Expected: PASS.

- [ ] **Step 3: Commit** `docs: many papers`.

- [ ] **Step 4: Device check, with the owner.**
  - Set up the mockup's three papers, with the Courier on a spare Kindle.
  - Run `deckle digest schedule`, and check `launchctl list | grep deckle` shows three digest
    jobs and the watcher.
  - Run each paper by hand. Plug in each Kindle in turn and check each gets only its paper and
    keeps its own `keep`.
  - Check the Lakeside email arrives on the Paperwhite.
