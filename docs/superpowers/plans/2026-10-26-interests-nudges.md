# Interests, increment 4: Nudges in the Paper Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** With `[learning] nudges = true`, the paper itself carries what Deckle noticed, one tiny page per suggestion with a "Tap here" link; a tap on the Kindle is an accept at the next connection, with no Mac in the loop.

**Architecture:** Because the Kindle's page history records the start of the page a jump was made from (spec §15), a nudge must be its own page, so each suggestion becomes its own section (`nudge:<key>`, one XHTML file, hence one KFX section) appended after everything else. The build gets the suggestions through `GatherOptions.nudges`; the layout records the sections as it records any; at connect time `learn::accept_nudges` resolves each history position through `section_at` and accepts the `nudge:` sections it finds, writing the interest exactly as the app's button would.

**Tech Stack:** Rust (`deckle-digest` gather and render, `deckle-readership` learn, `deckle-cli`, `deckle-desktop`), React copy, docs.

**Spec:** `docs/superpowers/specs/2026-10-25-interests-design.md` §7 as amended below, §8, §10, §11 item 4, §15.

## Global Constraints

- **§7 amended by §15:** one nudge per page, not per line. A `nudge:<key>` section has one article: the suggestion's `why`, then a paragraph with one link, "Tap here to make <name> a section." (`href="front.xhtml"`), then "Change your mind on the Daily tab." No back links, no "In this section" list, no front-page tile, not in `news`. Up to 3 nudges per edition, after the last section (so after On This Day).
- Nudges print only when `cfg.learning.enabled && cfg.learning.nudges` and only from that paper's own `interests.json` (the caller passes them; `deckle-digest` never reads the file).
- **Accept rule:** a history position whose `section_at` key starts with `nudge:` accepts that key, once; `learn::accept` writes the entry (name/query as the app would), `ensure_section`, `accepted[key]`; the caller saves the paper's config and `interests.json` and notes "Added <name> to your interests (you tapped it in <weekday>'s paper)." A key already accepted or no longer a candidate is skipped silently.
- Nudge sections never enter the editor's prompt (not in `news`), never count as readership rows (keys unknown to the config), never become story starts (no titles passed), never go to the Morgue's story list (`kind_name` → "other" is fine).
- `[learning] nudges` is off by default; the app gets a second checkbox under the learning one: "Also print them in the paper, tappable". The hardware check (a nudge tap seen in a `.yjf`) is recorded in spec §17 when it happens; until then the docs say so.
- Tests offline. `cargo fmt --check`, workspace clippy, tests for deckle-digest, deckle-readership, deckle-cli, deckle-desktop; `pnpm build && pnpm test`. Commit per task on `claude/daily-interests-nudges`.

## Review Focus

- A nudge whose name holds `&` or `<`: the page escapes it, the EPUB stays valid. (Task 1)
- An edition built with nudges, then the suggestion is dismissed on the Mac before the Kindle connects: a tap still accepts (the tap is the newer decision); a key that was accepted on the Mac meanwhile is skipped. (Task 2)
- Two Kindles tap the same nudge: accepted once, one note. (Task 2)
- Readership and the page plan show no row for a nudge; the front page's Inside index lists it with "1 item". (Task 1)
- Learning off, nudges on: nothing prints. (Task 1)

---

### Task 1: Nudge sections in the edition

**Files:**
- Modify: `crates/deckle-digest/src/lib.rs` (`GatherOptions.nudges: Vec<Nudge>`, `Nudge { key, name, why }`, `sources::nudges::section`, appended in `gather_with_editor` after the kinds loop, before `drop_template_photos`)
- Create: `crates/deckle-digest/src/sources/nudges.rs`
- Modify: `crates/deckle-digest/src/render.rs` (no back line and no "In this section" for `nudge:` sections)
- Test: `crates/deckle-digest/tests/nudges.rs` (new)

**Interfaces:**
```rust
#[derive(Clone, Debug, PartialEq, Eq)] pub struct Nudge { pub key: String, pub name: String, pub why: String }
pub struct GatherOptions { …, pub nudges: Vec<Nudge> }
pub mod sources::nudges { pub const KICKER: &str = "Deckle noticed"; pub fn section(n: &Nudge) -> Section; pub fn key(k: &str) -> String /* "nudge:<k>" */; pub fn is_nudge(key: &str) -> bool; pub const MAX: usize = 3; }
```
`section`: `key = nudge:<key>`, `title = format!("Deckle noticed: {name}")`, `kicker = Some(KICKER)`, one `Article { title: name, body: "<p>{esc why}</p><p><a href=\"front.xhtml\">Tap here to make {esc name} a section.</a></p><p class=\"meta\">Change your mind on the Daily tab.</p>", .. }`. `gather_with_editor` appends `opts.nudges.iter().take(MAX)` only when `cfg.learning.enabled && cfg.learning.nudges`. `render::section_page` skips the back line and the list when `is_nudge(&s.key)`.

- [ ] **Step 1: Failing tests** (`tests/nudges.rs`, with the `Fake`/`config` helpers of `tests/interests.rs`): `nudges_print_as_their_own_sections_after_everything` (two nudges, `learning.nudges = true` → the last two sections are `nudge:bryce young` and `nudge:brandon sanderson`, titles "Deckle noticed: …", not in `g.news`; rendered files: their XHTML holds `href="front.xhtml"` and "Tap here to make Bryce Young a section.", no `class="back"`, no "In this section"; the front page's index lists them with "1 item"); `a_name_with_markup_is_escaped` (`name = "Tom & <Jerry>"` → `&amp;` and `&lt;`, and the EPUB passes `deckle_core::inspect` or the existing validity check the other tests use); `nudges_need_learning_on_and_the_flag` (flag off, or learning off → no nudge sections); `at_most_three`.
- [ ] **Step 2:** FAIL. **Step 3:** implement. **Step 4:** PASS + `cargo test -p deckle-digest` green. **Step 5:** commit `feat(digest): nudges — what Deckle noticed as tappable pages in the paper`.

---

### Task 2: A tap accepts

**Files:**
- Modify: `crates/deckle-readership/src/learn.rs` (`accept_nudges`), `crates/deckle-cli/src/lib.rs` (after `on_connection` in `sync_to`; `gather_options_for` fills `nudges`), `apps/desktop/src-tauri/src/kindle.rs` (after `on_connection_at`)
- Test: `crates/deckle-readership/tests/learn.rs`, `crates/deckle-cli/tests/learn.rs`

**Interfaces:**
```rust
/// Nudges tapped on the Kindle: every history position of every read whose section key starts with `nudge:` names a candidate; each is accepted once (as `accept` does). Returns the names added, for the caller to save `cfg` and `learning` and to note.
pub fn accept_nudges(learning: &mut Learning, cfg: &mut Config, state: &State, today: NaiveDate) -> Vec<String>;
/// The suggestions to print as nudges: `suggest` (top 3) when `cfg.learning.nudges`, as (key, name, why).
pub fn nudges_for(learning: &Learning, cfg: &Config, today: NaiveDate) -> Vec<(String, String, String)>;
```
CLI `sync_to`: after `on_connection`, `accept_nudges`; on a non-empty result `papers::save(path, paper)` and `learning.save`, and a note per name. The desktop's Kindle session: the same for the main paper. `gather_options_for` fills `opts.nudges` from `nudges_for` (when the flag is on).

- [ ] **Step 1: Failing tests.** learn: a `State` whose layout has `nudge:bryce young` at 9000..9500 and a read with history `[9100]` → `accept_nudges` returns `["Bryce Young"]`, `cfg.interests` has it, `accepted` set; a second call returns nothing; a position in `horoscope` returns nothing; a key not in `candidates` (dismissed and pruned) returns nothing; two reads (K, K2) at 9100 → one name. CLI `tests/learn.rs`: the fake Kindle's layout gains a `nudge:bryce young` section at 150_000 and the fixture `.yjf` is replaced by a synthetic one with a history record at 150_100 (`common` builders aren't visible here: write the bytes with the same helpers copied, or add a small `deckle_readership::krds::synthetic_for_tests`… prefer copying the 20-line builders into the test); after `digest push`, `digest.toml` holds `name = "Bryce Young"` and stdout/stderr mentions "Added Bryce Young".
- [ ] **Step 2:** FAIL. **Step 3:** implement. **Step 4:** PASS, clippy on cli/desktop. **Step 5:** commit `feat: a tap on a nudge makes it an interest at the next connection`.

---

### Task 3: Switch, docs, spec, verification, PR

- [ ] `DailyView.tsx`: under the learning checkbox, a second one bound to `learning.nudges` (disabled when learning is off): "Also print what Deckle noticed in the paper, as pages you can tap to accept." `docs/daily.md` "Deckle noticed": a paragraph on nudges and the flag, and that it ships off until confirmed on a Kindle. Spec §17 implementation notes (one nudge per page; the accept rule; hardware status).
- [ ] Verification as in the other increments; issue "Interests, part 4: nudges in the paper"; PR with `Closes #N`; bind.
