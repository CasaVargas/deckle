# Publisher subscriptions, PR 1: accounts and sessions — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deckle can hold an account for a publisher (`[[subscriptions]]`, password in the
keychain), keep a signed-in cookie session per site across a run and across days, and report
that state from the CLI and the Daily tab. No adapter ships yet, so every subscriber story
still prints as a headline, now with a note that says why.

**Architecture:** `secret.rs` grows a second keychain service for sites. A new
`publisher.rs` defines the `Publisher` and `SiteClient` traits, `Access` and the (empty)
adapter registry. A new `session.rs` holds `SessionNet`, a wrapper around the anonymous
`Http` with one cookie-carrying `ureq::Agent` per site, a per-site mutex around login, a
failure counter, and a JSON jar file in the household folder. `Net` gains `get_page_as`,
which `full_article`, Your Reading and web articles call; the CLI and the app build a
`SessionNet` where they build `Http` today.

**Tech Stack:** Rust 2024 workspace; `ureq` 3 with its `cookies` feature (`cookie_store`
0.22, JSON save/load through `CookieJar::save_json` / `load_json`); `keyring` 3; `zeroize`;
`clap`; Tauri 2 + React/TS with vitest.

**Spec:** `docs/superpowers/specs/2026-10-25-publisher-subscriptions-design.md` (§3, §4, §5,
§6 trait only, §9, §10, §14 PR 1).

## Global Constraints

- Keychain service for sites: `com.casavargas.deckle.site`; account `<username>@<site>`.
- Headless env fallback: `DECKLE_SITE_PASSWORD_<SITE>` where `<SITE>` is the site uppercased
  with every non-alphanumeric character as `_` (`thepilot.com` → `THEPILOT_COM`); read only
  when there is no keychain at all, as `DECKLE_SMTP_PASSWORD` is.
- A password never appears in `digest.toml`, a report, a log, an error string, a `Debug`
  output or a Tauri result.
- The jar file: `sessions/<site>.json` beside `digest.toml` (household, not a paper's state
  folder), written to `<file>.tmp` then renamed, mode 0600 on Unix.
- Two login failures in one process stop every later login attempt for that site.
- Only the site's host and its subdomains ever see the jar; every other host is fetched by
  the anonymous `Http`.
- `[[subscriptions]]` is a household key: a `papers/<slug>.toml` that sets it is refused with
  `papers/<slug>.toml: [[subscriptions]] is set in digest.toml for every paper`.
- `deny_unknown_fields` on every new table; saves go through the `toml_edit` merge and leave
  an empty list out.
- Page fetch limit stays 20 MB; signed-in fetches use `BROWSER_AGENT`.
- The browser user agent and timeouts in `fetch.rs` are unchanged.

## Review Focus

1. `site = "https://www.thepilot.com/"` or `"ThePilot.com"`: the config loader normalizes to
   a bare lowercase host without `www.`, or rejects a value with a scheme or path with a
   message naming the field. (Task 1.)
2. A story URL on `www.thepilot.com` when `site = "thepilot.com"`: the session matches the
   host itself and any subdomain, never `notthepilot.com`. (Task 5.)
3. A corrupt or stale `sessions/<site>.json`: the run starts with an empty jar and a note,
   never a crash or a refused run. (Task 4.)
4. A scheduled 05:30 run on a locked keychain: `Walled { NotSignedIn }` with the
   reason in the site status, the paper builds, nothing panics. (Task 5.)
5. A username or password with `&`, `+` or non-ASCII characters reaches the login form
   percent-encoded; the adapter trait's `login` receives the plain values. (Task 3's
   contract, pinned in Task 5's fake adapter test.)

---

### Task 1: `[[subscriptions]]` in the config

**Files:**
- Modify: `crates/deckle-digest/src/config.rs` (the `Config` struct near line 57, `STARTER`
  near line 1063 beside the `[email]` block, `save` near line 1183)
- Modify: `crates/deckle-digest/src/papers.rs` (`household_key` at line 63, `load_extra`)
- Test: `crates/deckle-digest/src/config.rs` tests module, `crates/deckle-digest/src/papers.rs`
  tests module

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(deny_unknown_fields)]
  pub struct SubscriptionConfig {
      pub site: String,                 // bare lowercase host, no scheme, path or `www.`
      pub username: String,
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub kind: Option<String>,         // "blox" once detected; None until then
  }
  impl SubscriptionConfig {
      pub fn validate(&self) -> Result<(), String>;
      pub fn normalized_site(raw: &str) -> Result<String, String>;
  }
  // on Config:
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub subscriptions: Vec<SubscriptionConfig>,
  pub fn subscription(&self, host: &str) -> Option<&SubscriptionConfig>; // host or subdomain match
  ```

- [ ] **Step 1: Write the failing tests** in `config.rs`'s tests:

```rust
#[test]
fn subscriptions_parse_and_round_trip() {
    let text = "[[subscriptions]]\nsite = \"thepilot.com\"\nusername = \"me@example.com\"\n";
    let cfg = Config::parse(text).unwrap();
    assert_eq!(cfg.subscriptions.len(), 1);
    assert_eq!(cfg.subscriptions[0].site, "thepilot.com");
    assert_eq!(cfg.subscriptions[0].kind, None);
    assert!(cfg.subscription("www.thepilot.com").is_some());
    assert!(cfg.subscription("thepilot.com").is_some());
    assert!(cfg.subscription("notthepilot.com").is_none());
    let empty = Config::default();
    assert!(!toml::to_string(&empty).unwrap().contains("subscriptions"));
}

#[test]
fn a_site_is_a_bare_host() {
    assert_eq!(SubscriptionConfig::normalized_site("https://www.ThePilot.com/").unwrap(), "thepilot.com");
    assert_eq!(SubscriptionConfig::normalized_site("thepilot.com").unwrap(), "thepilot.com");
    let e = SubscriptionConfig::normalized_site("thepilot.com/news").unwrap_err();
    assert!(e.contains("site"), "{e}");
    let cfg = Config::parse("[[subscriptions]]\nsite = \"thepilot.com\"\nusername = \"\"\n").unwrap();
    assert!(cfg.subscriptions[0].validate().unwrap_err().contains("username"));
}

#[test]
fn the_starter_subscriptions_block_parses_once_uncommented() {
    // Same shape as the existing `[email]` starter test: uncomment the block, parse, assert
    // one subscription for "thepilot.com".
}
```

And in `papers.rs`'s tests:

```rust
#[test]
fn a_paper_file_cant_set_subscriptions() {
    // write papers/kid.toml with a [[subscriptions]] table; load_all must Err with
    // "papers/kid.toml: [[subscriptions]] is set in digest.toml for every paper"
}

#[test]
fn a_paper_inherits_the_households_subscriptions() {
    // main has one subscription; papers/kid.toml has none; the loaded kid config has it
}
```

- [ ] **Step 2: Run them to make sure they fail**

Run: `cargo test -p deckle-digest subscriptions a_site_is_a_bare_host a_paper_file_cant_set a_paper_inherits`
Expected: compile errors (`SubscriptionConfig` undefined).

- [ ] **Step 3: Implement.** `normalized_site`: trim, strip `http://`/`https://`, cut at the
  first `/`, `?` or `#` (an error if anything followed the host), lowercase, strip a leading
  `www.`; reject empty or a host with spaces or without a dot. Call it from
  `Config::parse`'s normalize path so a loaded site is always normalized. `validate`:
  non-empty username. `Config::subscription(host)`: `host == site || host.ends_with(&format!(".{site}"))`.
  In `papers.rs`: add `"subscriptions"` to the `household_key` loop (message as the Global
  Constraints say) and copy `main.subscriptions` in `load_extra`; in `config::save`, remove
  `subscriptions` from an extra paper's document beside `sync` and `reading`. Add the
  commented starter block from spec §3 (the `[[subscriptions]]` one only) after `[email]`.
  Call `validate` for each entry from `Config::validate` and `save`.

- [ ] **Step 4: Run the tests and the crate's existing config tests**

Run: `cargo test -p deckle-digest config:: papers::`
Expected: PASS, including the existing starter round-trip tests.

- [ ] **Step 5: Commit**

```bash
git add crates/deckle-digest/src/config.rs crates/deckle-digest/src/papers.rs
git commit -m "feat(daily): [[subscriptions]] household config for publisher accounts"
```

### Task 2: Site passwords in the keychain

**Files:**
- Modify: `crates/deckle-digest/src/secret.rs`
- Test: its tests module

**Interfaces:**
- Consumes: `SubscriptionConfig` (Task 1).
- Produces:
  ```rust
  pub const SITE_SERVICE: &str = "com.casavargas.deckle.site";
  pub fn site_account(sub: &SubscriptionConfig) -> String;          // "<username>@<site>"
  pub fn site_env(site: &str) -> String;                            // "DECKLE_SITE_PASSWORD_THEPILOT_COM"
  pub fn site_password(sub: &SubscriptionConfig, keys: &dyn Keyring, env: &dyn Fn(&str) -> Option<String>)
      -> Result<(Zeroizing<String>, Source), SecretError>;
  pub fn has_site_password(sub, keys, env) -> bool;
  pub fn set_site(sub, keys, password: &str) -> Result<(), SecretError>;
  pub fn forget_site(sub, keys) -> Result<bool, SecretError>;
  // Keyring gains a service parameter so one trait serves both stores:
  pub trait Keyring {
      fn get(&self, service: &str, account: &str) -> Result<Option<Zeroizing<String>>, KeyringError>;
      fn set(&self, service: &str, account: &str, password: &str) -> Result<(), KeyringError>;
      fn delete(&self, service: &str, account: &str) -> Result<bool, KeyringError>;
  }
  ```
  `SecretError::NotSet` gains a `hint: &'static str` (the CLI command to run), so the SMTP
  message stays `deckle digest email password` and the site one says
  `deckle digest subscribe <site>`.

- [ ] **Step 1: Write the failing tests** (mirroring the SMTP ones):

```rust
#[test]
fn site_entries_live_in_their_own_service() {
    assert_eq!(SITE_SERVICE, "com.casavargas.deckle.site");
    assert_eq!(site_account(&sub()), "me@example.com@thepilot.com");
    assert_eq!(site_env("thepilot.com"), "DECKLE_SITE_PASSWORD_THEPILOT_COM");
    let keys = MemoryKeyring::default();
    set(&cfg(), &keys, "smtp-pw").unwrap();
    assert!(!has_site_password(&sub(), &keys, &env_with(None)), "the SMTP entry isn't a site entry");
}

#[test]
fn site_password_keychain_then_env_then_error() {
    // keychain wins; keychain without entry => NotSet whose message contains
    // "deckle digest subscribe thepilot.com"; unavailable keychain reads the site env var
}

#[test]
fn site_errors_never_carry_the_password() { /* same grep as the SMTP test, over site fns */ }
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p deckle-digest secret::` → compile errors.

- [ ] **Step 3: Implement.** Thread `service` through `Keyring`, `SystemKeyring::entry` and
  `MemoryKeyring` (key the map by `format!("{service}\0{account}")`). Update the SMTP
  functions and their callers (`mail.rs`, `daily.rs` in the app, `lib.rs` in the CLI) to pass
  `SERVICE`. Site functions mirror the SMTP ones; `site_env` is
  `format!("DECKLE_SITE_PASSWORD_{}", site.to_ascii_uppercase().replace(|c: char| !c.is_ascii_alphanumeric(), "_"))`.

- [ ] **Step 4: Run the whole workspace's tests that touch secrets**

Run: `cargo test -p deckle-digest secret:: mail:: && cargo test -p deckle-cli digest_email && cargo test -p deckle-desktop email_status`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/deckle-digest/src/secret.rs crates/deckle-digest/src/mail.rs crates/deckle-cli/src/lib.rs apps/desktop/src-tauri/src/daily.rs
git commit -m "feat(daily): site passwords in the keychain beside the SMTP one"
```

### Task 3: The `Publisher` trait, `Access`, and `Net::get_page_as`

**Files:**
- Create: `crates/deckle-digest/src/publisher.rs`
- Modify: `crates/deckle-digest/src/fetch.rs` (the `Net` trait), `crates/deckle-digest/src/lib.rs`
  (`pub mod publisher;`, re-export `Access`)
- Test: `publisher.rs` tests module

**Interfaces:**
- Produces (verbatim from spec §5 and §6, with `get_large` left to PR 3):
  ```rust
  pub trait SiteClient: Sync {
      fn get(&self, url: &str) -> Result<Vec<u8>, String>;
      fn post_form(&self, url: &str, body: &str) -> Result<Vec<u8>, String>;
  }
  pub struct Entitlement { pub services: Vec<String> }
  pub trait Publisher: Sync {
      fn kind(&self) -> &'static str;
      fn detect(&self, html: &str) -> bool;
      fn signed_in(&self, html: &str) -> bool;
      fn login(&self, site: &str, user: &str, pw: &str, c: &dyn SiteClient) -> Result<(), String>;
      fn entitlement(&self, site: &str, c: &dyn SiteClient) -> Result<Entitlement, String>;
      fn unlock(&self, html: &str, ent: &Entitlement) -> Option<String>;
      fn sections(&self) -> &[(&'static str, &'static str)];
      fn feed_url(&self, site: &str, slug: &str, n: usize) -> String;
  }
  pub fn registry() -> Vec<Box<dyn Publisher>>;      // empty in PR 1
  pub fn detect_kind(html: &str) -> Option<&'static str>;
  #[derive(Clone, Debug, PartialEq, Eq)]
  pub enum Access { Open, Subscription { site: String, service: String }, Walled { site: String, why: Walled } }
  #[derive(Clone, Debug, PartialEq, Eq)]
  pub enum Walled { NotSignedIn, LoginFailed(String), NotInPlan, NoAdapter }
  impl Walled { pub fn note(&self, site: &str) -> String; }
  // notes, exact copy:
  //   NotSignedIn   -> "Subscriber story (not signed in to <site>)."
  //   LoginFailed(e)-> "Subscriber story (sign-in to <site> failed: <e>)."
  //   NotInPlan     -> "Subscriber story (your <site> plan doesn't include it)."
  //   NoAdapter     -> "Subscriber story (Deckle doesn't know how <site> signs in yet)."
  // on Net:
  fn get_page_as(&self, url: &str) -> Result<(Vec<u8>, Access), String> { self.get_page(url).map(|b| (b, Access::Open)) }
  ```
  `Walled::LoginFailed`'s string never contains a password: it is the adapter's own message.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn the_registry_is_empty_and_nothing_is_detected() {
    assert!(registry().is_empty());
    assert_eq!(detect_kind("<html>tncms</html>"), None);
}
#[test]
fn walled_notes() {
    assert_eq!(Walled::NotSignedIn.note("thepilot.com"), "Subscriber story (not signed in to thepilot.com).");
    assert_eq!(Walled::NoAdapter.note("thepilot.com"), "Subscriber story (Deckle doesn't know how thepilot.com signs in yet).");
}
#[test]
fn a_closure_net_is_open() {
    let net = |_: &str| Ok(b"<p>hi</p>".to_vec());
    let (b, a) = net.get_page_as("https://x.test/").unwrap();
    assert_eq!(a, Access::Open);
    assert_eq!(b, b"<p>hi</p>");
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p deckle-digest publisher::` → compile errors.
- [ ] **Step 3: Implement** `publisher.rs` as the interface says; `detect_kind` asks each
  registry entry's `detect`.
- [ ] **Step 4: Run** `cargo test -p deckle-digest publisher:: fetch::` → PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(daily): Publisher trait, Access, and Net::get_page_as"`.

### Task 4: `UreqSite`: one cookie-carrying agent per site, with a jar file

**Files:**
- Create: `crates/deckle-digest/src/session.rs` (this task: the client and the jar; Task 5
  adds `SessionNet` to the same file)
- Modify: `Cargo.toml` (workspace): `ureq = { version = "3", features = ["platform-verifier", "cookies"] }`
- Test: `session.rs` tests module

**Interfaces:**
- Consumes: `SiteClient` (Task 3); `BROWSER_AGENT`, `LIMIT`, `read` from `fetch.rs` (make
  them `pub(crate)`).
- Produces:
  ```rust
  pub struct UreqSite { site: String, agent: ureq::Agent }
  impl UreqSite {
      pub fn new(site: &str) -> Self;                           // 25 s timeout, BROWSER_AGENT, platform roots, cookies on
      pub fn load_jar(&self, path: &Path) -> Result<(), String>; // missing file: Ok; bad JSON: Err (caller notes it, keeps going)
      pub fn save_jar(&self, path: &Path) -> Result<(), String>; // tmp + rename, 0600
      pub fn clear_jar(&self);
      pub fn cookie_names(&self) -> Vec<String>;                // for tests and status, never values
  }
  impl SiteClient for UreqSite { … }                           // get / post_form through `agent`
  pub fn jar_path(household_dir: &Path, site: &str) -> PathBuf; // household_dir/sessions/<site>.json
  ```

- [ ] **Step 1: Write the failing tests.** Use a `std::net::TcpListener` on `127.0.0.1:0`
  that answers one request with `Set-Cookie: sess=abc; Path=/` and records the request
  headers (a 30-line helper in the tests module; no new dev-dependency).

```rust
#[test]
fn the_jar_round_trips_through_its_file() {
    // GET the fixture server once (it sets `sess`), save_jar to a tempdir, new UreqSite,
    // load_jar, assert cookie_names() == ["sess"]; on Unix assert the file mode is 0o600.
}
#[test]
fn a_bad_jar_file_is_an_error_not_a_panic() {
    // write "not json" to the path; load_jar returns Err containing "sessions/"
    // and cookie_names() is empty afterwards.
}
#[test]
fn a_missing_jar_file_is_fine() { /* load_jar on a path that doesn't exist => Ok */ }
#[test]
fn the_cookie_goes_back_only_to_its_host() {
    // after the Set-Cookie from 127.0.0.1, a second GET to the same server carries
    // "Cookie: sess=abc"; a GET through a fresh UreqSite for another site doesn't.
}
```

- [ ] **Step 2: Run** `cargo test -p deckle-digest session::` → compile errors.
- [ ] **Step 3: Implement.** The agent: `ureq::Agent::config_builder()` as `fetch::agent`
  does; with the `cookies` feature on, every `Agent` owns its own `SharedCookieJar` (ureq
  3.5 `agent.rs:154`), so one `UreqSite` = one agent = one jar, and there is no switch to
  set. (The anonymous `Http` builds a fresh agent per request, so it keeps no cookies, as
  today.) Jar I/O through
  `agent.cookie_jar_lock().save_json(&mut file)` / `.load_json(reader)`; `load_json` after
  `clear()` so a reload replaces. Write the temp file with `OpenOptions` and
  `std::os::unix::fs::OpenOptionsExt::mode(0o600)` under `#[cfg(unix)]`.
- [ ] **Step 4: Run** `cargo test -p deckle-digest session::` → PASS; `cargo build --workspace` still builds (the `cookies` feature is additive).
- [ ] **Step 5: Commit** — `git commit -m "feat(daily): per-site cookie client with a household jar file"`.

### Task 5: `SessionNet`

**Files:**
- Modify: `crates/deckle-digest/src/session.rs`
- Modify: `crates/deckle-digest/src/lib.rs` (`pub use session::SessionNet;`)
- Test: `session.rs` tests module

**Interfaces:**
- Consumes: `Http`, `Net` (fetch.rs); `Publisher`, `SiteClient`, `Access`, `Walled`,
  `registry` (Task 3); `UreqSite`, `jar_path` (Task 4); `SubscriptionConfig` (Task 1);
  `site_password`, `Keyring`, `Source` (Task 2).
- Produces:
  ```rust
  pub struct SiteAccount { pub site: String, pub username: String, pub kind: Option<String>,
                           pub password: Option<Zeroizing<String>>, pub password_from: Option<Source>,
                           pub password_error: Option<String> }
  #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
  pub enum SessionState { NotTried, SignedIn { service: Option<String> }, LoginFailed(String), NoAdapter, NoPassword(String) }
  #[derive(Clone, Debug, Serialize)]
  pub struct SiteStatus { pub site: String, pub username: String, pub kind: Option<String>,
                          pub has_password: bool, pub state: SessionState, pub jar_note: Option<String> }
  pub struct SessionNet { … }   // as spec §5, with `client: Box<dyn SiteClient>` per site
  impl SessionNet {
      pub fn new(inner: Http, accounts: Vec<SiteAccount>, adapters: Vec<Box<dyn Publisher>>, jar_dir: Option<PathBuf>) -> Self;
      pub fn with_clients(inner: Http, sites: Vec<(SiteAccount, Box<dyn SiteClient>)>, adapters: Vec<Box<dyn Publisher>>) -> Self; // tests
      pub fn from_config(config_path: &Path, subs: &[SubscriptionConfig], keys: &dyn Keyring, env: &dyn Fn(&str) -> Option<String>) -> Self;
      pub fn site_for(&self, url: &str) -> Option<String>;        // the account whose site matches the URL's host or a subdomain
      pub fn status(&self) -> Vec<SiteStatus>;
      pub fn save_jars(&self) -> Vec<String>;                     // notes for anything that failed to save
      pub fn sign_in(&self, site: &str) -> SessionState;          // force a login now (the CLI/app "test"); counts toward failures
  }
  impl Net for SessionNet { get/get_page/post_form -> inner; get_page_as -> the dance }
  ```
  `get_page_as` for a URL with an account, in order: fetch through the site's client; with no
  adapter for `kind` → `(bytes, Walled NoAdapter)`; adapter `signed_in` → `entitlement`
  (once per run, cached in `Site`), `unlock` → `Subscription { service }` or
  `Walled NotInPlan`; a stranger's page → lock the site's mutex, re-check `state`: if
  `failures >= 2` → `Walled LoginFailed(last)`; no password → `Walled NotSignedIn`; else
  `login`, on `Ok` refetch once and continue as signed in, on `Err` bump `failures`, record,
  return `Walled LoginFailed(e)`. Errors from the network itself are `Err`, as `get_page` is.

- [ ] **Step 1: Write the failing tests** with a test-only `FakeSite: SiteClient` (an
  `Arc<Mutex<State>>` with `signed_in: bool`, `logins: Vec<(String /*url*/, String /*body*/)>`,
  `accept: bool`, serving `"<html>USER</html>"` when signed in and `"<html>LOGIN</html>"`
  otherwise) and a `FakeBlox: Publisher` (`kind() == "fake"`, `signed_in` = contains
  `USER`, `login` posts `username=…&password=…` through the client and `Err("bad password")`
  when the fake refuses, `entitlement` = `["plus"]`, `unlock` = `Some(html + "<!--unlocked-->")`
  only when `ent.services` contains `"plus"`).

```rust
#[test] fn a_strangers_page_logs_in_once_then_refetches() { /* logins.len()==1, Access::Subscription{service:"plus"}, body ends with "<!--unlocked-->" */ }
#[test] fn a_live_session_skips_the_login() { /* signed_in=true from the start: logins is empty */ }
#[test] fn eight_parallel_stories_log_in_exactly_once() { /* feeds::par_map over 8 URLs; logins.len()==1 */ }
#[test] fn two_failures_stop_the_retries() { /* accept=false; 5 fetches => logins.len()==2, all Walled LoginFailed("bad password") */ }
#[test] fn no_password_means_not_signed_in_without_a_login_attempt() { /* password None => Walled NotSignedIn, logins empty, status().state == NoPassword(..) */ }
#[test] fn an_account_without_an_adapter_is_walled_no_adapter() { /* kind None and empty adapters */ }
#[test] fn only_the_site_and_its_subdomains_use_the_session() {
    // site_for("https://www.thepilot.com/a") == Some("thepilot.com");
    // site_for("https://notthepilot.com/") == None; get_page_as there is Access::Open through inner
}
#[test] fn the_login_form_carries_the_credentials_encoded() { /* username "a&b@x.test", password "p+w d": the posted body contains "username=a%26b%40x.test" and "password=p%2Bw+d" (fetch::encode) */ }
#[test] fn status_never_carries_the_password() { /* format!("{:?}", status()) and serde_json of it lack the password */ }
#[test] fn from_config_reads_passwords_and_notes_a_bad_jar() { /* MemoryKeyring + tempdir with a bad sessions/<site>.json: status().jar_note is Some */ }
```

- [ ] **Step 2: Run** `cargo test -p deckle-digest session::` → compile errors.
- [ ] **Step 3: Implement** as the interface and spec §5 say. `from_config` builds
  `UreqSite`s, loads each jar (a failure becomes `jar_note`), and reads each password with
  `site_password` (an error becomes `password_error`, shown as `NoPassword(msg)`). Pick the
  adapter by `kind`; with `kind == None`, fetch the site's home page once through the client
  and `detect_kind` it (PR 1: always `None` → `NoAdapter`). The URL's host comes from
  `url::Url::parse`.
- [ ] **Step 4: Run** `cargo test -p deckle-digest session::` → PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(daily): SessionNet, signed-in fetches per site"`.

### Task 6: Stories go through `get_page_as`

**Files:**
- Modify: `crates/deckle-digest/src/sources/feeds.rs` (`full_article`, line 197),
  `crates/deckle-digest/src/sources/reading.rs` (line 84), `crates/deckle-digest/src/web_book.rs`
  (line 103), `crates/deckle-digest/src/lib.rs` (`Article`: add `pub access: Access` to `Extra`
  with `Default` = `Open`)
- Test: `crates/deckle-digest/tests/build.rs` (existing fixtures), `feeds.rs` tests

**Interfaces:**
- Consumes: `Net::get_page_as`, `Access`, `Walled::note` (Task 3).
- Produces: `Extra.access: Access`; a walled story's note is `Walled::note(site)` where
  `UNAVAILABLE` is used today, and the story's body is the feed's paragraphs, as today.

- [ ] **Step 1: Write the failing tests** in `feeds.rs`:

```rust
#[test]
fn a_walled_story_prints_the_feeds_text_with_the_reason() {
    // a Net whose get_page_as returns (b"<html>LOGIN</html>", Walled{site:"thepilot.com", why: NotSignedIn});
    // full_article => body from item.paragraphs, meta/note contains
    // "Subscriber story (not signed in to thepilot.com).", extra.access == that Walled
}
#[test]
fn an_open_story_is_unchanged() { /* closure Net: extra.access == Open; same body as before this task */ }
```

- [ ] **Step 2: Run** `cargo test -p deckle-digest feeds::` → compile errors.
- [ ] **Step 3: Implement.** In `full_article`, swap `net.get_page(&url)` for
  `net.get_page_as(&url)`; on `Walled` skip extraction and return `article(item, Some(&note))`
  with `extra.access` set; on `Subscription`/`Open` continue as today and set `extra.access`.
  Same one-line swap in `reading.rs` and `web_book.rs`; there, a `Walled` page falls into the
  existing "no readable article" error with the note appended: `"no readable article: <note>"`.
- [ ] **Step 4: Run** `cargo test -p deckle-digest` (whole crate: fixtures use closure nets, so
  everything stays `Open`) → PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(daily): stories fetch through the site session; walled ones say why"`.

### Task 7: The CLI: `digest subscribe`, and `SessionNet` where `Http` was

**Files:**
- Modify: `crates/deckle-cli/src/cli.rs` (`DigestCommand`, after `Email` at line 507),
  `crates/deckle-cli/src/lib.rs` (dispatch near line 2738; `build_gathered` call sites at
  2277, 2388, 2413; `next` at 2698; `digest_lint` at 1050)
- Test: `crates/deckle-cli/tests/cli.rs`

**Interfaces:**
- Consumes: `SessionNet::from_config`, `status`, `sign_in`, `save_jars` (Task 5);
  `set_site`, `forget_site`, `site_account` (Task 2); `SubscriptionConfig` (Task 1).
- Produces:
  ```rust
  /// Papers you subscribe to: sign Deckle in with your own account.
  Subscribe {
      /// The paper's site, e.g. thepilot.com. Alone: list every subscription and its state.
      site: Option<String>,
      #[arg(long)] username: Option<String>,
      /// Read the password from one line of stdin instead of asking.
      #[arg(long)] stdin: bool,
      /// Sign in now and report the result, without changing anything.
      #[arg(long)] test: bool,
      /// Remove the account and its keychain entry.
      #[arg(long)] forget: bool,
  }
  fn household_net(path: &Path, cfg: &Config) -> SessionNet;   // from_config with SystemKeyring and std::env
  ```
  Behavior: `subscribe` alone lists `<site>  <username>  password: yes|no  session: <state>`
  per entry (JSON: `Vec<SiteStatus>`), "no subscriptions" when none. `subscribe <site>`
  with `--username` (required when the site is new) adds or updates the entry (normalized
  site), saves the config, then reads the password (no echo, prompt `Password for
  <username> at <site>: `, or `--stdin`), saves it, then signs in once and prints the state.
  `--test` only signs in. `--forget` deletes the keychain entry and the config entry.

- [ ] **Step 1: Write the failing tests** (the CLI tests avoid the real keychain as the email
  ones do: everything below stops before it or only lists):

```rust
#[test] fn digest_subscribe_is_listed_with_its_flags() { /* `digest subscribe --help` mentions username, stdin, test, forget */ }
#[test] fn digest_subscribe_lists_none_on_a_starter_config() { /* stdout contains "no subscriptions" */ }
#[test] fn digest_subscribe_needs_a_username_for_a_new_site() { /* `subscribe thepilot.com --stdin` with no --username: exit 1, stderr contains "--username" */ }
#[test] fn digest_subscribe_normalizes_the_site_in_the_config() {
    // `subscribe https://www.ThePilot.com/ --username me@x.test --forget` on a starter:
    // --forget on an unknown site exits 1 with "no subscription for thepilot.com" (normalized name in the message)
}
```

- [ ] **Step 2: Run** `cargo test -p deckle-cli digest_subscribe` → FAIL (unknown subcommand).
- [ ] **Step 3: Implement** the command (model on `digest_email`), `household_net`, and
  replace `&deckle_digest::Http` with `&household_net(&path, &cfg)` at the four build/next
  sites. `digest_lint`: load the config when the file exists (else plain `Http`), use
  `get_page_as`, and print `signed in as <username> at <site>` (or the `Walled` note) as the
  first text line / an `"access"` field in JSON. After a run or build, call `save_jars()` and
  print its notes to stderr.
- [ ] **Step 4: Run** `cargo test -p deckle-cli` → PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(cli): deckle digest subscribe; runs use the site sessions"`.

### Task 8: The app: Tauri commands and the Subscriptions card

**Files:**
- Modify: `apps/desktop/src-tauri/src/daily.rs` (beside `email_status` at line 390),
  `apps/desktop/src-tauri/src/lib.rs` (register the commands at line 78),
  `apps/desktop/src/types.ts` (beside `EmailStatus`), `apps/desktop/src/api.ts` (line 124),
  `apps/desktop/src/DailyView.tsx` (a card after `emailCard`)
- Create: `apps/desktop/src/subscriptions.ts` (pure helpers), `apps/desktop/src/subscriptions.test.ts`
- Test: `daily.rs` tests; vitest

**Interfaces:**
- Consumes: `SessionNet`, `SiteStatus`, `SessionState` (Task 5); `set_site`, `forget_site` (Task 2).
- Produces (Rust, each a `#[tauri::command]`):
  ```rust
  pub fn subscriptions_status() -> Result<Vec<SiteStatus>, String>;        // never a password
  pub fn subscription_set_password(site: String, password: String) -> Result<Vec<SiteStatus>, String>;
  pub fn subscription_test(site: String) -> Result<SessionState, String>;   // sign_in now, async like email_test
  pub fn subscription_forget(site: String) -> Result<Vec<SiteStatus>, String>;
  ```
  TS: `SiteStatus`, `SessionState` (a tagged union mirroring serde's default
  externally-tagged form), `api.subscriptionsStatus/subscriptionSetPassword/subscriptionTest/subscriptionForget`,
  and `subscriptionLine(s: SiteStatus): string` with exact copy:
  - `NotTried` → `Not signed in yet`
  - `SignedIn { service: "Pilot Plus" }` → `Signed in · Pilot Plus, active`; `service: null` → `Signed in`
  - `LoginFailed(e)` → `Sign-in failed: <e>`
  - `NoAdapter` → `Deckle doesn't know how <site> signs in yet`
  - `NoPassword(e)` → `No password: <e>`
  The card: rows from the draft's `subscriptions` (site, username inputs; add / remove), a
  write-only password field per row with *Save password*, *Test sign-in*, the
  `subscriptionLine`, and `jar_note` in muted text when present. Saving the draft saves the
  config as every other card does.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn subscriptions_status_never_returns_the_password() {
    // MemoryKeyring with a site password; status_with(&cfg, &keys, &env) serialized to JSON
    // has has_password: true and no occurrence of the password.
}
```
```ts
test("subscription lines", () => {
  expect(subscriptionLine({ ...s, state: "NotTried" })).toBe("Not signed in yet");
  expect(subscriptionLine({ ...s, state: { SignedIn: { service: "Pilot Plus" } } })).toBe("Signed in · Pilot Plus, active");
  expect(subscriptionLine({ ...s, state: { LoginFailed: "bad password" } })).toBe("Sign-in failed: bad password");
  expect(subscriptionLine({ ...s, site: "thepilot.com", state: "NoAdapter" })).toBe("Deckle doesn't know how thepilot.com signs in yet");
});
```

- [ ] **Step 2: Run** `cargo test -p deckle-desktop subscriptions_status` and `npm --prefix apps/desktop test -- subscriptions` → FAIL.
- [ ] **Step 3: Implement** the commands (a `subscriptions_status_with(cfg, keys, env)` the
  test calls, like `email_status_with`), the TS types, the helper and the card.
- [ ] **Step 4: Run** both test commands → PASS; then `npm --prefix apps/desktop run build` → no type errors; open the app (`cargo tauri dev` or the project's run skill) and check the card renders with one row, saves a site, and *Test sign-in* shows `Deckle doesn't know how thepilot.com signs in yet`.
- [ ] **Step 5: Commit** — `git commit -m "feat(app): Subscriptions card on the Daily tab"`.

### Task 9: Docs

**Files:**
- Modify: `docs/daily.md` (a "Papers you subscribe to" section after "Email to Kindle"),
  `CHANGELOG.md`, `README.md` (one line under "The Daily (preview)")

- [ ] **Step 1: Write** the section: the config block from spec §3, the `subscribe` command
  forms, where the password lives, the jar file and how to delete it, and that no paper is
  supported yet ("The first paper, The Pilot and other BLOX sites, comes in the next
  release"). CHANGELOG under Unreleased: "The Daily: accounts for the papers you subscribe to
  (`deckle digest subscribe`), with the password in the keychain. No publisher is supported
  yet; stories still print as headlines, now saying why."
- [ ] **Step 2: Check** `cargo test --workspace --no-fail-fast` (PDFium tests may fail in a
  fresh worktree; that is environmental, see the Tolaria gotcha note) and
  `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] **Step 3: Commit** — `git commit -m "docs(daily): subscriptions"`, push the branch, open
  the PR with `--repo CasaVargas/deckle --base main --head <branch>`, body `Closes` nothing
  (no issue yet; the spec PR is #177).
