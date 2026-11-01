# Chapter Recaps (increment 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An opt-in pass that writes a short, labelled AI recap at the end of each chapter of a
Kindle-bound EPUB, with `deckle run --recaps`.

**Architecture:** A new `deckle_core::recaps` module shaped like `whoswho` (EPUB bytes in, bytes
out, injected `Post` for tests), reusing `whoswho`'s zip/OPF/XHTML scanning helpers made
`pub(crate)`. `pipeline::prepare_with` rewrites `normalized.epub` with it between normalize and
convert. Requests go through `deckle_xray::llm::complete`; results are cached per chapter as plain
text under the Deckle cache dir.

**Tech Stack:** Rust 2024, `zip`, `roxmltree`, `sha2`, `clap` (CLI), existing `deckle_xray::llm`.

**Spec:** `docs/superpowers/specs/2026-11-01-ai-extras-design.md`

## Global Constraints

- Off by default. `recaps` without an LLM warns exactly: `Chapter recaps need an LLM; set one in Settings`.
- A recap failure never fails a run; `add` returns `Err` only for an unreadable EPUB.
- Min chapter length **800** words; part sizes **2,000** words (self-hosted) / **40,000** (hosted).
- `max_tokens` **4096** per request; `llm.rs` is not changed.
- Words: whitespace-separated runs, but each Han, kana, Hangul or Thai char counts as one word.
- Inserted markup exactly as spec §7 (aside, `deckle-recap` class, id `deckle-recap-dK-sJ`,
  "Chapter recap", "AI-generated summary ({model}), not part of the original book.").
- Prompt must contain: `Do not guess at or hint about anything that happens later.`
- Cache: `{cache_dir}/recaps/{sha256 hex}.txt`; key = prompt version + provider + model + chapter text.
- Warnings: `Chapter recap skipped for {label}: {error}`, at most 3, then `…and N more chapter recaps skipped`.
- Run focused tests only (shared target dir): `cargo test -p deckle-core --test it recaps`, etc.
- Don't touch `crates/deckle-docx`, `crates/deckle-pdf`, import code (other sessions own them).

## Review Focus

- Chapters wrapped in `<div class="chapter"><h2>…`: the recap must land before the `<div`, not
  inside the next chapter's wrapper (else it shows at the top of the next chapter's page).
- A recap answer containing `<`, `&` or quotes must come out XML-escaped and the XHTML still parse.
- A book run twice through `add` (cache or no cache) must not get a second set of asides.
- An API key left empty in settings must fall back to `ANTHROPIC_API_KEY` / `OPENAI_API_KEY`,
  as X-Ray does.
- A chapter with no `h1`/`h2` at all (one spine file = one chapter) still gets a recap before
  `</body>`.

---

### Task 1: Share Who's who's EPUB helpers

**Files:**
- Modify: `crates/deckle-core/src/whoswho.rs`

**Interfaces:**
- Produces (`pub(crate)`): `unzip(&[u8]) -> Result<Vec<(String, Vec<u8>)>, String>`;
  `struct Book { opf_path, title, author, lang, spine, nav, ncx, nonlinear: HashSet<String>, fixed_layout: bool }`
  with `Book::read(&[(String, Vec<u8>)]) -> Result<Book, String>` and
  `Book::nav_labels(&self, files) -> HashMap<String, String>`; `struct Doc`, `struct Seg` (fields
  `pub(crate)`); `scan(&str) -> Doc`; `tag_end(&str, usize) -> usize`;
  `tag_name(&str) -> Option<(&str, bool, bool)>`; `decode(&str, usize) -> (String, Vec<usize>)`;
  `esc(&str) -> String`.

- [ ] **Step 1:** Make the items above `pub(crate)`. In `Book::read`, fill `nonlinear` with the
  spine paths whose `itemref` has `linear="no"` (the spine itself is unchanged), and set
  `fixed_layout` when a `meta` with `property="rendition:layout"` has text `pre-paginated`.
- [ ] **Step 2:** `cargo test -p deckle-core --test it whoswho` → all pass (Who's who unchanged).
- [ ] **Step 3:** Commit `refactor(whoswho): share EPUB scanning helpers with recaps`.

### Task 2: Chapters, word counting and parts

**Files:**
- Create: `crates/deckle-core/src/recaps.rs` (module doc pointing at the spec); register in `crates/deckle-core/src/lib.rs`
- Test: `crates/deckle-core/tests/it/recaps.rs` (add `mod recaps;` in `tests/it/main.rs`)

**Interfaces:**
- Produces:
  - `pub const MIN_WORDS: usize = 800; pub const LOCAL_PART_WORDS: usize = 2_000; pub const HOSTED_PART_WORDS: usize = 40_000;`
  - `pub fn word_count(text: &str) -> usize`
  - `pub fn parts(text: &str, max_words: usize) -> Vec<String>` — paragraphs (`\n\n`-separated)
    packed greedily, in order, each part ≤ `max_words`; a paragraph longer than `max_words` is cut
    at word boundaries (char boundaries for the one-char words).
  - `pub(crate) struct Chapter { pub doc: usize, pub section: usize, pub insert_at: usize, pub heading: Option<String>, pub text: String }`
  - `pub(crate) fn chapters(doc_index: usize, xhtml: &str) -> Vec<Chapter>` — one per `section`
    of `scan`, text = its segs joined (`"\n\n"` between blocks, `""` within a block); `heading` =
    decoded, whitespace-collapsed text of the `h1`/`h2` opening it; `insert_at` = byte offset
    before the next section's heading, backed up over any opening tags immediately before it
    (only whitespace between), else the offset of `</body>`.

- [ ] **Step 1: Write failing tests** in `tests/it/recaps.rs`:
  - `word_count_counts_spaced_words`: `word_count("The cat sat.\n\nOn a mat")` == 6.
  - `word_count_counts_each_cjk_char`: 1,000 copies of `"猫"` → 1000; `"彼は ねこ"` → 4.
  - `parts_packs_paragraphs_in_order`: 5 paragraphs of 600 words, max 2,000 → 2 parts of 1,800
    and 1,200 words; joined back they equal the input words in order.
  - `parts_cuts_a_long_paragraph`: one 5,000-word paragraph, max 2,000 → 3 parts (2000/2000/1000).
  - `one_part_when_it_fits`: 900 words, max 40,000 → 1 part.
  (`chapters` is `pub(crate)`; it is exercised through `add_with` in Task 3.)
- [ ] **Step 2:** `cargo test -p deckle-core --test it recaps` → fail (module missing).
- [ ] **Step 3:** Implement `word_count`, `parts`, `Chapter`, `chapters` as above. One-char-word
  ranges: Han (`\u{4E00}-\u{9FFF}`, `\u{3400}-\u{4DBF}`, `\u{F900}-\u{FAFF}`), kana
  (`\u{3040}-\u{30FF}`), Hangul (`\u{AC00}-\u{D7AF}`), Thai (`\u{0E00}-\u{0E7F}`).
- [ ] **Step 4:** Tests pass.
- [ ] **Step 5:** Commit `feat(recaps): chapters, word counting and parts (#29)`.

### Task 3: Recaps into the EPUB

**Files:**
- Modify: `crates/deckle-core/src/recaps.rs`
- Test: `crates/deckle-core/tests/it/recaps.rs`

**Interfaces:**
- Consumes: Task 1 helpers; Task 2 `chapters`, `parts`, `word_count`; `crate::xray::{LlmConfig, Post}`;
  `deckle_xray::llm::{complete, http_post, Provider}`; `crate::wordwise::default_cache_dir()`.
- Produces:
  - `#[derive(Clone, Debug)] pub struct RecapOptions { pub llm: LlmConfig, pub cache_dir: Option<PathBuf> }`
  - `#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)] pub struct RecapSummary { pub chapters: usize, pub recapped: usize, pub cached: usize, pub model: String, pub warnings: Vec<String> }`
    (`chapters` = eligible chapters, `recapped` = asides written, `cached` = of those, from cache)
  - `pub fn add(bytes: &[u8], opts: &RecapOptions) -> Result<(Vec<u8>, Option<RecapSummary>), String>` (real HTTP)
  - `pub fn add_with(bytes: &[u8], opts: &RecapOptions, post: &mut Post<'_>) -> Result<(Vec<u8>, Option<RecapSummary>), String>`
  - `Ok((unchanged bytes, None))` for: fixed layout, already recapped (`id="deckle-recap-` present
    in any spine doc), or no eligible chapter.

- [ ] **Step 1: Write failing tests.** Copy the `epub(chapters)` builder from `tests/it/whoswho.rs`
  (chapter bodies are raw XHTML after the `<h1>`), plus a mock `Post` that records
  `(url, body)` and answers an OpenAI-shaped `{"choices":[{"message":{"content":…},"finish_reason":"stop"}]}`.
  Use `LlmConfig { provider: Provider::OpenAi, model: "test-model".into(), endpoint: "http://localhost:11434/v1".into(), api_key: String::new() }`
  (self-hosted) unless the test says hosted (`endpoint: String::new()`, `api_key: "k".into()`). Tests:
  - `long_chapters_end_with_a_recap_and_short_ones_dont`: chapters of 900, 300, 1,200 words →
    c0 and c2 each contain exactly one `class="deckle-recap"` holding the mock text, the label
    `AI-generated summary (test-model), not part of the original book.`, and the aside sits after
    the chapter's last paragraph; c1 has none; OPF spine/manifest and nav bytes unchanged; summary
    `{chapters: 2, recapped: 2, cached: 0}`; every chapter XHTML still parses with `roxmltree`.
  - `two_chapters_in_one_file_each_get_their_own`: one file `<h2>Two</h2>…900 words…<div class="chapter"><h2>Three</h2>…900 words…`
    → two asides; the first sits before `<div class="chapter">`; ids `deckle-recap-d0-s1`/`-s2`
    (match whatever section numbering `scan` gives, asserted literally); the two request bodies
    name `Chapter: Two` and `Chapter: Three`.
  - `request_is_spoiler_safe`: c0 text has marker `ALPHA`, c1 has `OMEGA` (both chapters long) →
    the first request's body contains `Do not guess at or hint about anything that happens later.`
    and `ALPHA` but not `OMEGA`; `max_tokens` is 4096.
  - `hosted_openai_uses_max_completion_tokens`: hosted config → body has `"max_completion_tokens":4096`.
  - `a_long_chapter_on_a_local_server_is_split_then_merged`: one 5,000-word chapter, local →
    4 requests (3 parts + 1 merge, the merge body contains the three part answers in order); same
    chapter hosted → 1 request.
  - `answer_is_escaped_and_cleaned`: mock answers `Recap: "Tom & <Jerry>"` → aside text is
    `Tom &amp; &lt;Jerry&gt;`.
  - `one_failure_is_a_warning`: mock returns `Err("status: 500")` for the 2nd request only →
    other chapters recapped; `warnings == ["Chapter recap skipped for Two: LLM request failed: status: 500"]`
    (label per Task 2 heading rule).
  - `all_failures_leave_the_book_unchanged`: every request errs → bytes equal input; 5 failing
    chapters → 4 warnings, the last `…and 2 more chapter recaps skipped`.
  - `cache_makes_a_second_run_free`: `cache_dir: Some(tmp)`; first run N requests; second run on
    the original bytes → 0 requests, identical output bytes, `cached == recapped`.
  - `already_recapped_and_fixed_layout_are_unchanged`: output of a first run fed back → unchanged,
    `None`, 0 requests; an OPF with `<meta property="rendition:layout">pre-paginated</meta>` → unchanged.
  - `nonlinear_spine_items_are_skipped`: a long chapter with `linear="no"` → no aside.
  - `empty_key_falls_back_to_env`: covered by reading the key with the same rule as
    `xray.rs:209` (`std::env::var(provider.env_key())` when `api_key` is empty); no test (env
    mutation is unsafe in parallel tests).
- [ ] **Step 2:** Run → fail.
- [ ] **Step 3: Implement.** Per eligible chapter (≥ `MIN_WORDS`, spine doc not in `nonlinear`):
  label = heading, else `nav_labels` entry when that doc has one chapter, else `Chapter N`.
  Part size = `LOCAL_PART_WORDS` when `provider == OpenAi` and the endpoint is non-empty and
  differs from `Provider::OpenAi.default_endpoint()` (trim trailing `/`), else `HOSTED_PART_WORDS`.
  Prompts (constants; `const PROMPT_VERSION: u32 = 1` goes into the cache key):
  - System: `You write short chapter recaps for an e-reader. Answer with the recap text only.`
  - Chapter (one part): `Book: {title} · Author: {author} · Language: {lang} · Chapter: {label}\n\nWrite a recap of this chapter in 2–4 sentences, in the book's language, present tense. Say only what happens in this chapter. Do not guess at or hint about anything that happens later. Do not judge or praise the writing.\n\n{text}`
  - Part: same header, then `This is part {i} of {n} of the chapter. Write a recap of this part in 1–2 sentences, …` (same rule sentences).
  - Merge: same header, then `These are recaps of the parts of one chapter, in order. Combine them into one recap of the chapter in 2–4 sentences, …` (same rule sentences), then the part recaps one per line.
  Cleanup: trim; strip one leading `Recap:`/`Summary:` (case-insensitive); strip wrapping `"…"`,
  `“…”`, `*…*`; empty → failure. Cache read before, write after a success (create dirs; a cache
  I/O error is ignored). Insert asides back to front per doc; rewrite with `crate::kepub::write`.
- [ ] **Step 4:** `cargo test -p deckle-core --test it recaps` → pass. `cargo clippy -p deckle-core --all-targets -- -D warnings` clean.
- [ ] **Step 5:** Commit `feat(recaps): recap each chapter into the EPUB (#29)`.

### Task 4: Kindle pipeline and CLI

**Files:**
- Modify: `crates/deckle-core/src/pipeline.rs` (`RunOptions`, `RunReport`, `prepare_with`)
- Modify: every `RunOptions { … }` literal (`grep -rn "RunOptions {" crates apps`) — add `recaps: false` after `wikipedia`
- Modify: `crates/deckle-cli/src/cli.rs` (`--recaps` on `run`), `crates/deckle-cli/src/lib.rs` (pass it; warn on the EPUB-reader path)
- Test: `crates/deckle-core/tests/it/pipeline.rs`

**Interfaces:**
- Consumes: `recaps::{add, RecapOptions, RecapSummary}`.
- Produces: `RunOptions.recaps: bool` ("Chapter recaps (#29); needs `llm`."), `RunReport.recaps: Option<RecapSummary>`.

- [ ] **Step 1: Write failing tests** in `tests/it/pipeline.rs` (unix, fake `ok.sh`, a generated
  EPUB with two 900-word chapters written to a temp file using a small builder like Task 3's):
  - `recaps_without_an_llm_warn_and_ship`: `recaps: true, llm: None` → delivered, warnings contain
    `Chapter recaps need an LLM; set one in Settings`, `report.recaps == None`.
  - `recaps_reach_the_normalized_epub`: a one-thread `TcpListener` on `127.0.0.1:0` answering
    every request with HTTP 200 and the OpenAI JSON (read headers, then `Content-Length` bytes);
    `llm: Some(OpenAi, endpoint http://127.0.0.1:{port}/v1)`, `keep_work_dir: true` →
    `work_dir/normalized.epub` holds 2 `deckle-recap` asides; `report.recaps.recapped == 2`;
    `stage_ms` has `recaps`.
  - `an_unreachable_llm_is_a_warning_not_a_failure`: endpoint `http://127.0.0.1:1/v1` →
    `error == None`, delivered, a warning starting `Chapter recap skipped for`.
  Existing pipeline tests get `recaps: false` and must pass unchanged.
- [ ] **Step 2:** Run `cargo test -p deckle-core --test it pipeline` → fail.
- [ ] **Step 3: Implement** in `prepare_with`, right after normalize's report is stored and before
  the `convert` stage: if `opts.recaps` and `opts.llm` is `None`, push the warning; if both, run a
  stage named `"recaps"` (time it in `stage_ms`, call `on_stage("recaps")`) that reads
  `normalized`, calls `recaps::add(&bytes, &RecapOptions { llm, cache_dir: Some(default_cache_dir()) })`,
  writes the bytes back when a summary came back, extends `report.warnings` with the summary's
  warnings and sets `report.recaps`. An `Err` from `add` becomes the warning `Chapter recaps skipped: {e}`.
  The recap stage never sets `report.error`.
- [ ] **Step 4:** CLI: `#[arg(long)] recaps: bool` on `run` with doc
  `/// Adds an AI-written recap at the end of each chapter (needs --llm). With a hosted provider the book's text is sent to it; with a local server nothing leaves this computer.`;
  set `recaps` in the `run` `RunOptions`; on the `send_to_reader` path, when `recaps` is set, print
  `Chapter recaps go to Kindle only for now` to stderr once.
- [ ] **Step 5:** Run `cargo test -p deckle-core --test it pipeline`, `cargo test -p deckle-core --test it recaps`,
  `cargo build -p deckle-cli -p deckle-app` (and `cargo check -p deckle-desktop` if it builds in
  this worktree), `cargo clippy -p deckle-core -p deckle-cli -p deckle-app --all-targets -- -D warnings`. All green.
- [ ] **Step 6:** Commit `feat(recaps): chapter recaps on the Kindle path and deckle run --recaps (#29)`.

### Task 5: Ship

- [ ] **Step 1:** `cargo fmt --all -- --check`.
- [ ] **Step 2:** Push; open the PR with `gh pr create --repo CasaVargas/deckle --head claude/feat-ai-extras-09e59b`,
  body says `Part of #29` (increments 2–4 remain), lists the spec, tests run and what's not verified (no real-LLM or device run).
