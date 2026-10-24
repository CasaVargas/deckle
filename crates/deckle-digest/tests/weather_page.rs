// SPDX-License-Identifier: GPL-3.0-or-later
//! The Weather & Almanac page, assembled (almanac design §4).

use chrono::{NaiveDate, NaiveDateTime};
use chrono_tz::Tz;
use deckle_digest::config::Config;
use deckle_digest::day::{Agenda, Entry, When};
use deckle_digest::frost::Usual;
use deckle_digest::sources::weather::{Forecast, PageInputs, page, parse, section};
use deckle_digest::{Edition, Gathered, render};

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 22).unwrap()
}

fn ny() -> Tz {
    "America/New_York".parse().unwrap()
}

fn cfg() -> Config {
    Config {
        place: "New York, NY".into(),
        latitude: Some(40.71),
        longitude: Some(-74.01),
        ..Config::default()
    }
}

/// The saved New York forecast, with rain likely at 4 PM today.
fn forecast() -> Forecast {
    let mut f = parse(include_bytes!("fixtures/weather/forecast.json")).unwrap();
    let i = f
        .hourly
        .time
        .iter()
        .position(|t| t == "2026-10-22T16:00")
        .unwrap();
    f.hourly.precipitation_probability[i] = Some(70.0);
    f.hourly.weather_code[i] = Some(61);
    f
}

fn timed(title: &str, start: &str, end: &str) -> Entry {
    let p = |s: &str| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap();
    Entry {
        when: When::Timed {
            start: p(start),
            end: p(end),
            from_yesterday: false,
        },
        title: title.into(),
        private: false,
        uid: format!("{title}@t"),
    }
}

fn agenda() -> Agenda {
    Agenda {
        entries: vec![
            timed(
                "Drop-off, then the 9:00 standup",
                "2026-10-22 08:15",
                "2026-10-22 09:15",
            ),
            timed("Lunch with Sam", "2026-10-22 12:30", "2026-10-22 13:30"),
            timed("School pickup", "2026-10-22 15:45", "2026-10-22 16:30"),
            timed("Book club", "2026-10-22 19:00", "2026-10-22 21:00"),
        ],
        ..Default::default()
    }
}

fn inputs<'a>(
    cfg: &'a Config,
    forecast: Result<Forecast, String>,
    agenda: Option<Agenda>,
) -> PageInputs<'a> {
    PageInputs {
        cfg,
        date: date(),
        tz: ny(),
        forecast,
        frost: Some(Usual {
            first_autumn: Some((10, 29)),
            last_spring: Some((4, 12)),
        }),
        agenda,
    }
}

/// The section as the paper prints it: its XHTML page inside the EPUB.
fn xhtml(section: deckle_digest::Section) -> String {
    let g = Gathered {
        edition: Edition {
            paper: "The Deckle Daily".into(),
            date: date(),
            place: "New York, NY".into(),
            weather_line: None,
            masthead: Default::default(),
            sections: vec![section],
            assets: Vec::new(),
        },
        news: Vec::new(),
        reading: None,
        long_reads: Vec::new(),
        reading_outcomes: Vec::new(),
        notes: Vec::new(),
    };
    let files = render::render(&g, Vec::new());
    let (_, bytes) = files
        .into_iter()
        .find(|(p, _)| p == "OEBPS/s1.xhtml")
        .unwrap();
    String::from_utf8(bytes).unwrap()
}

#[test]
fn golden_page() {
    let c = cfg();
    let (s, line, assets) = page(&inputs(&c, Ok(forecast()), Some(agenda())));
    assert!(line.is_some());
    let got = xhtml(s);
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/weather/golden-page.xhtml"
    );
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(path, &got).unwrap();
    }
    assert_eq!(got, std::fs::read_to_string(path).unwrap());
    // Every picture the page shows is shipped with it.
    for (href, png) in &assets {
        assert!(got.contains(href.as_str()), "{href}");
        assert!(!png.is_empty());
    }
    for want in [
        "Weather &amp; Almanac",
        "THU",
        "First frost usually Oct 29 here",
        "Orionids",
        "School pickup",
        "Rain likely around 4 PM, during School pickup.",
        "Forecast: Open-Meteo · Sky and moon worked out on your Mac",
    ] {
        assert!(got.contains(want), "missing {want:?}");
    }
}

#[test]
fn forecast_down_rest_prints() {
    let c = cfg();
    let (s, line, assets) = page(&inputs(&c, Err("timeout".into()), Some(agenda())));
    assert_eq!(line, None);
    assert!(assets.is_empty());
    assert!(s.unavailable.is_none());
    assert!(
        s.articles[0]
            .body
            .contains("Forecast unavailable: timeout."),
        "{}",
        s.articles[0].body
    );
    let titles: Vec<&str> = s.articles.iter().map(|a| a.title.as_str()).collect();
    assert!(
        titles.contains(&"The almanac") && titles.contains(&"Your Day"),
        "{titles:?}"
    );
    // Without a forecast there's no rain note.
    assert!(!xhtml(s).contains("likely around"));
}

#[test]
fn calendar_down_rest_prints() {
    let c = cfg();
    let down = Agenda {
        errors: vec!["https://cal.example/…: timed out".into()],
        ..Default::default()
    };
    let (s, _, _) = page(&inputs(&c, Ok(forecast()), Some(down)));
    let html = xhtml(s);
    assert!(html.contains("THU") && html.contains("The almanac"));
    assert!(html.contains("Calendar unavailable: https://cal.example/…: timed out"));
}

#[test]
fn no_calendar_no_your_day() {
    let c = cfg();
    let (s, _, _) = page(&inputs(&c, Ok(forecast()), None));
    assert!(!s.articles.iter().any(|a| a.title == "Your Day"));
}

#[test]
fn metric_times() {
    let c = Config {
        units: deckle_digest::config::Units::Metric,
        ..cfg()
    };
    let html = xhtml(page(&inputs(&c, Ok(forecast()), Some(agenda()))).0);
    assert!(html.contains("around 16:00"), "{html}");
    assert!(!html.contains(" AM") && !html.contains(" PM"), "{html}");
}

#[test]
fn no_location_unavailable() {
    let c = Config::default();
    let net = |u: &str| -> Result<Vec<u8>, String> { panic!("fetched {u}") };
    let (s, line, assets) = section(&c, date(), ny(), &net);
    assert!(s.unavailable.is_some());
    assert_eq!(line, None);
    assert!(assets.is_empty());
}
