// SPDX-License-Identifier: GPL-3.0-or-later
//! Your Day: today's events from .ics calendars (almanac design §5).

use std::sync::Mutex;

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use chrono_tz::Tz;
use deckle_digest::config::{Config, DayConfig};
use deckle_digest::day::{Entry, When, agenda, mask, on_date, read_calendar};

fn ny() -> Tz {
    "America/New_York".parse().unwrap()
}

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn at(date: NaiveDate, h: u32, m: u32) -> NaiveDateTime {
    date.and_time(NaiveTime::from_hms_opt(h, m, 0).unwrap())
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/ics/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn entries(name: &str, date: NaiveDate) -> Vec<Entry> {
    on_date(&fixture(name), date, ny()).unwrap().0
}

fn starts(es: &[Entry]) -> Vec<(String, NaiveTime)> {
    es.iter()
        .filter_map(|e| match e.when {
            When::Timed { start, .. } => Some((e.title.clone(), start.time())),
            When::AllDay => None,
        })
        .collect()
}

fn t(h: u32, m: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(h, m, 0).unwrap()
}

#[test]
fn weekly_with_exdate_and_move() {
    let pickup = |date| starts(&entries("weekly.ics", date));
    assert_eq!(
        pickup(d(2026, 10, 8)),
        [("School pickup".into(), t(15, 45))]
    );
    assert_eq!(pickup(d(2026, 10, 15)), []);
    assert_eq!(
        pickup(d(2026, 10, 22)),
        [("School pickup".into(), t(16, 0))]
    );
    assert_eq!(pickup(d(2026, 10, 21)), []);
}

#[test]
fn across_clock_change() {
    let day = |date| starts(&entries("zones.ics", date));
    // London's clocks change a week before New York's.
    assert_eq!(
        day(d(2026, 10, 25)),
        [
            ("Call with Mum".into(), t(5, 0)),
            ("Church, then brunch".into(), t(9, 0))
        ]
    );
    assert_eq!(
        day(d(2026, 11, 1)),
        [
            ("Call with Mum".into(), t(4, 0)),
            ("Church, then brunch".into(), t(9, 0))
        ]
    );
}

#[test]
fn floating_and_all_day() {
    let es = entries("misc.ics", d(2026, 10, 22));
    let drop = es.iter().find(|e| e.title == "Drop-off").unwrap();
    assert_eq!(
        drop.when,
        When::Timed {
            start: at(d(2026, 10, 22), 8, 15),
            end: at(d(2026, 10, 22), 8, 30),
            from_yesterday: false
        }
    );
    assert!(
        es.iter()
            .any(|e| e.title == "Ana's birthday" && e.when == When::AllDay)
    );
    for (date, shows) in [
        (d(2026, 10, 19), false),
        (d(2026, 10, 20), true),
        (d(2026, 10, 22), true),
        (d(2026, 10, 23), false),
    ] {
        let has = entries("misc.ics", date)
            .iter()
            .any(|e| e.title == "Conference");
        assert_eq!(has, shows, "{date}");
    }
    // A VALARM's own SUMMARY never replaces the event's.
    assert!(!es.iter().any(|e| e.title == "Alarm text"));
}

#[test]
fn crosses_midnight() {
    let es = entries("misc.ics", d(2026, 10, 22));
    let night = es.iter().find(|e| e.title == "Night shift").unwrap();
    assert_eq!(
        night.when,
        When::Timed {
            start: at(d(2026, 10, 21), 22, 0),
            end: at(d(2026, 10, 22), 10, 0),
            from_yesterday: true
        }
    );
    assert!(
        entries("misc.ics", d(2026, 10, 21))
            .iter()
            .any(|e| e.title == "Night shift")
    );
}

#[test]
fn cancelled_dropped() {
    assert!(
        !entries("misc.ics", d(2026, 10, 22))
            .iter()
            .any(|e| e.title.contains("lunch"))
    );
}

#[test]
fn private_is_busy() {
    let es = entries("misc.ics", d(2026, 10, 22));
    let busy: Vec<_> = es.iter().filter(|e| e.private).collect();
    assert_eq!(busy.len(), 2);
    assert!(busy.iter().all(|e| e.title == "Busy"));
    assert!(
        !es.iter()
            .any(|e| e.title == "Doctor" || e.title == "Salary review")
    );
    // 11:00 UTC is 7:00 in New York.
    assert!(starts(&es).contains(&("Busy".into(), t(7, 0))));
}

/// A `Net` that serves fixtures by URL and remembers what was asked.
struct Served {
    pages: Vec<(&'static str, Result<String, String>)>,
    asked: Mutex<Vec<String>>,
}

impl deckle_digest::fetch::Net for Served {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        self.asked.lock().unwrap().push(url.to_owned());
        self.pages
            .iter()
            .find(|(u, _)| *u == url)
            .map(|(_, r)| r.clone().map(String::into_bytes))
            .unwrap_or_else(|| Err(format!("no fixture for {url}")))
    }
}

fn served(pages: Vec<(&'static str, Result<String, String>)>) -> Served {
    Served {
        pages,
        asked: Mutex::new(Vec::new()),
    }
}

fn day_cfg(calendars: &[&str]) -> DayConfig {
    DayConfig {
        calendars: calendars.iter().map(|s| (*s).to_owned()).collect(),
        ..Default::default()
    }
}

#[test]
fn dupes_collapse() {
    let net = served(vec![
        ("https://a.example/a.ics", Ok(fixture("dupe-a.ics"))),
        ("https://b.example/b.ics", Ok(fixture("dupe-b.ics"))),
    ]);
    let a = agenda(
        &day_cfg(&["https://a.example/a.ics", "https://b.example/b.ics"]),
        d(2026, 10, 22),
        ny(),
        &net,
    );
    assert!(a.errors.is_empty(), "{:?}", a.errors);
    assert_eq!(a.entries.len(), 1);
}

#[test]
fn agenda_sorted_all_day_first() {
    let net = served(vec![("https://x.example/m.ics", Ok(fixture("misc.ics")))]);
    let a = agenda(
        &day_cfg(&["https://x.example/m.ics"]),
        d(2026, 10, 22),
        ny(),
        &net,
    );
    let kinds: Vec<bool> = a.entries.iter().map(|e| e.when == When::AllDay).collect();
    let first_timed = kinds.iter().position(|k| !k).unwrap();
    assert!(kinds[..first_timed].iter().all(|k| *k) && kinds[first_timed..].iter().all(|k| !k));
    let times = starts(&a.entries);
    // The night shift started yesterday, so it sorts first; the rest run in order.
    assert_eq!(times[0].0, "Night shift");
    assert!(times[1..].windows(2).all(|w| w[0].1 <= w[1].1), "{times:?}");
}

#[test]
fn outlook_zone_mapped() {
    let (es, guessed) = on_date(&fixture("outlook.ics"), d(2026, 10, 22), ny()).unwrap();
    assert!(starts(&es).contains(&("Standup".into(), t(12, 0))));
    // The unknown zone is read as local time, and the agenda says so.
    assert!(starts(&es).contains(&("Rover check".into(), t(9, 0))));
    assert!(guessed);
}

#[test]
fn known_zones_are_not_guessed() {
    assert!(
        !on_date(&fixture("zones.ics"), d(2026, 10, 25), ny())
            .unwrap()
            .1
    );
}

#[test]
fn webcal_rewritten() {
    let net = served(vec![(
        "https://cal.example/s3cr3t/basic.ics",
        Ok(fixture("weekly.ics")),
    )]);
    let body = read_calendar("webcal://cal.example/s3cr3t/basic.ics", &net).unwrap();
    assert!(body.contains("School pickup"));
    assert_eq!(
        *net.asked.lock().unwrap(),
        ["https://cal.example/s3cr3t/basic.ics"]
    );
}

#[test]
fn file_scheme_refused() {
    let net = served(vec![]);
    for spec in ["file:///x.ics", "http://cal.example/x.ics"] {
        let err = read_calendar(spec, &net).unwrap_err();
        assert!(err.contains("not supported"), "{spec}: {err}");
    }
    assert!(net.asked.lock().unwrap().is_empty());
}

#[test]
fn path_read() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("Work.ics");
    std::fs::write(&p, fixture("weekly.ics")).unwrap();
    let net = served(vec![]);
    assert!(
        read_calendar(p.to_str().unwrap(), &net)
            .unwrap()
            .contains("School pickup")
    );
    // `~/` is the home folder.
    let home = std::env::var("HOME").unwrap();
    if let Ok(rel) = p.strip_prefix(&home) {
        let tilde = format!("~/{}", rel.display());
        assert!(read_calendar(&tilde, &net).is_ok());
    }
}

#[test]
fn mask_hides_secret() {
    assert_eq!(
        mask("https://calendar.google.com/calendar/ical/abc/private-xyz/basic.ics"),
        "https://calendar.google.com/…"
    );
    assert_eq!(
        mask("webcal://p01-caldav.icloud.com/published/2/s3cr3t"),
        "https://p01-caldav.icloud.com/…"
    );
    assert_eq!(mask("/Users/j/Work.ics"), "Work.ics");
    assert_eq!(mask("~/Calendars/Home.ics"), "Home.ics");
}

#[test]
fn errors_masked() {
    let net = served(vec![(
        "https://cal.example/s3cr3t/basic.ics",
        Err("https://cal.example/s3cr3t/basic.ics: timed out".into()),
    )]);
    let a = agenda(
        &day_cfg(&["https://cal.example/s3cr3t/basic.ics"]),
        d(2026, 10, 22),
        ny(),
        &net,
    );
    assert_eq!(a.errors.len(), 1);
    assert!(
        a.errors[0].starts_with("https://cal.example/…: "),
        "{}",
        a.errors[0]
    );
    assert!(!a.errors[0].contains("s3cr3t"), "{}", a.errors[0]);
}

#[test]
fn html_login_page_is_error() {
    let net = served(vec![
        (
            "https://old.example/expired.ics",
            Ok("<!doctype html><html>Sign in</html>".into()),
        ),
        ("https://ok.example/w.ics", Ok(fixture("weekly.ics"))),
    ]);
    let a = agenda(
        &day_cfg(&[
            "https://old.example/expired.ics",
            "https://ok.example/w.ics",
        ]),
        d(2026, 10, 22),
        ny(),
        &net,
    );
    assert_eq!(a.errors.len(), 1);
    assert!(a.errors[0].ends_with("not a calendar"), "{}", a.errors[0]);
    assert_eq!(a.entries.len(), 1);
}

#[test]
fn untouched_config_has_no_day_table() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("digest.toml");
    deckle_digest::config::save(&p, &Config::default()).unwrap();
    let saved = std::fs::read_to_string(&p).unwrap();
    assert!(
        !saved.lines().any(|l| l.trim_start().starts_with("[day]")),
        "{saved}"
    );
    let c = Config::parse(deckle_digest::config::STARTER).unwrap();
    assert_eq!(c.day, DayConfig::default());
    assert_eq!(c.day.max_items, 8);
    assert!(c.day.weather_note);
    assert!(deckle_digest::config::STARTER.contains("# [day]"));
}
