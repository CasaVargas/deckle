# Interests, increment 1: Interest Sections Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `[[interests]]` entries in a paper's TOML, each its own section built from a Google News search per interest (plus `also` searches and `feeds`), filtered for relevance, lead in full and the rest briefs, with a row in the page plan, readership, an Interests card on the Daily tab, a CLI, and docs.

**Architecture:** `SectionKind::Interests` stands for every `[[interests]]` entry the way `Feeds` stands for `[[feeds]]`; keys are `interests:<name>`. A new `sources/interests.rs` builds one section per entry on top of a merge-and-dedupe helper lifted out of `local_news`, applies the title relevance filter and the `not` list, and re-checks the fetched lead. `gather_with_editor` gains an `Interests` branch that leaves a quiet section out with a runner note. Readership, the page plan, the editor's section filter, the bundle, the desktop types and the CLI each learn the new kind.

**Tech Stack:** Rust 2024 (`crates/deckle-digest`, `deckle-readership`, `deckle-cli`; `toml`/`toml_edit`, `feed_rs`, `regex`, `clap`, `assert_cmd`), Tauri 2 + React/TypeScript + vitest (`apps/desktop`).

**Spec:** `docs/superpowers/specs/2026-10-25-interests-design.md` (§3, §4, §5.4 Interests card only, §5.5 list/add/remove, §8, §10, §11 item 1). Learning (§5), the editor (§6) and nudges (§7) are later increments; this plan only parses and saves `[learning]` so files written now keep working.

## Global Constraints

- Everything is additive: a `digest.toml` or paper file without the new keys parses, builds and saves exactly as before; existing tests stay green unchanged.
- Config shape (spec §3): `[[interests]]` with `name` (required, unique case-insensitively per paper), `query` (default: the name in double quotes), `also: Vec<String>`, `not: Vec<String>`, `feeds: Vec<String>`, `max_items` (default 5), `full_text` (default `"lead"`), `max_age_hours` (default 72). `[learning]` with `enabled` (default true), `window_days` (14), `min_stories` (3), `nudges` (false); left out of a save at its defaults.
- Key: `interests:<name>`. Kicker: `Your interests`. Section title: the name.
- Search URL: `google_news_url(query, max_age_hours)` for `query` and each `also`; search items go through `as_briefs`; feed items keep their descriptions.
- Relevance (spec §4.3): a story stays only if its title or a feed item's description contains `name` or an `also` phrase, case-insensitive, at word boundaries (a one-word phrase matches a whole word only), and contains no `not` phrase. The fetched lead is re-checked on the first 600 characters of its plain text; a failing lead is dropped and the next tried, at most 3 fetches (`LEAD_TRIES`).
- A quiet day (zero relevant stories, no source error) leaves the section out of the edition and adds the note `Nothing new about <name> today.`; every source failing prints "Unavailable today" as feeds do; `[[interests]]` present but `"interests"` missing from `sections` adds the note `You have interests but "interests" isn't in sections`.
- Starter: `"interests"` goes after `"local"` and before `"feeds"` in `sections`; the `[[interests]]` block is commented in `STARTER` and `PAPER_STARTER`.
- Interest sections are news sections: pushed into `Gathered.news`, dedupe through `printed`/`taken` like `feed_section`, editor section filter includes `interests:`.
- Desktop: the app is main-paper-only for this increment (spec call 9); the Tauri commands keep using `daily_save` (the whole config) so no new command is needed for the card.
- Tests offline only (fake `Net`), no personal data in fixtures. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm -C apps/desktop build && pnpm -C apps/desktop test` all pass before the PR. Commit per task on this branch (`claude/daily-interests-feature-c76a22`); never push to `main`.

## Review Focus

- A `not` phrase that is a prefix of a wanted word ("Rust" vs "Rust Belt", "trust"): boundaries must cut the right way in both the keep and the drop direction. (Task 2)
- An interest whose `query` carries Google operators (`"Rust" programming OR "Rust language" -game`): the URL still encodes and `when:` is still appended once. (Task 2)
- Two interests sharing a story, or an interest sharing one with Local: it prints once, in the first section in paper order, and the later section refills to its budget. (Task 3)
- A paper file (`papers/<slug>.toml`) with interests saved from the CLI: the household keys don't leak in and the comments stay. (Tasks 1, 5)
- The Daily tab's Interests card with a freshly added blank row: the config still saves and loads; the section prints "Unavailable today (give a name)" rather than failing the build. (Tasks 1, 3, 6)

---

### Task 1: Config: `[[interests]]`, `[learning]`, the kind and its key

**Files:**
- Modify: `crates/deckle-digest/src/config.rs` (`SectionKind`, `section_key`, `Config`, `normalize`, `parse`, `STARTER`, `PAPER_STARTER`, tests)
- Modify: `crates/deckle-digest/src/bundle.rs:231-245` (`kind_name`)
- Test: `crates/deckle-digest/src/config.rs` (unit tests module)

**Interfaces:**
- Produces:
  ```rust
  pub enum SectionKind { …, Interests, … }            // serde "interests"
  pub fn section_key(SectionKind::Interests, Some(name)) -> "interests:<name>"
  #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
  #[serde(deny_unknown_fields)]
  pub struct InterestConfig {
      pub name: String,
      #[serde(default, skip_serializing_if = "String::is_empty")] pub query: String,
      #[serde(default, skip_serializing_if = "Vec::is_empty")] pub also: Vec<String>,
      #[serde(default, skip_serializing_if = "Vec::is_empty")] pub not: Vec<String>,
      #[serde(default, skip_serializing_if = "Vec::is_empty")] pub feeds: Vec<String>,
      #[serde(default)] pub max_items: usize,        // 0 = unset; normalize → 5
      #[serde(default, skip_serializing_if = "Option::is_none")] pub full_text: Option<FullText>,
      #[serde(default)] pub max_age_hours: u32,      // 0 = unset; normalize → 72
  }
  impl InterestConfig {
      pub fn search_query(&self) -> String;           // query, or "\"<name>\"" when empty
      pub fn searches(&self) -> Vec<String>;          // search_query() then each `also`, trimmed, no repeats, no empties
      pub fn full(&self) -> FullText;                 // full_text.unwrap_or(FullText::Lead)
      pub fn phrases(&self) -> Vec<&str>;             // name then `also`, for relevance
  }
  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] #[serde(default, deny_unknown_fields)]
  pub struct LearningConfig { pub enabled: bool, pub window_days: u32, pub min_stories: u32, pub nudges: bool }
  impl LearningConfig { pub fn is_default(&self) -> bool }   // Default: true, 14, 3, false
  pub struct Config { …, #[serde(default, skip_serializing_if = "Vec::is_empty")] pub interests: Vec<InterestConfig>,
                      #[serde(default, skip_serializing_if = "LearningConfig::is_default")] pub learning: LearningConfig, … }
  ```
- `Config::parse` errors with `duplicate interest "<a>" and "<b>"` when two names match case-insensitively (after trim).

- [ ] **Step 1: Write the failing tests** in `config.rs`'s test module:

```rust
#[test]
fn interests_parse_with_defaults_and_key() {
    let c = Config::parse("sections = [\"interests\"]\n[[interests]]\nname = \"Rust\"\n[[interests]]\nname = \"Carolina Panthers\"\nquery = \"\\\"Carolina Panthers\\\" NFL\"\nalso = [\"Bryce Young\"]\nnot = [\"Florida Panthers\"]\nmax_items = 3\nfull_text = false\nmax_age_hours = 24\n").unwrap();
    assert_eq!(c.sections, [SectionKind::Interests]);
    let r = &c.interests[0];
    assert_eq!(r.search_query(), "\"Rust\"");
    assert_eq!(r.searches(), vec!["\"Rust\"".to_owned()]);
    assert_eq!((r.max_items, r.max_age_hours, r.full()), (5, 72, FullText::Lead));
    let p = &c.interests[1];
    assert_eq!(p.searches(), vec!["\"Carolina Panthers\" NFL".to_owned(), "Bryce Young".to_owned()]);
    assert_eq!(p.phrases(), vec!["Carolina Panthers", "Bryce Young"]);
    assert_eq!((p.max_items, p.max_age_hours, p.full()), (3, 24, FullText::No));
    assert_eq!(section_key(SectionKind::Interests, Some("Rust")), "interests:Rust");
    assert!(c.learning.is_default() && c.learning.enabled);
}

#[test]
fn duplicate_interest_names_are_an_error() {
    let e = Config::parse("[[interests]]\nname = \"Rust\"\n[[interests]]\nname = \" rust \"\n").unwrap_err();
    assert!(e.contains("duplicate interest"), "{e}");
}

#[test]
fn learning_is_left_out_of_a_save_at_defaults_and_kept_when_set() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("digest.toml");
    std::fs::write(&path, "title = \"T\"\n").unwrap();
    let mut cfg = Config::load(&path).unwrap();
    save(&path, &cfg).unwrap();
    assert!(!std::fs::read_to_string(&path).unwrap().contains("[learning]"));
    cfg.learning.nudges = true;
    save(&path, &cfg).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("[learning]") && text.contains("nudges = true"), "{text}");
    assert_eq!(Config::load(&path).unwrap().learning, cfg.learning);
}

#[test]
fn interests_save_round_trip_keeps_comments() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("digest.toml");
    std::fs::write(&path, "sections = [\"interests\"]\n\n[[interests]]\n# the team\nname = \"Carolina Panthers\" # go cats\nnot = [\"Florida Panthers\"]\n").unwrap();
    let mut cfg = Config::load(&path).unwrap();
    cfg.interests.push(InterestConfig { name: "Rust".into(), ..Default::default() });
    save(&path, &cfg).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("# the team") && text.contains("# go cats") && text.contains("name = \"Rust\""), "{text}");
    assert!(!text.contains("query ="), "defaults stay out:\n{text}");
    assert_eq!(Config::load(&path).unwrap().interests, cfg.interests);
}

#[test]
fn starters_interest_block_parses_once_uncommented() {
    for starter in [STARTER, PAPER_STARTER] {
        let text: String = starter.lines().map(|l| {
            if l.starts_with("# [[interests]]") || l.starts_with("# name = \"Carolina") || l.starts_with("# not = ") { l.trim_start_matches("# ").to_owned() } else { l.to_owned() }
        }).collect::<Vec<_>>().join("\n");
        let c = Config::parse(&text).unwrap();
        assert_eq!(c.interests.len(), 1, "{text}");
        assert_eq!(c.interests[0].name, "Carolina Panthers");
    }
    let c = Config::parse(STARTER).unwrap();
    let i = c.sections.iter().position(|k| *k == SectionKind::Interests).unwrap();
    assert_eq!(c.sections[i - 1], SectionKind::Local);
    assert_eq!(c.sections[i + 1], SectionKind::Feeds);
}
```

Update `starter_parses` to expect 10 sections, and `section_keys` (the existing test at ~1417) to also assert the interests key.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p deckle-digest --lib config:: 2>&1 | tail -20`
Expected: compile errors on `SectionKind::Interests`, `InterestConfig`, `learning`.

- [ ] **Step 3: Implement.** Add the variant (between `Local` and `Feeds` in the enum, doc comment "Every `[[interests]]` entry, each its own section, in file order (interests design §3)"), the `section_key` arm, the two structs, the `Config` fields (with `Default` updated), `normalize` filling `max_items` 0→5 and `max_age_hours` 0→72, the duplicate check in `parse` after `normalize` (compare `name.trim().to_lowercase()`), `kind_name` in `bundle.rs` returning `"interests"`. Starter block, placed right before the `# Each [[feeds]] entry` comment in `STARTER` and after the `sections` line's neighbours in `PAPER_STARTER`:

```toml
# Things you care about, each its own section: a Google News search per interest, filtered
# for relevance. `deckle digest interests add "Rust"` adds one from the terminal.
# [[interests]]
# name = "Carolina Panthers"      # the section's title; the search is the name in quotes
# query = "\"Carolina Panthers\" NFL"   # your own search instead; Google's OR and -word work
# also = ["Bryce Young"]          # more searches, merged; a story naming one is relevant
# not = ["Florida Panthers"]      # a story whose headline has one of these is dropped
# feeds = []                      # RSS feeds merged in
# max_items = 5
# full_text = "lead"              # top story whole, the rest briefs; true or false
# max_age_hours = 72

# What the paper notices about your reading, on this Mac only (suggestions on the Daily tab).
# [learning]
# enabled = true
# nudges = false                  # the suggestions as tappable lines in the paper itself
```

and `sections = ["reading", "weather", "local", "interests", "feeds", "shelf", …]` in `STARTER`; `PAPER_STARTER`'s `sections` gains `"interests"` after `"local"` too. Fix every `match` on `SectionKind` the compiler flags in `crates/deckle-digest` and `crates/deckle-readership` with the minimum that compiles (the real behaviour comes in Tasks 3 and 4): `lib.rs` gather `SectionKind::Interests => {}` for now, `next/plan.rs` and `report.rs` `KINDS` arrays gain the variant, `paper_name` strips `interests:` like `feeds:`.

- [ ] **Step 4: Run the config tests**

Run: `cargo test -p deckle-digest --lib config:: 2>&1 | tail -20`
Expected: all pass, including the updated `starter_parses`.

- [ ] **Step 5: Whole-workspace compile check**

Run: `cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -20`
Expected: clean (non-exhaustive matches fixed in step 3).

- [ ] **Step 6: Commit**

```bash
git add crates/deckle-digest/src/config.rs crates/deckle-digest/src/bundle.rs crates/deckle-digest/src/lib.rs crates/deckle-digest/src/next/plan.rs crates/deckle-readership/src/report.rs
git commit -m "feat(digest): [[interests]] and [learning] config, the interests section kind"
```

---

### Task 2: Relevance and the shared fetch-and-merge helper

**Files:**
- Modify: `crates/deckle-digest/src/sources/feeds.rs` (`local_news` → calls the helper; `section_of`, `LEAD_TRIES`, `UNAVAILABLE`, `teaser` become `pub(crate)`)
- Create: `crates/deckle-digest/src/sources/interests.rs`
- Modify: `crates/deckle-digest/src/sources/mod.rs` (`pub mod interests;`)
- Test: unit tests in both files

**Interfaces:**
- Produces, in `feeds.rs`:
  ```rust
  /// Google News searches (as briefs) and feeds, fetched, merged, deduplicated by title, newest first. Errors are "Google News: …" or "<host>: …".
  pub(crate) fn fetch_merged(searches: &[String], feeds: &[String], max_age_hours: u32, now: DateTime<Utc>, net: &dyn Net) -> (Vec<Item>, Vec<String>);
  ```
  `local_news` uses it (same output as before: the existing `sections.rs`/`build.rs` tests prove it). Feed items get `source = source.or(feed title)` as today.
- Produces, in `interests.rs`:
  ```rust
  /// `text` contains `phrase` case-insensitively at word boundaries (Unicode alphanumerics on either side break a match).
  pub fn contains_phrase(text: &str, phrase: &str) -> bool;
  /// Title (and description) name the interest and none of its `not` phrases.
  pub fn relevant(interest: &InterestConfig, title: &str, description: &str) -> bool;
  ```

- [ ] **Step 1: Write the failing tests** in `interests.rs`:

```rust
#[test]
fn phrases_match_at_word_boundaries() {
    assert!(contains_phrase("Why Rust is winning", "rust"));
    assert!(!contains_phrase("Trust me", "Rust"));
    assert!(!contains_phrase("Rusty nails", "Rust"));
    assert!(contains_phrase("Carolina Panthers win", "carolina panthers"));
    assert!(contains_phrase("e-ink readers compared", "e-ink"));
    assert!(contains_phrase("Panthers' QB", "Panthers"));
}

#[test]
fn relevance_needs_a_phrase_and_no_not() {
    let i = InterestConfig { name: "Rust".into(), also: vec!["Cargo".into()], not: vec!["Rust Belt".into(), "corrosion".into()], ..Default::default() };
    assert!(relevant(&i, "Rust 2.0 lands", ""));
    assert!(relevant(&i, "Cargo gets workspaces", ""));
    assert!(relevant(&i, "New release", "Rust 2.0 is here"));          // description counts
    assert!(!relevant(&i, "Rust Belt voters", ""));
    assert!(!relevant(&i, "Trusted sources", ""));
    assert!(!relevant(&i, "Rust", "corrosion study"));                   // not in description
}
```

and in `feeds.rs`' test module:

```rust
#[test]
fn fetch_merged_dedupes_and_names_feed_sources() {
    struct Two;
    impl Net for Two {
        fn get(&self, url: &str) -> Result<Vec<u8>, String> {
            if url.contains("news.google.com") {
                Ok(b"<rss version=\"2.0\"><channel><title>G</title><item><title>Alpha - WXII</title><link>https://x/a</link><pubDate>Tue, 20 Oct 2026 10:00:00 +0000</pubDate></item></channel></rss>".to_vec())
            } else if url == "https://f.example/rss" {
                Ok(b"<rss version=\"2.0\"><channel><title>The Feed</title><item><title>Alpha!</title><link>https://x/a2</link><pubDate>Tue, 20 Oct 2026 09:00:00 +0000</pubDate></item><item><title>Beta</title><link>https://x/b</link><pubDate>Tue, 20 Oct 2026 11:00:00 +0000</pubDate></item></channel></rss>".to_vec())
            } else { Err("offline".into()) }
        }
    }
    let now = "2026-10-20T22:00:00Z".parse().unwrap();
    let (items, errors) = fetch_merged(&["Alpha".into()], &["https://f.example/rss".into(), "https://bad.example/rss".into()], 72, now, &Two);
    assert_eq!(items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(), ["Beta", "Alpha"]);
    assert_eq!(items[1].source.as_deref(), Some("WXII"));
    assert_eq!(items[0].source.as_deref(), Some("The Feed"));
    assert_eq!(errors, ["bad.example: offline"]);
}
```

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p deckle-digest --lib sources:: 2>&1 | tail -20`
Expected: FAIL to compile (`fetch_merged`, `contains_phrase`, `relevant` missing).

- [ ] **Step 3: Implement.** `fetch_merged` is the body of `local_news` from its first line to `items.sort_by_key(...)` (search loop, feed loop, dedupe by `title_key`, newest first), with `errors` for feeds as `format!("{}: {e}", host(url))`. `local_news` becomes: `fetch_merged(&searches, feeds, cfg.local.max_age_hours, now, net)`, truncate, mode, `section_of`. `contains_phrase`: lowercase both (`to_lowercase`), find every occurrence, accept one whose neighbours are not `char::is_alphanumeric`. `relevant`: `interest.phrases().iter().any(|p| contains_phrase(title, p) || contains_phrase(description, p)) && !interest.not.iter().any(|n| contains_phrase(title, n) || contains_phrase(description, n))`; empty `not` entries are skipped.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p deckle-digest 2>&1 | tail -20`
Expected: all pass, including the existing `sections.rs` and `build.rs` Local tests.

- [ ] **Step 5: Commit**

```bash
git add crates/deckle-digest/src/sources
git commit -m "feat(digest): relevance matching and a shared search+feeds fetch for interests"
```

---

### Task 3: `interest_section` and the gather branch

**Files:**
- Modify: `crates/deckle-digest/src/sources/interests.rs`
- Modify: `crates/deckle-digest/src/lib.rs:460-560` (gather loop, key loop)
- Modify: `crates/deckle-digest/src/editor.rs:116` (section filter)
- Test: `crates/deckle-digest/tests/interests.rs` (new; copy the `Fake`, `rss`, `fixture`, `now`, `config` helpers from `tests/sections.rs`)

**Interfaces:**
- Produces:
  ```rust
  /// One `[[interests]]` entry as its own section, keyed and kickered. `None` when nothing relevant was found and no source failed (a quiet day).
  pub fn interest_section(interest: &InterestConfig, cfg: &Config, now: DateTime<Utc>, net: &dyn Net, taken: &mut HashSet<String>) -> Option<Section>;
  pub const KICKER: &str = "Your interests";
  pub fn quiet_note(name: &str) -> String;      // "Nothing new about <name> today."
  pub const NOT_IN_SECTIONS: &str = "You have interests but \"interests\" isn't in sections";
  ```
- Consumes: `fetch_merged`, `relevant`, `section_of`, `full_article`, `article`, `as_briefs`, `title_key`, `LEAD_TRIES`, `UNAVAILABLE`, `super::unavailable` (Task 2).

- [ ] **Step 1: Write the failing integration tests** in `tests/interests.rs`. `search(q)` builds the URL with `google_news_url(q, 72)`; `rust()` is `InterestConfig { name: "Rust", not: ["Rust Belt"], ..Default }` normalised (`max_items` 5, `max_age_hours` 72):

```rust
#[test]
fn an_interest_searches_its_name_and_also_and_keeps_only_relevant_stories() {
    let net = Fake::new(vec![
        (search("\"Rust\""), rss(&[("Rust 2.0 lands - Outlet", "https://ok.example/1", 10), ("Trust funds - Outlet", "https://ok.example/2", 9), ("Rust Belt voters - Outlet", "https://ok.example/3", 8)])),
        (search("Cargo"), rss(&[("Cargo gets workspaces - Outlet", "https://ok.example/4", 7)])),
    ]);
    let mut i = rust(); i.also = vec!["Cargo".into()];
    let s = interest_section(&i, &config(), now(), &net, &mut HashSet::new()).unwrap();
    assert_eq!(s.key, "interests:Rust");
    assert_eq!(s.title, "Rust");
    assert_eq!(s.kicker.as_deref(), Some("Your interests"));
    assert_eq!(s.articles.iter().map(|a| a.title.as_str()).collect::<Vec<_>>(), ["Rust 2.0 lands", "Cargo gets workspaces"]);
    assert!(net.seen().contains(&search("\"Rust\"")) && net.seen().contains(&search("Cargo")));
}

#[test]
fn lead_is_fetched_whole_and_the_rest_are_briefs() {
    let net = Fake::new(vec![(search("\"Rust\""), rss(&[("Rust 2.0 lands - Outlet", "https://ok.example/1", 10), ("Rust jobs grow - Outlet", "https://ok.example/2", 9)]))]);
    let s = interest_section(&rust(), &config(), now(), &net, &mut HashSet::new()).unwrap();
    assert_eq!(net.pages(), ["https://ok.example/1"]);
    assert!(s.articles[0].teaser.is_some());
    assert!(is_brief(&s.articles[1]));
}

#[test]
fn a_lead_whose_text_hits_not_is_dropped_and_the_next_fetched() {
    // the WXII fixture's text mentions neither phrase; a custom page for /1 does
    let net = Fake::with_page("https://bad-lead.example/1", "<html><body><h1>Rust 2.0</h1><p>From the Rust Belt, a story about steel and corrosion that runs long enough to be a paragraph of eighty characters.</p></body></html>",
        vec![(search("\"Rust\""), rss(&[("Rust 2.0 - Outlet", "https://bad-lead.example/1", 10), ("Rust jobs grow - Outlet", "https://ok.example/2", 9)]))]);
    let s = interest_section(&rust(), &config(), now(), &net, &mut HashSet::new()).unwrap();
    assert_eq!(s.articles.iter().map(|a| a.title.as_str()).collect::<Vec<_>>(), ["Rust jobs grow"]);
    assert_eq!(net.pages(), ["https://bad-lead.example/1", "https://ok.example/2"]);
}

#[test]
fn a_quiet_day_is_none_and_all_sources_failing_is_unavailable() {
    let net = Fake::new(vec![(search("\"Rust\""), rss(&[("Trust funds - Outlet", "https://ok.example/2", 9)]))]);
    assert!(interest_section(&rust(), &config(), now(), &net, &mut HashSet::new()).is_none());
    let s = interest_section(&rust(), &config(), now(), &Fake::new(vec![]), &mut HashSet::new()).unwrap();
    assert!(s.unavailable.as_deref().unwrap().contains("Google News"), "{:?}", s.unavailable);
}

#[test]
fn titles_taken_by_an_earlier_section_are_skipped_and_kept_ones_recorded() {
    let net = Fake::new(vec![(search("\"Rust\""), rss(&[("Rust 2.0 lands - Outlet", "https://ok.example/1", 10), ("Rust jobs grow - Outlet", "https://ok.example/2", 9)]))]);
    let mut taken = HashSet::from([title_key("Rust 2.0 lands")]);
    let mut i = rust(); i.full_text = Some(FullText::No);
    let s = interest_section(&i, &config(), now(), &net, &mut taken).unwrap();
    assert_eq!(s.articles.len(), 1);
    assert!(taken.contains(&title_key("Rust jobs grow")));
}

#[test]
fn gather_places_interest_sections_where_the_kind_sits_and_notes_quiet_ones() {
    let mut cfg = config();
    cfg.sections = vec![SectionKind::Interests, SectionKind::Horoscope];
    cfg.interests = vec![rust(), InterestConfig { name: "Quiet".into(), max_items: 5, max_age_hours: 72, ..Default::default() }];
    let net = Fake::new(vec![(search("\"Rust\""), rss(&[("Rust 2.0 lands - Outlet", "https://ok.example/1", 10)])), (search("\"Quiet\""), rss(&[]))]);
    let g = deckle_digest::gather(&cfg, "2026-10-20".parse().unwrap(), now(), &net);
    assert_eq!(g.edition.sections.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), ["interests:Rust", "horoscope"]);
    assert_eq!(g.news, [0]);
    assert!(g.notes.iter().any(|n| n == "Nothing new about Quiet today."), "{:?}", g.notes);
}

#[test]
fn interests_without_the_kind_in_sections_are_noted_not_printed() {
    let mut cfg = config();
    cfg.sections = vec![SectionKind::Horoscope];
    cfg.interests = vec![rust()];
    let g = deckle_digest::gather(&cfg, "2026-10-20".parse().unwrap(), now(), &Fake::new(vec![]));
    assert_eq!(g.edition.sections.len(), 1);
    assert!(g.notes.iter().any(|n| n.contains("isn't in sections")), "{:?}", g.notes);
}

#[test]
fn a_blank_interest_prints_unavailable_with_a_hint() {
    let i = InterestConfig { name: "".into(), max_items: 5, max_age_hours: 72, ..Default::default() };
    let s = interest_section(&i, &config(), now(), &Fake::new(vec![]), &mut HashSet::new()).unwrap();
    assert!(s.unavailable.as_deref().unwrap().contains("give a name"));
}
```

`Fake::with_page(url, html, feeds)` extends the copied `Fake` with one extra `(url, html)` page served before the `ok.example` rule. `search(q)` must produce the same string the gatherer requests: `google_news_url(q, 72)`.

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p deckle-digest --test interests 2>&1 | tail -20`
Expected: FAIL to compile (`interest_section` missing).

- [ ] **Step 3: Implement `interest_section`.** Blank name → `super::unavailable("Interest", "give a name")` with key `interests:`. Otherwise: `fetch_merged(&interest.searches(), &interest.feeds, interest.max_age_hours, now, net)`; `items.retain(|i| relevant(interest, &i.title, &i.paragraphs.join(" ")))`; `items.retain(|i| !taken.contains(&title_key(&i.title)))`; if `items.is_empty() && errors.is_empty()` return `None`; if `items.is_empty()` return the unavailable section (keyed, kickered). For `FullText::Lead`: loop the first `LEAD_TRIES` items, `full_article`; skip on `UNAVAILABLE`; drop the item (remove from `items`) when `!relevant(interest, &a.title, &plain_600(&a.body))` where `plain_600` strips tags from the body and takes the first 600 chars (reuse `crate::article` helpers if one strips tags, else a small regex); then whole + briefs as `section_of` does (or extend `section_of` with a `lead_ok: &dyn Fn(&Article) -> bool` parameter and pass `|_| true` from the feed callers; your choice, say which in the commit). Other modes: `section_of`. Truncate to `max_items` before fetching; record kept titles into `taken`. Set `key = section_key(SectionKind::Interests, Some(&interest.name))`, `kicker = Some(KICKER.into())`.

- [ ] **Step 4: Wire `gather_with_editor`.** Before the kinds loop: if `!cfg.interests.is_empty() && !kinds.contains(&SectionKind::Interests)`, push `NOT_IN_SECTIONS`. In the loop: `SectionKind::Interests => for interest in &cfg.interests { match interest_section(interest, cfg, now, net, &mut printed) { Some(s) => { news.push(edition.sections.len()); edition.sections.push(s) } None => notes.push(quiet_note(&interest.name)) } }`. In the key-assignment loop after the match, leave sections whose key is already set alone (`if s.key.is_empty()`), so the omitted-section index shift can't mislabel anything. In `editor.rs` the section filter adds `|| section.key.starts_with("interests:")`.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p deckle-digest 2>&1 | tail -20`
Expected: all pass, including `tests/editor.rs` (the filter change must not alter its expectations; if an editor test asserts the exact prompt sections, interests weren't in those configs).

- [ ] **Step 6: Commit**

```bash
git add crates/deckle-digest/src/sources/interests.rs crates/deckle-digest/src/lib.rs crates/deckle-digest/src/editor.rs crates/deckle-digest/tests/interests.rs
git commit -m "feat(digest): interest sections — one hunt per [[interests]] entry, quiet days left out"
```

---

### Task 4: Readership rows, suggestions and the page plan know interests

**Files:**
- Modify: `crates/deckle-readership/src/report.rs` (`current_keys`, `paper_name`, MoveUp branch, `apply`)
- Modify: `crates/deckle-digest/src/next/plan.rs` (`PlanRow.interest`, `rows` Interests branch, `taken` pass, lead candidates)
- Modify: `crates/deckle-digest/src/next/live.rs:73-110` (`sources` adds interests)
- Test: `crates/deckle-readership/tests/report.rs`, `crates/deckle-digest/tests/next.rs`

**Interfaces:**
- Produces: `PlanRow { …, pub interest: Option<usize>, … }` (serde camelCase like the rest: `interest`), set for interest rows, `None` elsewhere. Interest rows: `key = interests:<name>`, `kind = Interests`, `detail` = the search query (`search_query()`), plus `" + N feeds"` when it has feeds, `budget = "<max> stories"`, `items = Some(max_items)`; `stories` from the live cache for each `searches()` URL and feed, filtered with `relevant`, merged like the Feeds branch.
- `apply(Drop, "interests:<name>")` removes that entry from `cfg.interests`; MoveUp suggestions are offered for `interests:` keys as for `feeds:`.

- [ ] **Step 1: Write the failing tests.** In `crates/deckle-readership/tests/report.rs` (mirror the existing feed drop/move tests):

```rust
#[test]
fn interest_sections_get_rows_and_drop_removes_the_entry() {
    // a config with sections = ["interests"], interests = [Rust, Cargo]; a state whose layouts
    // list "interests:Rust" and "interests:Cargo" (use the file's existing layout builder)
    let r = report(&state, &cfg);
    assert_eq!(r.sections.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(), ["interests:Rust", "interests:Cargo"]);
    assert_eq!(r.sections[1].name, "Cargo");
    let s = Suggestion { id: "drop:interests:Cargo".into(), kind: SuggestionKind::Drop, key: "interests:Cargo".into(), title: String::new(), why: String::new() };
    let mut cfg2 = cfg.clone();
    apply(&s, &mut cfg2).unwrap();
    assert_eq!(cfg2.interests.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["Rust"]);
}
```

In `crates/deckle-digest/tests/next.rs` (mirror the feed-row test there):

```rust
#[test]
fn interest_rows_show_the_search_and_filter_the_cache_for_relevance() {
    // cfg: sections [local, interests]; interests = [Rust (not: Rust Belt)]; live cache holding
    // google_news_url("\"Rust\"", 72) → ["Rust 2.0 lands", "Trust funds", "Rust Belt voters"]
    let p = next_paper(...);
    let row = p.plan.iter().find(|r| r.key == "interests:Rust").unwrap();
    assert_eq!(row.kind, SectionKind::Interests);
    assert_eq!(row.interest, Some(0));
    assert_eq!(row.detail, "\"Rust\"");
    assert_eq!(row.items, Some(5));
    // one relevant story planned
    assert_eq!(row.budget, "5 stories");
    assert_eq!(planned_for(&p, "interests:Rust"), 1);   // use whatever the file already exposes for planned counts
}
```

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p deckle-readership --test report 2>&1 | tail -8; cargo test -p deckle-digest --test next 2>&1 | tail -8`
Expected: FAIL (`interest` field / wrong rows / apply error).

- [ ] **Step 3: Implement.** `report.rs`: `current_keys` adds `SectionKind::Interests => cfg.interests.iter().map(|i| section_key(*k, Some(&i.name))).collect()`; the MoveUp `if row.key.starts_with("feeds:")` becomes `starts_with("feeds:") || starts_with("interests:")`; `apply` adds a `Drop` arm for `interests:`. `live.rs` `sources`: for each interest, each `searches()` query → `Source { url: google_news_url(q, i.max_age_hours), max_age_hours: i.max_age_hours, max_items: i.max_items, briefs: true }` and each feed `briefs: false`. `plan.rs`: add `interest: Option<usize>` to `PlanRow` (and `None` in both constructors), an `Interests` branch in `rows` shaped like `Feeds` with `relevant` applied to the merged items, `Interests` next to `Feeds` in the `taken` pass and the lead candidates (`Tier::Feed`).

- [ ] **Step 4: Run the tests**

Run: `cargo test -p deckle-digest -p deckle-readership 2>&1 | tail -20`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/deckle-readership crates/deckle-digest/src/next
git commit -m "feat(digest): interests in readership, the page plan and tomorrow's paper"
```

---

### Task 5: CLI `deckle digest interests`

**Files:**
- Modify: `crates/deckle-cli/src/cli.rs` (`DigestCommand::Interests`, `InterestsCommand`)
- Modify: `crates/deckle-cli/src/lib.rs` (dispatch, `digest_interests`)
- Test: `crates/deckle-cli/tests/interests.rs` (new; `setup()` modelled on `tests/papers.rs` without the fake Kindles)

**Interfaces:**
```rust
pub enum DigestCommand { …, /// Things you care about, each its own section.
    Interests { #[command(subcommand)] action: Option<InterestsCommand> }, … }
pub enum InterestsCommand {
    /// Add one; the search defaults to the name in quotes.
    Add { name: String, #[arg(long)] query: Option<String>, #[arg(long = "also")] also: Vec<String>, #[arg(long = "not")] not: Vec<String>, #[arg(long)] max_items: Option<usize> },
    Remove { name: String },
}
fn digest_interests(json: bool, path: &Path, paper: Option<&str>, action: Option<cli::InterestsCommand>) -> Result<ExitCode, String>;
```
- `list` (no action): text `Rust — "Rust" · 5 stories` per line, `--json` an array of the `InterestConfig`s. `add` writes through `deckle_digest::papers::save(path, &cfg)` after pushing the entry (it errors on a duplicate through `Config::parse` semantics: check case-insensitively first and return `an interest named "<x>" already exists`), inserts `"interests"` into `sections` before `"feeds"` (or at the end) when missing, and prints `added Rust (section "interests" added to sections)` / `added Rust`. `remove` prints `removed Rust` or errors `no interest named "<x>"`. `--paper <slug>` picks the paper's file (`paper_config`, `papers::file_of`).

- [ ] **Step 1: Write the failing tests** in `tests/interests.rs`:

```rust
#[test]
fn add_list_remove_round_trip_on_the_main_paper() {
    let h = setup();   // digest.toml with sections = ["horoscope"]
    deckle(&h).args(["digest", "interests", "add", "Rust", "--not", "Rust Belt", "--also", "Cargo"]).assert().success().stdout(predicates::str::contains("added Rust (section \"interests\" added to sections)"));
    let text = std::fs::read_to_string(&h.config).unwrap();
    assert!(text.contains("[[interests]]") && text.contains("name = \"Rust\"") && text.contains("Rust Belt") && text.contains("sections = [\"horoscope\", \"interests\"]"), "{text}");
    deckle(&h).args(["digest", "interests", "add", "rust"]).assert().failure().stderr(predicates::str::contains("already exists"));
    deckle(&h).args(["--json", "digest", "interests"]).assert().success().stdout(predicates::str::contains("\"also\":[\"Cargo\"]"));
    deckle(&h).args(["digest", "interests", "remove", "Rust"]).assert().success().stdout(predicates::str::contains("removed Rust"));
    deckle(&h).args(["digest", "interests", "remove", "Rust"]).assert().failure().stderr(predicates::str::contains("no interest named"));
}

#[test]
fn add_on_another_paper_writes_its_own_file_only() {
    let h = setup();
    deckle(&h).args(["digest", "papers", "new", "--title", "The Kid Courier"]).assert().success();
    deckle(&h).args(["digest", "--paper", "kid-courier", "interests", "add", "Dinosaurs"]).assert().success();
    let kid = std::fs::read_to_string(h.config.with_file_name("papers/kid-courier.toml")).unwrap();
    assert!(kid.contains("name = \"Dinosaurs\"") && !kid.contains("[sync]"), "{kid}");
    assert!(!std::fs::read_to_string(&h.config).unwrap().contains("Dinosaurs"));
}
```

(`deckle(&h)` = `Command::cargo_bin("deckle")` with `HOME`/`APPDATA`/`XDG_CONFIG_HOME` pointed at `h.home`, as `papers.rs` does; check the JSON key order the serializer actually emits before asserting a substring and adjust to a `serde_json` parse if needed.)

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p deckle-cli --test interests 2>&1 | tail -10`
Expected: FAIL (`interests` is not a known subcommand).

- [ ] **Step 3: Implement** the clap variants and `digest_interests`, dispatched from the `Digest` match next to `Papers`. Load with `paper_config(&path, paper)`; save with `deckle_digest::papers::save(&path, &cfg)` (it routes to `digest.toml` or the paper's file). Share the "insert `interests` before `feeds`" step with the desktop later by putting it on `Config`: `pub fn ensure_section(&mut self, kind: SectionKind, before: SectionKind) -> bool` in `config.rs` (returns whether it was added), with a unit test.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p deckle-cli --test interests --test papers 2>&1 | tail -10`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/deckle-cli crates/deckle-digest/src/config.rs
git commit -m "feat(cli): deckle digest interests — list, add, remove, per paper"
```

---

### Task 6: Desktop: types, page plan, the Interests card

**Files:**
- Modify: `apps/desktop/src/types.ts` (`SectionKind`, `SECTION_LABEL`, `ALL_SECTIONS`, `InterestConfig`, `DailyConfig.interests?`, `DailyConfig.learning?`, `PagePlanRow.interest`)
- Modify: `apps/desktop/src/pagePlan.ts` (`matches`, `reorder` block logic per grouped kind, `setBudget`)
- Create: `apps/desktop/src/interests.ts` (pure helpers) and `apps/desktop/src/interests.test.ts`
- Modify: `apps/desktop/src/DailyView.tsx:720-750` (the card, between Topics and Your feeds)
- Modify: `apps/desktop/src/PagePlanCard.tsx:65,105` (toggle title for interests)
- Test: `apps/desktop/src/pagePlan.test.ts`, `apps/desktop/src/interests.test.ts`

**Interfaces:**
```ts
export interface InterestConfig { name: string; query?: string; also?: string[]; not?: string[]; feeds?: string[]; max_items: number; full_text?: boolean | "lead"; max_age_hours?: number }
export interface LearningConfig { enabled: boolean; window_days: number; min_stories: number; nudges: boolean }
// interests.ts
export function addInterest(cfg: DailyConfig, name: string): DailyConfig;      // trims; no-op on blank or case-insensitive duplicate; pushes {name, max_items: 5}; adds "interests" to sections before "feeds" (else at the end) when missing
export function removeInterest(cfg: DailyConfig, index: number): DailyConfig;
export function setList(cfg: DailyConfig, index: number, key: "also" | "not", text: string): DailyConfig;   // comma-separated → trimmed, non-empty strings; empty list → key removed
export function listText(v: string[] | undefined): string;                      // join(", ")
```
- `SECTION_LABEL.interests = "Your interests"`; `ALL_SECTIONS` gains `"interests"` after `"local"`.
- `setBudget` case `"interests"`: `cfg.interests[row.interest].max_items = v`. `reorder`: the block rule that keeps feeds together applies to interests rows too (each kind its own block); `matches` checks `cfg.interests[r.interest]?.name === r.name`.

- [ ] **Step 1: Write the failing tests.** `interests.test.ts`:

```ts
test("addInterest adds the kind before feeds once and ignores duplicates", () => {
  const cfg = base({ sections: ["local", "feeds"], interests: undefined });
  const a = addInterest(cfg, " Rust ");
  expect(a.interests).toEqual([{ name: "Rust", max_items: 5 }]);
  expect(a.sections).toEqual(["local", "interests", "feeds"]);
  expect(addInterest(a, "rust")).toBe(a);
  expect(addInterest(a, "  ")).toBe(a);
  const b = addInterest(base({ sections: ["horoscope"] }), "Cargo");
  expect(b.sections).toEqual(["horoscope", "interests"]);
});
test("setList parses comma lists and drops an emptied key", () => {
  const cfg = addInterest(base({}), "Rust");
  const w = setList(cfg, 0, "not", " Rust Belt, corrosion ,, ");
  expect(w.interests![0].not).toEqual(["Rust Belt", "corrosion"]);
  expect(listText(w.interests![0].not)).toBe("Rust Belt, corrosion");
  expect(setList(w, 0, "not", "").interests![0].not).toBeUndefined();
});
```

`pagePlan.test.ts` additions (use the file's existing row/config builders):

```ts
test("setBudget edits an interest's max_items", () => { /* row {kind:"interests", interest:0, key:"interests:Rust", name:"Rust"} → cfg.interests[0].max_items === 7 */ });
test("reorder keeps interest rows a block and writes [[interests]] order", () => { /* two interest rows swapped → cfg.interests reversed; a lottery row dropped between them snaps to the block's nearest end */ });
```

- [ ] **Step 2: Run to see them fail**

Run: `pnpm -C apps/desktop test 2>&1 | tail -15`
Expected: FAIL (module `./interests` missing; type errors on `interest`).

- [ ] **Step 3: Implement** the types, `interests.ts`, the `pagePlan.ts` generalisation (replace `isFeed` with `grouped(r) = r.kind === "feeds" || r.kind === "interests"` and run the snap logic per kind: a moved row of kind K must touch K's block; a non-grouped row dropped inside a block snaps out), and the card in `DailyView.tsx` right after the Topics block:

```tsx
<div className="label">Your interests</div>
<p className="muted small">Things you care about, each its own section: Deckle searches the news for them every morning.</p>
<div className="feeds">
  {(draft.interests ?? []).map((it, i) => ( <div key={i} className="feed interest">
      <input value={it.name} placeholder="e.g. Carolina Panthers" onChange={(e) => edit((c) => (c.interests![i].name = e.target.value))} />
      <input value={listText(it.also)} placeholder="Also: Bryce Young, Dave Canales" onChange={(e) => edit((c) => Object.assign(c, setList(c, i, "also", e.target.value)))} />
      <input value={listText(it.not)} placeholder="Leave out: Florida Panthers, hockey" onChange={(e) => edit((c) => Object.assign(c, setList(c, i, "not", e.target.value)))} />
      <button className="icon small" aria-label="Remove interest" onClick={() => edit((c) => c.interests!.splice(i, 1))}><Icon name="x" size={14} /></button>
  </div> ))}
  <form onSubmit={(e) => { e.preventDefault(); edit((c) => Object.assign(c, addInterest(c, newInterest))); setNewInterest(""); }}>
    <input value={newInterest} placeholder="Add an interest…" onChange={(e) => setNewInterest(e.target.value)} />
  </form>
</div>
```

(`edit` mutates a draft in place in this file; `setList`/`addInterest` return new objects, hence `Object.assign`. If `edit`'s shape makes that awkward, add `editWith(f: (c) => DailyConfig)` beside it.) `PagePlanCard.tsx`: the toggle title reads "Switches all your interests" for `kind === "interests"`.

- [ ] **Step 4: Run tests and the type check**

Run: `pnpm -C apps/desktop test 2>&1 | tail -15 && pnpm -C apps/desktop build 2>&1 | tail -5`
Expected: tests pass; `tsc --noEmit` and vite build clean.

- [ ] **Step 5: Look at it.** Run the desktop app (`pnpm -C apps/desktop tauri dev`, or the `/run` skill) with a `digest.toml` holding one interest; the Daily tab shows the Interests card between Topics and Your feeds, adding "Rust" and saving writes `[[interests]]` to the file and a `Rust` row appears in the page plan. Take a screenshot for the PR.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src
git commit -m "feat(desktop): Your interests card and interest rows in the page plan"
```

---

### Task 7: Docs, full verification, PR

**Files:**
- Modify: `docs/daily.md` (new `### Your interests` after `### Topics and feeds`)
- Modify: `docs/superpowers/specs/2026-10-25-interests-design.md` (append `## 14. Implementation notes, PR 1 (2026-10-25)`: what landed, what moved, in the style of the many-papers spec §12)

- [ ] **Step 1: Write the docs section** (copy the `[[interests]]` block from the starter, then: what relevance means, `not`, quiet days left out, the budget default, the Interests card, the CLI lines from spec §5.5 that exist now, and one line that suggestions and nudges come later).

- [ ] **Step 2: Full verification**

Run:
```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace --no-fail-fast 2>&1 | tail -30
pnpm -C apps/desktop build && pnpm -C apps/desktop test 2>&1 | tail -10
```
Expected: every command exits 0; the test tail shows `test result: ok` for each crate with 0 failed.

- [ ] **Step 3: A live build, once.** With network: `cargo run -p deckle-cli -- digest build --config <a temp digest.toml with [[interests]] name = "Rust", not = ["Rust Belt"]>` and open the EPUB's `interests` section XHTML; confirm the search returned relevant stories and the quoted query survived encoding (spec §9 risk). Note the outcome in the PR body.

- [ ] **Step 4: Commit, push, PR**

```bash
git add docs
git commit -m "docs(daily): Your interests"
git push
gh pr create --title "feat(digest): interests, part 1 — sections that hunt" --body "…Closes #<issue> …"
```

Open the GitHub issue for increment 1 first if none exists (`gh issue create` titled "Interests, part 1: interest sections", body linking the spec), so the PR body's `Closes #N` has a target. Then bind the PR with the ccd_pr tools and read its CI.
