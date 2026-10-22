# Highlights Out, as Markdown — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Read `documents/My Clippings.txt` off a connected Kindle and keep one append-only Markdown file per book in a folder the user picks, from the CLI, the Kindle tab and `digest watch` (issue #60).

**Architecture:** A new pure crate, `deckle-highlights`, parses the clippings file, groups it into books, renders Markdown and appends to files, tracking what it wrote in `highlights.json`. It knows nothing about devices. `deckle-core` gets one function that reads the file's bytes over an existing `Conn` (mass storage, MTP or folder), and the sync watcher learns a `clippings` action that rides in the same session as push and prune. The CLI and the desktop app glue the two together.

**Tech Stack:** Rust 2024 workspace (serde, serde_json, sha1 from `[workspace.dependencies]`), Tauri 2 commands, React/TypeScript Kindle tab.

**Spec:** `docs/superpowers/specs/2026-10-22-highlights-export-design.md`

## Global Constraints

- Append only: never rewrite, reorder or delete bytes already in an exported file (spec §2.4).
- The same entry is never written twice to a file, across exports and across Kindles (spec anti-goals).
- One MTP session per action: never open a second `Conn` for the clippings read inside an action that already has one (spec §3).
- No network calls. Nothing leaves the Mac.
- Automatic export is driven by the existing `[sync] clippings` key in `digest.toml` (default `false`, per-device overrides). No new key, no migration (spec §6).
- `highlights.json` lives next to `library.json`: `deckle_library::config::default_path().with_file_name("highlights.json")`.
- Bookmarks are skipped. English clippings are required; for other languages an entry with text whose kind isn't recognised becomes a highlight, and its raw line 2 is kept (spec §4).
- CLI parity with the app, `--json` on every new command (v1 goal 7).
- New source files start with `// SPDX-License-Identifier: GPL-3.0-or-later` and a `//!` doc line, like the rest of the workspace.

## Review Focus

1. **No clippings file** (a new Kindle, or one where nothing was ever highlighted): the result is "no highlights yet", an empty result and exit code 0, not an error. Tests: Task 4 (`read_on` returns `None`), Task 6 (CLI prints `no highlights on this Kindle yet`).
2. **Bytes that aren't clean UTF-8** (stray invalid bytes, BOM, mixed CRLF/LF): decode lossily and keep going. Test: Task 1 `invalid_utf8_is_decoded_lossily`.
3. **Output folder missing or unwritable** (iCloud Drive not mounted, permissions): the error names the folder, and `highlights.json` isn't updated for books whose file failed, so the next export retries them. Test: Task 3 `a_failed_write_leaves_state_untouched`.
4. **A large clippings file** (years of reading, around 5 MB and 20,000 entries): parse and group finish in under a second in a release build. Test: Task 2 `twenty_thousand_entries_group_quickly` (debug build, budget 3 s).
5. **A multi-paragraph highlight**: every line keeps its `> ` prefix, and blank lines inside it become `>` lines, so the passage stays one blockquote. Test: Task 2 `multi_paragraph_highlight_stays_one_blockquote`.

---

### Task 0: Device checks on the owner's Scribe (manual, gates Task 5)

No code. The owner does this with the Scribe 2024 plugged in. Record the results in `docs/devices.md` (the Scribe row) as part of Task 8.

- [ ] **Step 1: Idle session check.** With nothing in the outbox, open a book on the Scribe, plug it in, and run `target/debug/deckle kindle books`. That command opens and closes one MTP session.
  Record: did the Scribe leave the open book for its home screen? **No** → Task 5 sets `IDLE_SESSION_OK = true`. **Yes** → leave it `false`.
- [ ] **Step 2: Clippings check.** Highlight two passages in a sideloaded book and add a note to one. Using an MTP client (Android File Transfer or OpenMTP), confirm that `documents/My Clippings.txt` exists, then copy it out.
  Record: the exact line-2 format for a Deckle-sent KFX (does it have `page N`?). Add the copied file, with your own personal text trimmed, to `fixtures/clippings/scribe-2024.txt` in Task 1.

### Task 1: `deckle-highlights` crate — parsing

**Files:**
- Create: `crates/deckle-highlights/Cargo.toml` (deps: `serde`, `serde_json`, `sha1`, `thiserror` from the workspace; dev: `tempfile`)
- Create: `crates/deckle-highlights/src/lib.rs` (module list and re-exports)
- Create: `crates/deckle-highlights/src/parse.rs`
- Create: `fixtures/clippings/variants.txt` (BOM + CRLF), `fixtures/clippings/README.md` (one line per variant, what it covers)
- Modify: `Cargo.toml` (add `deckle-highlights = { path = "crates/deckle-highlights" }` to `[workspace.dependencies]`; the `crates/*` glob already makes it a member)
- Test: `crates/deckle-highlights/tests/parse.rs`

**Interfaces:**
- Produces:
  ```rust
  pub enum Kind { Highlight, Note, Bookmark }
  pub struct Entry {
      pub title: String, pub author: String,          // author "" when absent
      pub kind: Kind,
      pub page: Option<String>,                         // as written: "88", "xii"
      pub location: Option<(u32, u32)>,                 // (a, a) for a single location
      pub added: Added,
      pub text: String,                                 // trimmed, inner newlines kept
      pub raw_meta: Option<String>,                     // line 2, only when kind wasn't recognised
  }
  pub struct Added { pub raw: String, pub date: Option<String> } // date: "YYYY-MM-DD"
  pub struct Parsed { pub entries: Vec<Entry>, pub error: Option<ParseError> }
  pub struct ParseError { pub line: usize, pub message: String }  // 1-based line in the file
  pub fn parse(bytes: &[u8]) -> Parsed;
  ```
  All types derive `Clone, Debug, PartialEq, Serialize`.

- [ ] **Step 1: Write the fixture.** `fixtures/clippings/variants.txt` holds, in order: (a) a highlight with page and location range; (b) a note at a location inside (a)'s range; (c) a bookmark; (d) a title with parentheses, `The Count of Monte Cristo (Illustrated) (Alexandre Dumas)`; (e) a book with no author group, `notes-2026`; (f) an entry with no page field, `- Your Highlight at location 220-221 | Added on …`; (g) a PDF entry, `- Your Highlight on page 4 | Added on …`; (h) an extended highlight overlapping (a), added later, at `Location 1338-1345`; (i) a Spanish entry, `- Tu subrayado en la página 12 | posición 100-102 | Añadido el lunes, 3 de marzo de 2025 10:14:22`; (j) a highlight whose text has two paragraphs separated by a blank line. Save it with a UTF-8 BOM and CRLF line endings.

- [ ] **Step 2: Write the failing tests** in `tests/parse.rs`:
  ```rust
  #[test] fn parses_every_variant() {
      let p = parse(&std::fs::read(fixture("variants.txt")).unwrap());
      assert!(p.error.is_none());
      assert_eq!(p.entries.len(), 10);
      let a = &p.entries[0];
      assert_eq!((a.title.as_str(), a.author.as_str()), ("Walden", "Henry David Thoreau"));
      assert_eq!(a.kind, Kind::Highlight);
      assert_eq!(a.page.as_deref(), Some("88"));
      assert_eq!(a.location, Some((1340, 1342)));
      assert_eq!(a.added.date.as_deref(), Some("2025-03-03"));
      assert_eq!(p.entries[1].kind, Kind::Note);
      assert_eq!(p.entries[2].kind, Kind::Bookmark);
      assert_eq!(p.entries[3].title, "The Count of Monte Cristo (Illustrated)");
      assert_eq!(p.entries[3].author, "Alexandre Dumas");
      assert_eq!((p.entries[4].title.as_str(), p.entries[4].author.as_str()), ("notes-2026", ""));
      assert_eq!(p.entries[5].page, None);
      assert_eq!(p.entries[5].location, Some((220, 221)));
      assert_eq!((p.entries[6].page.as_deref(), p.entries[6].location), (Some("4"), None));
      let es = &p.entries[8];
      assert_eq!(es.kind, Kind::Highlight);
      assert!(es.raw_meta.as_deref().unwrap().starts_with("- Tu subrayado"));
      assert_eq!(es.added.date, None);
      assert!(p.entries[9].text.contains("\n\n"));
  }
  #[test] fn invalid_utf8_is_decoded_lossily() {
      let mut b = b"Walden (Henry David Thoreau)\n- Your Highlight on page 1 | Location 1-2 | Added on Monday, March 3, 2025 10:14:22 AM\n\nwoods \xFF here\n==========\n".to_vec();
      let p = parse(&b); assert_eq!(p.entries.len(), 1); assert!(p.entries[0].text.contains('\u{FFFD}'));
      b.clear(); assert!(parse(&b).entries.is_empty());
  }
  #[test] fn a_broken_entry_reports_its_line_and_keeps_the_rest() {
      // entry 2 has no "- " meta line
      let s = "A (B)\n- Your Highlight on page 1 | Location 1-2 | Added on Monday, March 3, 2025 10:14:22 AM\n\none\n==========\nC (D)\nnot a meta line\n==========\nE (F)\n- Your Note on Location 5 | Added on Monday, March 3, 2025 10:14:22 AM\n\nthree\n==========\n";
      let p = parse(s.as_bytes());
      assert_eq!(p.entries.len(), 2);
      assert_eq!(p.error.as_ref().unwrap().line, 7);
  }
  ```

- [ ] **Step 3: Run** `cargo test -p deckle-highlights`. Expected: compile failure, because `parse` isn't defined yet.

- [ ] **Step 4: Implement `parse(bytes: &[u8]) -> Parsed`** in `parse.rs`. Use `String::from_utf8_lossy`, strip a leading `\u{FEFF}`, normalise `\r\n` to `\n`, and split on lines equal to `==========`.
  - **Title and author:** the author is the content of the last `(...)` group when line 1 ends with `)`. Match the parentheses from the end so nested parentheses work.
  - **Kind:** English kind words are `Highlight`, `Note` and `Bookmark`. Match `page (\S+)` and `[Ll]ocation (\d+)(?:-(\d+))?`. The date is the text after `Added on `.
  - **English date:** parse `<Weekday>, <Month> <d>, <yyyy> …` with a month-name table. Don't add chrono.
  - **Unrecognised kind:** an entry with text becomes a `Highlight`, with `raw_meta = Some(line2)`. One without text becomes a `Bookmark`.
  - **First failure:** the first malformed entry sets `error` (its 1-based line number, counting the BOM-stripped text), and parsing continues with the next entry.

- [ ] **Step 5: Run** `cargo test -p deckle-highlights`. Expected: all 3 pass.

- [ ] **Step 6: Commit**
  ```bash
  git add Cargo.toml crates/deckle-highlights fixtures/clippings
  git commit -m "feat(highlights): parse My Clippings.txt (#60)"
  ```

### Task 2: Grouping, entry ids and Markdown rendering

**Files:**
- Create: `crates/deckle-highlights/src/book.rs`, `crates/deckle-highlights/src/render.rs`
- Create: `fixtures/clippings/variants.walden.md` (golden)
- Test: `crates/deckle-highlights/tests/render.rs`

**Interfaces:**
- Consumes: `Entry`, `Kind`, `parse` (Task 1).
- Produces:
  ```rust
  pub enum Item {
      Highlight { entry: Entry, notes: Vec<Entry> },
      Note { entry: Entry, of: Option<String> },   // standalone; `of`: start of its passage, filled by export (Task 3)
  }
  pub struct Book { pub key: String, pub title: String, pub author: String, pub editions: bool, pub items: Vec<Item> }
  pub fn edition_date(entry: &Entry) -> Option<&str>;          // the part after " — " for an edition entry
  pub const EDITIONS_KEY: &str = "deckle-daily";
  pub fn group(entries: Vec<Entry>) -> Vec<Book>;            // books in first-seen order, items by location
  pub fn entry_id(e: &Entry) -> String;                       // 40-char sha1 hex
  pub fn render_new_file(book: &Book) -> String;              // frontmatter + "# Title" + items
  pub fn render_items(book: &Book, items: &[&Item]) -> String; // items only, for appending
  ```

- [ ] **Step 1: Write the failing tests** in `tests/render.rs`:
  ```rust
  #[test] fn groups_collapse_and_attach() {
      let books = group(parse(&read("variants.txt")).entries);
      let walden = books.iter().find(|b| b.title == "Walden").unwrap();
      // (a) and (h) overlap: one highlight remains, the later (h); note (b) is attached to it
      let hs: Vec<_> = walden.items.iter().filter(|i| matches!(i, Item::Highlight{..})).collect();
      assert_eq!(hs.len(), 1);
      let Item::Highlight { entry, notes } = hs[0] else { unreachable!() };
      assert_eq!(entry.location, Some((1338, 1345)));
      assert_eq!(notes.len(), 1);
      assert!(books.iter().all(|b| b.items.iter().all(|i| !matches!(i, Item::Note{entry,..} if entry.kind == Kind::Bookmark))));
  }
  #[test] fn editions_share_one_book() {
      let e = |t: &str| Entry { title: t.into(), author: "Deckle".into(), ..highlight_at(10) };
      let books = group(vec![e("The Morning Deckle — Tuesday, October 20, 2026"), e("The Morning Deckle — Wednesday, October 21, 2026")]);
      assert_eq!(books.len(), 1);
      assert_eq!(books[0].key, EDITIONS_KEY);
      assert_eq!(books[0].title, "The Morning Deckle");
  }
  #[test] fn ids_ignore_whitespace_and_case_of_title_but_not_text() {
      let a = highlight_at(10); let mut b = a.clone(); b.title = format!("  {}  ", a.title.to_uppercase());
      assert_eq!(entry_id(&a), entry_id(&b));
      b.text.push('!'); assert_ne!(entry_id(&a), entry_id(&b));
  }
  #[test] fn renders_the_golden_file() {
      let books = group(parse(&read("variants.txt")).entries);
      let walden = books.iter().find(|b| b.title == "Walden").unwrap();
      assert_eq!(render_new_file(walden), String::from_utf8(read("variants.walden.md")).unwrap());
  }
  #[test] fn multi_paragraph_highlight_stays_one_blockquote() {
      let mut h = highlight_at(10); h.text = "one\n\ntwo".into();
      let b = Book { key: "k".into(), title: "T".into(), author: "".into(), editions: false, items: vec![Item::Highlight { entry: h, notes: vec![] }] };
      assert!(render_items(&b, &b.items.iter().collect::<Vec<_>>()).contains("> one\n>\n> two\n"));
  }
  #[test] fn twenty_thousand_entries_group_quickly() {
      let s: String = (0..20_000).map(|i| format!("Book {} (A)\n- Your Highlight on page {i} | Location {i}-{} | Added on Monday, March 3, 2025 10:14:22 AM\n\npassage {i}\n==========\n", i % 300, i + 1)).collect();
      let t = std::time::Instant::now();
      let books = group(parse(s.as_bytes()).entries);
      assert_eq!(books.len(), 300);
      assert!(t.elapsed() < std::time::Duration::from_secs(3));
  }
  ```
  `highlight_at(loc)` and `read(name)` are test helpers in the same file. The golden `variants.walden.md` is the spec §5 example, with this exact shape for each highlight:
  ```
  > <text, each line prefixed "> ", blank lines as ">">

  - page 88 · location 1338–1345 · 2025-03-03
  - Note: <note text>

  ```
  Leave out a missing part and its ` · `. The location uses an en dash, and a single location prints as `location 220`. A standalone note renders as `- Note on “<first 8 words>…” (location N): <text>` when `of` is set, or `- Note (location N): <text>` otherwise.

- [ ] **Step 2: Run** `cargo test -p deckle-highlights --test render`. Expected: compile failure.

- [ ] **Step 3: Implement** `group`, `entry_id`, `render_new_file` and `render_items`.
  - **`group`:**
    - Skip bookmarks.
    - **Book key:** `EDITIONS_KEY` for author `Deckle` with a title containing ` — `; else the normalised `title\u{1F}author`.
    - **Editions:** the book's `title` is the text before ` — ` of its first entry, and `editions` is `true`. Inside an editions book, items sort by edition date first (in file order of first appearance), then location. The renderers write `## <edition_date>` before the first item of each edition in what they render.
    - **Overlaps:** within a book, two highlights overlap when their location ranges intersect. Keep the one with the larger index in the file.
    - **Notes:** a note attaches to the highlight whose range contains its location (`a <= n <= b`). Otherwise it becomes `Item::Note { of: None }`.
    - **Order:** sort items by location start; entries with no location go last, in file order.
  - **`entry_id`:** sha1 over `lowercase(trim(title)) \x1F lowercase(trim(author)) \x1F kind \x1F location \x1F text` (text as is, trimmed).
  - **Frontmatter:** `title`, `author` (omitted when empty), `source: kindle`, `tags: [highlights]`. YAML-quote the title when it contains `:` or `#`.

- [ ] **Step 4: Run** `cargo test -p deckle-highlights`. Expected: all pass. Hand-check the golden file against spec §5 once.

- [ ] **Step 5: Commit** `feat(highlights): group books, attach notes, render Markdown (#60)`

### Task 3: State and append-only export

**Files:**
- Create: `crates/deckle-highlights/src/export.rs`, `crates/deckle-highlights/src/state.rs`
- Test: `crates/deckle-highlights/tests/export.rs`

**Interfaces:**
- Consumes: `Book`, `Item`, `entry_id`, `render_new_file`, `render_items` (Task 2).
- Produces:
  ```rust
  #[derive(Default, Serialize, Deserialize)]
  pub struct State { pub folder: Option<PathBuf>, pub books: BTreeMap<String, BookState> }
  #[derive(Default, Serialize, Deserialize)]
  pub struct BookState { pub file: String, pub written: BTreeSet<String> }
  impl State { pub fn load(path: &Path) -> State; pub fn save(&self, path: &Path) -> io::Result<()>; }  // load: missing/corrupt file → default
  pub struct Pending { pub key: String, pub title: String, pub author: String, pub file: String, pub total: usize, pub new: usize, pub markdown: String }
  pub fn pending(books: &[Book], dir: &Path, state: &State) -> Vec<Pending>;  // what export would write; markdown = full new file or the appended block
  pub struct Written { pub file: String, pub new: usize, pub created: bool }
  pub struct ExportReport { pub written: Vec<Written>, pub failed: Vec<(String, String)> }  // (file, error)
  pub fn export(books: &[Book], dir: &Path, state: &mut State) -> ExportReport;
  pub fn file_name(title: &str) -> String;
  ```

- [ ] **Step 1: Write the failing tests:**
  ```rust
  #[test] fn first_export_creates_then_appends_only_new() {
      let dir = tempdir(); let mut st = State::default();
      let r = export(&books(&["h:10:one"]), dir.path(), &mut st);
      assert_eq!((r.written[0].new, r.written[0].created), (1, true));
      let path = dir.path().join("Walden.md");
      let mut text = fs::read_to_string(&path).unwrap();
      text.push_str("\nmy own thoughts\n"); fs::write(&path, &text).unwrap();
      let r = export(&books(&["h:10:one", "h:20:two"]), dir.path(), &mut st);
      assert_eq!((r.written[0].new, r.written[0].created), (1, false));
      let after = fs::read_to_string(&path).unwrap();
      assert!(after.starts_with(&text), "existing bytes untouched");
      assert_eq!(after.matches("> two").count(), 1);
      assert!(export(&books(&["h:10:one", "h:20:two"]), dir.path(), &mut st).written.is_empty());
  }
  #[test] fn a_second_kindle_with_the_same_highlight_adds_nothing() { /* same entries, fresh parse → written.is_empty() */ }
  #[test] fn a_note_added_later_is_appended_with_its_passage() {
      // export "h:10:I went to the woods because I wished to live", then export with "n:10:read again"
      assert!(fs::read_to_string(&path).unwrap().contains("- Note on “I went to the woods because I wished to…” (location 10): read again"));
  }
  #[test] fn a_deleted_file_is_written_fresh() { /* export, delete Walden.md, export → created: true, contains "> one" */ }
  #[test] fn colliding_titles_get_the_author() {
      // "Walden" by Thoreau and "Walden" by Someone Else → "Walden.md" and "Walden (Someone Else).md"
      // a pre-existing user file "Walden.md" unknown to state → first book becomes "Walden (Henry David Thoreau).md"
  }
  #[test] fn editions_keep_their_file_after_the_paper_is_renamed() {
      // export edition of "The Morning Deckle", then edition of "The Lakeside Gazette" → both in "The Morning Deckle.md", under "## <date>" headings
  }
  #[test] fn a_failed_write_leaves_state_untouched() {
      let mut st = State::default();
      let r = export(&books(&["h:10:one"]), Path::new("/nonexistent/deckle-test"), &mut st);
      assert_eq!(r.failed.len(), 1);
      assert!(r.failed[0].1.contains("/nonexistent/deckle-test"));
      assert!(st.books.is_empty());
  }
  #[test] fn file_names_are_safe() {
      assert_eq!(file_name("A/B: C?"), "A-B- C-.md");
      assert!(file_name(&"x".repeat(300)).len() <= 123);
      assert_eq!(file_name("  .hidden. "), "hidden.md");
  }
  #[test] fn pending_matches_what_export_writes() { /* pending(...).markdown == appended bytes of the next export */ }
  ```
  `books(&["h:10:text", "n:10:text"])` is a test helper that builds Walden entries (kind:location:text) and runs them through `group`.

- [ ] **Step 2: Run** `cargo test -p deckle-highlights --test export`. Expected: compile failure.

- [ ] **Step 3: Implement.**
  - **New entries:** an item is new when its entry id, or any of its notes' ids, isn't in `written`.
  - **Highlight not yet written:** render it with its new notes.
  - **Highlight already written, notes new:** render each new note as `Item::Note { of: Some(first eight words of the highlight) }`.
  - **Missing file:** if the mapped file is gone, reset `written` and write the whole book with `render_new_file`.
  - **Appending:** open with `OpenOptions::append`. Prefix `\n` when the file doesn't end with a newline.
  - **State:** update it only after a successful write. Error strings include the full path.
  - **`file_name`:** replace `/ \ : * ? " < > |` and control characters with `-`, trim spaces and dots, cap the stem at 120 bytes on a char boundary, and append `.md`.
  - **Collisions:** a candidate is taken if another key in `state` maps to it, or if it exists on disk without being this book's mapped file. Then try `<Title> (<Author>).md`, then append ` 2`, ` 3`, and so on.
  - **Editions:** the file name comes from the edition book's `title` at creation, and stays as recorded.
  - **`State::save`:** write `highlights.json.tmp`, then rename it.

- [ ] **Step 4: Run** `cargo test -p deckle-highlights`. Expected: all pass.

- [ ] **Step 5: Commit** `feat(highlights): append-only export with highlights.json state (#60)`

### Task 4: Read the clippings file over any transport

**Files:**
- Create: `crates/deckle-core/src/clippings.rs` (+ `pub mod clippings;` in `lib.rs`)
- Modify: `crates/deckle-core/src/fake_mtp.rs` (file contents + `GET_OBJECT`)
- Test: in `clippings.rs` (`#[cfg(test)]`, like `sync.rs`)

**Interfaces:**
- Consumes: `Conn`, `Target`, `DeliverError` (`deliver.rs`); `Mtp::find_path`, `Mtp::get_object` (`deckle-transport`).
- Produces:
  ```rust
  pub const CLIPPINGS: &str = "My Clippings.txt";
  /// documents/My Clippings.txt from the connection's target; Ok(None) when there isn't one.
  pub fn read_on(conn: &mut Conn<'_>) -> Result<Option<Vec<u8>>, DeliverError>;
  ```
  Also, in `fake_mtp` (test-only): `pub fn put_bytes(path: &str, bytes: &[u8])`.

- [ ] **Step 1: Extend the fake Kindle.** Add `contents: HashMap<u32, Vec<u8>>` to `Kindle`. `put_bytes` works like `put`, sets `size`, and stores the bytes. `GET_OBJECT` replies with `data(c.code, tx, &contents[h])` (empty if none), then OK. If `Wire::read` receives a buffer smaller than the queued reply, it hands the reply out over several reads: keep the remainder at the front of `replies`.

- [ ] **Step 2: Write the failing tests:**
  ```rust
  #[test] fn reads_from_a_folder_and_mass_storage() { /* Target::Folder(dir) with dir/"My Clippings.txt"; Target::MassStorage root/documents/… → Some(bytes) */ }
  #[test] fn reads_over_mtp_in_the_connection_it_is_given() {
      fake::reset(&[]); fake::put_bytes("My Clippings.txt", b"Walden (H)\n");
      let target = mtp_target();
      let mut conn = Conn::with_opener(&target, fake::open);
      assert_eq!(read_on(&mut conn).unwrap().as_deref(), Some(&b"Walden (H)\n"[..]));
      drop(conn);
      assert_eq!(fake::opens(), 1);
  }
  #[test] fn no_file_is_none_not_an_error() { /* empty folder, empty fake → Ok(None) for both */ }
  ```

- [ ] **Step 3: Run** `cargo test -p deckle-core clippings`. Expected: compile failure.

- [ ] **Step 4: Implement `read_on`.**
  - **Folder:** `<dir>/My Clippings.txt` (a folder export stands in for `documents/`, the way `inventory::list_on` treats it).
  - **Mass storage:** `<root>/documents/My Clippings.txt`.
  - **MTP:** `find_documents`, then `find_child` (or `find_path`) for the file, then `get_object`. A missing file is `Ok(None)`.

- [ ] **Step 5: Run** `cargo test -p deckle-core`. Expected: all pass, including the existing sync and inventory tests that use the fake.

- [ ] **Step 6: Commit** `feat(core): read My Clippings.txt over mass storage and MTP (#60)`

### Task 5: The watcher's clippings action, and `digest watch` exports

**Files:**
- Modify: `crates/deckle-core/src/sync.rs` (`Actions`, `Plan`, `Watcher`, `run_with`; tests)
- Modify: `crates/deckle-core/src/queue.rs` (`FlushReport` gains `#[serde(skip)] pub clippings: Option<Vec<u8>>`)
- Modify: `crates/deckle-digest/src/config.rs` (doc comment on `clippings`; starter template line 361 → `clippings = false   # write Kindle highlights as Markdown (set the folder in the app or with deckle highlights export --to)`; `any_clippings` doc)
- Modify: `crates/deckle-cli/src/lib.rs` (`sync_actions`, the watch loop, `sync_to`; remove the "isn't built yet" warning)
- Modify: `crates/deckle-cli/Cargo.toml` (+ `deckle-highlights.workspace = true`)

**Interfaces:**
- Consumes: `deckle_core::clippings::read_on` (Task 4); `deckle_highlights::{parse, group, export, State}` (Tasks 1–3).
- Produces:
  ```rust
  pub struct Actions { pub push: bool, pub prune: bool, pub clippings: bool }   // Default: clippings false
  pub struct Plan { pub push: bool, pub prune: bool, pub clippings: bool }
  pub const IDLE_SESSION_OK: bool = /* Task 0 result */;
  ```
  Also, in `deckle-cli`: `fn export_highlights(bytes: &[u8]) -> Result<deckle_highlights::ExportReport, String>`. It loads the state, needs `state.folder`, exports, then saves.

- [ ] **Step 1: Write the failing watcher tests** (in `sync.rs`; first update the `PUSH`/`PRUNE`/`BOTH`/`NONE` consts and the existing literals with `clippings: false`):
  ```rust
  #[test] fn clippings_ride_along_with_a_push_once_per_connection() {
      let a = files(&["a.kfx"]); let on = Actions { push: true, prune: true, clippings: true };
      let mut w = Watcher::default();
      assert_eq!(w.tick(Some("K"), &a, on), Plan { push: true, prune: true, clippings: true });
      let b = files(&["b.kfx"]);
      assert_eq!(w.tick(Some("K"), &b, on), Plan { push: true, prune: true, clippings: false }, "once per connection");
  }
  #[test] fn an_idle_plug_in_exports_only_when_sessions_are_harmless() {
      let on = Actions { push: true, prune: true, clippings: true };
      let mut w = Watcher::default();
      let want = Plan { push: false, prune: false, clippings: IDLE_SESSION_OK };
      assert_eq!(w.tick(Some("K"), &[], on), want);
      assert_eq!(w.tick(Some("K"), &[], on), NONE);
  }
  #[test] fn clippings_share_the_push_session() {
      // fake MTP with an outbox entry and My Clippings.txt; run(..) with clippings: true
      // → report.clippings == Some(bytes), fake::opens() == 1
  }
  ```
  The existing `an_unconfigured_device_behaves_as_watch_did` test must pass unchanged, because the default has clippings off.

- [ ] **Step 2: Run** `cargo test -p deckle-core sync`. Expected: the new tests fail.

- [ ] **Step 3: Implement.**
  - **`Connected`** gains `clipped: bool`.
  - **On a push or prune plan:** set `clippings = actions.clippings && !conn.clipped`, and mark `clipped`.
  - **With no push or prune due:** if `actions.clippings && IDLE_SESSION_OK && !conn.clipped`, return a clippings-only plan.
  - **`Plan::is_empty`:** include `clippings`.
  - **`run_with`:** after push and prune, when `plan.clippings`, set `report.clippings = read_on(conn)?`. A clippings-only plan opens the session through the same `conn`.

- [ ] **Step 4: Wire the CLI.**
  - **`sync_actions`:** passes `a.clippings`.
  - **`sync_to`:** if `flush.clippings` is `Some`, calls `export_highlights`.
    - With no folder set, print once per connection: `highlights: no folder set; run deckle highlights export --to <dir> once`.
    - Otherwise print `highlights: N new in M files` (or emit them in the JSON outcome as `highlights`).
  - **The warning:** delete the `any_clippings` "isn't built yet" lines.

- [ ] **Step 5: Run** `cargo test --workspace`. Expected: all pass.

- [ ] **Step 6: Commit** `feat(sync): export Kindle highlights in digest watch's session (#60)`

### Task 6: `deckle highlights list|export`

**Files:**
- Modify: `crates/deckle-cli/src/cli.rs` (new `Command::Highlights`, `HighlightsCommand`)
- Modify: `crates/deckle-cli/src/lib.rs` (`fn highlights(...)`)
- Test: `crates/deckle-cli/tests/highlights.rs`

**Interfaces:**
- Consumes: `read_on` (Task 4), `parse`, `group`, `pending`, `export`, `State` (Tasks 1–3), `resolve_target` (existing).
- Produces the CLI surface:
  ```rust
  /// Highlights and notes on a Kindle, as Markdown files (one per book).
  Highlights {
      /// `device` for the first connected Kindle, or a folder standing in for documents/.
      #[arg(long, default_value = "device", global = true)] from: String,
      #[command(subcommand)] action: HighlightsCommand,
  }
  pub enum HighlightsCommand {
      /// Books with highlights, how many, and how many are new.
      List,
      /// Write new highlights into the folder (remembered after the first --to).
      Export { #[arg(long)] to: Option<PathBuf> },
  }
  ```
  (`--from` rather than the spec's implied `--to device`, because `--to` names the output folder here.)

- [ ] **Step 1: Write the failing tests.** Run the built binary against a folder fixture with `HOME`/config redirected the way `tests/cli.rs` does:
  ```rust
  #[test] fn export_writes_files_and_remembers_the_folder() {
      // from = tmp/kindle containing a copy of fixtures/clippings/variants.txt as "My Clippings.txt"
      // `deckle highlights --from <kindle> export --to <out> --json` → exit 0, out/Walden.md exists,
      //   JSON {"written":[{"file":"Walden.md","new":…,"created":true},…],"failed":[]}
      // second run without --to → exit 0, "written": []
  }
  #[test] fn list_shows_counts() { /* `highlights --from <kindle> list --json` → [{"title":"Walden","total":…,"new":…}, …] */ }
  #[test] fn no_clippings_is_not_an_error() { /* empty kindle folder → exit 0, stdout "no highlights on this Kindle yet" */ }
  #[test] fn export_without_a_folder_says_how() { /* no --to, no state → exit 2, stderr contains "--to" */ }
  ```

- [ ] **Step 2: Run** `cargo test -p deckle-cli --test highlights`. Expected: fail.

- [ ] **Step 3: Implement `fn highlights(json: bool, from: &str, action: cli::HighlightsCommand) -> Result<ExitCode, String>`**, dispatched from `run` like `kindle`.
  - **Connection:** one `Conn` for the read.
  - **`--to`:** canonicalised (create the folder if missing), then stored in `state.folder`.
  - **Exit codes:** `1` when the read fails, `2` for a missing folder, `0` otherwise. A non-empty `failed` still exits `0` but prints each failure to stderr.
  - **Parse errors:** when `Parsed.error` is set, print `warning: My Clippings.txt line N: <message>` to stderr (and include `"parse_error"` in the JSON), and still export what parsed. Add an assertion for it to `export_writes_files_and_remembers_the_folder`, using a copy of the fixture with one broken entry appended.

- [ ] **Step 4: Run** `cargo test -p deckle-cli`. Expected: all pass.

- [ ] **Step 5: Commit** `feat(cli): deckle highlights list|export (#60)`

### Task 7: Kindle tab and Settings in the desktop app

**Files:**
- Modify: `apps/desktop/src-tauri/Cargo.toml` (+ `deckle-highlights`)
- Modify: `apps/desktop/src-tauri/src/kindle.rs` (`DeviceShelf` gains `highlights: Vec<HighlightRow>` and `highlights_error: Option<String>`; `shelf_on` reads clippings in the same `conn`; new commands)
- Modify: `apps/desktop/src-tauri/src/lib.rs` (register the commands)
- Modify: `apps/desktop/src/api.ts`, `apps/desktop/src/types.ts`
- Create: `apps/desktop/src/Highlights.tsx` (the Kindle tab section)
- Modify: `apps/desktop/src/KindleView.tsx` (render `<Highlights>` below the book list when `shelf.highlights.length > 0`)
- Modify: `apps/desktop/src/settings/Kindles.tsx` (a "Highlights" block above the device list)
- Test: Rust tests in `kindle.rs` (`#[cfg(test)]` already exists)

**Interfaces:**
- Consumes: Tasks 1–4.
- Produces:
  ```rust
  #[derive(Serialize)] pub struct HighlightRow { pub key: String, pub title: String, pub author: String, pub file: String, pub total: usize, pub new: usize, pub markdown: String }
  #[derive(Serialize)] pub struct HighlightsStatus { pub folder: Option<String>, pub clippings: bool }
  #[tauri::command] pub fn highlights_status() -> HighlightsStatus;
  #[tauri::command] pub fn highlights_set(folder: Option<String>, clippings: bool) -> Result<HighlightsStatus, String>; // folder → highlights.json; clippings → digest.toml [sync] clippings via deckle_digest::config::save
  #[tauri::command] pub async fn highlights_write(target: RunTarget) -> Result<deckle_highlights::ExportReport, String>; // own Conn, like device_remove
  ```
  TypeScript, in `api.ts`: `highlightsStatus()`, `highlightsSet(folder, clippings)`, `highlightsWrite(target)`.

- [ ] **Step 1: Write the failing Rust test** in `kindle.rs`: `shelf_on` over `Target::Folder` holding `My Clippings.txt` returns `highlights` with Walden's `new > 0` and a `markdown` starting with `---\ntitle: Walden`. A folder without the file returns `highlights: []` and `highlights_error: None`. A file that fails to parse still returns the rows it could read, plus `highlights_error: Some("line N: …")`.
- [ ] **Step 2: Run** `cargo test -p deckle-desktop kindle`. Expected: fail.
- [ ] **Step 3: Implement the Rust side.** If `highlights_set` gets a `digest.toml` that doesn't exist yet, write `deckle_digest::config::STARTER` first, as `daily_setup` does. Then set `sync.clippings` and save. The save keeps the user's comments, the same path `daily_save` uses.
- [ ] **Step 4: Run** the test. Expected: pass.
- [ ] **Step 5: Build the UI.**
  - **Layout:** follow board 9 of the canvas, in the app's existing classes (`card`, `row-pair`, `ghost`, `primary`, `muted small`, `mono`).
    - **Left:** the books, each with title, "N highlights" and "+N new" in `--accent`. Clicking a book selects it.
    - **Right:** `Writes to <folder>/<file>`, **Change…**, a **Write N files** primary button, and a `<pre>` with the selected book's `markdown`.
    - **No folder:** the button reads **Choose a folder…** and opens the folder dialog (as DailyView's Inbox **Change…** does), then writes.
    - **After writing:** show `N new highlights in M files`, then refresh the shelf.
  - **Settings, Kindles page:** a "Highlights" `Row` with the folder and **Change…**, and a `Switch` labelled `Update these files every time this Kindle connects`, bound to `highlights_set`. Under it, the note `Runs with the morning schedule (deckle digest watch).`
- [ ] **Step 6: Verify in the running app.** Use the `run` skill, or `pnpm tauri dev` from `apps/desktop`. Point the Kindle tab at a folder target holding `fixtures/clippings/variants.txt` renamed to `My Clippings.txt`. Screenshot the Highlights section, press Write, open the output folder, and toggle the Settings switch. Then check that `digest.toml` gained `clippings = true` with its comments intact. Also run `pnpm test` and `pnpm build` in `apps/desktop`.
- [ ] **Step 7: Commit** `feat(desktop): Highlights on the Kindle tab and in Settings (#60)`

### Task 8: Docs and the real-device pass

**Files:**
- Modify: `README.md` (a short "Highlights" section after the Daily paragraph: what it does, the CLI lines, and that it reads sideloaded books Amazon's export misses)
- Modify: `docs/daily.md` (the `[sync]` paragraph: `clippings` now writes highlights)
- Modify: `docs/devices.md` (Scribe row: Task 0 results, clippings path confirmed)
- Modify: `CHANGELOG.md` (Unreleased: "Highlights and notes out as Markdown")

- [ ] **Step 1: Write the docs above,** matching each file's existing voice and density.
- [ ] **Step 2: Real-device pass on the Scribe** (the owner, or with the owner present): `target/debug/deckle highlights export --to /tmp/hl --json`. Check that `/tmp/hl/<book>.md` matches what was highlighted on the device. Then the Kindle tab, with the Scribe connected.
- [ ] **Step 3: Run** `cargo test --workspace && cargo clippy --workspace -- -D warnings`. Expected: clean.
- [ ] **Step 4: Commit** `docs: highlights export (#60)`
