// SPDX-License-Identifier: GPL-3.0-or-later
//! Signed-in sessions for publisher sites: one cookie-carrying agent per site, with its
//! cookie jar kept in `sessions/<site>.json` beside `digest.toml`.

use std::fs::File;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};

use crate::fetch::{BROWSER_AGENT, agent, read};
use crate::publisher::SiteClient;

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
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
        let mut file = opts.open(&tmp).map_err(err)?;
        {
            let jar = self.agent.cookie_jar_lock();
            jar.save_json(&mut file)
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
        file.flush().map_err(err)?;
        drop(file);
        std::fs::rename(&tmp, path).map_err(err)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// (Max-Age: ureq saves only persistent cookies, so a bare session cookie would not survive the jar file.)
    /// Serves `n` sequential requests, each answered with `Set-Cookie: sess=abc`; records
    /// each request's raw head. Returns the base URL and the record.
    fn fixture(n: usize) -> (String, Arc<Mutex<Vec<String>>>) {
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
        a.save_jar(&path).unwrap();
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
        let (url, seen) = fixture(3);
        let a = UreqSite::new("a");
        a.get(&url).unwrap();
        a.get(&url).unwrap();
        UreqSite::new("b").get(&url).unwrap();
        let seen = seen.lock().unwrap();
        let has = |r: &str| r.to_lowercase().contains("cookie: sess=abc");
        assert!(!has(&seen[0]));
        assert!(has(&seen[1]), "{}", seen[1]);
        assert!(!has(&seen[2]), "{}", seen[2]);
    }
}
