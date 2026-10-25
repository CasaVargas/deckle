// SPDX-License-Identifier: GPL-3.0-or-later
//! Signed-in sessions for publisher sites: one cookie-carrying agent per site, with its
//! cookie jar kept in `sessions/<site>.json` beside `digest.toml`.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use serde::Serialize;
use zeroize::Zeroizing;

use crate::config::SubscriptionConfig;
use crate::fetch::{BROWSER_AGENT, Http, Net, agent, read};
use crate::publisher::{Access, Entitlement, Publisher, SiteClient, Walled, registry};
use crate::secret::{Keyring, Source, site_password};

/// Where a site's cookie jar lives: `household_dir/sessions/<site>.json`.
pub fn jar_path(household_dir: &Path, site: &str) -> PathBuf {
    household_dir.join("sessions").join(format!("{site}.json"))
}

/// One site's HTTP client. With ureq's `cookies` feature every `Agent` owns its own jar,
/// so one `UreqSite` is one agent is one jar.
pub struct UreqSite {
    #[allow(dead_code)]
    site: String,
    agent: ureq::Agent,
}

impl UreqSite {
    pub fn new(site: &str) -> Self {
        Self {
            site: site.to_string(),
            agent: agent(BROWSER_AGENT, 25),
        }
    }

    /// Replaces the jar with the file's cookies. A missing file is fine; a corrupt one is
    /// an error naming the path, and the jar is left empty.
    pub fn load_jar(&self, path: &Path) -> Result<(), String> {
        let mut jar = self.agent.cookie_jar_lock();
        jar.clear();
        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        if let Err(e) = jar.load_json(BufReader::new(file)) {
            jar.clear();
            return Err(format!("{}: unreadable cookie jar: {e}", path.display()));
        }
        Ok(())
    }

    /// Writes the jar to `<path>.tmp`, then renames it into place (0600 on Unix).
    pub fn save_jar(&self, path: &Path) -> Result<(), String> {
        let err = |e: std::io::Error| format!("{}: {e}", path.display());
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(err)?;
        }
        let mut tmp_name = path.as_os_str().to_owned();
        tmp_name.push(".tmp");
        let tmp = PathBuf::from(tmp_name);
        // A stale temp file would keep its old mode; start from nothing.
        let _ = std::fs::remove_file(&tmp);
        let written = (|| -> Result<(), String> {
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create_new(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
            let mut file = opts.open(&tmp).map_err(err)?;
            self.agent
                .cookie_jar_lock()
                .save_json(&mut file)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            file.flush().map_err(err)?;
            drop(file);
            std::fs::rename(&tmp, path).map_err(err)
        })();
        if written.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        written
    }

    pub fn clear_jar(&self) {
        self.agent.cookie_jar_lock().clear();
    }

    /// Cookie names only, never values (tests and status).
    pub fn cookie_names(&self) -> Vec<String> {
        let jar = self.agent.cookie_jar_lock();
        let mut names: Vec<String> = jar.iter().map(|c| c.name().to_string()).collect();
        names.sort();
        names
    }
}

impl SiteClient for UreqSite {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        read(self.agent.get(url).call())
    }

    fn post_form(&self, url: &str, body: &str) -> Result<Vec<u8>, String> {
        read(
            self.agent
                .post(url)
                .header(
                    "Content-Type",
                    "application/x-www-form-urlencoded;charset=UTF-8",
                )
                .send(body),
        )
    }
}

/// `UreqSite` shared between the site's fetch client and its jar saver.
impl SiteClient for Arc<UreqSite> {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        (**self).get(url)
    }
    fn post_form(&self, url: &str, body: &str) -> Result<Vec<u8>, String> {
        (**self).post_form(url, body)
    }
}

/// One `[[subscriptions]]` account with its password, as read for this run.
pub struct SiteAccount {
    pub site: String,
    pub username: String,
    pub kind: Option<String>,
    pub password: Option<Zeroizing<String>>,
    pub password_from: Option<Source>,
    /// Why there is no password (the keychain's message); never the password itself.
    pub password_error: Option<String>,
}

impl std::fmt::Debug for SiteAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SiteAccount")
            .field("site", &self.site)
            .field("username", &self.username)
            .field("kind", &self.kind)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("password_from", &self.password_from)
            .field("password_error", &self.password_error)
            .finish()
    }
}

/// Where a site's session stands in this run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum SessionState {
    NotTried,
    SignedIn { service: Option<String> },
    LoginFailed(String),
    NoAdapter,
    NoPassword(String),
}

/// A site's account and session, for the CLI and the app. Never carries the password.
#[derive(Clone, Debug, Serialize)]
pub struct SiteStatus {
    pub site: String,
    pub username: String,
    pub kind: Option<String>,
    pub has_password: bool,
    pub state: SessionState,
    pub jar_note: Option<String>,
}

/// What changes during a run, behind the site's short-held lock.
struct Run {
    state: SessionState,
    failures: u8,
    last_failure: String,
    /// Successful logins so far: a fetch that saw a stranger's page knows whether
    /// someone signed in after it looked.
    logins: u32,
}

struct Site {
    account: SiteAccount,
    client: Box<dyn SiteClient>,
    /// The real client and its jar file, for `save_jars`.
    jar: Option<(PathBuf, Arc<UreqSite>)>,
    jar_note: Option<String>,
    /// The login critical section: held across `login` only. Lock order: `login`, then `run`.
    login: Mutex<()>,
    run: Mutex<Run>,
    /// The adapter's index in `SessionNet::adapters`, resolved once.
    adapter: OnceLock<Option<usize>>,
    /// `services/`, fetched once per run (a failure counts as no services).
    entitlement: OnceLock<Entitlement>,
}

impl Site {
    fn run(&self) -> MutexGuard<'_, Run> {
        self.run.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn no_password_note(&self) -> String {
        self.account
            .password_error
            .clone()
            .unwrap_or_else(|| "no password saved".into())
    }
}

/// [`Http`] plus one signed-in session per `[[subscriptions]]` site (publisher
/// subscriptions design §5). Only a site's host and its subdomains use its session.
pub struct SessionNet {
    inner: Http,
    sites: HashMap<String, Site>,
    adapters: Vec<Box<dyn Publisher>>,
}

impl SessionNet {
    /// Real clients; with `jar_dir` (the household folder) each jar loads from, and saves to,
    /// `sessions/<site>.json` there.
    pub fn new(
        inner: Http,
        accounts: Vec<SiteAccount>,
        adapters: Vec<Box<dyn Publisher>>,
        jar_dir: Option<PathBuf>,
    ) -> Self {
        let sites = accounts
            .into_iter()
            .map(|account| {
                let client = Arc::new(UreqSite::new(&account.site));
                let mut jar_note = None;
                let jar = jar_dir.as_ref().map(|dir| {
                    let path = jar_path(dir, &account.site);
                    if let Err(e) = client.load_jar(&path) {
                        jar_note = Some(format!("{e}; starting signed out"));
                    }
                    (path, client.clone())
                });
                let site = Site::new(account, Box::new(client), jar, jar_note);
                (site.account.site.clone(), site)
            })
            .collect();
        SessionNet {
            inner,
            sites,
            adapters,
        }
    }

    /// Given clients and no jar files (tests).
    pub fn with_clients(
        inner: Http,
        sites: Vec<(SiteAccount, Box<dyn SiteClient>)>,
        adapters: Vec<Box<dyn Publisher>>,
    ) -> Self {
        let sites = sites
            .into_iter()
            .map(|(account, client)| {
                let site = Site::new(account, client, None, None);
                (site.account.site.clone(), site)
            })
            .collect();
        SessionNet {
            inner,
            sites,
            adapters,
        }
    }

    /// The household's accounts: passwords from the keychain (or the environment when there
    /// is none), jars from `sessions/` beside `config_path`, adapters from the registry.
    pub fn from_config(
        config_path: &Path,
        subs: &[SubscriptionConfig],
        keys: &dyn Keyring,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Self {
        let household = config_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let accounts = subs
            .iter()
            .map(|sub| {
                let (password, password_from, password_error) = match site_password(sub, keys, env)
                {
                    Ok((pw, from)) => (Some(pw), Some(from), None),
                    Err(e) => (None, None, Some(e.to_string())),
                };
                SiteAccount {
                    site: sub.site.trim().to_lowercase(),
                    username: sub.username.trim().to_string(),
                    kind: sub.kind.clone(),
                    password,
                    password_from,
                    password_error,
                }
            })
            .collect();
        SessionNet::new(Http, accounts, registry(), Some(household))
    }

    /// The account whose site is the URL's host or a parent domain of it.
    pub fn site_for(&self, url: &str) -> Option<String> {
        let parsed = url::Url::parse(url).ok()?;
        let host = parsed.host_str()?.trim_end_matches('.').to_lowercase();
        self.sites
            .keys()
            .filter(|site| host == **site || host.ends_with(&format!(".{site}")))
            .max_by_key(|site| site.len())
            .cloned()
    }

    pub fn status(&self) -> Vec<SiteStatus> {
        let mut out: Vec<SiteStatus> = self
            .sites
            .values()
            .map(|s| {
                let kind = s
                    .adapter
                    .get()
                    .copied()
                    .flatten()
                    .map(|i| self.adapters[i].kind().to_string())
                    .or_else(|| s.account.kind.clone());
                SiteStatus {
                    site: s.account.site.clone(),
                    username: s.account.username.clone(),
                    kind,
                    has_password: s.account.password.is_some(),
                    state: s.run().state.clone(),
                    jar_note: s.jar_note.clone(),
                }
            })
            .collect();
        out.sort_by(|a, b| a.site.cmp(&b.site));
        out
    }

    /// Writes every real jar back; the notes name any that failed.
    pub fn save_jars(&self) -> Vec<String> {
        let mut notes: Vec<String> = self
            .sites
            .values()
            .filter_map(|s| s.jar.as_ref())
            .filter_map(|(path, client)| client.save_jar(path).err())
            .collect();
        notes.sort();
        notes
    }

    /// Signs in now (the "test sign-in"), whatever the jar holds. Counts toward the two
    /// failures that stop logins for the run.
    pub fn sign_in(&self, site: &str) -> SessionState {
        let Some(s) = self.sites.get(site) else {
            return SessionState::NoAdapter;
        };
        let Some(adapter) = self.adapter(s) else {
            s.run().state = SessionState::NoAdapter;
            return SessionState::NoAdapter;
        };
        let _login = s.login.lock().unwrap_or_else(|e| e.into_inner());
        match self.log_in(s, adapter) {
            Err(why) => match why {
                Walled::NotSignedIn => SessionState::NoPassword(s.no_password_note()),
                Walled::LoginFailed(e) => SessionState::LoginFailed(e),
                Walled::NotInPlan | Walled::NoAdapter => SessionState::NoAdapter,
            },
            Ok(()) => {
                let ent = self.entitlement(s, adapter);
                let state = SessionState::SignedIn {
                    service: ent.services.first().cloned(),
                };
                s.run().state = state.clone();
                state
            }
        }
    }

    /// The site's adapter: by `kind`, or detected once from its home page.
    fn adapter(&self, s: &Site) -> Option<&dyn Publisher> {
        let i = *s.adapter.get_or_init(|| match &s.account.kind {
            Some(k) => self.adapters.iter().position(|p| p.kind() == k),
            None => {
                let home = s.client.get(&format!("https://{}/", s.account.site)).ok()?;
                let html = String::from_utf8_lossy(&home);
                self.adapters.iter().position(|p| p.detect(&html))
            }
        });
        i.map(|i| self.adapters[i].as_ref())
    }

    fn entitlement(&self, s: &Site, adapter: &dyn Publisher) -> Entitlement {
        s.entitlement
            .get_or_init(|| {
                adapter
                    .entitlement(&s.account.site, s.client.as_ref())
                    .unwrap_or(Entitlement {
                        services: Vec::new(),
                    })
            })
            .clone()
    }

    /// One login attempt, unless two have failed or there is no password. The caller holds
    /// `s.login`.
    fn log_in(&self, s: &Site, adapter: &dyn Publisher) -> Result<(), Walled> {
        {
            let run = s.run();
            if run.failures >= 2 {
                return Err(Walled::LoginFailed(run.last_failure.clone()));
            }
        }
        let Some(pw) = s.account.password.as_ref() else {
            s.run().state = SessionState::NoPassword(s.no_password_note());
            return Err(Walled::NotSignedIn);
        };
        match adapter.login(&s.account.site, &s.account.username, pw, s.client.as_ref()) {
            Ok(()) => {
                let mut run = s.run();
                run.logins += 1;
                run.state = SessionState::SignedIn { service: None };
                Ok(())
            }
            Err(e) => {
                // The adapter's message should never hold the password; make sure.
                let e = if pw.is_empty() {
                    e
                } else {
                    e.replace(pw.as_str(), "<redacted>")
                };
                let mut run = s.run();
                run.failures = run.failures.saturating_add(1);
                run.last_failure = e.clone();
                run.state = SessionState::LoginFailed(e.clone());
                Err(Walled::LoginFailed(e))
            }
        }
    }

    /// The dance (design §5): fetch, sign in if a stranger's page, refetch, unlock.
    fn fetch_signed_in(&self, s: &Site, url: &str) -> Result<(Vec<u8>, Access), String> {
        let site = s.account.site.clone();
        let walled = |body: Vec<u8>, why: Walled| {
            Ok((
                body,
                Access::Walled {
                    site: site.clone(),
                    why,
                },
            ))
        };
        let seen = s.run().logins;
        let mut body = s.client.get(url)?;
        let Some(adapter) = self.adapter(s) else {
            s.run().state = SessionState::NoAdapter;
            return walled(body, Walled::NoAdapter);
        };
        if !adapter.signed_in(&String::from_utf8_lossy(&body)) {
            let login = s.login.lock().unwrap_or_else(|e| e.into_inner());
            // Another fetch may have signed in while this one waited for the lock.
            let raced = s.run().logins != seen;
            if !raced && let Err(why) = self.log_in(s, adapter) {
                return walled(body, why);
            }
            drop(login);
            body = s.client.get(url)?;
        }
        let ent = self.entitlement(s, adapter);
        let service = ent.services.first().cloned();
        let unlocked = adapter.unlock(&String::from_utf8_lossy(&body), &ent);
        s.run().state = SessionState::SignedIn {
            service: service.clone(),
        };
        match unlocked {
            Some(html) => Ok((
                html.into_bytes(),
                Access::Subscription {
                    site: site.clone(),
                    service: service.unwrap_or_default(),
                },
            )),
            None => walled(body, Walled::NotInPlan),
        }
    }
}

impl Site {
    fn new(
        account: SiteAccount,
        client: Box<dyn SiteClient>,
        jar: Option<(PathBuf, Arc<UreqSite>)>,
        jar_note: Option<String>,
    ) -> Self {
        let state = if account.password.is_some() {
            SessionState::NotTried
        } else {
            SessionState::NoPassword(
                account
                    .password_error
                    .clone()
                    .unwrap_or_else(|| "no password saved".into()),
            )
        };
        Site {
            account,
            client,
            jar,
            jar_note,
            login: Mutex::new(()),
            run: Mutex::new(Run {
                state,
                failures: 0,
                last_failure: String::new(),
                logins: 0,
            }),
            adapter: OnceLock::new(),
            entitlement: OnceLock::new(),
        }
    }
}

impl Net for SessionNet {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        self.inner.get(url)
    }

    fn get_page(&self, url: &str) -> Result<Vec<u8>, String> {
        self.inner.get_page(url)
    }

    fn post_form(&self, url: &str, body: &str) -> Result<Vec<u8>, String> {
        self.inner.post_form(url, body)
    }

    fn get_page_as(&self, url: &str) -> Result<(Vec<u8>, Access), String> {
        match self.site_for(url).and_then(|k| self.sites.get(&k)) {
            Some(s) => self.fetch_signed_in(s, url),
            None => self.inner.get_page(url).map(|b| (b, Access::Open)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// (Max-Age: ureq saves only persistent cookies, so a bare session cookie would not survive the jar file.)
    /// Serves `n` sequential requests, each answered with `Set-Cookie: sess=abc`; records
    /// each request's raw head. Returns the base URL and the record.
    pub(super) fn fixture(n: usize) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let rec = seen.clone();
        std::thread::spawn(move || {
            for _ in 0..n {
                let Ok((mut s, _)) = listener.accept() else {
                    return;
                };
                let mut buf = Vec::new();
                let mut b = [0u8; 1024];
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    match s.read(&mut b) {
                        Ok(0) | Err(_) => break,
                        Ok(k) => buf.extend_from_slice(&b[..k]),
                    }
                }
                rec.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&buf).into_owned());
                let body = "ok";
                let _ = write!(
                    s,
                    "HTTP/1.1 200 OK\r\nSet-Cookie: sess=abc; Path=/; Max-Age=3600\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        (url, seen)
    }

    fn tempdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("deckle-session-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn the_jar_round_trips_through_its_file() {
        let (url, _) = fixture(1);
        let a = UreqSite::new("a");
        a.get(&url).unwrap();
        assert_eq!(a.cookie_names(), ["sess"]);
        let dir = tempdir("rt");
        let path = jar_path(&dir, "a");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path.with_extension("json.tmp"), "stale").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let tmp = path.with_extension("json.tmp");
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        a.save_jar(&path).unwrap();
        assert!(!path.with_extension("json.tmp").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let b = UreqSite::new("a");
        b.load_jar(&path).unwrap();
        assert_eq!(b.cookie_names(), ["sess"]);
    }

    #[test]
    fn a_bad_jar_file_is_an_error_not_a_panic() {
        let dir = tempdir("bad");
        let path = jar_path(&dir, "a");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "not json").unwrap();
        let s = UreqSite::new("a");
        let err = s.load_jar(&path).unwrap_err();
        assert!(err.contains("sessions/"), "{err}");
        assert!(s.cookie_names().is_empty());
    }

    #[test]
    fn a_missing_jar_file_is_fine() {
        let dir = tempdir("missing");
        assert!(UreqSite::new("a").load_jar(&jar_path(&dir, "a")).is_ok());
    }

    #[test]
    fn the_cookie_goes_back_only_to_its_host() {
        let (url, seen) = fixture(2);
        let (other, other_seen) = fixture(1);
        let a = UreqSite::new("a");
        a.get(&url).unwrap();
        a.get(&url).unwrap();
        // The same agent, holding the cookie, asks a different host name.
        let port = other.rsplit(':').next().unwrap();
        a.get(&format!("http://localhost:{port}")).unwrap();
        let has = |r: &str| r.to_lowercase().contains("cookie: sess=abc");
        let seen = seen.lock().unwrap();
        assert!(!has(&seen[0]));
        assert!(has(&seen[1]), "{}", seen[1]);
        let other_seen = other_seen.lock().unwrap();
        assert!(!has(&other_seen[0]), "{}", other_seen[0]);
    }
}

#[cfg(test)]
mod net_tests {
    use super::*;
    use crate::fetch::{Net, encode};
    use crate::publisher::{Access, Entitlement, Publisher, Walled};
    use std::sync::{Arc, Mutex};

    const PW: &str = "hunter2-very-secret";

    #[derive(Default)]
    struct State {
        signed_in: bool,
        logins: Vec<(String, String)>,
        accept: bool,
        gets: Vec<String>,
    }

    /// A publisher site in memory: a member's page once a login it accepts has happened.
    #[derive(Clone, Default)]
    struct FakeSite(Arc<Mutex<State>>);

    impl FakeSite {
        fn new(signed_in: bool, accept: bool) -> Self {
            FakeSite(Arc::new(Mutex::new(State {
                signed_in,
                accept,
                ..Default::default()
            })))
        }
        fn logins(&self) -> Vec<(String, String)> {
            self.0.lock().unwrap().logins.clone()
        }
        fn gets(&self) -> Vec<String> {
            self.0.lock().unwrap().gets.clone()
        }
        fn page(&self) -> Vec<u8> {
            if self.0.lock().unwrap().signed_in {
                b"<html>USER</html>".to_vec()
            } else {
                b"<html>LOGIN</html>".to_vec()
            }
        }
    }

    impl SiteClient for FakeSite {
        fn get(&self, url: &str) -> Result<Vec<u8>, String> {
            self.0.lock().unwrap().gets.push(url.to_string());
            // Slow enough that parallel fetches overlap.
            std::thread::sleep(std::time::Duration::from_millis(5));
            Ok(self.page())
        }
        fn post_form(&self, url: &str, body: &str) -> Result<Vec<u8>, String> {
            {
                let mut s = self.0.lock().unwrap();
                s.logins.push((url.to_string(), body.to_string()));
                if s.accept {
                    s.signed_in = true;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
            Ok(self.page())
        }
    }

    struct FakeBlox;

    impl Publisher for FakeBlox {
        fn kind(&self) -> &'static str {
            "fake"
        }
        fn detect(&self, html: &str) -> bool {
            html.contains("FAKE")
        }
        fn signed_in(&self, html: &str) -> bool {
            html.contains("USER")
        }
        fn login(
            &self,
            site: &str,
            user: &str,
            pw: &str,
            c: &dyn SiteClient,
        ) -> Result<(), String> {
            let body = format!("username={}&password={}", encode(user), encode(pw));
            let page = c.post_form(&format!("https://{site}/users/login/"), &body)?;
            if String::from_utf8_lossy(&page).contains("USER") {
                Ok(())
            } else {
                Err("bad password".into())
            }
        }
        fn entitlement(&self, _site: &str, _c: &dyn SiteClient) -> Result<Entitlement, String> {
            Ok(Entitlement {
                services: vec!["plus".into()],
            })
        }
        fn unlock(&self, html: &str, ent: &Entitlement) -> Option<String> {
            ent.services
                .iter()
                .any(|s| s == "plus")
                .then(|| format!("{html}<!--unlocked-->"))
        }
        fn sections(&self) -> &[(&'static str, &'static str)] {
            &[]
        }
        fn feed_url(&self, site: &str, slug: &str, _n: usize) -> String {
            format!("https://{site}/{slug}.rss")
        }
    }

    fn account(username: &str, password: Option<&str>, kind: Option<&str>) -> SiteAccount {
        SiteAccount {
            site: "thepilot.com".into(),
            username: username.into(),
            kind: kind.map(str::to_string),
            password: password.map(|p| Zeroizing::new(p.to_string())),
            password_from: password.map(|_| Source::Keychain),
            password_error: None,
        }
    }

    fn net(site: &FakeSite, acct: SiteAccount) -> SessionNet {
        SessionNet::with_clients(
            Http,
            vec![(acct, Box::new(site.clone()) as Box<dyn SiteClient>)],
            vec![Box::new(FakeBlox)],
        )
    }

    const STORY: &str = "https://www.thepilot.com/news/a-story.html";

    #[test]
    fn a_strangers_page_logs_in_once_then_refetches() {
        let site = FakeSite::new(false, true);
        let n = net(&site, account("me", Some(PW), Some("fake")));
        let (body, access) = n.get_page_as(STORY).unwrap();
        assert_eq!(site.logins().len(), 1);
        assert_eq!(
            access,
            Access::Subscription {
                site: "thepilot.com".into(),
                service: "plus".into()
            }
        );
        let body = String::from_utf8(body).unwrap();
        assert!(body.ends_with("<!--unlocked-->"), "{body}");
        assert!(body.contains("USER"), "the refetched page: {body}");
        assert_eq!(
            n.status()[0].state,
            SessionState::SignedIn {
                service: Some("plus".into())
            }
        );
        // A second story reuses the session.
        n.get_page_as(STORY).unwrap();
        assert_eq!(site.logins().len(), 1);
    }

    #[test]
    fn a_live_session_skips_the_login() {
        let site = FakeSite::new(true, true);
        let n = net(&site, account("me", Some(PW), Some("fake")));
        let (_, access) = n.get_page_as(STORY).unwrap();
        assert!(matches!(access, Access::Subscription { .. }), "{access:?}");
        assert!(site.logins().is_empty());
    }

    #[test]
    fn eight_parallel_stories_log_in_exactly_once() {
        let site = FakeSite::new(false, true);
        let n = net(&site, account("me", Some(PW), Some("fake")));
        let urls: Vec<String> = (0..8)
            .map(|i| format!("https://www.thepilot.com/news/{i}.html"))
            .collect();
        let got = crate::sources::feeds::par_map(&urls, |u| n.get_page_as(u).unwrap().1);
        assert_eq!(site.logins().len(), 1);
        for a in got {
            assert!(matches!(a, Access::Subscription { .. }), "{a:?}");
        }
    }

    #[test]
    fn two_failures_stop_the_retries() {
        let site = FakeSite::new(false, false);
        let n = net(&site, account("me", Some(PW), Some("fake")));
        for _ in 0..5 {
            let (body, access) = n.get_page_as(STORY).unwrap();
            assert_eq!(body, b"<html>LOGIN</html>");
            assert_eq!(
                access,
                Access::Walled {
                    site: "thepilot.com".into(),
                    why: Walled::LoginFailed("bad password".into())
                }
            );
        }
        assert_eq!(site.logins().len(), 2);
        assert_eq!(
            n.status()[0].state,
            SessionState::LoginFailed("bad password".into())
        );
        // A forced sign-in is also refused once the site has failed twice.
        assert_eq!(
            n.sign_in("thepilot.com"),
            SessionState::LoginFailed("bad password".into())
        );
        assert_eq!(site.logins().len(), 2);
    }

    #[test]
    fn no_password_means_not_signed_in_without_a_login_attempt() {
        let site = FakeSite::new(false, true);
        let n = net(&site, account("me", None, Some("fake")));
        let (_, access) = n.get_page_as(STORY).unwrap();
        assert_eq!(
            access,
            Access::Walled {
                site: "thepilot.com".into(),
                why: Walled::NotSignedIn
            }
        );
        assert!(site.logins().is_empty());
        let st = &n.status()[0];
        assert!(!st.has_password);
        assert!(matches!(st.state, SessionState::NoPassword(_)), "{st:?}");
    }

    #[test]
    fn an_account_without_an_adapter_is_walled_no_adapter() {
        let site = FakeSite::new(false, true);
        let n = SessionNet::with_clients(
            Http,
            vec![(
                account("me", Some(PW), None),
                Box::new(site.clone()) as Box<dyn SiteClient>,
            )],
            Vec::new(),
        );
        for _ in 0..2 {
            let (body, access) = n.get_page_as(STORY).unwrap();
            assert_eq!(body, b"<html>LOGIN</html>");
            assert_eq!(
                access,
                Access::Walled {
                    site: "thepilot.com".into(),
                    why: Walled::NoAdapter
                }
            );
        }
        assert!(site.logins().is_empty());
        let home = site
            .gets()
            .iter()
            .filter(|u| *u == "https://thepilot.com/")
            .count();
        assert_eq!(home, 1, "the home page is fetched once to detect the kind");
        assert_eq!(n.status()[0].state, SessionState::NoAdapter);
    }

    #[test]
    fn a_set_kind_never_fetches_the_home_page() {
        let site = FakeSite::new(true, true);
        let n = net(&site, account("me", Some(PW), Some("fake")));
        n.get_page_as(STORY).unwrap();
        assert_eq!(site.gets(), [STORY]);
    }

    #[test]
    fn only_the_site_and_its_subdomains_use_the_session() {
        let site = FakeSite::new(true, true);
        let n = net(&site, account("me", Some(PW), Some("fake")));
        assert_eq!(
            n.site_for("https://www.thepilot.com/a").as_deref(),
            Some("thepilot.com")
        );
        assert_eq!(
            n.site_for("https://thepilot.com/a").as_deref(),
            Some("thepilot.com")
        );
        assert_eq!(n.site_for("https://notthepilot.com/"), None);
        assert_eq!(
            n.site_for("https://bloximages.newyork1.vip.townnews.com/x.jpg"),
            None
        );
        assert_eq!(n.site_for("not a url"), None);
        // Another host goes through the anonymous client (a local server here).
        let (url, _) = super::tests::fixture(1);
        let (body, access) = n.get_page_as(&url).unwrap();
        assert_eq!(access, Access::Open);
        assert_eq!(body, b"ok");
        assert!(site.gets().is_empty(), "the site's client never saw it");
    }

    #[test]
    fn the_login_form_carries_the_credentials_encoded() {
        let site = FakeSite::new(false, true);
        let n = net(&site, account("a&b@x.test", Some("p+w d"), Some("fake")));
        n.get_page_as(STORY).unwrap();
        let logins = site.logins();
        assert_eq!(logins.len(), 1);
        let (url, body) = &logins[0];
        assert_eq!(url, "https://thepilot.com/users/login/");
        assert!(body.contains("username=a%26b%40x.test"), "{body}");
        assert!(body.contains("password=p%2Bw+d"), "{body}");
    }

    #[test]
    fn sign_in_forces_a_login() {
        let site = FakeSite::new(false, true);
        let n = net(&site, account("me", Some(PW), Some("fake")));
        assert_eq!(
            n.sign_in("thepilot.com"),
            SessionState::SignedIn {
                service: Some("plus".into())
            }
        );
        assert_eq!(site.logins().len(), 1);
        assert_eq!(n.sign_in("nosuch.example"), SessionState::NoAdapter);
    }

    #[test]
    fn status_never_carries_the_password() {
        let site = FakeSite::new(false, false);
        let n = net(&site, account("me", Some(PW), Some("fake")));
        n.get_page_as(STORY).unwrap();
        let st = n.status();
        assert!(st[0].has_password);
        let shown = format!("{st:?} {}", serde_json::to_string(&st).unwrap());
        assert!(!shown.contains(PW), "{shown}");
        let acct = format!("{:?}", account("me", Some(PW), Some("fake")));
        assert!(!acct.contains(PW), "{acct}");
    }

    #[test]
    fn a_login_error_echoing_the_password_is_scrubbed() {
        struct Leaky;
        impl Publisher for Leaky {
            fn kind(&self) -> &'static str {
                "fake"
            }
            fn detect(&self, _: &str) -> bool {
                false
            }
            fn signed_in(&self, _: &str) -> bool {
                false
            }
            fn login(&self, _: &str, _: &str, pw: &str, _: &dyn SiteClient) -> Result<(), String> {
                Err(format!("rejected {pw}"))
            }
            fn entitlement(&self, _: &str, _: &dyn SiteClient) -> Result<Entitlement, String> {
                Err("no".into())
            }
            fn unlock(&self, _: &str, _: &Entitlement) -> Option<String> {
                None
            }
            fn sections(&self) -> &[(&'static str, &'static str)] {
                &[]
            }
            fn feed_url(&self, _: &str, _: &str, _: usize) -> String {
                String::new()
            }
        }
        let site = FakeSite::new(false, false);
        let n = SessionNet::with_clients(
            Http,
            vec![(
                account("me", Some(PW), Some("fake")),
                Box::new(site.clone()) as Box<dyn SiteClient>,
            )],
            vec![Box::new(Leaky)],
        );
        let (_, access) = n.get_page_as(STORY).unwrap();
        let shown = format!("{access:?} {:?}", n.status());
        assert!(!shown.contains(PW), "{shown}");
    }

    #[test]
    fn from_config_reads_passwords_and_notes_a_bad_jar() {
        use crate::config::SubscriptionConfig;
        use crate::secret::{MemoryKeyring, set_site};
        let dir = tempfile::tempdir().unwrap();
        let sub = SubscriptionConfig {
            site: "thepilot.com".into(),
            username: "me".into(),
            kind: None,
        };
        let other = SubscriptionConfig {
            site: "example.com".into(),
            username: "you".into(),
            kind: None,
        };
        let keys = MemoryKeyring::default();
        set_site(&sub, &keys, PW).unwrap();
        let jar = jar_path(dir.path(), "thepilot.com");
        std::fs::create_dir_all(jar.parent().unwrap()).unwrap();
        std::fs::write(&jar, "not json").unwrap();
        let n = SessionNet::from_config(
            &dir.path().join("digest.toml"),
            &[sub, other],
            &keys,
            &|_| None,
        );
        let st = n.status();
        assert_eq!(st.len(), 2);
        let pilot = st.iter().find(|s| s.site == "thepilot.com").unwrap();
        assert!(pilot.has_password);
        assert!(pilot.jar_note.is_some(), "{pilot:?}");
        assert_eq!(pilot.state, SessionState::NotTried);
        let ex = st.iter().find(|s| s.site == "example.com").unwrap();
        assert!(!ex.has_password);
        assert!(ex.jar_note.is_none());
        match &ex.state {
            SessionState::NoPassword(m) => assert!(m.contains("deckle digest subscribe"), "{m}"),
            s => panic!("{s:?}"),
        }
        assert!(!format!("{st:?}").contains(PW));
        // Saving writes every jar back (an empty one for a site never fetched).
        assert!(n.save_jars().is_empty());
        assert!(jar_path(dir.path(), "example.com").exists());
    }
}
