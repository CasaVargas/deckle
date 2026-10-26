// SPDX-License-Identifier: GPL-3.0-or-later
//! How far you read (readership design §7): the Daily tab's report, its suggestions, the
//! setting, and recording reading positions in the Kindle tab's own session.

use std::path::{Path, PathBuf};

use deckle_core::deliver::Conn;
use deckle_readership::{Report, State};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadershipView {
    /// `[sync] readership` in `digest.toml`.
    pub enabled: bool,
    pub report: Report,
}

/// Paper `paper`'s config (`None`: the main paper, `digest`).
fn load_config(digest: &Path, paper: Option<&str>) -> Result<deckle_digest::Config, String> {
    match digest.exists() {
        true => {
            deckle_digest::papers::load_one(digest, paper.unwrap_or(deckle_digest::papers::MAIN))
        }
        false => deckle_digest::Config::parse(deckle_digest::config::STARTER),
    }
}

pub fn view_at(state: &Path, digest: &Path, paper: Option<&str>) -> Result<ReadershipView, String> {
    let cfg = load_config(digest, paper)?;
    Ok(ReadershipView {
        enabled: cfg.sync.readership,
        report: deckle_readership::report(&State::load(state), &cfg),
    })
}

fn suggestion(
    state: &Path,
    digest: &Path,
    paper: Option<&str>,
    id: &str,
) -> Result<deckle_readership::Suggestion, String> {
    view_at(state, digest, paper)?
        .report
        .suggestions
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| "That suggestion no longer applies.".to_owned())
}

/// Makes suggestion `id` to `digest.toml`, keeping its comments. It's then set aside like a
/// dismissed one: the editions already read still say what they said, so a Move up would
/// otherwise come straight back and move the feed again.
pub fn apply_at(
    state: &Path,
    digest: &Path,
    paper: Option<&str>,
    id: &str,
) -> Result<ReadershipView, String> {
    let s = suggestion(state, digest, paper, id)?;
    let mut cfg = load_config(digest, paper)?;
    deckle_readership::apply(&s, &mut cfg)?;
    deckle_digest::papers::save(digest, &cfg)?;
    let mut st = State::load(state);
    deckle_readership::dismiss(&mut st, &s);
    st.save(state)?;
    view_at(state, digest, paper)
}

pub fn dismiss_at(
    state: &Path,
    digest: &Path,
    paper: Option<&str>,
    id: &str,
) -> Result<ReadershipView, String> {
    let s = suggestion(state, digest, paper, id)?;
    let mut st = State::load(state);
    deckle_readership::dismiss(&mut st, &s);
    st.save(state)?;
    view_at(state, digest, paper)
}

/// `[sync] readership` is a household setting, in `digest.toml`.
pub fn set_at(state: &Path, digest: &Path, on: bool) -> Result<ReadershipView, String> {
    let mut cfg = load_config(digest, None)?;
    if cfg.sync.readership != on {
        cfg.sync.readership = on;
        deckle_digest::config::save(digest, &cfg)?;
    }
    view_at(state, digest, None)
}

/// Reads and records the editions' reading positions over `conn`, when readership is on for
/// this Kindle. Never fails the caller: a problem is only logged.
pub fn record_on(conn: &mut Conn<'_>, state: &Path, digest: &Path) {
    let device = deckle_core::sync::device_id(conn.target);
    let on = load_config(digest, None).is_ok_and(|c| c.sync.for_device(&device).readership);
    let mut st = State::load(state);
    let stems = st.stems();
    if !on || stems.is_empty() {
        return;
    }
    let found = match deckle_core::yjf::read_on(conn, &stems) {
        Ok(f) => f,
        Err(e) => return eprintln!("readership: {e}"),
    };
    let found: Vec<_> = found
        .into_iter()
        .map(|(stem, on)| {
            let on = match on {
                deckle_core::yjf::OnDevice::Absent => deckle_readership::OnDevice::Absent,
                deckle_core::yjf::OnDevice::Unopened => deckle_readership::OnDevice::Unopened,
                deckle_core::yjf::OnDevice::Opened(b) => deckle_readership::OnDevice::Opened(b),
            };
            (stem, on)
        })
        .collect();
    let now = deckle_digest::chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    st.record_reads(&device, &found, &now);
    if let Err(e) = st.save(state) {
        eprintln!("readership: {e}");
    }
}

// ---- Tauri commands -----------------------------------------------------------------

/// Paper `paper`'s readership file (the main paper's, or its own in `papers/<slug>/`), and
/// `digest.toml`.
pub fn paths_for(digest: &Path, paper: Option<&str>) -> Result<(PathBuf, PathBuf), String> {
    let state = match paper.filter(|s| *s != deckle_digest::papers::MAIN) {
        None => deckle_readership::state_path(),
        Some(slug) => deckle_digest::papers::load_one(digest, slug)?
            .state_file("readership.json")
            .unwrap_or_else(deckle_readership::state_path),
    };
    Ok((state, digest.to_path_buf()))
}

fn paths(paper: Option<&str>) -> Result<(PathBuf, PathBuf), String> {
    paths_for(&deckle_digest::config::default_path(), paper)
}

#[tauri::command]
pub fn readership_report(paper: Option<String>) -> Result<ReadershipView, String> {
    let (s, d) = paths(paper.as_deref())?;
    view_at(&s, &d, paper.as_deref())
}

#[tauri::command]
pub fn readership_apply(id: String, paper: Option<String>) -> Result<ReadershipView, String> {
    let (s, d) = paths(paper.as_deref())?;
    apply_at(&s, &d, paper.as_deref(), &id)
}

#[tauri::command]
pub fn readership_dismiss(id: String, paper: Option<String>) -> Result<ReadershipView, String> {
    let (s, d) = paths(paper.as_deref())?;
    dismiss_at(&s, &d, paper.as_deref(), &id)
}

/// Turning it on or off is the household's, so the view after is the main paper's.
#[tauri::command]
pub fn readership_set(on: bool) -> Result<ReadershipView, String> {
    let (s, d) = paths(None)?;
    set_at(&s, &d, on)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deckle_core::deliver::Target;
    use deckle_readership::layout::{Layout, SectionStart};
    use deckle_readership::state::Read;

    const DIGEST: &str = "# mine\nsections = [\"weather\", \"lottery\"] # order\n";

    fn layout(date: deckle_digest::chrono::NaiveDate) -> Layout {
        let sec = |key: &str, name: &str, start| SectionStart {
            key: key.into(),
            name: name.into(),
            start,
            items: None,
            stories: Vec::new(),
        };
        Layout {
            date,
            title: "The Deckle Daily".into(),
            file: format!("deckle-daily-{date}"),
            end: 10_000,
            sections: vec![
                sec("front", "Front page", 0),
                sec("weather", "Weather", 1000),
                sec("lottery", "Lottery", 8000),
            ],
        }
    }

    /// 14 opened editions; the lottery reached on 2.
    fn seeded(dir: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
        let mut s = State::default();
        let start: deckle_digest::chrono::NaiveDate = "2026-09-01".parse().unwrap();
        for i in 0..14u64 {
            let date = start + deckle_digest::chrono::Days::new(i);
            s.record_layout(layout(date));
            let f = if i < 2 { 8500 } else { 4900 };
            let r = Read {
                opened: true,
                furthest: Some(f),
                last: Some(f),
                last_read: None,
                seen: "x".into(),
                history: Vec::new(),
            };
            s.reads.entry(date).or_default().insert("K".into(), r);
        }
        let (state, digest) = (dir.join("readership.json"), dir.join("digest.toml"));
        s.save(&state).unwrap();
        std::fs::write(&digest, DIGEST).unwrap();
        (state, digest)
    }

    #[test]
    fn apply_by_id_writes_digest_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let (state, digest) = seeded(tmp.path());
        let v = view_at(&state, &digest, None).unwrap();
        assert!(v.enabled);
        assert_eq!(v.report.suggestions[0].id, "drop:lottery");
        let v = apply_at(&state, &digest, None, "drop:lottery").unwrap();
        let text = std::fs::read_to_string(&digest).unwrap();
        assert!(text.contains("sections = [\"weather\"] # order"), "{text}");
        assert!(v.report.sections.iter().all(|s| s.key != "lottery"));
        assert!(
            apply_at(&state, &digest, None, "drop:lottery").is_err(),
            "gone now"
        );
    }

    #[test]
    fn an_applied_move_up_does_not_come_straight_back() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::default();
        let start: deckle_digest::chrono::NaiveDate = "2026-09-01".parse().unwrap();
        let sec = |key: &str, name: &str, start| SectionStart {
            key: key.into(),
            name: name.into(),
            start,
            items: None,
            stories: Vec::new(),
        };
        for i in 0..10u64 {
            let date = start + deckle_digest::chrono::Days::new(i);
            s.record_layout(Layout {
                date,
                title: "The Deckle Daily".into(),
                file: format!("deckle-daily-{date}"),
                end: 10_000,
                sections: vec![
                    sec("front", "Front page", 0),
                    sec("weather", "Weather", 1000),
                    sec("feeds:A", "A", 2000),
                    sec("feeds:B", "B", 7000),
                ],
            });
            let r = Read {
                opened: true,
                furthest: Some(3000),
                last: Some(3000),
                last_read: None,
                seen: "x".into(),
                history: Vec::new(),
            };
            s.reads.entry(date).or_default().insert("K".into(), r);
        }
        let (state, digest) = (
            tmp.path().join("readership.json"),
            tmp.path().join("digest.toml"),
        );
        s.save(&state).unwrap();
        std::fs::write(
            &digest,
            "sections = [\"weather\", \"feeds\"]\n\n[[feeds]]\nname = \"A\"\nurl = \"https://a.example/f\"\n\n[[feeds]]\nname = \"B\"\nurl = \"https://b.example/f\"\n",
        )
        .unwrap();
        assert!(
            view_at(&state, &digest, None)
                .unwrap()
                .report
                .suggestions
                .iter()
                .any(|x| x.id == "up:feeds:B")
        );
        let v = apply_at(&state, &digest, None, "up:feeds:B").unwrap();
        assert!(
            v.report
                .suggestions
                .iter()
                .all(|x| !x.id.starts_with("up:")),
            "{:?}",
            v.report.suggestions
        );
        let cfg = deckle_digest::Config::load(&digest).unwrap();
        assert_eq!(cfg.feeds[0].name, "B");
    }

    #[test]
    fn dismiss_and_set() {
        let tmp = tempfile::tempdir().unwrap();
        let (state, digest) = seeded(tmp.path());
        let v = dismiss_at(&state, &digest, None, "drop:lottery").unwrap();
        assert!(v.report.suggestions.iter().all(|s| s.id != "drop:lottery"));
        let v = set_at(&state, &digest, false).unwrap();
        assert!(!v.enabled);
        assert!(
            std::fs::read_to_string(&digest)
                .unwrap()
                .contains("readership = false")
        );
    }

    #[test]
    fn the_kindle_tab_session_records_reading_positions() {
        let tmp = tempfile::tempdir().unwrap();
        let date: deckle_digest::chrono::NaiveDate = "2026-10-21".parse().unwrap();
        let mut s = State::default();
        s.record_layout(layout(date));
        let state = tmp.path().join("readership.json");
        s.save(&state).unwrap();
        let digest = tmp.path().join("digest.toml");
        std::fs::write(&digest, DIGEST).unwrap();
        let docs = tmp.path().join("kindle");
        let sdr = docs.join("deckle-daily-2026-10-21.sdr");
        std::fs::create_dir_all(&sdr).unwrap();
        std::fs::write(docs.join("deckle-daily-2026-10-21.kfx"), b"KFX").unwrap();
        let yjf = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../fixtures/yjf/daily-2026-10-21.yjf"),
        )
        .unwrap();
        std::fs::write(sdr.join("deckle-daily-2026-10-219d6e5f44.yjf"), yjf).unwrap();
        let target = Target::Folder(docs);
        record_on(&mut Conn::new(&target), &state, &digest);
        let st = State::load(&state);
        let reads = &st.reads[&date];
        let r = reads.values().next().unwrap();
        assert!(r.opened);
        assert_eq!(r.furthest, Some(103303));
    }
}
