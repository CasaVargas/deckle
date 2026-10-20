# Save to Tomorrow's Paper Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Saved articles (CLI, Inbox folder, bookmarklet) print in the next Deckle Daily as
"Your Reading", and long reads become their own books.

**Architecture:** A JSONL queue in `deckle-digest` (`reading.rs`) is fed by three capture
paths. A `reading` section kind builds stories through the existing §5a extraction. Long
reads render as one-article EPUBs. `deckle-cli` owns the commands, the conversions and the
queue commit (only `digest run` commits).

**Tech Stack:** Rust 2024, serde_json, existing `dom_smoothie` extraction, `std::net` for
the bookmarklet listener, and base64 (already in the workspace) for `data:` images.

**Spec:** `docs/superpowers/specs/2026-10-20-save-to-paper-design.md` (read with
`2026-10-20-daily-digest-design.md` §5a).

## Global Constraints

- Queue file `<data dir>/deckle/reading.jsonl`, where data dir is the parent of
  `config::default_path()`. Saved pages go in `<data dir>/deckle/reading/<id>/`.
- Duplicates: a URL already `pending` (compared without its fragment or `utm_*` parameters)
  is not added again.
- States: `pending` → `printed` | `book` | `failed`. `failed` after 3 tries.
- Inbox: default `~/Deckle/Inbox`; files modified less than 5 s ago wait; processed files go
  to `Saved/` or to `Unreadable/` with a `.txt` saying why.
- Bookmarklet: `127.0.0.1:47219`, `GET /save?token=…&url=…&title=…`, localhost only, with
  a random token stored beside the queue.
- `reading.max_items = 10`, `reading.long_read_words = 4000`. Long reads have no word cap
  and are named `deckle-reads-YYYY-MM-DD-<slug>`. Paper stories keep the 6,000-word cap.
- Only `digest run` commits queue state. A run for date D takes `pending` items plus those
  whose `printed_in` is D. `build` is a preview.
- When saves are pending and `reading` isn't in `sections`, the section is inserted first
  and a note is printed. Config files are never rewritten.
- The news keeps the cover's lead. Saved stories compete for grid spots, labelled YOUR
  READING.

## Review Focus

1. **The same URL saved twice from different channels** (CLI then bookmarklet) → one
   queue entry. Test in Task 1.
2. **A malformed line in `reading.jsonl`** (edited by hand) → the line is kept on rewrite,
   other entries still work. Test in Task 1.
3. **An Inbox `.txt` with blank lines and non-URL text** → URLs are queued, the junk is
   ignored, and the file goes to `Saved/`. Test in Task 2.
4. **A saved `.html` whose photos are in `<name>_files/` or are `data:` URIs** → the photos
   appear in the story. Test in Task 3.
5. **A bookmarklet request with a wrong or missing token** → 403, nothing queued. Test in
   Task 6.

---

### Task 1: The reading queue

**Files:**
- Create: `crates/deckle-digest/src/reading.rs` (register `pub mod reading;` in `lib.rs`)
- Test: `crates/deckle-digest/tests/reading.rs`

**Interfaces:**
- Produces:
  - `pub enum State { Pending, Printed, Book, Failed }` (serde lowercase)
  - `pub struct Entry { id, url: Option<String>, file: Option<PathBuf>, title: Option<String>, saved_at: DateTime<Utc>, source: String, state: State, tries: u32, printed_in: Option<NaiveDate>, note: Option<String> }`
  - `pub struct ReadingQueue { pub path: PathBuf }`
  - `ReadingQueue::default_path() -> PathBuf`
  - `ReadingQueue::new(path) -> Self`
  - `load(&self) -> Vec<Entry>`
  - `add_url(&self, url: &str, title: Option<&str>, source: &str, now) -> Result<Added, String>`, where `enum Added { New(String), Duplicate(String) }`
  - `add_page(&self, html: &Path, source: &str, now) -> Result<Added, String>`
  - `remove(&self, id_or_url: &str) -> Result<bool, String>`
  - `for_edition(&self, date: NaiveDate, max: usize) -> Vec<Entry>`
  - `commit(&self, date: NaiveDate, outcomes: &[(String, Outcome)]) -> Result<(), String>`, where `enum Outcome { Printed, Book, Failed(String) }`. A `Failed` below 3 tries stays pending with `tries + 1` and `note`.
  - `pub fn normalize_url(url: &str) -> String`

- [ ] **Step 1: Write failing tests**
  - `add_then_duplicate`: add `https://ex.com/a?utm_source=x#top`, then `https://ex.com/a`. The second returns `Duplicate` with the same id, and `load().len() == 1`.
  - `for_edition_takes_pending_and_same_day_reprints`: a pending entry, one printed on D, one printed on D-1. `for_edition(D, 10)` returns the first two, oldest first.
  - `three_failures_mark_failed`: commit `Failed("paywall")` three times. `state == Failed`, `tries == 3`.
  - `malformed_lines_survive_rewrite`: write `not json\n` plus one valid line, then `remove(id)`. The file still contains `not json`.
  - `add_page_copies_files_folder`: a temp `a.html` plus `a_files/p.jpg`, then `add_page`. `<data>/reading/<id>/a.html` and `a_files/p.jpg` exist, and `url` is the page's `<link rel="canonical">` when present.
- [ ] **Step 2:** Run `cargo test -p deckle-digest --test reading`. Expected: FAIL (module missing).
- [ ] **Step 3: Implement `reading.rs`.**
  - Ids: `r-<yyyymmdd>-<4 hex of FNV(url or path)>`.
  - Lock: create `reading.lock` with `create_new`; retry for 2 s, then fail.
  - Every write goes to a temp file, then a rename.
  - Unparseable lines are kept verbatim in place.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat(reading): JSONL reading queue`.

### Task 2: Inbox folder scan

**Files:**
- Create: `crates/deckle-digest/src/inbox.rs`
- Modify: `config.rs`: add `pub reading: ReadingConfig { inbox: String = "~/Deckle/Inbox", max_items: usize = 10, long_read_words: usize = 4000 }`, `SectionKind::Reading`, and in `STARTER` put `"reading"` first in `sections` with a `[reading]` block as in spec §6.
- Test: `crates/deckle-digest/tests/inbox.rs`

**Interfaces:**
- Consumes: `ReadingQueue::add_url`, `add_page`.
- Produces:
  - `pub fn scan(dir: &Path, queue: &ReadingQueue, now: SystemTime) -> ScanReport`, where `ScanReport { queued: usize, unreadable: Vec<(PathBuf, String)> }`
  - `pub fn expand_home(p: &str) -> PathBuf`
- [ ] **Step 1: Failing tests**
  - `webloc_url_txt_and_html_are_queued`: a `.webloc` (plist `<key>URL</key><string>…`), a `.url` (`URL=` line), a `.txt` with two URLs, a blank line and `hello`, and an `.html`. Expect `queued == 5` and all four files in `Saved/`.
  - `fresh_files_wait`: a file with mtime = now. Not moved, `queued == 0`.
  - `unreadable_goes_aside_with_reason`: a `.png`. Moved to `Unreadable/` with `<name>.txt`.
  - `missing_inbox_is_created`.
- [ ] **Step 2:** Run. Expected: FAIL.
- [ ] **Step 3: Implement `scan`.**
  - Only regular files at the top level are scanned (not `Saved/` or `Unreadable/`).
  - URLs are found with `https?://\S+`.
  - Name clashes in `Saved/` get a ` (n)` suffix.
- [ ] **Step 4:** Run. Expected: PASS. Also run `cargo test -p deckle-digest config`, because `STARTER` must still parse.
- [ ] **Step 5:** Commit `feat(reading): Inbox folder`.

### Task 3: Extraction for saved reading

**Files:**
- Modify: `crates/deckle-digest/src/article.rs`
- Test: `crates/deckle-digest/tests/article.rs` (extend)

**Interfaces:**
- Produces:
  - `pub fn extract_with(html: &str, url: &str, max_words: Option<usize>) -> Option<Extracted>`. The existing `extract` calls it with `Some(6000)`.
  - `pub fn load_photo(url: &str, net: &dyn Net) -> Option<Vec<u8>>`: handles `data:image/*;base64,…`, `file://` and http(s) (via `net.get_page`).
  - `fetch_photos` uses `load_photo`.
- [ ] **Step 1: Failing tests**
  - `long_read_is_not_capped`: a synthetic page with 9,000 words. `extract_with(.., None).words == 9000`, and `extract(..).words <= 6000`.
  - `saved_page_photos_from_files_folder_and_data_uri`: a saved page with `<img src="a_files/p.png">` (a 400×300 PNG on disk) and a `data:` PNG. Base `file:///…/a.html`. `fetch_photos` keeps 2.
- [ ] **Step 2:** Run. Expected: FAIL.
- [ ] **Step 3:** Implement. `resolve` must accept the `file` scheme when the base is `file://`. `load_photo` reads `file://` paths only under the base page's directory.
- [ ] **Step 4:** Run the whole `deckle-digest` suite. Expected: PASS.
- [ ] **Step 5:** Commit `feat(reading): uncapped and local-page extraction`.

### Task 4: The Your Reading section and long reads

**Files:**
- Create: `crates/deckle-digest/src/sources/reading.rs` and `crates/deckle-digest/src/longread.rs`
- Modify:
  - `lib.rs`: `Gathered` gains `reading: Option<usize>`, `long_reads: Vec<LongRead>` and `reading_outcomes: Vec<(String, Outcome)>`. `gather` inserts `Reading` first when pending saves exist and it isn't configured, and sets `Gathered::notes: Vec<String>`. `build` writes long-read EPUBs next to the paper.
  - `cover.rs`: the lead comes from `news` only. Grid candidates are the remaining news headlines followed by reading articles, labelled `YOUR READING`.
  - `render.rs`: none beyond what `longread` reuses.
- Test: `crates/deckle-digest/tests/build.rs` (extend)

**Interfaces:**
- Consumes: `ReadingQueue::for_edition`, `extract_with`, `fetch_photos`, `place_photos`, `photo_token`.
- Produces:
  - `pub struct LongRead { pub id: String, pub title: String, pub byline: Option<String>, pub words: usize, pub article: Article, pub file_stem: String }`, where `file_stem = format!("deckle-reads-{date}-{slug}")` and the slug is the title in lowercase ascii, 40 chars, hyphens.
  - `pub fn section(entries: &[Entry], cfg: &Config, net: &dyn Net, now) -> (Section, Vec<LongRead>, Vec<(String, Outcome)>)`. Meta reads `Saved Tue 2:03 PM · <outlet> · about N min` (N = words/230, at least 1). A long read's pointer article is titled with the article's title, its body `In your library: <title>, about N min.`
  - `pub fn write_long_read(lr: &LongRead, date: NaiveDate, path: &Path) -> Result<(), String>`: a one-article EPUB with a "Deckle Reads" typographic cover (`cover::render_reads(title, byline)`). Author is the byline, or the outlet.
  - `pub const READS_PREFIX: &str = "deckle-reads-";`
- [ ] **Step 1: Failing tests** (fixture net serving saved pages by URL; the queue lives in a tempdir and is passed via `Config.reading`. Add `pub queue_path: Option<PathBuf>` to `ReadingConfig`, `#[serde(skip)]`, so tests can point at a temp queue.)
  - `saved_article_prints_first_as_your_reading`: one pending entry for the WXII fixture URL. The first section is titled `Your Reading`, its article meta contains `about`, and the outcome is `Printed`.
  - `long_read_becomes_its_own_book`: a 5,000-word page. `long_reads.len() == 1`, the pointer body contains `In your library`, and `build` writes `deckle-reads-2026-10-20-*.epub`, which is a valid EPUB (mimetype first, XHTML parses).
  - `failing_save_stays_pending_with_a_try`: the URL is offline. The article is absent, and the outcome is `Failed(_)`.
  - `reading_section_inserted_when_not_configured`: `sections` without `reading` plus one pending entry. The section is present, first, and `notes` has one line.
  - `cover_lead_stays_news`: with saves present, `cover` still renders (smoke test). `gathered.edition.headlines(&gathered.news, 1)` is unaffected.
- [ ] **Step 2:** Run. Expected: FAIL.
- [ ] **Step 3:** Implement. Reuse the `feeds::par_map` worker pool; make it `pub(crate)`.
- [ ] **Step 4:** Run the `deckle-digest` suite and clippy. Expected: PASS, clean.
- [ ] **Step 5:** Commit `feat(reading): Your Reading section and long reads`.

### Task 5: CLI: `deckle save` and committing in `digest run`

**Files:**
- Modify: `crates/deckle-cli/src/cli.rs` and `crates/deckle-cli/src/main.rs`
- Test: `crates/deckle-cli/tests/cli.rs` (extend), with `DECKLE_READING_QUEUE` as an env override for the queue path in tests (read in `ReadingQueue::default_path`).

**Interfaces:**
- Consumes: Tasks 1–4.
- Produces:
  - `deckle save [<url|file.html>…] [--title T] [--list] [--remove ID] [--bookmarklet]`
  - In `digest build|run|watch`: `inbox::scan` first.
  - `digest run`: after the paper converts, each `LongRead` EPUB goes through `pipeline::run` with the cover rule (cde auto, Word Wise auto). It goes to the folder target, or into the outbox under `<file_stem>.kfx` with `replaces: None`. Then `ReadingQueue::commit(date, outcomes)`, where long reads count as `Book`. If the paper fails, nothing is committed.
  - `digest build`: never commits; prints `preview: N saved stories not marked`.
- [ ] **Step 1: Failing tests**
  - `save_list_remove_roundtrip`: `save https://ex.com/a` → stdout `saved r-…`; `save --list` shows it as pending; `save --remove <id>`; `--list` is empty.
  - `save_duplicate_says_so`: stdout contains `already saved`.
  - `bookmarklet_prints_javascript_with_token`: starts with `javascript:` and contains `127.0.0.1:47219/save?token=`.
- [ ] **Step 2:** Run `cargo test -p deckle-cli`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run. Expected: PASS.
- [ ] **Step 5:** Commit `feat(cli): deckle save; digest run commits the reading queue`.

### Task 6: Bookmarklet listener in `digest watch`

**Files:**
- Create: `crates/deckle-digest/src/bookmarklet.rs`
- Modify: `crates/deckle-cli/src/main.rs` (`Watch` spawns the listener thread)

**Interfaces:**
- Produces:
  - `pub const PORT: u16 = 47219;`
  - `pub fn token(dir: &Path) -> std::io::Result<String>`: 32 hex chars, created once, in `bookmarklet.token` beside the queue.
  - `pub fn handle(request_line: &str, token: &str, queue: &ReadingQueue, now) -> (u16, String)`: status and HTML body.
  - `pub fn serve(queue: ReadingQueue, token: String) -> std::io::Result<()>`: a blocking accept loop on `127.0.0.1:PORT`.
  - `pub fn javascript(token: &str) -> String`: opens `http://127.0.0.1:47219/save?token=…&url=`+`encodeURIComponent(location.href)`+`&title=…` in a 420×200 popup.
- [ ] **Step 1: Failing tests** (unit, in the module)
  - `good_request_queues_and_says_saved`: 200, the body contains `Saved to tomorrow's paper`, and the queue has 1 entry.
  - `wrong_or_missing_token_is_403`: 403, and nothing is queued.
  - `non_get_or_other_path_is_404`.
- [ ] **Step 2:** Run. Expected: FAIL.
- [ ] **Step 3:** Implement.
  - Read only the request line; percent-decode `url` and `title`.
  - The response HTML carries a close button (`window.close()`) and escaped text.
  - `serve` drops the connection after one request.
  - A bind failure (port in use) is printed once, and watch continues without the bookmarklet.
- [ ] **Step 4:** Run. Expected: PASS.
- [ ] **Step 5:** Commit `feat(reading): bookmarklet listener in digest watch`.

### Task 7: Docs and live check

**Files:**
- Modify: `README.md` (the Deckle Daily section: saving, Inbox, iPhone Shortcut, bookmarklet) and the spec's Status line.
- [ ] **Step 1:** `cargo fmt --all`, `cargo clippy --workspace --all-targets` (clean) and `cargo test --workspace` (pass).
- [ ] **Step 2: Live checks.**
  - `deckle save https://www.npr.org/…` (any current NPR story);
  - a `.webloc` dropped in the Inbox;
  - a long-read URL (over 4,000 words);
  - `deckle digest run --to <scratch folder>`.

  Expected: the paper's first section is Your Reading with the two stories, a
  `deckle-reads-…kfx` is beside the paper, and `save --list` shows `printed` and `book`.
- [ ] **Step 3:** Commit `docs: save to tomorrow's paper`, push, and open a PR with `Closes #61`.
