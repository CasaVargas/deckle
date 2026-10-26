// SPDX-License-Identifier: GPL-3.0-or-later
//! BLOX (TownNews) papers (publisher subscriptions design §6).
//!
//! A premium BLOX page carries a few `p.subscriber-preview` paragraphs in clear, then one
//! hidden `div.subscriber-only.encrypted-content` per remaining paragraph whose text is the
//! HTML-escaped ROT47 of the paragraph's HTML. [`Blox::unlock`] un-shifts them only when the
//! account's services cover the page's `paid_access_service_ids`: the text is trivially
//! decodable, so the entitlement check is the whole of the rule.

use std::sync::LazyLock;

use dom_query::Document;
use regex::Regex;
use serde_json::Value;

use super::{Entitlement, Publisher, SiteClient};
use crate::fetch::encode;

/// The section slugs a BLOX paper's search feed knows, with their display names.
pub const SECTIONS: &[(&str, &str)] = &[
    ("news", "News"),
    ("sports", "Sports"),
    ("opinion", "Opinion"),
    ("obituaries", "Obituaries"),
    ("business", "Business"),
    ("lifestyles", "Lifestyles"),
];

/// The encrypted paragraphs of a premium page.
const ENCRYPTED: &str = "div.subscriber-only.encrypted-content";

/// The ROT47 shift: each byte in `33..=126` moves 47 places round that range; everything
/// else (spaces, non-ASCII) is unchanged. Its own inverse.
pub fn rot47(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '!'..='~' => (33 + (c as u8 - 33 + 47) % 94) as char,
            _ => c,
        })
        .collect()
}

static PAID_IDS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"paid_access_service_ids\s*=\s*\[([^\]]*)\]").unwrap());

/// The page's `paid_access_service_ids` as strings; empty when the page lists none.
pub fn paid_access_service_ids(html: &str) -> Vec<String> {
    page_service_ids(html).unwrap_or_default()
}

/// The page's service ids; `None` when the page names `paid_access_service_ids` in a shape
/// we can't read (which must fail closed, not count as "lists none"): assigned more than once,
/// or named anywhere outside the one assignment we parsed.
fn page_service_ids(html: &str) -> Option<Vec<String>> {
    let found: Vec<regex::Captures> = PAID_IDS.captures_iter(html).collect();
    let named = html.matches("paid_access_service_ids").count();
    match found.as_slice() {
        [] if named == 0 => Some(Vec::new()),
        [c] if named == 1 => Some(
            c[1].split(',')
                .map(|id| id.trim().trim_matches(|q| q == '"' || q == '\'').trim())
                .filter(|id| !id.is_empty())
                .map(str::to_string)
                .collect(),
        ),
        _ => None,
    }
}

/// A JSON id, number or string, as a string.
fn id_of(v: &Value) -> Option<String> {
    match v {
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        _ => None,
    }
}

/// A flag that may arrive as a bool, a number or a string.
fn truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_i64().is_some_and(|n| n != 0),
        Value::String(s) => matches!(s.trim().to_lowercase().as_str(), "true" | "1" | "active"),
        _ => false,
    }
}

/// Lenient: an array, or an object with a `services` array; each service an object with
/// `id` (number or string) and `active`/`is_active`/`status == "active"`; or a bare id list.
/// A service object with none of those flags counts as inactive. The error never quotes the
/// reply.
pub fn parse_services(json: &str) -> Result<Entitlement, String> {
    let v: Value = serde_json::from_str(json.trim())
        .map_err(|_| "the services reply isn't JSON".to_string())?;
    let list = match &v {
        Value::Array(a) => a,
        Value::Object(o) => match o.get("services") {
            Some(Value::Array(a)) => a,
            _ => return Err("the services reply has no services list".into()),
        },
        _ => return Err("the services reply isn't a services list".into()),
    };
    let mut ent = Entitlement::default();
    for item in list {
        let (id, active, name) = match item {
            Value::Object(o) => {
                let Some(id) = o.get("id").and_then(id_of) else {
                    continue;
                };
                let active = ["active", "is_active"]
                    .iter()
                    .filter_map(|k| o.get(*k))
                    .any(truthy)
                    || o.get("status")
                        .and_then(Value::as_str)
                        .is_some_and(|s| s.trim().eq_ignore_ascii_case("active"));
                let name = ["name", "title"]
                    .iter()
                    .filter_map(|k| o.get(*k).and_then(Value::as_str))
                    .map(str::trim)
                    .find(|n| !n.is_empty())
                    .map(str::to_string);
                (id, active, name)
            }
            bare => match id_of(bare) {
                Some(id) => (id, true, None),
                None => continue,
            },
        };
        if active && !ent.services.contains(&id) {
            if let Some(name) = name {
                ent.names.insert(id.clone(), name);
            }
            ent.services.push(id);
        }
    }
    Ok(ent)
}

/// The anti-goal (design §6.3): the account's active services must include one the page
/// names, or, when the page names none, the account must have some active service.
fn entitled(page_ids: &[String], ent: &Entitlement) -> bool {
    if page_ids.is_empty() {
        !ent.services.is_empty()
    } else {
        ent.services.iter().any(|s| page_ids.contains(s))
    }
}

/// The paper's canonical origin: BLOX papers answer on `www.` (the bare host redirects).
fn origin(site: &str) -> String {
    if site.starts_with("www.") {
        format!("https://{site}")
    } else {
        format!("https://www.{site}")
    }
}

/// The login form: its action and its hidden fields.
fn login_form(html: &str) -> Option<(String, Vec<(String, String)>)> {
    let doc = Document::from(html);
    let form = doc.select("form#access-modal-login-form");
    let action = form.attr("action")?.trim().to_string();
    let hidden = form
        .select("input[type=hidden]")
        .nodes()
        .iter()
        .filter_map(|n| {
            let name = n.attr("name")?.to_string();
            let value = n.attr("value").map(|v| v.to_string()).unwrap_or_default();
            Some((name, value))
        })
        .filter(|(name, _)| name != "username" && name != "password")
        .collect();
    Some((action, hidden))
}

/// TownNews BLOX: The Pilot and many other community papers.
pub struct Blox;

impl Publisher for Blox {
    fn kind(&self) -> &'static str {
        "blox"
    }

    fn detect(&self, html: &str) -> bool {
        html.contains("tncms")
    }

    /// A stranger's page renders both header blocks (CSS hides one), so only a filled-in
    /// screen name means signed in.
    fn signed_in(&self, html: &str) -> bool {
        Document::from(html)
            .select(".tn-user-screenname")
            .nodes()
            .iter()
            .any(|n| !n.text().trim().is_empty())
    }

    fn login(&self, site: &str, user: &str, pw: &str, c: &dyn SiteClient) -> Result<(), String> {
        let site = site.trim().trim_end_matches('.').to_lowercase();
        let (page, page_url) = c.get_final(&format!("https://{site}/users/login/"))?;
        let (action, hidden) = login_form(&String::from_utf8_lossy(&page))
            .ok_or_else(|| "the site's login page has no login form".to_string())?;
        // A relative action is relative to where the login page ended up, after redirects.
        let action = url::Url::parse(&page_url)
            .and_then(|base| base.join(&action))
            .map_err(|_| "the login form points at another host".to_string())?;
        let host = action
            .host_str()
            .unwrap_or("")
            .trim_end_matches('.')
            .to_lowercase();
        if action.scheme() != "https" || !(host == site || host.ends_with(&format!(".{site}"))) {
            return Err("the login form points at another host".into());
        }
        let mut fields: Vec<String> = hidden
            .iter()
            .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
            .collect();
        fields.push(format!("username={}", encode(user)));
        fields.push(format!("password={}", encode(pw)));
        let reply = c.post_form(action.as_str(), &fields.join("&"))?;
        if self.signed_in(&String::from_utf8_lossy(&reply)) {
            return Ok(());
        }
        let home = c.get(&format!("https://{host}/"))?;
        if self.signed_in(&String::from_utf8_lossy(&home)) {
            Ok(())
        } else {
            Err("the site refused the login".into())
        }
    }

    fn entitlement(&self, site: &str, c: &dyn SiteClient) -> Result<Entitlement, String> {
        let reply = c.get(&format!("{}/tncms/subscription/services/", origin(site)))?;
        parse_services(&String::from_utf8_lossy(&reply))
    }

    fn unlock(&self, html: &str, ent: &Entitlement) -> Option<String> {
        let doc = Document::from(html);
        let blocks = doc.select(ENCRYPTED);
        if blocks.is_empty() {
            // Nothing to decode: a free story, as the site served it.
            return Some(html.to_string());
        }
        // An ids line we can't read fails closed: only a page with no such line lists none.
        let page_ids = page_service_ids(html)?;
        if !entitled(&page_ids, ent) {
            return None;
        }
        for node in blocks.nodes() {
            // The parser has already unescaped the text; what's left is the shifted HTML.
            node.replace_with_html(rot47(&node.text()));
        }
        Some(doc.html().to_string())
    }

    fn sections(&self) -> &[(&'static str, &'static str)] {
        SECTIONS
    }

    fn feed_url(&self, site: &str, slug: &str, n: usize) -> String {
        format!(
            "{}/search/?f=rss&t=article&c={}&l={n}&s=start_time&sd=desc",
            origin(site),
            encode(slug)
        )
    }
}

/// What [`fixture_page`] builds.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Page<'a> {
    pub signed_in: bool,
    pub service_ids: &'a [&'a str],
    pub preview: &'a [&'a str],
    pub premium: &'a [&'a str],
    pub login_form: bool,
}

/// A BLOX article page on the public markup's skeleton, from synthetic paragraphs only.
#[cfg(test)]
pub fn fixture_page(opts: Page) -> String {
    let esc = |s: &str| html_escape::encode_text(s).to_string();
    let screen = if opts.signed_in { "Pat Reader" } else { "" };
    let ids = opts
        .service_ids
        .iter()
        .map(|id| match id.parse::<u64>() {
            Ok(n) => n.to_string(),
            Err(_) => format!("\"{id}\""),
        })
        .collect::<Vec<_>>()
        .join(", ");
    let form = if opts.login_form {
        r#"<form id="access-modal-login-form" method="post" action="https://www.thepilot.com/users/login/">
<input type="hidden" name="referer_url" value="">
<input type="text" name="username" id="user-username">
<input type="password" name="password" id="user-password">
<button type="submit">Log in</button>
</form>"#
    } else {
        ""
    };
    let mut body = String::new();
    for p in opts.preview {
        body.push_str(&format!("<p class=\"subscriber-preview\">{}</p>\n", esc(p)));
    }
    for p in opts.premium {
        let html = format!("<p>{}</p>", esc(p));
        body.push_str(&format!(
            "<div class=\"subscriber-only encrypted-content\" style=\"display:none\">{}</div>\n",
            esc(&rot47(&html))
        ));
    }
    format!(
        r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><title>A synthetic story | thepilot.com</title>
<script src="https://bloximages.example.test/tncms/tncms.js"></script>
<script>var tncms = tncms || {{}}; tncms.subscription = tncms.subscription || {{}};
tncms.subscription.paid_access_service_ids = [{ids}];</script>
</head><body>
<header><ul class="nav">
<li class="aplus-user-login not-logged-in"><a href="/users/login/">Log In</a></li>
<li class="aplus-user-controls logged-in"><span class="tn-user-screenname">{screen}</span><a href="/users/logout/">Log out</a></li>
</ul></header>
{form}
<main><article>
<h1 class="headline">A synthetic story</h1>
<div class="asset-body" data-subscription-required-class="asset-body">
{body}</div>
</article></main>
<footer><p>Footer text.</p></footer>
</body></html>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::publisher::{Entitlement, Publisher, SiteClient, detect_kind};
    use std::sync::Mutex;

    fn ent(ids: &[&str]) -> Entitlement {
        Entitlement {
            services: ids.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    /// Synthetic prose: `n` sentences numbered so a test can find the last one.
    fn para(tag: &str, words: usize) -> String {
        let mut s: Vec<String> = (0..words).map(|i| format!("word{i}")).collect();
        s.push(format!("closing{tag}."));
        format!("The {tag} paragraph says {}", s.join(" "))
    }

    #[test]
    fn rot47_round_trips_and_decodes_blox() {
        assert_eq!(rot47("kAm"), "<p>");
        assert_eq!(rot47(&rot47("Hello, <b>x</b>")), "Hello, <b>x</b>");
        assert_eq!(rot47("é ü"), "é ü", "non-ASCII and spaces pass through");
    }

    #[test]
    fn a_strangers_page_with_both_header_blocks_is_not_signed_in() {
        let page = fixture_page(Page::default());
        assert!(page.contains("aplus-user-login not-logged-in"), "{page}");
        assert!(page.contains("aplus-user-controls logged-in"), "{page}");
        assert!(page.contains("tn-user-screenname"), "{page}");
        assert!(!Blox.signed_in(&page));
    }

    #[test]
    fn a_signed_in_page_has_a_screen_name() {
        let page = fixture_page(Page {
            signed_in: true,
            ..Default::default()
        });
        assert!(Blox.signed_in(&page));
    }

    fn premium_page(ids: &[&str]) -> (String, Vec<String>, Vec<String>) {
        let preview: Vec<String> = (0..2).map(|i| para(&format!("preview{i}"), 10)).collect();
        let premium: Vec<String> = (0..3).map(|i| para(&format!("premium{i}"), 10)).collect();
        let p: Vec<&str> = preview.iter().map(String::as_str).collect();
        let q: Vec<&str> = premium.iter().map(String::as_str).collect();
        let page = fixture_page(Page {
            signed_in: true,
            service_ids: ids,
            preview: &p,
            premium: &q,
            login_form: false,
        });
        (page, preview, premium)
    }

    #[test]
    fn unlock_needs_the_pages_service() {
        let (page, preview, premium) = premium_page(&["3"]);
        // The premium text is not in clear on the page.
        assert!(!page.contains(&premium[0]));
        let out = Blox
            .unlock(&page, &ent(&["3"]))
            .expect("the service covers the page");
        assert!(!out.contains("display:none"), "{out}");
        assert!(!out.contains("encrypted-content"), "{out}");
        let order: Vec<usize> = preview
            .iter()
            .chain(premium.iter())
            .map(|p| {
                assert_eq!(out.matches(p.as_str()).count(), 1, "{p} once in {out}");
                out.find(p.as_str()).unwrap()
            })
            .collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
        // The anti-goal: trivially decodable, but not this account's to read.
        assert_eq!(Blox.unlock(&page, &ent(&["9"])), None);
        assert_eq!(Blox.unlock(&page, &ent(&[])), None);
    }

    #[test]
    fn a_page_without_service_ids_unlocks_for_any_active_service() {
        let (page, _, premium) = premium_page(&[]);
        let out = Blox.unlock(&page, &ent(&["1"])).unwrap();
        assert!(out.contains(&premium[2]));
        assert_eq!(Blox.unlock(&page, &ent(&[])), None);
    }

    #[test]
    fn an_ids_line_we_cant_read_fails_closed() {
        let (page, _, _) = premium_page(&["3"]);
        let line = "tncms.subscription.paid_access_service_ids = [3];";
        assert!(page.contains(line), "{page}");
        for odd in [
            r#"var cfg = {"paid_access_service_ids":[3]};"#,
            "tncms.subscription.paid_access_service_ids = someVar;",
            // assigned twice: the first (empty) list must not read as "lists none"
            "tncms.subscription.paid_access_service_ids = []; \
             tncms.subscription.paid_access_service_ids = [3];",
            // one parsed assignment, and the name again somewhere we didn't parse
            "tncms.subscription.paid_access_service_ids = [1]; \
             cfg.paid_access_service_ids.push(3);",
        ] {
            let odd_page = page.replace(line, odd);
            assert_eq!(Blox.unlock(&odd_page, &ent(&["1"])), None, "{odd}");
            assert_eq!(Blox.unlock(&odd_page, &ent(&["3"])), None, "{odd}");
        }
        // No such line at all: the page lists none, and any active service reads it.
        let none = page.replace(line, "");
        assert!(!none.contains("paid_access_service_ids"));
        assert!(Blox.unlock(&none, &ent(&["1"])).is_some());
    }

    #[test]
    fn a_page_with_nothing_encrypted_comes_back_unchanged() {
        let page = fixture_page(Page {
            signed_in: true,
            preview: &["Just a free paragraph."],
            ..Default::default()
        });
        assert_eq!(Blox.unlock(&page, &ent(&[])), Some(page.clone()));
    }

    #[test]
    fn unlocked_page_extracts_to_the_full_story() {
        let preview: Vec<String> = (0..2).map(|i| para(&format!("preview{i}"), 28)).collect();
        let premium: Vec<String> = (0..6).map(|i| para(&format!("premium{i}"), 28)).collect();
        let p: Vec<&str> = preview.iter().map(String::as_str).collect();
        let q: Vec<&str> = premium.iter().map(String::as_str).collect();
        let page = fixture_page(Page {
            signed_in: true,
            service_ids: &["3"],
            preview: &p,
            premium: &q,
            login_form: false,
        });
        let out = Blox.unlock(&page, &ent(&["3"])).unwrap();
        let x = crate::article::extract(&out, "https://www.thepilot.com/news/x.html")
            .expect("a full story");
        assert!(x.words >= 120, "{}", x.words);
        assert!(x.xhtml.contains("closingpremium5."), "{}", x.xhtml);
        assert!(x.xhtml.contains("closingpreview0."), "{}", x.xhtml);
    }

    #[test]
    fn parse_services_accepts_the_shapes_we_expect() {
        let e = parse_services(
            r#"[{"id": 3, "active": true, "name": "Pilot Plus"}, {"id": "4", "active": false}]"#,
        )
        .unwrap();
        assert_eq!(e.services, vec!["3"]);
        assert_eq!(e.names.get("3").map(String::as_str), Some("Pilot Plus"));
        let e = parse_services(
            r#"{"services": [{"id": 5, "is_active": true}, {"id": 6, "status": "active"}, {"id": 7, "status": "expired"}]}"#,
        )
        .unwrap();
        assert_eq!(e.services, vec!["5", "6"]);
        let e = parse_services(r#"[8, "9"]"#).unwrap();
        assert_eq!(e.services, vec!["8", "9"]);
        let e = parse_services(r#"{"services": []}"#).unwrap();
        assert!(e.services.is_empty());
        assert!(parse_services(r#"{"error": "nope"}"#).is_err());
        assert!(parse_services("42").is_err());
    }

    #[test]
    fn a_services_reply_we_cant_read_is_an_error_not_a_panic() {
        let e = parse_services("<html><body>secret page text</body></html>").unwrap_err();
        assert!(!e.contains("secret page text"), "{e}");
    }

    #[test]
    fn paid_access_ids_parse() {
        let page = "<script>tncms.subscription.paid_access_service_ids = [3, \"4\"];</script>";
        assert_eq!(paid_access_service_ids(page), vec!["3", "4"]);
        assert_eq!(
            paid_access_service_ids("<html></html>"),
            Vec::<String>::new()
        );
        assert_eq!(
            paid_access_service_ids("paid_access_service_ids = [];"),
            Vec::<String>::new()
        );
    }

    /// A site in memory: answers GETs from a table and records every request.
    #[derive(Default)]
    struct Recorder {
        pages: Vec<(String, String)>,
        /// Request URL -> where its redirects end.
        finals: Vec<(String, String)>,
        post_reply: String,
        gets: Mutex<Vec<String>>,
        posts: Mutex<Vec<(String, String)>>,
    }

    impl SiteClient for Recorder {
        fn get(&self, url: &str) -> Result<Vec<u8>, String> {
            self.gets.lock().unwrap().push(url.to_string());
            self.pages
                .iter()
                .find(|(u, _)| u == url)
                .map(|(_, p)| p.clone().into_bytes())
                .ok_or_else(|| format!("404 {url}"))
        }
        fn get_final(&self, url: &str) -> Result<(Vec<u8>, String), String> {
            let end = self
                .finals
                .iter()
                .find(|(u, _)| u == url)
                .map_or(url, |(_, f)| f.as_str());
            Ok((self.get(url)?, end.to_string()))
        }
        fn post_form(&self, url: &str, body: &str) -> Result<Vec<u8>, String> {
            self.posts
                .lock()
                .unwrap()
                .push((url.to_string(), body.to_string()));
            Ok(self.post_reply.clone().into_bytes())
        }
    }

    fn login_page() -> String {
        fixture_page(Page {
            login_form: true,
            ..Default::default()
        })
    }

    #[test]
    fn login_posts_to_the_forms_action_with_the_fields_encoded() {
        let member = fixture_page(Page {
            signed_in: true,
            ..Default::default()
        });
        let c = Recorder {
            pages: vec![("https://thepilot.com/users/login/".into(), login_page())],
            post_reply: member,
            ..Default::default()
        };
        Blox.login("thepilot.com", "a@b.test", "p+w", &c).unwrap();
        let posts = c.posts.lock().unwrap();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].0, "https://www.thepilot.com/users/login/");
        let body = &posts[0].1;
        assert!(body.contains("referer_url="), "{body}");
        assert!(body.contains("username=a%40b.test"), "{body}");
        assert!(body.contains("password=p%2Bw"), "{body}");
    }

    #[test]
    fn login_checks_the_origin_when_the_reply_is_not_a_members_page() {
        let member = fixture_page(Page {
            signed_in: true,
            ..Default::default()
        });
        let c = Recorder {
            pages: vec![
                ("https://thepilot.com/users/login/".into(), login_page()),
                ("https://www.thepilot.com/".into(), member),
            ],
            post_reply: "<html>redirecting</html>".into(),
            ..Default::default()
        };
        Blox.login("thepilot.com", "a@b.test", "pw", &c).unwrap();
        let stranger = Recorder {
            pages: vec![
                ("https://thepilot.com/users/login/".into(), login_page()),
                ("https://www.thepilot.com/".into(), login_page()),
            ],
            post_reply: "<html>Wrong password for a@b.test</html>".into(),
            ..Default::default()
        };
        let e = Blox
            .login("thepilot.com", "a@b.test", "pw", &stranger)
            .unwrap_err();
        assert_eq!(e, "the site refused the login");
    }

    #[test]
    fn a_relative_action_resolves_against_the_login_pages_final_url() {
        let member = fixture_page(Page {
            signed_in: true,
            ..Default::default()
        });
        let page = login_page().replace("https://www.thepilot.com/users/login/", "/users/login/");
        let c = Recorder {
            pages: vec![("https://thepilot.com/users/login/".into(), page)],
            finals: vec![(
                "https://thepilot.com/users/login/".into(),
                "https://www.thepilot.com/users/login/".into(),
            )],
            post_reply: member,
            ..Default::default()
        };
        Blox.login("thepilot.com", "a@b.test", "pw", &c).unwrap();
        let posts = c.posts.lock().unwrap();
        assert_eq!(posts[0].0, "https://www.thepilot.com/users/login/");
    }

    #[test]
    fn a_mixed_case_site_matches_its_own_host() {
        let c = Recorder {
            pages: vec![("https://thepilot.com/users/login/".into(), login_page())],
            post_reply: fixture_page(Page {
                signed_in: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        Blox.login("ThePilot.COM", "a", "b", &c).unwrap();
        assert_eq!(c.posts.lock().unwrap().len(), 1);
    }

    #[test]
    fn login_refuses_a_form_that_points_elsewhere() {
        for action in [
            "https://evil.test/login",
            "//evil.test/users/login/",
            "https://www.thepilot.com@evil.test/users/login/",
            "https://thepilot.com.evil.test/users/login/",
            "http://www.thepilot.com/users/login/",
        ] {
            let page = login_page().replace("https://www.thepilot.com/users/login/", action);
            let c = Recorder {
                pages: vec![("https://thepilot.com/users/login/".into(), page)],
                ..Default::default()
            };
            let e = Blox
                .login("thepilot.com", "a@b.test", "pw", &c)
                .unwrap_err();
            assert_eq!(e, "the login form points at another host", "{action}");
            assert!(c.posts.lock().unwrap().is_empty(), "{action}");
        }
    }

    #[test]
    fn a_login_page_without_the_form_is_an_error() {
        let c = Recorder {
            pages: vec![(
                "https://thepilot.com/users/login/".into(),
                fixture_page(Page::default()),
            )],
            ..Default::default()
        };
        assert!(Blox.login("thepilot.com", "a", "b", &c).is_err());
        assert!(c.posts.lock().unwrap().is_empty());
    }

    #[test]
    fn entitlement_reads_the_services_page() {
        let c = Recorder {
            pages: vec![(
                "https://www.thepilot.com/tncms/subscription/services/".into(),
                r#"[{"id": 3, "active": true}]"#.into(),
            )],
            ..Default::default()
        };
        assert_eq!(
            Blox.entitlement("thepilot.com", &c).unwrap().services,
            vec!["3"]
        );
    }

    #[test]
    fn feed_urls_use_the_www_origin() {
        assert_eq!(
            Blox.feed_url("thepilot.com", "news", 15),
            "https://www.thepilot.com/search/?f=rss&t=article&c=news&l=15&s=start_time&sd=desc"
        );
        assert_eq!(Blox.sections().len(), 6);
        assert_eq!(Blox.kind(), "blox");
    }

    #[test]
    fn the_registry_detects_blox() {
        assert_eq!(detect_kind(&fixture_page(Default::default())), Some("blox"));
        assert_eq!(detect_kind("<html><p>plain</p></html>"), None);
    }
}
