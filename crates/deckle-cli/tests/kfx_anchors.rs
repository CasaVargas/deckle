// SPDX-License-Identifier: GPL-3.0-or-later
//! A built edition goes through the real Kindle Previewer and `kpf_to_kfx` (the path `deckle digest
//! run` takes): do the front page's anchors and its column table survive into the KFX?
//!
//! Needs the installed Kindle Previewer, so it is opt-in like the other real-Previewer test:
//! `DECKLE_PREVIEWER_REAL=1 cargo test -p deckle-cli --test kfx_anchors`.

use std::collections::HashMap;
use std::path::Path;

use deckle_digest::chrono::{DateTime, NaiveDate, Utc};
use deckle_digest::{Config, build};
use deckle_kfx::ion::{Struct, Value};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../deckle-digest/tests/fixtures")
            .join(name),
    )
    .unwrap()
}

/// The saved responses `crates/deckle-digest/tests/build.rs` serves; anything else is a dead net.
fn fake(url: &str) -> Result<Vec<u8>, String> {
    let name = if url.starts_with("https://api.open-meteo.com/") {
        "weather.json"
    } else if url.starts_with("https://news.google.com/") {
        "googlenews.xml"
    } else if url.starts_with("https://hnrss.org/") {
        "hn.xml"
    } else if url.contains("d6yy-54nr") {
        "powerball.json"
    } else if url.contains("5xaw-6ayf") {
        "megamillions.json"
    } else if url.contains("onthisday") {
        "onthisday.json"
    } else {
        return Err(format!("offline: {url}"));
    };
    Ok(fixture(name))
}

/// The decoded KFX, indexed by element id.
struct Kfx<'a> {
    /// `$145` content name -> its strings.
    content: HashMap<String, Vec<String>>,
    /// Storyline element id (`$155`) -> element.
    nodes: HashMap<i64, &'a Struct>,
    /// Anchor name (`$266`/`$180`) -> target element id (`$183`/`$155`).
    anchors: HashMap<String, i64>,
    /// Elements in document order.
    order: Vec<i64>,
}

impl<'a> Kfx<'a> {
    fn new(doc: &'a deckle_kfx::container::KfxDoc) -> Self {
        let mut k = Kfx {
            content: HashMap::new(),
            nodes: HashMap::new(),
            anchors: HashMap::new(),
            order: Vec::new(),
        };
        for f in doc.fragments.iter() {
            let Some(s) = f.value.as_struct() else {
                continue;
            };
            match f.ftype.as_str() {
                "$145" => {
                    let name = s.get("name").and_then(Value::as_sym).unwrap().0.clone();
                    let strings = s
                        .get("$146")
                        .and_then(Value::as_list)
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap().to_owned())
                        .collect();
                    k.content.insert(name, strings);
                }
                "$266" => {
                    let name = s.get("$180").and_then(Value::as_sym).unwrap().0.clone();
                    if let Some(eid) = s
                        .get("$183")
                        .and_then(Value::as_struct)
                        .and_then(|p| p.get("$155"))
                        .and_then(Value::as_int)
                    {
                        k.anchors.insert(name, eid);
                    }
                }
                _ => {}
            }
        }
        for f in doc.fragments.iter().filter(|f| f.ftype.as_str() == "$259") {
            if let Some(items) = f.value.as_struct().and_then(|s| s.get("$146")) {
                k.index(items);
            }
        }
        k
    }

    fn index(&mut self, list: &'a Value) {
        for v in list.as_list().into_iter().flatten() {
            if let Some(s) = v.as_struct() {
                if let Some(id) = s.get("$155").and_then(Value::as_int) {
                    self.nodes.insert(id, s);
                    self.order.push(id);
                }
                if let Some(children) = s.get("$146") {
                    self.index(children);
                }
            }
        }
    }

    /// The node's own text (its `$145` content entry), if any.
    fn own_text(&self, n: &Struct) -> Option<&str> {
        let r = n.get("$145")?.as_struct()?;
        let name = &r.get("name")?.as_sym()?.0;
        let i = r.get("$403")?.as_int()? as usize;
        Some(self.content.get(name)?.get(i)?.as_str())
    }

    /// All text in the node and its descendants, in order.
    fn text(&self, n: &Struct) -> String {
        let mut out = self.own_text(n).unwrap_or_default().to_owned();
        for c in n.get("$146").and_then(Value::as_list).into_iter().flatten() {
            match c {
                Value::String(s) => out.push_str(s),
                other => {
                    if let Some(cs) = other.as_struct() {
                        out.push_str(&self.text(cs));
                    }
                }
            }
        }
        out
    }

    /// `(linked text, target element id)` for every link on this node's own text.
    fn own_links(&self, n: &Struct) -> Vec<(String, i64)> {
        let (Some(text), Some(events)) = (self.own_text(n), n.get("$142").and_then(Value::as_list))
        else {
            return Vec::new();
        };
        let chars: Vec<char> = text.chars().collect();
        events
            .iter()
            .filter_map(|e| {
                let e = e.as_struct()?;
                let anchor = &e.get("$179")?.as_sym()?.0;
                let (off, len) = (
                    e.get("$143")?.as_int()? as usize,
                    e.get("$144")?.as_int()? as usize,
                );
                let linked: String = chars[off..off + len].iter().collect();
                Some((linked, *self.anchors.get(anchor)?))
            })
            .collect()
    }

    /// Every link in the subtree.
    fn links_under(&self, n: &Struct, out: &mut Vec<(String, i64)>) {
        out.extend(self.own_links(n));
        for c in n.get("$146").and_then(Value::as_list).into_iter().flatten() {
            if let Some(cs) = c.as_struct() {
                self.links_under(cs, out);
            }
        }
    }

    fn has_links(&self, id: i64) -> bool {
        let mut v = Vec::new();
        self.links_under(self.nodes[&id], &mut v);
        !v.is_empty()
    }

    /// The text of the element an anchor points at.
    fn target_text(&self, id: i64) -> String {
        self.text(self.nodes[&id])
    }
}

#[test]
fn front_page_anchors_and_table_survive_conversion() {
    if std::env::var("DECKLE_PREVIEWER_REAL").ok().as_deref() != Some("1") {
        eprintln!(
            "skipped: set DECKLE_PREVIEWER_REAL=1 to run against the installed Kindle Previewer"
        );
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let mut c = Config::parse(deckle_digest::config::STARTER).unwrap();
    // The owner's real queue, almanac and puzzles are never touched.
    c.reading.queue_path = Some(tmp.path().join("no-queue.jsonl"));
    c.almanac_path = Some(tmp.path().join("almanac.json"));
    c.puzzle_path = Some(tmp.path().join("puzzles.json"));
    c.horoscope.signs = vec!["leo".into(), "virgo".into()];
    c.max_age_hours = 24 * 7; // the fixtures were saved on 2026-10-20
    let date = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
    let now: DateTime<Utc> = "2026-10-20T22:00:00Z".parse().unwrap();
    let built = build(&c, date, now, tmp.path(), &fake).unwrap();

    let previewer = deckle_core::previewer::locate(None).expect("Kindle Previewer installed");
    let kpf =
        deckle_core::previewer::convert(&previewer, &built.epub, &tmp.path().join("pv")).unwrap();
    let out = deckle_kfx::convert::kpf_to_kfx(&kpf.kpf, &Default::default()).unwrap();
    let doc = deckle_kfx::container::read_kfx(&out.bytes).unwrap();
    let kfx = Kfx::new(&doc);

    // The link nodes: every element that carries a link on its own text.
    let linked: Vec<(i64, Vec<(String, i64)>)> = kfx
        .order
        .iter()
        .map(|id| (*id, kfx.own_links(kfx.nodes[id])))
        .filter(|(_, l)| !l.is_empty())
        .collect();
    assert!(
        linked.len() >= 20,
        "links collapsed: only {} linked elements",
        linked.len()
    );

    // (a) A front-page tile's link lands on that story's headline in its section: same words,
    // and the headline itself carries no link (the index entry, with the same words, does).
    let tile = linked
        .iter()
        .find(|(id, l)| {
            l.iter().any(|(t, _)| t == "On AI writing")
                && kfx.target_text(*id).contains("Hacker News ·")
        })
        .expect("front tile 'On AI writing' lost its link");
    let (_, target) = tile.1.iter().find(|(t, _)| t == "On AI writing").unwrap();
    assert_eq!(kfx.target_text(*target), "On AI writing");
    assert!(
        !kfx.has_links(*target),
        "link lands on the index entry, not the headline"
    );

    // The lead headline links to its story in the Local section.
    let lead = linked
        .iter()
        .flat_map(|(_, l)| l)
        .find(|(t, _)| t == "Multiple agencies respond to a standoff in Candor")
        .expect("lead lost its link");
    assert!(
        kfx.target_text(lead.1)
            .starts_with("Multiple agencies respond to a standoff in Candor")
    );

    // (b) A section story's back line links to the section top and to the front tile.
    let back = linked
        .iter()
        .find(|(id, _)| kfx.target_text(*id) == "↩ Hacker News · ↩ Front page")
        .expect("back line lost its links");
    assert_eq!(back.1.len(), 2, "{:?}", back.1);
    assert_eq!(back.1[0].0, "↩ Hacker News");
    assert_eq!(kfx.target_text(back.1[0].1), "Hacker News", "section #top");
    assert_eq!(back.1[1].0, "↩ Front page");
    let front_tile = kfx.target_text(back.1[1].1);
    assert!(
        front_tile.contains("On AI writing"),
        "front tile anchor: {front_tile:?}"
    );
    assert!(
        front_tile.contains("Hacker News ·"),
        "front tile anchor: {front_tile:?}"
    );

    // (c) The column table keeps every cell: a $278 table whose cells hold all six tiles.
    let tables: Vec<String> = kfx
        .order
        .iter()
        .map(|id| kfx.nodes[id])
        .filter(|n| n.get("$159").and_then(Value::as_sym).map(|s| s.0.as_str()) == Some("$278"))
        .map(|n| kfx.text(n))
        .collect();
    let table = tables
        .iter()
        .find(|t| t.contains("Carolina Sportsman"))
        .unwrap_or_else(|| panic!("no table holds the front-page tiles: {tables:?}"));
    for title in [
        "On AI writing",
        "When the rain stopped \"Tall boy\" dropped",
        "Academics have a duty to defend open knowledge systems like Wikipedia",
        "SimCity, but with Permits",
        "Claude designed protein binders against scorpion neurotoxins",
        "What shall I teach my son?",
    ] {
        assert!(table.contains(title), "table lost cell {title:?}");
    }
    assert!(
        table.contains("Carolina Sportsman"),
        "table lost a cell's byline"
    );
}
