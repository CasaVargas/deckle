# Publisher subscriptions: the paper you pay for, in full — design

**Status:** approved by the owner on 2026-10-25 with the §12 draft answers as rulings ·
**Owner:** Jonathan (CasaVargas LLC) ·
**Builds on:** `2026-10-20-daily-digest-design.md` (the paper, §5a full text; "paid or
paywalled content, logins, cookies" were a non-goal there, reversed here for the owner's own
subscriptions only), `2026-10-21-send-to-kindle-email-design.md` (the keychain pattern),
`2026-10-22-local-places-design.md` (sections from feeds), `2026-10-22-many-papers-design.md`
(household settings live in `digest.toml`), `2026-10-23-pdf-edition-design.md` and
`2026-10-23-other-readers-design.md` §9 (PDF delivery, reMarkable).

The owner subscribes to a local paper on BLOX. Today its stories reach the Deckle
Daily as headlines: the site serves the body only to a signed-in subscriber, and Deckle never
signs in. This design lets Deckle sign in to a paper the owner pays for, print its stories in
full in their own section, and, on print days, deliver the paper's own e-edition PDF to a
tablet. Nothing here reads anything the owner hasn't paid for.

## 1. Why

A local paper is the one source a morning paper can't do without, and almost every local paper
in the United States is behind a login now. The Deckle Daily fetches stories "as any reader
would", so a subscriber gets the same headline-only stub as a stranger. The owner has the
subscription; the app should use it.

**Success**, on a Wednesday morning:

- `digest.toml` has a `[[subscriptions]]` entry for `gazette.example`; the password went into
  the keychain once from the Daily tab, and the card says *Signed in · Gazette Plus, active*.
- The paper prints **The Gazette** as a section group after Local: News, Sports, Opinion,
  Obituaries, each with its stories whole, with photos, bylines and the usual typography pass.
- The Daily tab's section list shows *The Gazette · 14 stories, all in full*.
- Because Wednesday is a print day, the morning's e-edition PDF went to the reMarkable outbox
  (or the export folder) beside the Deckle Daily's own PDF.
- A `deckle digest lint <story url>` shows the story's paragraphs, not "no readable story".
- On a day the session expired and the login failed, every Gazette story prints as a headline
  with "Sign in to The Gazette failed; see the Daily tab", the rest of the paper is unaffected,
  and the card on the Daily tab is red.

## 2. Goals, non-goals, anti-goals

**Goals**

1. **An account per publisher**, on the owner's machine: domain, username, a password in the
   OS keychain (§4). Set from the CLI or the Daily tab, tested with one click.
2. **A session per publisher** the fetch layer keeps and reuses: log in once, carry the cookies
   across the edition and across days, log in again when the site bounces (§5).
3. **A publisher adapter** that turns the signed-in page into the full story. The first is
   BLOX (TownNews), which powers *The Gazette* and most small-town papers in the country (§6).
4. **The subscribed paper as its own section group** fed from the paper's own feeds, not a
   Google News search (§7).
5. **The e-edition**, the paper as printed, as a PDF for a reMarkable, a Scribe or an export
   folder, on print days (§8).
6. **Honest reporting**: the Daily tab, the run report and the edition bundle say whether a
   story came through the subscription, and why not when it didn't (§9).

**Non-goals**

- **No OAuth / "Sign in with Google" flows.** A username and password on the paper's own
  login form, or nothing. A paper that only offers social login is out (open question §12).
- **No adapters beyond BLOX in this increment.** The adapter is a trait so Gannett, Lee,
  Hearst and the rest can follow one by one; each is its own small design note.
- **No shared accounts across papers.** Subscriptions are a household setting in `digest.toml`
  (many-papers §call 2); every paper that lists the publisher uses the same login.
- **No reading of the browser's cookies.** Deckle logs in itself. Borrowing Safari's or
  Chrome's session is fragile and alarming.
- **No metered-paywall tricks**: no counter resets, no AMP or cache proxies, no referer games.

**Anti-goals (a build that does these has failed)**

- **A paywall bypass.** On BLOX the premium body is in the page for everyone, hidden and
  obfuscated (§6.1). Deckle decodes it **only after the site itself has confirmed an active
  subscription for the signed-in account on this request.** Without that, the story stays a
  headline, exactly as today, and no flag, env var or config key changes that.
- A password anywhere but the keychain (or the headless env var): not in `digest.toml`, not in
  the report, not in a log, not in an error string, not in `edition.json`.
- Credentials sent to any host other than the publisher's own, over anything but HTTPS.

## 3. Config

A household list in `digest.toml` only, additive. A config without it reads and builds as
before; a save leaves it out while empty. A `papers/<slug>.toml` that sets it is an error, as
for `[email]` (many-papers §call 2).

```toml
# Papers you subscribe to. The password isn't stored here: run
# `deckle digest subscribe gazette.example` (or use the Daily tab), which saves it in the system
# keychain. Deckle signs in as you and prints the stories your subscription covers.
[[subscriptions]]
site = "gazette.example"            # the paper's domain; the login goes only there, over HTTPS
username = "you@example.com"
# kind = "blox"                  # the paper's CMS; detected from the site when unset
```

Then, where the paper goes:

```toml
# A section group for a subscribed paper, in `sections` as "subscribed:gazette.example".
# ("papers" is taken: `papers/<slug>.toml` are the household's own papers.)
[[subscribed]]
site = "gazette.example"
name = "The Gazette"               # the group's title; default: the site's own name
sections = ["news", "sports", "opinion", "obituaries"]   # the paper's own sections, in order
# max_items = 6                  # stories per section; default: [local] max_items
# max_age_hours = 72             # default: [local] max_age_hours
# eedition = "remarkable"        # or an export folder's name, or "off" (default)
```

- `site` is a bare host. It must match a `[[subscriptions]]` entry, or the group prints its
  stories as a stranger sees them and notes "no subscription for gazette.example".
- `sections` are the adapter's section slugs (§6.3 lists BLOX's). Unknown slugs are a config
  error that names the known ones.
- `[[subscribed]]` is per paper file: Dad's paper can carry the Gazette's sports page only.
- `deny_unknown_fields` applies; saves go through the `toml_edit` merge.

## 4. The account and the keychain

Generalizes `secret.rs`, which today knows one SMTP entry.

- **Entry:** service `com.casavargas.deckle.site`, account `<username>@<site>`. A changed
  username or site is a new entry, so a stale password is never sent to the wrong host.
- **Setting it:**
  - CLI: `deckle digest subscribe <site> [--username <u>] [--stdin]` adds or updates the
    `[[subscriptions]]` entry, reads the password without echo (or one line from stdin), saves
    it, and logs in once to check (§5). `deckle digest subscribe <site> --forget` deletes the
    keychain entry and the config entry. `deckle digest subscribe` alone lists sites with
    their status.
  - App: a **Subscriptions** card on the Daily tab (§10). The Tauri command
    `subscription_set_password(site, password)` writes the keychain and never reads one back;
    the UI asks `subscriptions_status()` → `[{site, username, has_password, session}]`.
- **Reading it:** only inside the login request, as `Zeroizing<String>`, never logged.
- **Headless fallback:** with no keychain at all (Linux without Secret Service, CI),
  `DECKLE_SITE_PASSWORD_<SITE>` is read instead (`gazette.example` → `DECKLE_SITE_PASSWORD_GAZETTE_EXAMPLE`),
  and the report says so. A keychain that lacks the entry is "not set", not a reason to read
  the variable (email §13).
- **Code shape:** `secret.rs` grows a `Purpose { Smtp, Site(host) }` (or a second service
  constant) behind the same `Keys` trait, so the fake keychain in tests covers both.

## 5. Sessions

The fetch layer gains cookie-carrying, per-site sessions. `Net` stays the trait the sources
use and `Http` stays the plain anonymous client; the sessions live in a new wrapper that the
CLI and the app build once per run and hand to the sources in `Http`'s place.

```rust
/// What `Http` is today, plus sessions: built once per run from the household's accounts.
pub struct SessionNet {
    inner: Http,
    sites: HashMap<String, Mutex<Site>>,   // keyed by `[[subscriptions]] site`
    adapters: Vec<Box<dyn Publisher>>,
}
struct Site {
    username: String,
    kind: Option<&'static str>,
    agent: ureq::Agent,                    // its own cookie jar (ureq `cookies` feature)
    state: SessionState,
    failures: u8,
    entitlement: Option<Entitlement>,      // `services/` fetched once per run
}

pub trait Net: Sync {
    fn get(&self, url: &str) -> Result<Vec<u8>, String>;
    fn get_page(&self, url: &str) -> Result<Vec<u8>, String>;
    fn post_form(&self, url: &str, body: &str) -> Result<Vec<u8>, String>;
    /// A page fetched with its site's session, signed in and unlocked (§6) when the site has
    /// an account: the HTML a subscriber's browser would show, plus what happened.
    fn get_page_as(&self, url: &str) -> Result<(Vec<u8>, Access), String> {
        self.get_page(url).map(|b| (b, Access::Open))
    }
}

pub enum Access {
    Open,                                   // no account for this host: fetched as today
    Subscription { site: String, service: String },
    Walled { site: String, why: Walled },   // served as a stranger would see it
}
pub enum Walled { NotSignedIn, LoginFailed(String), NotInPlan, NoAdapter }
```

- **`get_page_as` does the whole dance**, so `full_article`, `web_book.rs` and
  `sources/reading.rs` change by one call each: fetch with the site's agent; ask the adapter
  `signed_in(html)`; if a stranger's page and no failure yet, take the site's mutex, re-check
  (another thread may have signed in meanwhile: stories fetch in parallel under
  `thread::scope` in `feeds.rs`), run `adapter.login`, refetch once; fetch `services/` once per
  run; `unlock` under the entitlement rule (§6.2). Each step that fails returns the page as is
  with the matching `Walled`, never an error: the story prints as a headline with a note.
- **The adapter gets a raw client, not `SessionNet`:** `login` and the `services/` fetch take
  the site's own `ureq::Agent` (behind a tiny `SiteClient` trait so tests can fake it), so there
  is no circularity between `Net` and `Publisher`.
- **One cookie jar per site**, in that site's `Agent`. Persisted as `sessions/<site>.json` (the
  `cookie_store` crate's serde form, which `ureq`'s `cookies` feature already pulls in; mode
  0600; written to a temp file and renamed) in the **household** folder beside `digest.toml`,
  not a paper's state folder: subscriptions are household settings, and one login serves every
  paper. The app's *Test sign-in* and a scheduled run may both write it; last writer wins,
  which is harmless since both hold a valid session.
- **Failures:** two login failures in one process mark the site `LoginFailed` and every later
  story on it prints as a headline without retrying (one bad password must not hammer the
  publisher or lock the account). The counter and the cached entitlement are per process, so
  per paper run.
- **Only the site's own host** (and its subdomains) ever sees the jar: a Gazette story's photo
  on `bloximages.newyork1.vip.townnews.com` is fetched anonymously, as today.
- **Timeouts and the 20 MB limit** stay. The browser user agent is used for signed-in fetches,
  as for pages today, including `services/`.
- A saved *page* (the bookmarklet's HTML) is unchanged: it's already what the owner could read.

## 6. Publisher adapters

```rust
pub trait Publisher: Sync {
    fn kind(&self) -> &'static str;                       // "blox"
    fn detect(&self, html: &str) -> bool;                 // this CMS?
    fn signed_in(&self, html: &str) -> bool;              // served to an account, not a stranger
    fn login(&self, site: &str, user: &str, pw: &str, c: &dyn SiteClient) -> Result<(), String>;
    fn entitlement(&self, site: &str, c: &dyn SiteClient) -> Result<Entitlement, String>;
    /// The page a subscriber's browser would show, or `None` when `ent` doesn't cover it.
    fn unlock(&self, html: &str, ent: &Entitlement) -> Option<String>;
    fn sections(&self) -> &[(&'static str, &'static str)];  // slug → display name
    fn feed_url(&self, site: &str, slug: &str, n: usize) -> String;
    fn eedition(&self, site: &str, day: NaiveDate, c: &dyn SiteClient) -> Result<Option<Vec<u8>>, String>;
}
pub struct Entitlement { pub services: Vec<String> }     // the account's active service ids
```

`SessionNet::get_page_as` (§5) runs these; `full_article` in `sources/feeds.rs` only swaps
`get_page` for `get_page_as` and carries `Access` into the story's note and the bundle.
Everything after that (extraction, lint, photos, typography, teaser) is unchanged, which is the
point: the adapter's only job is to produce the page a subscriber's browser would show.

### 6.1 BLOX (TownNews): what the site does (verified 2026-10-25 on gazette.example)

- **The body is in the page for everyone.** A premium story's paragraphs sit in
  `<div class="asset-body" data-subscription-required-class="asset-body">` as one
  `<div class="subscriber-only encrypted-content" style="display:none">…</div>` per paragraph,
  the inner HTML entity-escaped and then **ROT47**-shifted: `kAm` is `<p>`. The site's script
  unshifts them after it has checked the account. Readability drops the hidden divs, so today's
  result is "no readable story here", not garbage.
- **Entitlement:** `GET /tncms/subscription/services/` (JSON, with the session cookies) lists
  the account's services and whether each is active. The page itself carries
  `tncms.subscription.paid_access_service_ids` (which services unlock this story). A signed-in
  page shows the user's screen name in `.aplus-user`; a stranger's shows *Log in*.
- **Login:** `POST /users/login/`, form-encoded, with exactly the fields the public page's
  `#access-modal-login-form` carries: `referer_url` (hidden), `username`, `password`. No CSRF
  token on The Gazette's form today; the adapter still copies every hidden input it finds, so a
  site that adds one keeps working. Success is a redirect plus the session cookies; failure
  re-renders the form with an error block. The cookie names and the exact `services/` JSON
  need an account to see (spike 1); the fixtures are built from the public form and the
  owner's sanitized capture, in that order.
- **Feeds:** `/search/?f=rss&t=article&c=<section>&l=<n>&s=start_time&sd=desc`; `c=` is the
  section slug (`news`, `sports`, `opinion`, `obituaries`, `business`, `lifestyles` on a typical
  BLOX paper). Open, no login. Items link straight to the article page (no Google News hop).
- **E-edition:** `/eedition/` lists editions; each edition page has its page previews
  (`/eedition/<a>/<bc>/<uuid>/<id>.preview-300.jpg`) and, for a subscriber, per-page PDFs and
  a whole-edition download. The exact download URL pattern is a spike (§11); The Gazette prints
  Wednesday and Sunday.

### 6.2 BLOX: what Deckle does

- `detect`: `tncms` / `blox` markers in the page.
- `signed_in`: `.aplus-user` with a screen name and no `Log in` link → signed in. The marker
  is in every page's header, so a stale jar is caught on the first story. `entitlement`
  fetches `services/` once per run per site.
- `login`: parse the login form from the article page (or `/users/login/`), post it, confirm by
  refetching the article and seeing `signed_in`.
- `unlock`: **only when `Entitlement.services` contains one of the page's
  `paid_access_service_ids` (or the page needs none)**, replace each `encrypted-content` div
  with its unescaped, ROT47-unshifted HTML and drop the `display:none`. Return the rewritten
  page. Otherwise `None`, and the story prints as a headline with "subscriber story" in its
  note. The decoder lives in `publisher/blox.rs` and is called from nowhere else; a unit test
  asserts `unlock` returns `None` for an entitlement without the service even though the
  text is decodable.
- `sections`: the slug list above; a site may have more, so an unknown slug is tried once
  against the feed and rejected at config time only when the feed comes back empty *and* the
  site's section index doesn't list it (the simpler rule, "known list only", is fine for v1).

### 6.3 Detection and `kind`

`kind` unset: on first use, fetch the site's home page anonymously and ask each adapter's
`detect`. Save the result into the config (`kind = "blox"`) so later runs don't guess. No
adapter matching never blocks a run: stories on that site print as today with the note
*subscriber story (Deckle doesn't know how gazette.example signs in yet)*, `Access::Walled
{ why: NoAdapter }`, and the Subscriptions card says the same.

## 7. The subscribed paper in the Daily

- **Key and placement:** `subscribed:<site>` in `sections`, one entry for the group. The
  paper prints a divider page with the paper's name and date, then one section per slug, in
  the configured order. **Each section is a feed section:** the group expands into one
  synthetic `FeedConfig` per slug (`urls = [adapter.feed_url(site, slug, n)]`,
  `full_text = true`, the group's `max_items` and `max_age_hours`) and goes through
  `sources/feeds.rs` unchanged; only the divider and the keys are new.
- **Kicker:** the paper's name; the section title is the slug's display name ("Obituaries").
- **Dedupe:** the existing rule, by URL, in `sections` order: whichever section comes first
  prints the story. With `sections = ["local", "subscribed:gazette.example", …]` Local keeps
  what its search found and the group prints the rest; the other way round the group wins.
- **Lead:** only Local leads by rule; a subscribed paper's stories count like a feed's.
- **Readership and the page plan:** one row per section, keyed `subscribed:<site>:<slug>`; a
  Drop removes that slug from `sections`. The group's tile on Tomorrow's paper shows the
  paper's name and the count of stories "in full".
- **Edition bundle:** `story.access` ∈ `"open" | "subscription" | "walled"` is added to
  `edition.json` next to the existing `partial` flag. The current version is 1.1
  (`bundle::VERSION`); this is 1.2, additive, with the schema's `description: "1.2"` marker
  and the DeckleReader TypeScript type updated to match.
- **Email and USB delivery** are unchanged: the stories are text in the EPUB like any other.

## 8. The e-edition

The paper as printed, for the devices that can show a page.

- **Where:** `eedition = "remarkable"` sends it through the reMarkable outbox beside the
  Deckle Daily's PDF; `eedition = "<export name>"` writes
  `<site>-eedition-YYYY-MM-DD.pdf` into that export folder (an iPad's Files, a Boox). Not to a
  Kindle: a newspaper broadsheet PDF is unreadable on a 6-inch screen, and the Scribe gets it
  through an export folder if the owner wants it.
- **When:** each morning run asks the adapter for today's edition. "No edition today" is not
  an error; the report says *no print edition today*. One PDF per edition date; a rerun doesn't
  send it twice (the outbox name is the key).
- **Size:** BLOX e-editions run tens of MB. `SiteClient` gets a `get_large` with a 100 MB
  cap, used by `eedition` only; anything larger is skipped with a note. The 20 MB page limit
  is untouched.
- **What it builds on:** the reMarkable outbox and the PDF export folders are on `main`
  (other-readers design, phase 4 built); this only adds a second PDF to them.
- **Prune:** export folders prune `<site>-eedition-*.pdf` by the folder's `keep`; the
  reMarkable can't prune (other-readers §9).
- **Not in this increment:** stitching page images into a PDF when the site offers no
  download (spike first, §11), and cropping a broadsheet page to a tablet's screen.

## 9. Reporting

- **Run report** (`digest run` JSON and the terminal line): per site, `signed_in`,
  `service`, `stories_full`, `stories_walled`, `login_error`, `eedition` (sent / none today /
  error). The password and cookies never appear; a test greps the formatted report.
- **A story that stays a headline** says why in its note, one of: *subscriber story (not
  signed in)*, *subscriber story (sign-in failed)*, *subscriber story (your plan doesn't
  include it)*, or the existing *unavailable today*.
- **Daily tab:** the Subscriptions card (§10) and the section list's count.

## 10. CLI and app surface

- `deckle digest subscribe [<site>] [--username <u>] [--stdin] [--forget] [--test]`.
- `deckle digest lint <url>` uses the session when the host has an account and prints
  *signed in as …* on its first line, so the owner can check a paper in one command.
- `deckle digest eedition [--date YYYY-MM-DD] [-o file.pdf]` fetches one by hand.
- **Daily tab → Delivery and sources → Subscriptions card:** one row per site with its
  username, a write-only password field, *Test sign-in*, the session state (signed in as, the
  service name, or the error), and *Forget*. **The paper → Your papers (subscribed):** add a
  site from the subscriptions list, tick its sections, set the e-edition target.

## 11. Spikes before code

1. **BLOX login by hand with `curl`** against gazette.example with the owner's account: the
   cookie names, the redirect, and what `services/` returns (the form fields are already known,
   §6.1). Record it in the build notes without the values. **The owner runs this**, since it
   needs the account; PR 2's fixtures start from the public form and a synthetic `services/`
   reply and are corrected from the capture before PR 2 merges.
2. **The e-edition download URL** for a subscriber: whole-edition PDF or per-page PDFs, and
   their sizes. Also the owner's. **PR 3 is conditional on it**: no download URL, no PR 3 in
   this increment (stitching page images is a later design).
3. **`ureq` 3 cookie store** in and out of a file: the `cookies` feature pulls in
   `cookie_store` with its `serde_json` form, and a jar is per `Agent`. Confirm a saved jar
   round-trips and expired cookies are dropped on load. Half a day; it only changes the file
   format if it fails.

## 12. Rulings (the owner's open questions, approved 2026-10-25)

1. **A subscribed paper doesn't feed Local.** Local stays the Google News search plus
   whatever feeds it lists; the Gazette gets its own group. The owner trims Local if it turns
   redundant.
2. **The e-edition is opt in** (`eedition = "remarkable"`), never sent by default; the files
   are big.
3. **Social-login-only papers are refused** with a clear message. No cookie-import escape
   hatch; revisit if a paper the owner cares about needs it.

## 13. Tests the build must add

- **Config:** defaults; the starter's commented blocks parse once uncommented; a
  `[[subscribed]]` whose site has no subscription builds with the "no subscription" note; a
  `papers/<slug>.toml` with `[[subscriptions]]` is rejected; unknown section slugs are rejected
  naming the known ones; round-trip leaves empty lists out.
- **Secret:** site entries beside the SMTP one in the fake keychain; the env fallback by site;
  no password in any `Debug`/`Display` of the report, the status, or the errors.
- **Session (fake `SiteClient` that records cookies and calls):** a stranger's page triggers
  one login then a refetch; a live jar skips the login; **eight stories fetched in parallel
  against a stranger's jar log in exactly once**; two failures mark the site and stop retries
  for the run; the jar is sent to the site and its subdomains only; the jar round-trips
  through its file and an expired cookie is dropped on load.
- **BLOX adapter (fixtures: a real BLOX page with the values replaced):** `detect`;
  `signed_in` on a stranger's page, a signed-in page without the service, and one with it;
  `unlock` decodes ROT47 and entities correctly, **and returns `None` without the service**;
  `extract` on the unlocked page yields the paragraphs, the byline and the lead photo; the
  login form's hidden fields are carried through the POST.
- **Section group:** `subscribed:<site>` prints the divider and the sections in order, keyed
  `subscribed:<site>:<slug>`; dedupe against Local both ways round; Readership rows and Drop;
  page-plan tiles.
- **E-edition:** sent once per date to the reMarkable outbox fake; none today is a note, not
  an error; over-size skipped with a note.
- **Bundle:** `access` present and right for the three cases; schema 1.2 validates.
- **CLI:** `subscribe` list/add/forget/test with the fake keychain; `lint` prints *signed in
  as*; `eedition -o`.

## 14. Sizing and phasing

L overall; ships in three PRs that each leave the app working:

1. **Accounts and sessions** (M): `[[subscriptions]]`, `secret.rs` generalized, `SessionNet`
   and its jar file, the `Publisher` trait with no adapter, `subscribe` CLI, the
   Subscriptions card, `lint` signed in. A site with an account reports "doesn't know how to
   sign in" and the paper builds as today. `Http` stays; the CLI and app construct
   `SessionNet` where they construct `Http` now, and the tests that pass closures keep
   passing closures.
2. **BLOX and the section group** (M): the adapter, `unlock` under the entitlement rule,
   `[[subscribed]]`, the group in the paper, Readership and page plan, bundle `access` (1.2).
   Merges after spike 1's capture has corrected the fixtures.
3. **E-edition** (S–M, only if spike 2 finds a download): fetch, outbox and export, prune,
   `eedition` CLI.

What CI can't check is a real sign-in and a real e-edition download. Those are the owner's
check with `deckle digest subscribe gazette.example --test` and `deckle digest eedition -o`.

## 15. Build notes, PR 1 (rulings made while building)

- **ureq features.** ureq 3 needs both `cookies` and `json` for the jar file. ureq's `save_json` persists only cookies with an expiry, so a site whose login cookie is session-only is signed in again on each run (the login dance handles a stale jar). Revisit after spike 1 shows BLOX's cookie attributes; a hand-written jar via `CookieJar::iter`/`insert` is the fallback.
- **Login cap.** Login attempts per site per run are capped at 2, successes included. A login that returns Ok but still serves a stranger's page counts, and is reported as `signed in, but the site still served a stranger's page`.
- **Network errors in `login`** count as a failed attempt (the trait can't tell them apart). Revisit in PR 2 with a typed login error.
- **Cached failures.** A failed entitlement fetch or home-page detect is cached for the run.
- **Keychain.** `Keyring` gained a `service` parameter; `SecretError::NotSet` carries the site so the hint names `deckle digest subscribe <site>`.
- **No section group yet.** `[[subscribed]]`'s section group is PR 2; in PR 1 every story on a subscribed site is extracted as a stranger's page, as today; one that doesn't extract prints as a headline with its reason (and a saved Your Reading link waits without spending a try).
- **`--forget`** also deletes the session file.
- **The Subscriptions card** sits in "The paper" room, not Delivery.

## 16. Build notes, PR 2 (rulings made while building)

- **Signed-in marker.** BLOX is signed in when `tn-user-screenname` is present and non-empty. Spike 1 confirms it against a live account.
- **Plan parsing.** The services parser is lenient. A plan that can't be read is `Walled::PlanUnknown`: nothing is unlocked and the story prints as a stranger sees it, with the note "couldn't read your plan".
- **Origin.** The login POST goes to the login form's own `action`, wherever it points. The services URL and the section feeds use the `https://www.<site>` rule (the configured host with `www.` added unless it already starts with it), not the form's action. Spike 1 confirms that `www` is the canonical origin for a live BLOX site.
- **Feed spares.** A subscribed slug's feed asks for `max_items` + 5 (`SUBSCRIBED_SPARE`), so a group after Local can refill after dropping what Local printed; the press and the preview share one URL (one cache key), and the cut still uses `max_items`.
- **A withheld page that extracts.** `Walled { NotInPlan | PlanUnknown }` is premium for certain: if its clear preview still extracts, it prints with the walled note (§9). With an unreadable plan, a page with nothing encrypted is `Open`; `PlanUnknown` is only for an encrypted page.
- **`unlock` fails closed.** If the page's id list can't be read it unlocks nothing; a page with nothing encrypted is returned unchanged.
- **Dedupe.** By title, in `sections` order. Local skips what an earlier section printed and refills; places keep their own. The page plan does the same. A slug listed twice in a group, a site in two groups, and `max_items = 0` are config errors.
- **One kind.** A single `subscribed` section kind covers every `[[subscribed]]` group, in file order. This supersedes the per-site entry in §3.
- **Slugs** are validated against BLOX's list (news, sports, opinion, obituaries, business, lifestyles).
- **Divider page deferred.** The kicker carries the paper's name instead.
- **`Subscription.service`** is the account's first service for now.
- **Typed login error** is deferred to PR 3 (network errors still count as a failed attempt).
- **DeckleReader follow-up:** read the bundle's new `story.access` (1.2).

### 16.1 What the live site turned out to do (read from its public scripts, 2026-10-26)

Found after PR 2's code was written, from `user.js` and the `tnt.access.*` scripts a BLOX page
loads; none of it needed an account, and all of it changes PR 2's entitlement source:

- **Sign-in state lives in cookies, not the page.** `user.js` fills `__tnt.user` from the
  cookies `tncms-authtoken`, `tncms-user`, `tncms-screenname`, `tncms-avatarurl`,
  `tncms-isadmin`. A stranger's page and a subscriber's page carry the same header markup
  (CSS picks a block); `span.tn-user-screenname` is empty for everyone until JS fills it. So
  `Blox::signed_in(html)` as built (a non-empty screen name) would never be true. The right
  marker is the jar: a `tncms-authtoken` (or `tncms-user`) cookie after the login POST.
- **The account's services are in the `tncms-user` cookie** (`__tnt.user.services`, a
  comma-separated list of service ids, URL-encoded), set by the server at login. The page's
  own scripts compare `__tnt.user.services` with a product's `service_id`.
  `/tncms/subscription/services/` is the **purchase catalog** the offers modal reads, not the
  account's entitlements: PR 2's `entitlement()` reads the wrong thing. It fails safe (a
  catalog carries no "active" flag, so every service counts inactive and premium stories stay
  walled), but a real subscriber gets nothing unlocked until this is fixed.
- **The unscramble is exactly ROT47** (`__tnt.subscription.api.unscramble`, applied to each
  `.subscriber-only.encrypted-content` element's `textContent`, then the `encrypted-content`
  class is removed and `#article-body[data-content-encrypted]` flips to `false`). It runs from
  the access-granted callback list; what grants access, and whether the server instead sends a
  subscriber the page already decrypted, is only visible with an account.
- **`paid_access_service_ids` appears on a real page once, inside a comment**
  (`// GTM tncms.subscription.paid_access_service_ids`); the page's required ids are
  `var bloxServiceIDs = [];` (empty on a stranger's page) and `data-subscription-service=""`
  on the offer links. PR 2's fail-closed rule therefore walls every premium page as built.
- **Spike 1 is now a blocker, not a confirmation.** `tools/blox-spike.sh <site> <user> <url>`
  runs the login with your own account and prints the shapes (cookie names, the decoded
  `tncms-user` cookie with its token redacted, whether the premium page arrives decrypted, and
  the catalog reply) without printing secrets. PR 2's follow-up then: `signed_in` from the
  jar's cookie names (add `SiteClient::has_cookie(name)`), `entitlement` from the `tncms-user`
  cookie's services (add `SiteClient::cookie(name)`; the adapter decodes it in-process and
  never logs it), the page's ids from `bloxServiceIDs = [...]` with the comment mention
  ignored, and the catalog endpoint dropped.
