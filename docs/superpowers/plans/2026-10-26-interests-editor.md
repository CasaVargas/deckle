# Interests, increment 3: Editor Relevance and Naming Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** With the optional AI editor on, the one bounded completion per edition also scores each interest story's relevance (dropping the wrong Rusts, ordering the rest) and gives pending "Deckle noticed" candidates a clean name and a search query; everything else about the editor stays as it is.

**Architecture:** `editor::try_edit` adds two fields to its one request and answer: `interest` on each interest section (name, also) with a `relevance` answer of per-story scores, and `candidates` (phrase plus Tap headlines) with a `names` answer. Candidates travel in through `GatherOptions.naming` and names travel out through `Gathered.names`; `deckle-digest` never sees `interests.json`. The CLI's build loads the paper's `Learning`, passes eligible paper candidates, and writes the names back. `learn::suggest` and `accept` already honour `Candidate.named`.

**Tech Stack:** Rust (`deckle-digest` editor, `deckle-readership` learn, `deckle-cli`), React copy, docs.

**Spec:** `docs/superpowers/specs/2026-10-25-interests-design.md` §6, §8 (opt-in text), §10, §11 item 3.

## Global Constraints

- One completion per edition, same transport (`llm::complete`), same limits (80 stories, 20 sections, 12 signs, 160 KB prompt, 8,192 output tokens, 100 KB answer); plus 10 candidates × 5 headlines. Over any cap: no edit at all, one generic note (existing behaviour; the spec's "skip relevance only" is folded into this, see Review Focus).
- Relevance: score 0 / 1 / 2 per story of an `interests:` section. 0 is dropped; the rest stable-sorted by score descending (so recency order is kept within a score); an unscored story keeps score 1. A section left empty prints `unavailable = "Nothing new about <name> today."` with no brief. The lead is never refetched: the first story after ordering is whatever it is.
- Naming: a candidate's answer is `{key, name, query}`; `name` 1..=80 chars, `query` 1..=200 chars, both `valid_text`; unknown keys or duplicates invalidate the whole answer (as every other part does). A candidate with no answer stays unnamed.
- Privacy: only `interests:`/`feeds:`/`local` sections' titles and excerpts, interest names and `also` phrases, candidate phrases and **Tap** headlines enter the prompt. Never `not` phrases' context, shelf candidates, highlight hits' text, `[reading]`, calendars.
- Invalid answer → the edition is exactly as gathered and `names` is empty (no partial edits).
- Tests offline with an injected `Post`; no live provider. `cargo fmt --check`, workspace clippy, tests for deckle-digest, deckle-readership, deckle-cli; desktop `pnpm build && pnpm test`. Commit per task on `claude/daily-interests-editor`.

## Review Focus

- The model returns scores for a `feeds:` section (not an interest) or for a story in another section: the whole answer is invalid, the edition untouched. (Task 1)
- Every story of an interest section scores 0: the section prints "Nothing new about X today." and the front page stops leading with it. (Task 1)
- A duplicate group removes an interest story that was also scored: removal wins, no index panic. (Task 1)
- A candidate whose only hits are highlights or shelf days: never in the prompt, never named. (Task 3)
- The provider answers with the old shape (no `relevance`, no `names`): still valid; nothing changes. (Task 1, Task 2)

---

### Task 1: Relevance scores in the editor

**Files:**
- Modify: `crates/deckle-digest/src/editor.rs` (`SYSTEM`, `Answer.relevance`, input `interest`, validation, apply)
- Test: `crates/deckle-digest/tests/editor.rs`

**Interfaces:**
```rust
// request: each news section object gains, for interest sections only,
//   "interest": {"name": <name>, "also": [<also>…]}
// answer:
#[derive(Deserialize)] #[serde(deny_unknown_fields)]
struct Answer { duplicate_groups, briefs, horoscopes, #[serde(default)] relevance: Vec<Relevance> }
#[derive(Deserialize)] #[serde(deny_unknown_fields)]
struct Relevance { section: String, scores: Vec<Score> }
#[derive(Deserialize)] #[serde(deny_unknown_fields)]
struct Score { id: String, score: u8 }
pub const NOTHING_NEW: &str = "Nothing new about {name} today.";   // via a fn nothing_new(name)
```
`SYSTEM` gains: "For each section that carries an interest, return relevance: an object with section and scores, one {id, score} per story, score 2 when the story is about the interest, 1 when it only mentions it, 0 when it is about something else that shares the words." Validation: `section` must be an interest section id; each `id` must belong to that section; no id twice; score ≤ 2. Apply order: briefs, horoscopes, duplicate removals, then relevance (drop 0s, stable sort) computed on the surviving articles; a section emptied by relevance gets the note and loses its brief. The interest's `name`/`also` come from `cfg`: `edit` gains `interests: &[InterestConfig]` (pass `&cfg.interests` from `gather_with_editor`; the test harness passes `config().interests`).

- [ ] **Step 1: Failing tests.** Extend `config()` with `sections = [Feeds, Interests, Horoscope]` and one interest `Rust` (`also = ["Cargo"]`, `not = ["Rust Belt"]`), and `feed()` to answer the `"Rust"` search with three items: "Rust 2.0 lands", "Rust Belt voters", "Cargo workspaces" (newest first). Tests:
  - `interest_sections_carry_their_phrases_in_the_prompt`: the request body has `"interest":{"name":"Rust","also":["Cargo"]}` and no `"Rust Belt"` (the `not` list stays home).
  - `relevance_drops_zeros_and_orders_by_score_then_recency`: answer scores `Rust 2.0 lands` 1, `Rust Belt voters` 0, `Cargo workspaces` 2 → titles `["Cargo workspaces", "Rust 2.0 lands"]`; an unscored story keeps 1.
  - `all_zero_leaves_the_note_and_no_lead`: all 0 → `articles` empty, `unavailable == "Nothing new about Rust today."`, `brief` empty, and the front page lead is not from that section.
  - `scores_for_a_feed_section_or_a_foreign_story_invalidate_the_answer`: `section` = the feeds section id, or an id from another section → edition byte-identical to `original()`, one note.
  - `an_old_shaped_answer_still_applies`: `answer()` unchanged (no `relevance`) → briefs apply, interest order unchanged.
  - `a_duplicate_removal_and_a_score_on_the_same_story_do_not_panic`.
- [ ] **Step 2:** `cargo test -p deckle-digest --test editor` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** PASS, plus the existing editor tests unchanged.
- [ ] **Step 5:** Commit `feat(digest): the editor scores each interest story's relevance`.

---

### Task 2: Naming through the editor

**Files:**
- Modify: `crates/deckle-digest/src/lib.rs` (`GatherOptions.naming: Vec<NamingCandidate>` and drop `Copy`; `Gathered.names: Vec<Naming>`), `crates/deckle-digest/src/editor.rs`
- Test: `crates/deckle-digest/tests/editor.rs`

**Interfaces:**
```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamingCandidate { pub key: String, pub phrase: String, pub headlines: Vec<String> }   // ≤ 5 headlines each
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Naming { pub key: String, pub name: String, pub query: String }
pub struct GatherOptions { pub shelf_fraction_ok: bool, pub naming: Vec<NamingCandidate> }   // Clone + Default, not Copy
pub struct Gathered { …, pub names: Vec<Naming> }
pub fn edit(g, cfg, key, post, interests: &[InterestConfig], naming: &[NamingCandidate])   // fills g.names
```
Request: `"candidates": [{"key", "phrase", "headlines": [...]}]` (at most 10; more → the first 10 by order given). Answer: `#[serde(default)] names: Vec<Naming>` with `key` known, no duplicates, `name` ≤ 80 and `query` ≤ 200 chars, `valid_text`. `SYSTEM` gains: "For each candidate, return names: {key, name, query}: name is the subject as a reader would call it (a person's full name, a team, a place); query is a Google News search that finds stories about it and not its homonyms."

- [ ] **Step 1: Failing tests.** `candidates_go_in_and_names_come_out` (two candidates; answer names them; `g.names` has both; an answer naming an unknown key invalidates everything and `names` is empty); `more_than_ten_candidates_send_the_first_ten`; `an_answer_without_names_leaves_names_empty`. Every existing construction of `GatherOptions { … }` gains `naming: Vec::new()` (`GatherOptions::default()` where possible).
- [ ] **Step 2:** FAIL. **Step 3:** implement. **Step 4:** PASS, `cargo clippy -p deckle-digest -p deckle-cli -p deckle-desktop --all-targets -- -D warnings` clean (the dropped `Copy`).
- [ ] **Step 5:** Commit `feat(digest): the editor names what Deckle noticed`.

---

### Task 3: From `interests.json` to the editor and back

**Files:**
- Modify: `crates/deckle-readership/src/learn.rs`
- Modify: `crates/deckle-cli/src/lib.rs` (both `build_gathered` call sites in `digest build` and `digest run`)
- Test: `crates/deckle-readership/tests/learn.rs`, `crates/deckle-cli/tests/interests.rs`

**Interfaces:**
```rust
/// Paper candidates worth naming: the ones `suggest` would show that have no name yet and at least one Tap hit; each with up to 5 Tap headlines, newest first; at most 10.
pub fn naming_candidates(learning: &Learning, cfg: &Config, today: NaiveDate) -> Vec<(String, String, Vec<String>)>;   // (key, phrase, headlines)
/// Stores names for known keys; unknown keys are ignored. Returns how many were set.
pub fn record_names(learning: &mut Learning, names: &[(String, String, String)]) -> usize;   // (key, name, query)
```
CLI: in `digest build` and `digest run`, when `cfg.llm.enabled`, load `Learning::load(&Learning::path_for(&cfg))`, fill `opts.naming` from `naming_candidates`, and after the build `record_names` + save when `gathered.names` is non-empty. `digest interests` then prints the clean name (already: `suggest` uses `named`).

- [ ] **Step 1: Failing tests.** learn: `naming_candidates_skip_shelf_and_highlight_only_and_named`; `record_names_sets_known_keys_only`; suggest shows the clean name and `accept` uses the query once named (extend the existing accept test with a `named` candidate). CLI: a unit test isn't possible without a provider; the conversion is pure and tested in learn; the CLI wiring is checked by `cargo clippy` and by a `--json digest interests` run after seeding a `named` candidate (name shown, query used by accept) in `tests/interests.rs`.
- [ ] **Step 2:** FAIL. **Step 3:** implement. **Step 4:** PASS.
- [ ] **Step 5:** Commit `feat: name what Deckle noticed through the editor`.

---

### Task 4: Copy, docs, verification, PR

- [ ] `apps/desktop/src/DailyEditor.tsx:29`: append " With interests, the editor also judges which stories are about them and names what Deckle noticed." `docs/daily.md`: in the AI editor section (or "Your interests"), two sentences on relevance scoring and naming. Spec §16 implementation notes.
- [ ] `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p deckle-digest -p deckle-readership -p deckle-cli -p deckle-desktop`; `pnpm -C apps/desktop build && pnpm -C apps/desktop test`.
- [ ] Issue "Interests, part 3: the editor scores relevance and names candidates", push, PR with `Closes #N`, bind.
