# Newspaper Sections Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Topic sections (World, Sports, Technology, …), a front page laid out like the cover, section fronts, and "back" links from every story.

**Architecture:** `[[feeds]]` entries gain `topic`, `urls` and a per-entry `full_text`; `feed_section` merges all of an entry's sources like `local_news` does and `gather` dedupes titles across news sections. `front.rs` computes one `FrontPlan` (lead + tiles) that `render.rs` uses for the front page, section fronts and back-link targets. The Daily tab gets a Topics picker over the same `[[feeds]]` list.

**Tech Stack:** Rust (`crates/deckle-digest`, `toml`/`toml_edit`, `feed_rs`), Tauri + React/TypeScript + vitest (`apps/desktop`).

**Spec:** `docs/superpowers/specs/2026-10-22-newspaper-sections-design.md`

## Global Constraints

- Every change is additive: a `digest.toml` without the new keys parses, builds and saves exactly as before (existing tests stay green, unchanged).
- Topics are `WORLD, NATION, BUSINESS, TECHNOLOGY, ENTERTAINMENT, SPORTS, SCIENCE, HEALTH`, case-insensitive; URL `https://news.google.com/rss/headlines/section/topic/<TOPIC>?hl=en-US&gl=US&ceid=US:en`.
- Topic entries: `max_items` defaults to 5 (10 otherwise); `full_text` defaults to `"lead"` (top story whole, the rest `as_briefs`); `full_text` takes `true`, `false` or `"lead"`.
- Keys stay `feeds:<name>`; picker only touches entries with a `topic`, matched by `topic`, never by name; a name clash names the new entry "<Topic> (topic)".
- Front page: lead, then a `<table class="cols">` of up to 10 tiles (top story of each news section round-robin, minus the lead), then "Inside today". No CSS columns.
- Anchors: tile/lead `id="t<section>-<article>"`, story `id="a<N>"` (existing), section top `id="top"`. Back line after each story: "↩ <Section> · ↩ Front page"; the front-page link targets the tile when the story is a tile or the lead, else `front.xhtml`.
- Dedupe: title key (`feeds.rs` `key`), first news section in page-plan order wins, applied before the `max_items` cut; Your Reading is never deduped.
- No student/staff data in fixtures; offline tests only (fake `Net`). Commit per task on this branch; never push to `main`.

## Review Focus

- A topic's top-story fetch fails (paywall/timeout): the next story is tried, up to 3, then the section is all briefs. (Task 3)
- Every source of an entry fails: the section shows "Unavailable today", the other sections still build. (Task 3)
- A title shared by Local and a topic section prints once; a section emptied by dedupe says "No new stories today." (Task 4)
- Zero or an odd number of tiles (one news section, or none): no empty/ragged table, no table at all when there are no tiles. (Task 5)
- Headlines containing `&`, `<`, quotes, or a story whose title repeats within a section: markup stays valid and anchors stay unique. (Tasks 5, 6)
- The Topics picker with a hand-written feed named "Technology" or a feed with a `url` and no `topic`: nothing of the user's is changed or removed. (Task 9)

---

### Task 1: `Topic` and its URL

**Files:**
- Modify: `crates/deckle-digest/src/config.rs` (new type near `FeedConfig`)
- Test: `crates/deckle-digest/src/config.rs` (`#[cfg(test)]` module)

**Interfaces:**
- Produces: `pub enum Topic { World, Nation, Business, Technology, Entertainment, Sports, Science, Health }` with `pub const ALL: [Topic; 8]`, `pub fn parse(s: &str) -> Option<Topic>` (trim, case-insensitive), `pub fn slug(self) -> &'static str` ("SPORTS"), `pub fn title(self) -> &'static str` ("Sports"), `pub fn url(self) -> String`. Serde: serializes as the slug; deserializes via `parse`, with the error `unknown topic "X"; use one of WORLD, NATION, …`.

- [ ] **Step 1: Write failing tests**: `topic_parse_is_case_insensitive` (`parse(" sports ")` is `Some(Sports)`, `parse("golf")` is `None`); `topic_urls` (`Technology.url()` equals the spec URL with `TECHNOLOGY`; all eight differ); `topic_serde_round_trip` (a struct holding a `Topic` round-trips through `toml` as `"SPORTS"`; `"nonsense"` fails with a message listing all eight slugs).
- [ ] **Step 2:** Run `cargo test -p deckle-digest topic_ -- --nocapture`; expect compile failure/FAIL.
- [ ] **Step 3:** Implement `Topic` with `#[serde(try_from = "String", into = "String")]`.
- [ ] **Step 4:** Live-check the URL once: `curl -s -o /dev/null -w '%{http_code} %{content_type}\n' '<Technology URL>'` — expect `200` and an XML type. If not, find the current topic-feed form, fix `Topic::url` and the spec's URL line, and say so in the commit. This is the one task allowed to touch the network.
- [ ] **Step 5:** Run the tests; expect PASS. Commit `feat(digest): Google News topics`.

### Task 2: `FeedConfig` gains `topic`, `urls`, `full_text`

**Files:**
- Modify: `crates/deckle-digest/src/config.rs` (`FeedConfig`, `Config::parse`, `STARTER` comments), `apps/desktop/src/types.ts:143`
- Test: `config.rs` tests; `crates/deckle-digest/tests/build.rs` stays untouched and green

**Interfaces:**
- Consumes: `Topic` (Task 1).
- Produces:
  - `pub enum FullText { Yes, No, Lead }` (serde: `true`/`false`/`"lead"`).
  - `FeedConfig { name: String, #[serde(default)] url: String, #[serde(default, skip_serializing_if = "Vec::is_empty")] urls: Vec<String>, #[serde(default, skip_serializing_if = "Option::is_none")] topic: Option<Topic>, #[serde(default, skip_serializing_if = "Option::is_none")] full_text: Option<FullText>, max_items: usize }`.
  - `FeedConfig::sources(&self) -> Vec<String>`: topic URL, then `url` (if non-blank), then `urls`, deduplicated, in that order.
  - `FeedConfig::full(&self, global: bool) -> FullText`: the entry's value; else `Lead` for a topic entry; else `Yes`/`No` from `global`.
  - `Config::parse` fills `max_items == 0` (unset) with 5 for a topic entry, else 10, and fails `[[feeds]] "<name>": give a url, urls or topic` when `sources()` is empty.
  - TS: `FeedConfig { name: string; url: string; urls?: string[]; topic?: string; full_text?: boolean | "lead"; max_items: number }`.

- [ ] **Step 1: Write failing tests**: `old_feed_parses_unchanged` (url only: `sources() == [url]`, `max_items == 10`, `full(true) == Yes`); `topic_only_feed` (`max_items == 5`, `full(false) == Lead`, one source); `all_three_sources_in_order`; `no_source_is_an_error_naming_the_entry`; `full_text_values` (`true`, `false`, `"lead"` parse; `"sometimes"` errors); `save_round_trip_keeps_comments_and_new_fields` (a `[[feeds]]` entry with a comment, `topic`, `urls`, `full_text = "lead"` survives `save` unchanged apart from edits, following the existing `saving_keeps_comments_inside_feeds…` test).
- [ ] **Step 2:** Run `cargo test -p deckle-digest config::`; expect FAIL.
- [ ] **Step 3:** Implement the above. `default_feed_items()` returns 0 as the "unset" sentinel; normalization lives in `Config::parse`. Add a commented `# [[feeds]] name = "Sports" topic = "SPORTS"` example to `STARTER` without changing what `Config::parse(STARTER)` returns.
- [ ] **Step 4:** Run `cargo test -p deckle-digest` and `cargo clippy -p deckle-digest --all-targets -- -D warnings`; expect PASS (fix `f.url` uses in `next/plan.rs:295-299` and `next/live.rs:115` only as far as compilation needs: they keep using `f.url` until Task 7).
- [ ] **Step 5:** Commit `feat(digest): topic, urls and full_text on feeds`.

### Task 3: One section from many sources, top story whole

**Files:**
- Modify: `crates/deckle-digest/src/sources/feeds.rs` (`feed_section`, `section_of`, `key` → `pub(crate) fn title_key`)
- Create: `crates/deckle-digest/tests/sections.rs`

**Interfaces:**
- Consumes: `FeedConfig::sources/full`, `FullText`.
- Produces: `section_of(title, kicker, items, errors, cfg, net, mode: FullText) -> Section` (Local/places pass `Yes`/`No` from `cfg.full_text`, behavior unchanged); `pub fn feed_section(feed: &FeedConfig, cfg: &Config, now: DateTime<Utc>, net: &dyn Net, taken: &mut HashSet<String>) -> Section` (the `taken` set is used in Task 4; here it is only passed through).

- [ ] **Step 1: Write failing tests** in `tests/sections.rs` (copy `fake`/`fixture` from `tests/build.rs`, add a counting wrapper to record requested URLs):
  - `topic_feed_requests_the_topic_url` (topic `SPORTS` → exactly the Task 1 URL is requested).
  - `sources_are_merged_and_deduped_by_title` (topic + `urls` returning an overlapping title: one copy, newest first, cut to `max_items`).
  - `one_source_failing_keeps_the_others`; `all_failing_is_unavailable` (the section's `unavailable` is set, other sections unaffected).
  - `lead_mode_fetches_only_the_top_story_whole`: `full_text = "lead"`; article fetches limited to the first story (the rest are briefs: empty body but for the "Source:" line, `teaser == None`); the first story has a teaser.
  - `lead_mode_tries_the_next_story_when_the_first_cannot_be_read`: first story's page errors → second becomes the whole one; third attempt max, then all briefs.
  - `old_feed_behaviour_unchanged` (url-only entry builds as before).
- [ ] **Step 2:** Run `cargo test -p deckle-digest --test sections`; expect FAIL.
- [ ] **Step 3:** Rewrite `feed_section` over `feed.sources()`: Google topic items go through `as_briefs`, other feeds keep their text (as `local_news` does for its feeds); dedupe with `title_key`, sort newest first, drop items already in `taken` (inserted by Task 4; skip here), truncate to `max_items`. `section_of` with `Lead`: try `full_article` on items in order, up to 3, until one has no `UNAVAILABLE` body; every other item is `article(i, None)` with paragraphs cleared. Keep the `UNAVAILABLE` partition for `Yes`.
- [ ] **Step 4:** Run the whole crate's tests; expect PASS. Commit `feat(digest): topic sections merge sources and print the top story whole`.

### Task 4: Dedupe across news sections

**Files:**
- Modify: `crates/deckle-digest/src/lib.rs` (`gather`, news branches ~l.225-245), `sources/feeds.rs` (`feed_section` uses `taken`)
- Test: `crates/deckle-digest/tests/sections.rs`

**Interfaces:**
- Consumes: `title_key`, `feed_section(..., taken)`.
- Produces: `gather` keeps one `printed: HashSet<String>`; after Local and each place section is built it extends `printed` with the `title_key` of each article; each feed section receives `&mut printed` and inserts the keys it keeps. Reading is neither checked nor recorded.

- [ ] **Step 1: Write failing tests**: `shared_title_prints_once_in_the_first_section` (Local and a topic feed both carry "X"; only Local has it); `reordering_sections_moves_it` (feed first in `sections` → the feed keeps it); `dedupe_happens_before_the_cut` (topic with `max_items = 2` loses one story to Local and still prints 2 from its remaining items); `section_emptied_by_dedupe_says_no_new_stories` (`unavailable == Some("No new stories today.")`); `reading_is_never_deduped`.
- [ ] **Step 2:** Run; expect FAIL. **Step 3:** Implement. **Step 4:** Run the crate tests; expect PASS. Commit `feat(digest): one story prints once across sections`.

### Task 5: The front page

**Files:**
- Modify: `crates/deckle-digest/src/front.rs`, `crates/deckle-digest/src/render.rs` (`front_page`, `STYLE`)
- Test: `front.rs` tests; `crates/deckle-digest/tests/build.rs` (add one front-page assertion test)

**Interfaces:**
- Consumes: `candidates`, `lead`, `Edition::headlines`.
- Produces: `pub struct FrontPlan { pub lead: Option<(usize, usize)>, pub tiles: Vec<(usize, usize)> }` with `pub fn contains(&self, s: usize, a: usize) -> bool` (lead or tile) and `pub fn front_plan(g: &Gathered) -> FrontPlan` (lead by the existing rule; `tiles` = `g.edition.headlines(&g.news, 11)` minus the lead, first 10).
- `render.rs`: `front_page(g)` emits, after masthead and weather line: the lead (linked `<h2 class="lead" id="t<s>-<a>">`, meta line, teaser `<p>` when present); then, only when `tiles` is non-empty, `<table class="cols">` with two `<td>` per row, each holding `<p class="label">section</p><a id="t<s>-<a>" href="section<N>.xhtml#a<j>">headline</a>` and the meta line, padding the last row with one empty `<td>` when the count is odd; then the existing "Inside today" list. CSS for `table.cols`, `.label` (small caps), `.lead` appended to `STYLE`. All text through `esc`.

- [ ] **Step 1: Write failing tests**: `front_plan_excludes_lead_from_tiles`; `front_plan_caps_tiles_at_ten`; `front_plan_round_robins_sections` (first tiles are the tops of different sections); `front_plan_with_no_news_is_empty`; in `build.rs`: `front_page_has_lead_table_and_inside_index` (parse `OEBPS/front.xhtml` text: lead headline link first, `table class="cols"`, tile count equals plan, every tile `id` unique, "Inside today" present), `odd_tiles_pad_the_last_row`, `no_tiles_no_table`, `headlines_are_escaped` (title `A & B <c>` appears as `A &amp; B &lt;c&gt;`).
- [ ] **Step 2:** Run; expect FAIL. **Step 3:** Implement. **Step 4:** Run `cargo test -p deckle-digest`; expect PASS. Check the cover still uses the same lead (`cover_lead_matches_rule`). Commit `feat(digest): a front page laid out like the cover`.

### Task 6: Section fronts and back links

**Files:**
- Modify: `crates/deckle-digest/src/render.rs` (`section_page`, `render`, `STYLE`)
- Test: `crates/deckle-digest/tests/build.rs`

**Interfaces:**
- Consumes: `front::front_plan`.
- Produces: `section_page(e: &Edition, i: usize, plan: &FrontPlan) -> Vec<u8>`: `<h1 class="section-head" id="top">`; after the kicker, when `articles.len() >= 2`, `<ul class="index">` ("In this section") linking each headline to `#a<j+1>`; after each story body `<p class="back">` with "↩ <Section>" → `#top` and "↩ Front page" → `front.xhtml#t<i>-<j>` if `plan.contains(i, j)` else `front.xhtml`. `render` computes the plan once and passes it.

- [ ] **Step 1: Write failing tests** (in `build.rs`, reading section files from the built EPUB): `section_with_two_stories_has_headline_list`; `one_story_section_has_no_list`; `back_links_target_the_tile_when_on_the_front_page`; `back_links_fall_back_to_front_xhtml`; `unavailable_section_has_no_back_links`; `duplicate_titles_keep_anchors_unique`; `titles_are_escaped_in_the_list`.
- [ ] **Step 2:** Run; expect FAIL. **Step 3:** Implement. **Step 4:** Run the crate tests; expect PASS. Commit `feat(digest): section fronts and links back to the front page`.

### Task 7: Anchors and tables survive KFX

**Files:**
- Test: `crates/deckle-kfx/` (find the cheapest existing conversion test; add alongside)

**Interfaces:** Consumes the XHTML shape from Tasks 5–6 (`front.xhtml#t0-1`, `sectionN.xhtml#top`, a `<table class="cols">` with linked cells). Produces a test, or a written finding.

- [ ] **Step 1:** Write a test that converts a minimal EPUB with those links/table through the same path `deckle digest run` uses and asserts the produced KFX still contains the internal link targets (fragment anchors and table text).
- [ ] **Step 2:** Run it. If anchors or tables are dropped, **stop** and report to the owner with the failing output; do not work around it silently. If it passes, commit `test(kfx): front-page anchors and tables survive conversion`.

### Task 8: Tomorrow's paper knows topics

**Files:**
- Modify: `crates/deckle-digest/src/next/live.rs:113-117`, `crates/deckle-digest/src/next/plan.rs:290-310`
- Test: `crates/deckle-digest/tests/next.rs`

**Interfaces:**
- Consumes: `FeedConfig::sources`, `title_key`.
- Produces: live fetch requests every entry of `f.sources()` (one `LiveCache` entry per URL, same twice-an-hour limit); a feed row's stories merge its URLs' cached items, newest first, deduped by title, and drop titles already shown in an earlier news row (Local, places, earlier feeds); the row's detail is the first fetched feed title or the topic's `title()`.

- [ ] **Step 1: Write failing tests**: `topic_feed_is_fetched_by_its_topic_url`; `multi_source_row_merges_and_dedupes`; `row_drops_a_title_an_earlier_row_has` (the lead rule still agrees with the press for the same data); `url_only_feed_row_unchanged`.
- [ ] **Step 2:** Run `cargo test -p deckle-digest --test next`; expect FAIL. **Step 3:** Implement. **Step 4:** Run the crate tests; expect PASS. Commit `feat(digest): tomorrow's paper shows topic sections`.

### Task 9: The Topics picker

**Files:**
- Create: `apps/desktop/src/topics.ts`, `apps/desktop/src/topics.test.ts`
- Modify: `apps/desktop/src/DailyView.tsx` (near "Your feeds", ~l.576), `apps/desktop/src/types.ts`

**Interfaces:**
- Produces in `topics.ts`: `export const TOPICS: { slug: string; title: string }[]` (the eight, in spec order); `export function topicOn(feeds: FeedConfig[], slug: string): boolean` (a feed with that `topic`, case-insensitive); `export function toggleTopic(feeds: FeedConfig[], slug: string): FeedConfig[]`: off→on appends `{ name, url: "", topic: slug, max_items: 5 }` where `name` is the title, or `"<Title> (topic)"` if any feed already has that name; on→off removes only feeds whose `topic` matches, never by name; returns a new array.

- [ ] **Step 1: Write failing vitest tests**: `ticking_adds_a_topic_feed`; `unticking_removes_only_topic_feeds`; `a_handwritten_feed_with_the_same_name_survives_unticking`; `name_clash_gets_the_topic_suffix`; `url_feed_with_no_topic_is_untouched`; `case_insensitive_topic_match`.
- [ ] **Step 2:** Run `npm test -- topics` in `apps/desktop`; expect FAIL. **Step 3:** Implement `topics.ts`; in `DailyView.tsx` add a "Topics" row of eight toggles above "Your feeds", wired through the existing `edit((c) => …)` draft mechanism, and make the feed editor's URL field optional for topic entries (show "topic: Sports" instead of the URL box). **Step 4:** Run `npm test` and `npm run build` (typecheck); expect PASS. Run the app and check the picker adds/removes rows in the page plan, saving writes `topic = "SPORTS"` into `digest.toml` keeping comments (screenshot via the built-in browser if the app runs in a browser, else report not visually verified). Commit `feat(desktop): Topics picker in the Daily tab`.

### Task 10: Docs, changelog, final checks

**Files:**
- Modify: `docs/daily.md`, `CHANGELOG.md`, `docs/devices.md`

- [ ] **Step 1:** `docs/daily.md`: topics, `urls`, `full_text = "lead"`, dedupe rule, section fronts, back links, the quirk that jumping from the front page counts earlier sections as reached. `CHANGELOG.md` under Unreleased: one bullet. `docs/devices.md`: a pending row "Front page + topic sections test edition (Scribe 2024)" marked not yet checked.
- [ ] **Step 2:** Run `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, and `npm test` in `apps/desktop`; expect all green.
- [ ] **Step 3:** Build a sample edition from fixtures (`deckle digest run` against the fake network used in tests, or `cargo run -p deckle-cli -- digest run --preview` if that works offline) and open `front.xhtml` in the browser pane; screenshot the front page and one section page. Report what you saw; the e-ink look is not verifiable here.
- [ ] **Step 4:** Commit `docs: newspaper sections`; push the branch; open a PR with `Closes #` for any related issue (ask the owner for the number) and bind it with the ccd_pr tools.
