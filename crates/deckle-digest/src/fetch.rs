// SPDX-License-Identifier: GPL-3.0-or-later
//! HTTP for the sources. [`Net`] is shared by parallel article fetches, so it is `Sync`;
//! tests pass a closure that serves fixtures by URL (GET only).

use std::net::IpAddr;
use std::sync::RwLock;
use std::time::Duration;

use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{DefaultConnector, NextTimeout};

pub trait Net: Sync {
    /// An API or feed, fetched as Deckle.
    fn get(&self, url: &str) -> Result<Vec<u8>, String>;

    /// A publisher's web page or image, fetched as a desktop browser: many sites refuse
    /// unknown agents (design §5a).
    fn get_page(&self, url: &str) -> Result<Vec<u8>, String> {
        self.get(url)
    }

    /// A page fetched with any signed-in session, and how it was got. Plain nets are open.
    fn get_page_as(&self, url: &str) -> Result<(Vec<u8>, crate::publisher::Access), String> {
        self.get_page(url)
            .map(|b| (b, crate::publisher::Access::Open))
    }

    /// A form POST (Google News link decoding).
    fn post_form(&self, url: &str, _body: &str) -> Result<Vec<u8>, String> {
        Err(format!("POST not supported: {url}"))
    }
}

/// Any `Fn(url) -> body` is a GET-only [`Net`] (tests, fixtures).
impl<F: Fn(&str) -> Result<Vec<u8>, String> + Sync> Net for F {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        self(url)
    }
}

const USER_AGENT: &str = concat!(
    "Deckle/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/CasaVargas/deckle)"
);
pub(crate) const BROWSER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
     (KHTML, like Gecko) Version/18.0 Safari/605.1.15";
pub(crate) const LIMIT: u64 = 20 * 1024 * 1024;

/// Real HTTP: 15 s per API request, 25 s per publisher page (local sites can be slow). Only
/// public addresses, besides the origins the owner configured or allowed (see [`allow_config`]).
pub struct Http;

/// [`Http`] that may also reach the origin of one URL the owner typed themselves, such as
/// `deckle run http://nas.local/story`: an address in content (a feed's link, a page's image, a
/// redirect) never earns that.
pub struct HttpTyped(Vec<Origin>);

impl Http {
    /// [`Http`] that may also reach the origins of these URLs, which the owner typed.
    /// Anything that isn't an http(s) URL (a file path) is skipped.
    pub fn typed<'a>(urls: impl IntoIterator<Item = &'a str>) -> HttpTyped {
        HttpTyped(urls.into_iter().filter_map(Origin::of).collect())
    }
}

/// A scheme, host and port, compared exactly: `http://nas.local` and `http://nas.local:80` are
/// the same; `http://localhost:8080` and `http://localhost:9000` are not.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Origin {
    scheme: String,
    host: String,
    port: u16,
}

impl Origin {
    /// The origin of an `http`, `https` or `webcal` URL (a calendar's `webcal://` is fetched
    /// over https).
    pub fn of(url: &str) -> Option<Origin> {
        let uri: ureq::http::Uri = url.trim().parse().ok()?;
        Self::of_uri(&uri)
    }

    fn of_uri(uri: &ureq::http::Uri) -> Option<Origin> {
        let scheme = match uri.scheme_str()?.to_ascii_lowercase().as_str() {
            "http" => "http",
            "https" | "webcal" => "https",
            _ => return None,
        };
        let host = uri
            .host()?
            .trim_start_matches('[')
            .trim_end_matches(']')
            .trim_end_matches('.')
            .to_ascii_lowercase();
        let port = uri
            .port_u16()
            .unwrap_or(if scheme == "http" { 80 } else { 443 });
        Some(Origin {
            scheme: scheme.into(),
            host,
            port,
        })
    }

    /// A host that can't be on the public internet: a private address, or a name nobody can
    /// register.
    fn is_local(&self) -> bool {
        match self.host.parse::<IpAddr>() {
            Ok(ip) => !public_ip(ip),
            Err(_) => {
                let h = &self.host;
                !h.contains('.')
                    || [".localhost", ".local", ".home.arpa", ".internal"]
                        .iter()
                        .any(|s| h.ends_with(s))
            }
        }
    }
}

/// Origins on the owner's own network that fetches may reach.
static ALLOWED: RwLock<Vec<Origin>> = RwLock::new(Vec::new());

fn allowed(o: &Origin) -> bool {
    ALLOWED
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .contains(o)
}

/// Lets fetches reach what a paper's config names on the owner's own network: a feed, a
/// calendar or a subscription whose address is private or a local name (exactly that origin),
/// and every origin in `[fetch] allow_private`. A public name in the config earns nothing, so a
/// feed can't point its own name at this machine later. Called for each config loaded.
pub fn allow_config(cfg: &crate::config::Config) {
    let mut add: Vec<Origin> = Vec::new();
    let configured = cfg
        .feeds
        .iter()
        .flat_map(|f| std::iter::once(&f.url).chain(&f.urls))
        .chain(&cfg.day.calendars)
        .filter_map(|u| Origin::of(u))
        .chain(
            cfg.subscriptions
                .iter()
                .filter_map(|s| Origin::of(&format!("https://{}", s.site))),
        )
        .filter(Origin::is_local);
    add.extend(configured);
    add.extend(cfg.fetch.allow_private.iter().filter_map(|u| Origin::of(u)));
    if add.is_empty() {
        return;
    }
    let mut list = ALLOWED.write().unwrap_or_else(|e| e.into_inner());
    for o in add {
        if !list.contains(&o) {
            list.push(o);
        }
    }
}

/// Lets a test's local server through, as a configured private feed would be.
#[cfg(test)]
pub(crate) fn allow_for_test(url: &str) {
    let o = Origin::of(url).expect("an http URL");
    let mut list = ALLOWED.write().unwrap_or_else(|e| e.into_inner());
    if !list.contains(&o) {
        list.push(o);
    }
}

/// Whether an address is on the public internet. Anything else (this machine, the LAN,
/// link-local, carrier NAT, multicast, documentation and benchmarking ranges) is refused.
pub(crate) fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let [a, b, c, _] = v.octets();
            !(a == 0
                || a == 10
                || a == 127
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && b == 168)
                // Only these two /24s of 192.0.0.0/16 are special-use; the rest is public.
                || (a == 192 && b == 0 && (c == 0 || c == 2))
                || (a == 192 && b == 88 && c == 99)
                || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
                || (a == 203 && b == 0 && c == 113))
        }
        IpAddr::V6(v) => {
            let s = v.segments();
            // NAT64's well-known prefix (DNS64 on an IPv6-only network) carries an IPv4
            // address: judge that.
            if s[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
                let [a, b] = s[6].to_be_bytes();
                let [c, d] = s[7].to_be_bytes();
                return public_ip(IpAddr::from([a, b, c, d]));
            }
            // Global unicast only; special-use, documentation and IPv4 tunnelling ranges are
            // refused rather than trusted for the address inside them.
            (s[0] & 0xe000) == 0x2000
                && s[0] != 0x2002
                && !(s[0] == 0x2001 && (s[1] < 0x0200 || s[1] == 0x0db8))
                && !(s[0] == 0x3fff && s[1] < 0x1000)
        }
    }
}

/// Resolves only to public addresses, unless the URL's origin is allowed. It runs for every
/// connection, redirects included, and its answer is what TCP connects to, so a name can't
/// pass here and then point somewhere else.
#[derive(Debug)]
pub(crate) struct CheckedResolver<R = DefaultResolver> {
    inner: R,
    typed: Vec<Origin>,
}

impl<R: Resolver> Resolver for CheckedResolver<R> {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        // Through a proxy only the proxy's own address is resolved here (ureq hands it the
        // target); the proxy's rules apply to what it fetches.
        let is_proxy = config.proxy().is_some_and(|p| {
            uri.host().is_some_and(|h| h.eq_ignore_ascii_case(p.host()))
                && uri.port_u16().is_none_or(|port| port == p.port())
        });
        let origin = Origin::of_uri(uri);
        let ok_anywhere = is_proxy
            || origin
                .as_ref()
                .is_some_and(|o| self.typed.contains(o) || allowed(o));
        let addresses = self.inner.resolve(uri, config, timeout)?;
        if ok_anywhere {
            return Ok(addresses);
        }
        // A mixed answer is refused too: it's how rebinding hides a private address.
        if addresses.is_empty() || addresses.iter().any(|a| !public_ip(a.ip())) {
            let host = uri.host().unwrap_or("?");
            eprintln!(
                "fetch: refused {host}: it is on a private network (add it to \
                 [fetch] allow_private if it is yours)"
            );
            return Err(ureq::Error::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "private address refused",
            )));
        }
        Ok(addresses)
    }
}

/// [`agent`], optionally refusing any `http://` request, redirects included.
pub(crate) fn agent_with(user_agent: &str, timeout_secs: u64, https_only: bool) -> ureq::Agent {
    agent_typed(user_agent, timeout_secs, https_only, &[])
}

fn agent_typed(
    user_agent: &str,
    timeout_secs: u64,
    https_only: bool,
    typed: &[Origin],
) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(timeout_secs)))
        .https_only(https_only)
        .user_agent(user_agent)
        // The OS certificate store, so TLS-inspecting proxies (Zscaler, …) the machine trusts work.
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .build();
    ureq::Agent::with_parts(
        config,
        DefaultConnector::new(),
        CheckedResolver {
            inner: DefaultResolver::default(),
            typed: typed.to_vec(),
        },
    )
}

pub(crate) fn read(
    resp: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> Result<Vec<u8>, String> {
    let mut resp = resp.map_err(|e| e.to_string())?;
    resp.body_mut()
        .with_config()
        .limit(LIMIT)
        .read_to_vec()
        .map_err(|e| e.to_string())
}

fn get_with(typed: &[Origin], url: &str) -> Result<Vec<u8>, String> {
    read(agent_typed(USER_AGENT, 15, false, typed).get(url).call())
}

fn get_page_with(typed: &[Origin], url: &str) -> Result<Vec<u8>, String> {
    read(agent_typed(BROWSER_AGENT, 25, false, typed).get(url).call())
}

fn post_form_with(typed: &[Origin], url: &str, body: &str) -> Result<Vec<u8>, String> {
    read(
        agent_typed(BROWSER_AGENT, 15, false, typed)
            .post(url)
            .header(
                "Content-Type",
                "application/x-www-form-urlencoded;charset=UTF-8",
            )
            .send(body),
    )
}

impl Net for Http {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        get_with(&[], url)
    }

    fn get_page(&self, url: &str) -> Result<Vec<u8>, String> {
        get_page_with(&[], url)
    }

    fn post_form(&self, url: &str, body: &str) -> Result<Vec<u8>, String> {
        post_form_with(&[], url, body)
    }
}

impl Net for HttpTyped {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        get_with(&self.0, url)
    }

    fn get_page(&self, url: &str) -> Result<Vec<u8>, String> {
        get_page_with(&self.0, url)
    }

    fn post_form(&self, url: &str, body: &str) -> Result<Vec<u8>, String> {
        post_form_with(&self.0, url, body)
    }
}

/// Percent-encodes a query value.
pub fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    /// A one-shot HTTP server: answers `body`, or a redirect to `location`, and counts hits.
    fn serve(reply: String) -> (u16, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let h = hits.clone();
        std::thread::spawn(move || {
            for c in l.incoming() {
                let Ok(mut c) = c else { break };
                h.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let mut buf = [0u8; 4096];
                let _ = c.read(&mut buf);
                let _ = c.write_all(reply.as_bytes());
            }
        });
        (port, hits)
    }

    fn ok(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    #[test]
    fn the_address_rules() {
        for public in [
            "192.0.78.1",
            "93.184.216.34",
            "2606:4700::1",
            "64:ff9b::c000:4e01",
        ] {
            assert!(public_ip(ip(public)), "{public}");
        }
        for private in [
            "127.0.0.1",
            "10.1.2.3",
            "192.168.1.20",
            "172.16.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "192.0.0.1",
            "192.0.2.1",
            "::1",
            "::ffff:127.0.0.1",
            "fc00::1",
            "fe80::1",
            "64:ff9b::7f00:1",
            "64:ff9b::a00:1",
            "64:ff9b:1::1",
        ] {
            assert!(!public_ip(ip(private)), "{private}");
        }
    }

    #[test]
    fn origins_compare_exactly() {
        assert_eq!(
            Origin::of("http://NAS.local/feed.xml"),
            Origin::of("http://nas.local:80/other")
        );
        assert_ne!(
            Origin::of("http://localhost:8080/"),
            Origin::of("http://localhost:9000/")
        );
        assert_eq!(
            Origin::of("webcal://cal.local/a.ics"),
            Origin::of("https://cal.local:443/")
        );
        assert_eq!(Origin::of("/Users/me/book.epub"), None);
        assert_eq!(Origin::of("ftp://nas.local/"), None);
    }

    #[test]
    fn a_loopback_link_is_refused_before_connecting() {
        let (port, hits) = serve(ok("secret"));
        let e = Http.get(&format!("http://127.0.0.1:{port}/x")).unwrap_err();
        assert!(e.contains("private address refused"), "{e}");
        let e = Http
            .get_page(&format!("http://localhost:{port}/x"))
            .unwrap_err();
        assert!(e.contains("private address refused"), "{e}");
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn a_typed_address_may_be_local_but_not_where_it_redirects() {
        let (other, other_hits) = serve(ok("elsewhere"));
        let (port, _) = serve(ok("mine"));
        let url = format!("http://127.0.0.1:{port}/story");
        assert_eq!(Http::typed([url.as_str()]).get(&url).unwrap(), b"mine");
        // The same typed origin doesn't open another port.
        let e = Http::typed([url.as_str()])
            .get(&format!("http://127.0.0.1:{other}/"))
            .unwrap_err();
        assert!(e.contains("private address refused"), "{e}");
        // Nor does a redirect from it.
        let (hop, _) = serve(format!(
            "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{other}/\r\nContent-Length: 0\r\n\
             Connection: close\r\n\r\n"
        ));
        let hop_url = format!("http://127.0.0.1:{hop}/");
        assert!(Http::typed([hop_url.as_str()]).get(&hop_url).is_err());
        assert_eq!(other_hits.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn a_configured_private_feed_is_allowed_and_a_public_name_earns_nothing() {
        let (port, _) = serve(ok("<rss/>"));
        let (listed, _) = serve(ok("listed"));
        let (refused, hits) = serve(ok("nope"));
        let cfg = crate::config::Config::parse(&format!(
            "[[feeds]]\nname = \"NAS\"\nurl = \"http://127.0.0.1:{port}/feed.xml\"\n\
             [[feeds]]\nname = \"Public\"\nurl = \"http://localtest.me:{refused}/feed.xml\"\n\
             [fetch]\nallow_private = [\"http://127.0.0.1:{listed}\"]\n"
        ))
        .unwrap();
        allow_config(&cfg);
        assert_eq!(
            Http.get(&format!("http://127.0.0.1:{port}/feed.xml"))
                .unwrap(),
            b"<rss/>"
        );
        assert_eq!(
            Http.get(&format!("http://127.0.0.1:{listed}/x")).unwrap(),
            b"listed"
        );
        // An item in the feed linking to another port on the same machine.
        assert!(Http.get(&format!("http://127.0.0.1:{refused}/")).is_err());
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 0);
        // A bad allow_private entry is named.
        let e = crate::config::Config::parse("[fetch]\nallow_private = [\"nas\"]\n").unwrap_err();
        assert!(e.contains("allow_private"), "{e}");
    }

    #[derive(Debug)]
    struct Fixed(Vec<std::net::SocketAddr>);
    impl Resolver for Fixed {
        fn resolve(
            &self,
            _: &ureq::http::Uri,
            _: &ureq::config::Config,
            _: NextTimeout,
        ) -> Result<ResolvedSocketAddrs, ureq::Error> {
            let mut out = self.empty();
            for a in &self.0 {
                out.push(*a);
            }
            Ok(out)
        }
    }

    #[test]
    fn a_name_answering_public_and_private_is_refused() {
        let (port, hits) = serve(ok("x"));
        let local: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        for answer in [
            vec![local],
            vec!["93.184.216.34:80".parse().unwrap(), local],
        ] {
            let a = ureq::Agent::with_parts(
                ureq::Agent::config_builder().build(),
                DefaultConnector::new(),
                CheckedResolver {
                    inner: Fixed(answer),
                    typed: Vec::new(),
                },
            );
            assert!(a.get("http://rebound.example/").call().is_err());
        }
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn a_proxy_on_a_private_address_is_used() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let config = ureq::Agent::config_builder()
            .proxy(Some(
                ureq::Proxy::new(&format!("http://127.0.0.1:{port}")).unwrap(),
            ))
            .timeout_global(Some(Duration::from_millis(500)))
            .build();
        let a = ureq::Agent::with_parts(
            config,
            DefaultConnector::new(),
            CheckedResolver {
                inner: DefaultResolver::default(),
                typed: Vec::new(),
            },
        );
        let t = std::thread::spawn(move || l.accept().is_ok());
        let _ = a.get("http://example.com/").call();
        assert!(t.join().unwrap(), "the proxy was never reached");
    }
}
