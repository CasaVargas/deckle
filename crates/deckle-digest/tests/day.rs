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
    // (Only where `HOME` is set: Windows has no such variable.)
    if let Some(rel) = std::env::var("HOME")
        .ok()
        .and_then(|h| p.strip_prefix(h).ok().map(|r| r.to_owned()))
    {
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

// ---- Task 8: the printed list and the rain note ----------------------------------------

use deckle_digest::config::Units;
use deckle_digest::day::{Agenda, render, weather_note};
use deckle_digest::sources::weather::{Current, Daily, Forecast, Hourly};

fn timed(title: &str, h: u32, m: u32, mins: i64) -> Entry {
    let start = at(d(2026, 10, 22), h, m);
    Entry {
        when: When::Timed {
            start,
            end: start + chrono::Duration::minutes(mins),
            from_yesterday: false,
        },
        title: title.into(),
        private: false,
        uid: format!("{title}@t"),
    }
}

fn all_day(title: &str) -> Entry {
    Entry {
        when: When::AllDay,
        title: title.into(),
        private: false,
        uid: format!("{title}@t"),
    }
}

#[test]
fn render_order_and_overflow() {
    let mut entries = vec![all_day("Birthday"), all_day("Conference")];
    entries.push(Entry {
        when: When::Timed {
            start: at(d(2026, 10, 21), 22, 0),
            end: at(d(2026, 10, 22), 10, 0),
            from_yesterday: true,
        },
        title: "Night shift".into(),
        private: false,
        uid: "n@t".into(),
    });
    for h in 11..19 {
        entries.push(timed(&format!("Thing {h}"), h, 0, 30));
    }
    let a = Agenda {
        entries,
        ..Default::default()
    };
    let html = render(&a, 8, Units::Imperial);
    assert_eq!(html.matches("<tr").count(), 8, "{html}");
    assert!(html.contains("and 3 more"), "{html}");
    let birthday = html.find("Birthday").unwrap();
    assert!(birthday < html.find("Night shift").unwrap());
    assert!(html.contains("All day"));
    assert!(html.contains("until 10 AM"), "{html}");
    assert!(html.contains("11 AM"), "{html}");
}

#[test]
fn render_empty() {
    let html = render(&Agenda::default(), 8, Units::Imperial);
    assert!(html.contains("Nothing on the calendar today."));
}

#[test]
fn render_errors() {
    let a = Agenda {
        errors: vec!["https://cal.example/…: timed out".into()],
        guessed_zones: true,
        entries: vec![timed("Standup", 9, 0, 15)],
    };
    let html = render(&a, 8, Units::Metric);
    assert!(
        html.contains("Calendar unavailable: https://cal.example/…: timed out"),
        "{html}"
    );
    assert!(html.contains("Some times shown in local time."));
    assert!(html.contains("09:00"));
    assert!(!html.contains("Nothing on the calendar"));
}

#[test]
fn render_escapes() {
    let a = Agenda {
        entries: vec![timed("<b>&", 9, 0, 15)],
        ..Default::default()
    };
    let html = render(&a, 8, Units::Imperial);
    assert!(
        html.contains("&lt;b&gt;&amp;") && !html.contains("<b>"),
        "{html}"
    );
}

/// 2026-10-22 hourly: probability 80 and `code` at `wet` hours, else 10 and clear.
fn forecast(wet: &[u32], code: u32) -> Forecast {
    let hours: Vec<u32> = (0..24).collect();
    Forecast {
        current: Current {
            temperature_2m: 50.0,
            apparent_temperature: 48.0,
            relative_humidity_2m: 60.0,
            wind_speed_10m: 5.0,
            weather_code: 0,
        },
        daily: Daily {
            time: vec!["2026-10-22".into()],
            weather_code: vec![code],
            temperature_2m_max: vec![60.0],
            temperature_2m_min: vec![45.0],
            precipitation_probability_max: vec![Some(80.0)],
            sunrise: vec!["2026-10-22T07:20".into()],
            sunset: vec!["2026-10-22T18:10".into()],
        },
        hourly: Hourly {
            time: hours
                .iter()
                .map(|h| format!("2026-10-22T{h:02}:00"))
                .collect(),
            precipitation_probability: hours
                .iter()
                .map(|h| Some(if wet.contains(h) { 80.0 } else { 10.0 }))
                .collect(),
            weather_code: hours
                .iter()
                .map(|h| Some(if wet.contains(h) { code } else { 0 }))
                .collect(),
        },
    }
}

const DAY: fn() -> NaiveDate = || d(2026, 10, 22);

#[test]
fn note_rain() {
    let es = [timed("School pickup", 15, 45, 45)];
    assert_eq!(
        weather_note(&es, &forecast(&[16], 61), DAY(), Units::Imperial).as_deref(),
        Some("Rain likely around 4 PM, during School pickup.")
    );
}

#[test]
fn note_snow() {
    let es = [timed("School pickup", 15, 45, 45)];
    let note = weather_note(&es, &forecast(&[16], 73), DAY(), Units::Imperial).unwrap();
    assert!(note.starts_with("Snow likely"), "{note}");
}

#[test]
fn note_first_wet_only() {
    let es = [
        timed("Lunch", 12, 30, 60),
        timed("School pickup", 15, 45, 45),
    ];
    assert_eq!(
        weather_note(&es, &forecast(&[13, 16], 61), DAY(), Units::Imperial).as_deref(),
        Some("Rain likely around 1 PM, during Lunch.")
    );
}

#[test]
fn note_none() {
    let es = [timed("School pickup", 15, 45, 45), all_day("Conference")];
    assert_eq!(
        weather_note(&es, &forecast(&[], 61), DAY(), Units::Imperial),
        None
    );
    // Rain all day never mentions an all-day event.
    let all: Vec<u32> = (0..24).collect();
    assert_eq!(
        weather_note(
            &[all_day("Conference")],
            &forecast(&all, 61),
            DAY(),
            Units::Imperial
        ),
        None
    );
}

#[test]
fn note_private() {
    let mut e = timed("Busy", 16, 0, 60);
    e.private = true;
    assert_eq!(
        weather_note(&[e], &forecast(&[16], 61), DAY(), Units::Imperial).as_deref(),
        Some("Rain likely around 4 PM, during your 4 PM event.")
    );
}

#[test]
fn note_uses_forecast_clock() {
    // Hourly times and event times are both local wall-clock times: 16:00 matches 16:00,
    // whatever zone the Mac is in.
    let es = [timed("Pickup", 16, 0, 30)];
    assert!(weather_note(&es, &forecast(&[16], 61), DAY(), Units::Imperial).is_some());
    assert!(weather_note(&es, &forecast(&[17], 61), DAY(), Units::Imperial).is_none());
}

#[test]
fn note_metric_clock() {
    let es = [timed("School pickup", 15, 45, 45)];
    let note = weather_note(&es, &forecast(&[16], 61), DAY(), Units::Metric).unwrap();
    assert!(note.contains("around 16:00"), "{note}");
}

// ---- Final review fixes ------------------------------------------------------------------

fn ics(events: &[&str]) -> String {
    let mut s = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n");
    for e in events {
        s.push_str("BEGIN:VEVENT\r\n");
        for line in e.lines() {
            s.push_str(line.trim());
            s.push_str("\r\n");
        }
        s.push_str("END:VEVENT\r\n");
    }
    s.push_str("END:VCALENDAR\r\n");
    s
}

#[test]
fn one_bad_rule_skips_only_that_event() {
    // Google leaves UNTIL before DTSTART after a "this and following" split.
    let text = ics(&[
        "UID:split@t\nDTSTART;TZID=America/New_York:20261022T090000\nRRULE:FREQ=WEEKLY;UNTIL=20200101T000000Z\nSUMMARY:Old series",
        "UID:junk@t\nDTSTART;TZID=America/New_York:20261022T100000\nRRULE:FREQ=WEEKLY;X-EVOLUTION-ENDDATE=20301010T000000Z\nSUMMARY:Odd rule",
        "UID:ok@t\nDTSTART;TZID=America/New_York:20261022T110000\nSUMMARY:Dentist",
    ]);
    let (es, _) = on_date(&text, d(2026, 10, 22), ny()).unwrap();
    let titles: Vec<&str> = es.iter().map(|e| e.title.as_str()).collect();
    assert!(titles.contains(&"Dentist"), "{titles:?}");
}

#[test]
fn all_day_span_survives_spring_forward() {
    // Midnight Mar 7 to midnight Mar 9 is only 47 hours in New York.
    let text =
        ics(&["UID:trip@t\nDTSTART;VALUE=DATE:20260307\nDTEND;VALUE=DATE:20260309\nSUMMARY:Trip"]);
    for (date, shows) in [
        (d(2026, 3, 7), true),
        (d(2026, 3, 8), true),
        (d(2026, 3, 9), false),
    ] {
        let (es, _) = on_date(&text, date, ny()).unwrap();
        assert_eq!(es.iter().any(|e| e.title == "Trip"), shows, "{date}");
    }
}

#[test]
fn mask_strips_credentials() {
    assert_eq!(
        mask("https://jon:apppass@dav.example/cal.ics"),
        "https://dav.example/…"
    );
    assert_eq!(
        mask("webcal://jon@dav.example:8443/c"),
        "https://dav.example:8443/…"
    );
}

#[test]
fn absurd_duration_does_not_panic() {
    let text = ics(&[
        "UID:huge@t\nDTSTART;TZID=America/New_York:20261022T090000\nDURATION:P999999999999W\nSUMMARY:Forever",
        "UID:ok@t\nDTSTART;TZID=America/New_York:20261022T110000\nSUMMARY:Dentist",
    ]);
    let (es, _) = on_date(&text, d(2026, 10, 22), ny()).unwrap();
    assert!(es.iter().any(|e| e.title == "Dentist"));
}

#[test]
fn max_zero_prints_no_empty_table() {
    let a = Agenda {
        entries: vec![timed("Standup", 9, 0, 15)],
        ..Default::default()
    };
    let html = render(&a, 0, Units::Imperial);
    assert!(!html.contains("<table"), "{html}");
    assert!(html.contains("and 1 more"), "{html}");
}
