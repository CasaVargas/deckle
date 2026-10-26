# Publisher subscriptions, PR 2: the BLOX adapter and the subscribed paper — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A BLOX (TownNews) paper the owner subscribes to prints in full, as its own section
group fed from the paper's own RSS, with every story's access recorded in the edition bundle.

**Architecture:** `publisher/blox.rs` is the first `Publisher`: it recognizes a BLOX page,
signs in through the site's login form, reads the account's services, and unlocks a premium
story by un-shifting its ROT47 paragraphs, only when the services cover the page's
`paid_access_service_ids`. `[[subscribed]]` becomes a config list and `SectionKind::Subscribed`
a section kind; each group expands into one synthetic `FeedConfig` per slug and goes through
`feed_section`, so extraction, dedupe, Readership and the page plan are the existing code with
a new key prefix `subscribed:<site>:<slug>`. The bundle gains `story.access` (schema 1.2).

**Tech Stack:** Rust 2024 workspace; `dom_query` for the page's DOM (already a dependency);
`serde_json`; the existing `feeds.rs` section pipeline; Tauri 2 + React/TS with vitest.

**Spec:** `docs/superpowers/specs/2026-10-25-publisher-subscriptions-design.md` (§3 `[[subscribed]]`,
§6.1–§6.3, §7, §12 rulings, §14 PR 2, §15 build notes from PR 1). Builds on PR 1
(`claude/pilot-subscription-paywall-gaps-19dee0`); this branch is stacked on it.

## Global Constraints

- **The anti-goal.** `unlock` returns `Some` only when the entitlement's active service ids
  intersect the page's `paid_access_service_ids`, or the page lists none and the account has
  at least one active service. A unit test asserts `unlock` returns `None` for an account
  without the service even though the text is trivially decodable. No flag, env var or config
  key changes this.
- ROT47 is the shift: for each byte `33..=126`, `33 + (b - 33 + 47) % 94`; other bytes
  unchanged. On the wire a block's text is the HTML-escaped ROT47 of the paragraph's HTML
  (`html_escape(rot47("<p>…</p>"))`), verified on a live page: the DOM parser unescapes, then
  the adapter shifts. The fixture builder writes `html_escape(rot47(para_html))`.
- **Canonical host.** `https://thepilot.com/` answers 301 to `https://www.thepilot.com/`
  (verified). The adapter never hard-codes the bare host for a POST: it GETs
  `https://<site>/users/login/` (following redirects), takes the login form's absolute
  `action` (today `https://www.thepilot.com/users/login/`) and POSTs there, and uses that
  response's origin (`https://www.<site>` here) as the site's **origin** for the services URL
  and the feeds. The action's host must be the site or a subdomain of it, else
  `Err("the login form points at another host")`. The PR 1 jar is scoped to the site and its
  subdomains, so cookies set on `www` are sent on `www`.
- Login: `POST <form action>`, `application/x-www-form-urlencoded`, with the fields of the
  page's `#access-modal-login-form` (today `referer_url` hidden, `username`, `password`),
  copying every hidden input it finds, values percent-encoded with `fetch::encode`.
- Entitlement: `GET <origin>/tncms/subscription/services/` with the session, parsed leniently
  (Task 1); its exact shape is corrected after spike 1.
- Feeds: `<origin>/search/?f=rss&t=article&c=<slug>&l=<n>&s=start_time&sd=desc`, where
  `<origin>` is `https://www.<site>` for BLOX (the redirect target); `feed_url` takes the
  site and builds it with `www.`. Known slugs and display names: `news` News, `sports`
  Sports, `opinion` Opinion, `obituaries` Obituaries, `business` Business, `lifestyles`
  Lifestyles.
- **A page with nothing encrypted** (a free story on a signed-in account) is returned
  unchanged by `unlock` as `Some(page)`: nothing was decoded, so the anti-goal is untouched,
  and the story isn't mislabeled walled.
- **One kind for every group.** `SectionKind::Subscribed` prints every `[[subscribed]]` group
  in file order at its one place in `sections` (ruling: the spec's per-site `subscribed:<site>`
  entry is superseded; groups don't interleave with other sections). Turning a group on from
  the card inserts `subscribed` into `sections` right after `local` (at the end when there is
  no `local`), and removing the last group removes it.
- Slugs are validated at config load against BLOX's list, always (ruling: BLOX is the only
  adapter and the only feed pattern in PR 2; a second CMS gets its own rule when it lands).
- Group defaults: `max_age_hours` from `[local] max_age_hours`, `max_items` from `[local] max_items`.
  `feed_section` gains an options parameter for the kicker and the age window (Task 3).
- `Entitlement` gains `names: BTreeMap<String, String>` (service id → display name when the
  reply carries one); `Access::Subscription.service` and the card show the name when known,
  else the id.
- Keys: `subscribed:<site>:<slug>`; `subscribed` is the kind name for the bundle and the app.
- `[[subscribed]]` lives in a paper file (per paper); its `site` must match a household
  `[[subscriptions]]` entry or the group prints the stories as a stranger sees them with the
  note `No subscription for <site> in digest.toml; stories print as a stranger sees them.`
- Dedupe is the existing rule, by `title_key`, in `sections` order (ruling: the spec said URL;
  the pipeline dedupes by title, and a story is a story).
- Bundle `VERSION` becomes `"1.2"`; `story.access` ∈ `"open" | "subscription" | "walled"`;
  the schema marks it `"1.2"`. The DeckleReader app (another repo) is a follow-up, additive.
- `deny_unknown_fields` on the new table; saves leave an empty list out; a config without
  the table builds as before.
- Never commit a real article's text: the BLOX fixture is generated by a test helper from
  synthetic paragraphs.

## Review Focus

1. A BLOX page served to a stranger contains BOTH the `.aplus-user-login.not-logged-in` and
   `.aplus-user-controls.logged-in` blocks (CSS hides one), so presence alone cannot mean
   signed in; `signed_in` must use a marker that only a signed-in page carries (a non-empty
   `.tn-user-screenname` text), and the test must prove a stranger's page with both blocks is
   not signed in. (Task 1.)
2. A premium page's `subscriber-preview` paragraphs are plain text; `unlock` must keep them in
   order with the decoded ones, not duplicate or drop them. (Task 1.)
3. A `[[subscribed]]` slug the adapter doesn't know (`"obits"`) is rejected at config load with
   the known list, not silently empty. (Task 2.)
6. The adapter's `entitlement` reply that matches none of the lenient shapes: the session
   records the parse error in the site's status (`LoginFailed`-style text is wrong; use a new
   `SessionState::EntitlementUnknown(String)`), the story prints as a stranger sees it with
   `Walled NotInPlan`, and nothing panics. (Task 1 for the parser's `Err`; the session change
   is in Task 1's scope too, one small match arm.)
4. The same story in Local (via Google News) and in the group prints once, in whichever
   section comes first in `sections`. (Task 3.)
5. A Readership "Drop" on `subscribed:thepilot.com:sports` removes only that slug, and
   dropping the last slug removes the group. (Task 4.)

---

### Task 1: The BLOX adapter

**Files:**
- Create: `crates/deckle-digest/src/publisher/blox.rs` (move `publisher.rs` to
  `publisher/mod.rs` first, unchanged)
- Modify: `crates/deckle-digest/src/publisher/mod.rs` (`registry()` returns `vec![Box::new(blox::Blox)]`;
  `detect_kind` now finds it)
- Test: `blox.rs` tests module, with a fixture builder

**Interfaces:**
- Consumes: `Publisher`, `SiteClient`, `Entitlement` (PR 1); `fetch::encode`; `dom_query`.
- Produces:
  ```rust
  pub struct Blox;
  impl Publisher for Blox { … kind() == "blox" … }
  pub fn rot47(s: &str) -> String;
  /// The page's `paid_access_service_ids` as strings; empty when the page lists none.
  pub fn paid_access_service_ids(html: &str) -> Vec<String>;
  /// Lenient: an array, or an object with a `services` array; each service an object with
  /// `id` (number or string) and `active`/`is_active`/`status == "active"`; or a bare id list.
  pub fn parse_services(json: &str) -> Result<Entitlement, String>;
  pub const SECTIONS: &[(&str, &str)];   // the six slugs above
  #[cfg(test)] pub fn fixture_page(opts: Page) -> String; // Page { signed_in: bool, service_ids: &[&str], preview: &[&str], premium: &[&str], login_form: bool }
  ```
  `signed_in`: the page has a `.tn-user-screenname` element whose trimmed text is non-empty
  (spike 1 confirms; the marker lives in one function so it is one line to change).
  `detect`: the page contains `tncms` or `/tncms/`. `unlock`: with the entitlement rule above,
  replace each `div.subscriber-only.encrypted-content` by the ROT47-unshifted inner text (the
  DOM parser already unescaped it) without the `display:none`, leaving `subscriber-preview`
  paragraphs in place; return the whole page; a page with no such block returns `Some(page)`
  unchanged. `login`: GET `https://<site>/users/login/` (redirects followed), parse the form,
  check the action's host, POST, then `Ok(())` if the response or a following GET of the
  action's origin is `signed_in`, else `Err("the site refused the login")` (never echo the
  body). `entitlement`: GET `<origin>/tncms/subscription/services/`, `parse_services`.
  `sections`: `SECTIONS`. `feed_url(site, slug, n)`: the pattern above on `https://www.<site>`.
  Also in this task: `Entitlement.names`; `SessionState::EntitlementUnknown(String)` in
  `session.rs` when `entitlement` returns `Err` (status text `couldn't read your plan: <e>`),
  the story then `Walled NotInPlan`; and the PR 1 test `the_registry_is_empty_and_nothing_is_detected`
  becomes `the_registry_has_blox`. Check `session.rs` tests that call the real `registry()`
  for the NoAdapter path and point them at an empty adapter list instead.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn rot47_round_trips_and_decodes_blox() { assert_eq!(rot47("kAm"), "<p>"); assert_eq!(rot47(&rot47("Hello, <b>x</b>")), "Hello, <b>x</b>"); }
#[test] fn a_strangers_page_with_both_header_blocks_is_not_signed_in() { /* fixture_page(signed_in: false) contains both aplus-user blocks; !Blox.signed_in */ }
#[test] fn a_signed_in_page_has_a_screen_name() { /* fixture_page(signed_in: true) → signed_in */ }
#[test] fn unlock_needs_the_pages_service() {
    // page service_ids ["3"], premium 3 paragraphs: Entitlement{services: ["3"]} → Some with
    // the paragraphs decoded, preview paragraphs kept in order, no "display:none" left;
    // Entitlement{services: ["9"]} → None; Entitlement{services: []} → None.
}
#[test] fn a_page_without_service_ids_unlocks_for_any_active_service() { /* service_ids [] + services ["1"] → Some; services [] → None */ }
#[test] fn unlocked_page_extracts_to_the_full_story() { /* article::extract on the unlocked fixture (premium: 6 paragraphs of ~30 words) yields words >= 120 and the last premium sentence */ }
#[test] fn parse_services_accepts_the_shapes_we_expect() { /* array of {id, active}; {services:[…]}; bare ids; status:"active"; inactive excluded; garbage → Err */ }
#[test] fn paid_access_ids_parse() { /* `tncms.subscription.paid_access_service_ids = [3, "4"];` → ["3","4"]; absent → [] */ }
#[test] fn login_posts_to_the_forms_action_with_the_fields_encoded() { /* fake SiteClient records; the login page's form action is https://www.thepilot.com/users/login/ with hidden referer_url; the POST goes to that action; body has referer_url, username=a%40b.test, password=p%2Bw */ }
#[test] fn login_refuses_a_form_that_points_elsewhere() { /* action https://evil.test/login → Err, no POST recorded */ }
#[test] fn a_services_reply_we_cant_read_is_an_error_not_a_panic() { /* parse_services("<html>") → Err */ }
#[test] fn feed_urls_use_the_www_origin() { assert_eq!(Blox.feed_url("thepilot.com", "news", 15), "https://www.thepilot.com/search/?f=rss&t=article&c=news&l=15&s=start_time&sd=desc"); }
#[test] fn the_registry_detects_blox() { assert_eq!(detect_kind(&fixture_page(Default::default())), Some("blox")); }
```

- [ ] **Step 2: Run** `cargo test -p deckle-digest --lib -- publisher::blox` → compile errors.
- [ ] **Step 3: Implement** as the interface says. `fixture_page` builds the page from the
  public Pilot markup's skeleton (header with both `aplus-user` blocks and an empty or filled
  `tn-user-screenname`, `#access-modal-login-form` with the three fields, a `<script>` line
  `tncms.subscription.paid_access_service_ids = [...]`, `div.asset-body` with
  `p.subscriber-preview` then `div.subscriber-only.encrypted-content` blocks whose inner HTML
  is `rot47(html_escape(…))`), with synthetic paragraphs only.
- [ ] **Step 4: Run** the tests → PASS; `cargo clippy -p deckle-digest --all-targets -- -D warnings`.
- [ ] **Step 5: Commit** `feat(digest): BLOX adapter (sign-in, entitlement, ROT47 unlock)`.

### Task 2: `[[subscribed]]` and `SectionKind::Subscribed`

**Files:**
- Modify: `crates/deckle-digest/src/config.rs` (`SectionKind`, `section_key`, `Config`,
  `STARTER`/`PAPER_STARTER` commented block from spec §3, validation), `crates/deckle-digest/src/papers.rs`
  (nothing household: `[[subscribed]]` is per paper), `apps/desktop/src/types.ts`
  (`SectionKind` union, `SECTION_LABEL.subscribed = "Subscribed papers"`, `ALL_SECTIONS`,
  `SubscribedConfig`, `DailyConfig.subscribed?`)
- Test: `config.rs` tests; vitest for the label map if one exists

**Interfaces:**
- Produces:
  ```rust
  pub enum SectionKind { …, Subscribed }                      // serde "subscribed"
  #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(deny_unknown_fields)]
  pub struct SubscribedConfig {
      pub site: String,                                        // normalized like a subscription's
      #[serde(default, skip_serializing_if = "Option::is_none")] pub name: Option<String>,
      pub sections: Vec<String>,                               // adapter slugs, in order
      #[serde(default, skip_serializing_if = "Option::is_none")] pub max_items: Option<usize>,
      #[serde(default, skip_serializing_if = "Option::is_none")] pub max_age_hours: Option<u32>,
  }
  #[serde(skip_serializing_if = "Vec::is_empty")] pub subscribed: Vec<SubscribedConfig>,  // on Config
  pub fn subscribed_key(site: &str, slug: &str) -> String;     // "subscribed:<site>:<slug>"
  impl SubscribedConfig { pub fn title(&self) -> String; /* name or the site */ pub fn validate(&self, known: &[(&str,&str)]) -> Result<(), String>; }
  ```
  `section_key(SectionKind::Subscribed, Some(k))` returns `k` unchanged (the caller builds it
  with `subscribed_key`); `section_key(SectionKind::Subscribed, None)` returns `"subscribed"`
  (the kind's own key, used by the page plan's off-list and the app), and
  `readership::kind_of` must exclude `Subscribed` the way it excludes `Feeds`. Validation at
  parse (`Config::parse`, and `save` plus the app's `daily_save` normalize step, like
  `subscriptions`): site normalized with `SubscriptionConfig::normalized_site`; `sections`
  non-empty; each slug in `blox::SECTIONS`, else
  `[[subscribed]] thepilot.com: unknown section "obits"; known: news, sports, opinion, obituaries, business, lifestyles`.
  `Config.subscribed` is a paper-file setting: `papers.rs` neither refuses nor copies it.

- [ ] **Step 1: Tests** — parse and round-trip; empty list left out of a save; unknown slug
  rejected with that message; `subscribed_key`; `SectionKind::Subscribed` round-trips as
  `"subscribed"`; the starter's commented block parses once uncommented.
- [ ] **Step 2: Run** → fail. **Step 3: Implement.** **Step 4: Run** `cargo test -p deckle-digest config::` and `npm --prefix apps/desktop run build` → PASS.
- [ ] **Step 5: Commit** `feat(daily): [[subscribed]] config and the subscribed section kind`.

### Task 3: The group in the paper

**Files:**
- Modify: `crates/deckle-digest/src/lib.rs` (the section loop, `SectionKind::Subscribed` arm),
  `crates/deckle-digest/src/sources/feeds.rs` (`feed_section_with_kicker`, or a `kicker`
  parameter), `crates/deckle-digest/src/sources/mod.rs` if the unavailable helper needs a key
- Test: `crates/deckle-digest/tests/build.rs`

**Interfaces:**
- Consumes: `SubscribedConfig`, `subscribed_key` (Task 2); `Blox::feed_url`/`SECTIONS` (Task 1);
  `feed_section` (existing); `Config::subscription(host)` (PR 1).
- Produces:
  ```rust
  pub struct FeedOptions { pub kicker: Option<String>, pub max_age_hours: u32 }
  pub fn feed_section_with(feed: &FeedConfig, opts: FeedOptions, cfg: &Config, now: DateTime<Utc>, net: &dyn Net, taken: &mut HashSet<String>) -> Section;
  // `feed_section` becomes a thin call with `FeedOptions { kicker: None, max_age_hours: cfg.max_age_hours }`.
  /// Every (group, slug) the paper prints, in order: the one place that enumerates them.
  pub fn subscribed_sections(cfg: &Config) -> Vec<(&SubscribedConfig, &'static str /*slug*/, &'static str /*display*/)>;
  ```
  In the section loop: a `SectionKind::Subscribed` arm iterating `subscribed_sections(cfg)`,
  one `Section` per slug with `key = subscribed_key(site, slug)`, `title = display`,
  `kicker = Some(group.title())`, built by `feed_section_with` from a synthetic
  `FeedConfig { name: display, url: Blox.feed_url(site, slug, max_items), full_text: Some(FullText::Yes), max_items, ..Default }`
  and `FeedOptions { kicker, max_age_hours: group.max_age_hours.unwrap_or(cfg.local.max_age_hours) }`,
  `max_items = group.max_items.unwrap_or(cfg.local.max_items)`, sharing the edition's `printed`
  set and pushed onto `news` like a feed's stories (lead and puzzle theme). The key-assignment
  loop near `lib.rs:549` that indexes `cfg.feeds[i].name` gets a matching `Subscribed` arm
  over `subscribed_sections`. When the site has no `[[subscriptions]]` entry, the group still
  prints (as a stranger) and the edition's notes get the message in Global Constraints. The
  BLOX pattern is the only feed pattern in PR 2 (Global Constraints).

- [ ] **Step 1: Tests** in `tests/build.rs` with a closure net serving two RSS fixtures and
  article pages: (a) `sections = [local, subscribed]` with one `[[subscribed]]` of two slugs →
  two sections after Local with the right keys, titles and kicker; (b) a story in both the
  Local feed and the group prints once, in Local (Review Focus 4); (c) `sections = [subscribed, local]` → it prints in the group; (d) no `[[subscriptions]]` for the site → the note.
- [ ] **Step 2: Run** → fail. **Step 3: Implement.** **Step 4: Run** `cargo test -p deckle-digest` → PASS.
- [ ] **Step 5: Commit** `feat(daily): a subscribed paper as its own section group`.

### Task 4: Readership and the page plan

**Files:**
- Modify: `crates/deckle-readership/src/report.rs` (`paper_name`, `apply` Drop, `kind_of`),
  `crates/deckle-digest/src/next/plan.rs` (`rows` for `Subscribed`), `crates/deckle-digest/src/next/live.rs`
  (fetch the group's feed URLs like a feed's), `apps/desktop/src/pagePlan.ts` (budget edit for
  a subscribed row), `apps/desktop/src/types.ts` (`PlanRow` gains `subscribed?: { site, slug }` if
  the Rust row does)
- Test: `report.rs` tests; `plan.rs` tests; `pagePlan.test.ts`

**Interfaces:**
- `report.rs`: `KINDS` gains `Subscribed`; `kind_of` excludes it like `Feeds`; `current_keys`
  gets a `Subscribed` arm yielding one key per `subscribed_sections(cfg)` entry; `paper_name`
  gains a `cfg: &Config` parameter (update its callers) and returns `"<group title> · <display>"`
  for a `subscribed:` key (`"The Pilot · Sports"`, or `"thepilot.com · Sports"` without a
  name); `apply(Drop, "subscribed:<site>:<slug>")` removes the slug from that group's
  `sections` and removes the group when it is left empty (Review Focus 5); `MoveUp` stays
  feeds-only; the `starts_with("feeds:")` suggestion branch treats `subscribed:` the same way
  where it decides between per-section and per-kind suggestions.
- `plan.rs`: `KINDS` gains `Subscribed` (so it can sit in the off-list when no group exists:
  `rows` then returns nothing and the kind shows as off); `estimate()`, the dedupe arm
  `(SectionKind::Feeds, Some(s))` and the lead-candidate arm each get a `Subscribed` twin;
  `rows(.., Subscribed, ..)`: one `stories_row` per `subscribed_sections` entry with `key`, name
  `"<group title> · <display>"`, detail `"<site> · <slug>"`, the group's `max_items`, and
  `PlanRow.subscribed = Some(SubscribedRef { site, slug })` (new field, serde skip-if-none;
  TS `subscribed?: { site: string; slug: string }`); `live.rs` fetches the group's feed URLs
  like a feed's (same twice-an-hour limit).
- `pagePlan.ts`: a subscribed row's budget edit lands on its group's `max_items`; the group's
  rows are one atomic block for drag, move up/down and on/off, as feeds are ("feeds stay one
  block" logic extended to `subscribed`).
- The group's tile on Tomorrow's paper shows the paper's name.

- [ ] **Step 1: Tests** as the interfaces say. **Step 2: Run** → fail. **Step 3: Implement.** **Step 4: Run** `cargo test -p deckle-readership -p deckle-digest next::` and vitest → PASS.
- [ ] **Step 5: Commit** `feat(daily): Readership and the page plan know subscribed papers`.

### Task 5: Bundle `access`, schema 1.2

**Files:**
- Modify: `crates/deckle-digest/src/bundle.rs` (`VERSION = "1.2"`, `StoryJson.access`, `kind_name`),
  `docs/formats/edition-bundle.schema.json` (`access` enum with `"description": "1.2: …"`,
  `kind` enum gains `subscribed`), `docs/superpowers/specs/2026-10-23-edition-bundle-design.md`
  (a 1.2 line)
- Test: `bundle.rs` tests (the schema validation test; `access` right for Open, Subscription, Walled)

- [ ] **Step 1: Tests.** **Step 2: Run** → fail. **Step 3: Implement.** **Step 4: Run** `cargo test -p deckle-digest bundle::` → PASS.
- [ ] **Step 5: Commit** `feat(bundle): story.access (1.2)`.

### Task 6: The Subscriptions card edits the group

**Files:**
- Modify (all exist since PR 1): `apps/desktop/src/SubscriptionsCard.tsx` (per site: a
  "Print this paper" switch that adds/removes a `[[subscribed]]` entry in the draft,
  checkboxes for the six slugs, a stories-per-section number), `apps/desktop/src/subscriptions.ts`
  (pure helpers `toggleGroup(cfg, site)`, `toggleSlug(cfg, site, slug)`, `groupFor(cfg, site)`),
  `apps/desktop/src/subscriptions.test.ts`; `apps/desktop/src/types.test.ts` (positions in
  `ALL_SECTIONS` shift by one: `subscribed` goes right after `feeds`).
- The per-paper draft is the one Your papers edits (many papers §7); the `subscribed` list
  rides on `DailyConfig` through the existing save.

- [ ] **Step 1: vitest** for the three helpers: toggle on adds `{site, sections: ["news"]}` AND
  inserts `"subscribed"` into `sections` after `"local"` (at the end without `local`) when
  absent; unticking the last slug removes the group, and removing the last group removes
  `"subscribed"` from `sections`; `groupFor` finds by normalized site.
- [ ] **Step 2: Run** → fail. **Step 3: Implement.** **Step 4: Run** vitest, `npm run build` → PASS; exercise the card in the browser with the mocked IPC as PR 1 did (or the real window if a build gets through).
- [ ] **Step 5: Commit** `feat(app): choose a subscribed paper's sections on the card`.

### Task 7: Docs and checks

- `docs/daily.md`: the `[[subscribed]]` block, the BLOX paragraph ("The Pilot and most
  small-town papers"), what prints when the account is missing; the spec's §15 gains "Build
  notes, PR 2" (the signed-in marker chosen and that spike 1 confirms it; the lenient
  services parser; the canonical `www` origin from the form's action; dedupe by title; one
  `subscribed` kind for every group; slugs validated against BLOX's list; a page with nothing
  encrypted unlocks unchanged; the divider page deferred, the kicker carries the paper's name;
  the typed login error deferred to PR 3; DeckleReader follow-up for `access`); CHANGELOG;
  README line.
- `cargo test --workspace --no-fail-fast`, `cargo clippy --workspace --all-targets -- -D warnings`, vitest.
- Commit `docs(daily): subscribed papers`.
