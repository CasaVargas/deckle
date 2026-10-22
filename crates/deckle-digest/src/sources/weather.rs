// SPDX-License-Identifier: GPL-3.0-or-later
//! The Weather & Almanac page (almanac design §4): the forecast from Open-Meteo (free, no
//! key), and the sun, moon, sky and Your Day worked out on this machine.

use chrono::{Duration, NaiveDate};
use chrono_tz::Tz;
use serde::Deserialize;

use crate::config::{Config, Units};
use crate::fetch::Net;
use crate::tz::clock;
use crate::{Article, Section, day, esc, frost, moon, seasons, showers, sun};

const TITLE: &str = "Weather & Almanac";

#[derive(Debug, Deserialize)]
pub struct Forecast {
    pub current: Current,
    pub daily: Daily,
    /// For the strip's "am"/"pm" and the rain note only (almanac design §4.1).
    pub hourly: Hourly,
}

#[derive(Debug, Deserialize)]
pub struct Current {
    pub temperature_2m: f64,
    pub apparent_temperature: f64,
    pub relative_humidity_2m: f64,
    pub wind_speed_10m: f64,
    pub weather_code: u32,
}

#[derive(Debug, Deserialize)]
pub struct Daily {
    pub time: Vec<String>,
    pub weather_code: Vec<u32>,
    pub temperature_2m_max: Vec<f64>,
    pub temperature_2m_min: Vec<f64>,
    pub precipitation_probability_max: Vec<Option<f64>>,
    pub sunrise: Vec<String>,
    pub sunset: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Hourly {
    /// Local wall-clock times, `2026-10-22T16:00`.
    pub time: Vec<String>,
    pub precipitation_probability: Vec<Option<f64>>,
    /// Null for an hour the model doesn't cover.
    pub weather_code: Vec<Option<u32>>,
}

/// The forecast in `tz`, the paper's own clock, so hourly times line up with Your Day and
/// the edition date even when the Mac isn't where the forecast is for.
pub fn url(lat: f64, lon: f64, units: Units, tz: Tz) -> String {
    let (t, w) = match units {
        Units::Imperial => ("fahrenheit", "mph"),
        Units::Metric => ("celsius", "kmh"),
    };
    format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}\
         &current=temperature_2m,apparent_temperature,relative_humidity_2m,wind_speed_10m,weather_code\
         &daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,sunrise,sunset\
         &hourly=precipitation_probability,weather_code\
         &temperature_unit={t}&wind_speed_unit={w}&timezone={zone}&forecast_days=5",
        zone = tz.name().replace('/', "%2F")
    )
}

/// Parses a forecast; every daily series must cover every day so indexing is safe.
pub fn parse(json: &[u8]) -> Result<Forecast, String> {
    let f: Forecast = serde_json::from_slice(json).map_err(|e| e.to_string())?;
    let d = &f.daily;
    let n = d.time.len();
    if [
        d.weather_code.len(),
        d.temperature_2m_max.len(),
        d.temperature_2m_min.len(),
        d.precipitation_probability_max.len(),
        d.sunrise.len(),
        d.sunset.len(),
    ]
    .iter()
    .any(|&l| l != n)
    {
        return Err("forecast series have different lengths".into());
    }
    let h = &f.hourly;
    let n = h.time.len();
    if [h.precipitation_probability.len(), h.weather_code.len()]
        .iter()
        .any(|&l| l != n)
    {
        return Err("hourly forecast series have different lengths".into());
    }
    Ok(f)
}

/// WMO weather interpretation code → words.
pub fn describe(code: u32) -> &'static str {
    match code {
        0 => "Clear",
        1 => "Mostly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51 | 53 | 55 => "Drizzle",
        56 | 57 => "Freezing drizzle",
        61 => "Light rain",
        63 => "Rain",
        65 => "Heavy rain",
        66 | 67 => "Freezing rain",
        71 => "Light snow",
        73 => "Snow",
        75 => "Heavy snow",
        77 => "Snow grains",
        80..=82 => "Showers",
        85 | 86 => "Snow showers",
        95 => "Thunderstorms",
        96 | 99 => "Hailstorms",
        _ => "Unsettled",
    }
}

/// An hour this likely to be wet counts as wet.
pub(crate) const WET_PERCENT: f64 = 50.0;

/// The hours of daily entry `day` with rain or snow likely, as hours of the day.
fn wet_hours(f: &Forecast, day: usize) -> Vec<u32> {
    let date = &f.daily.time[day];
    f.hourly
        .time
        .iter()
        .zip(&f.hourly.precipitation_probability)
        .filter(|(t, p)| t.starts_with(date.as_str()) && p.unwrap_or(0.0) >= WET_PERCENT)
        .filter_map(|(t, _)| t.get(11..13)?.parse().ok())
        .collect()
}

/// The strip's word for daily entry `day`: "Showers, pm", "Frost at dawn".
pub fn strip_word(f: &Forecast, day: usize, units: Units) -> String {
    let code = f.daily.weather_code[day];
    let frost = match units {
        Units::Imperial => 32.0,
        Units::Metric => 0.0,
    };
    if code <= 2 && f.daily.temperature_2m_min[day] <= frost {
        return "Frost at dawn".into();
    }
    let word = describe(code);
    let wet = wet_hours(f, day);
    // Only rain, snow and storm words take a time of day.
    let falls = matches!(code, 51..=99);
    if !falls || wet.is_empty() || wet.iter().any(|h| *h < 12) && wet.iter().any(|h| *h >= 12) {
        word.into()
    } else if wet[0] < 12 {
        format!("{word}, am")
    } else {
        format!("{word}, pm")
    }
}

/// The strip's pictures; each always sits beside its word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Clear,
    Partly,
    Cloudy,
    Fog,
    Rain,
    Snow,
    Storm,
}

impl Icon {
    pub fn from_code(code: u32) -> Icon {
        match code {
            0 | 1 => Icon::Clear,
            2 => Icon::Partly,
            45 | 48 => Icon::Fog,
            51..=67 | 80..=82 => Icon::Rain,
            71..=77 | 85 | 86 => Icon::Snow,
            95..=99 => Icon::Storm,
            _ => Icon::Cloudy,
        }
    }

    pub fn href(self) -> &'static str {
        match self {
            Icon::Clear => "images/wx-clear.png",
            Icon::Partly => "images/wx-partly.png",
            Icon::Cloudy => "images/wx-cloudy.png",
            Icon::Fog => "images/wx-fog.png",
            Icon::Rain => "images/wx-rain.png",
            Icon::Snow => "images/wx-snow.png",
            Icon::Storm => "images/wx-storm.png",
        }
    }

    pub fn png(self) -> &'static [u8] {
        match self {
            Icon::Clear => include_bytes!("../../assets/weather/clear.png"),
            Icon::Partly => include_bytes!("../../assets/weather/partly.png"),
            Icon::Cloudy => include_bytes!("../../assets/weather/cloudy.png"),
            Icon::Fog => include_bytes!("../../assets/weather/fog.png"),
            Icon::Rain => include_bytes!("../../assets/weather/rain.png"),
            Icon::Snow => include_bytes!("../../assets/weather/snow.png"),
            Icon::Storm => include_bytes!("../../assets/weather/storm.png"),
        }
    }
}

fn deg(v: f64) -> String {
    format!("{}°", v.round() as i64)
}

/// The page's footer credit (almanac design §4).
const CREDIT: &str = "Forecast: Open-Meteo · Sky and moon worked out on your Mac";

/// Everything the page is built from; [`section`] fetches it, [`page`] lays it out.
pub struct PageInputs<'a> {
    pub cfg: &'a Config,
    pub date: NaiveDate,
    pub tz: Tz,
    pub forecast: Result<Forecast, String>,
    /// The usual frost dates, when the climate lookup has worked.
    pub frost: Option<frost::Usual>,
    /// Today's events, when calendars are configured.
    pub agenda: Option<day::Agenda>,
}

/// `(href, PNG)` pictures a page shows; the EPUB ships each once.
pub type Assets = Vec<(String, Vec<u8>)>;

/// The Weather & Almanac page, the front page's weather line, and the pictures it shows.
/// Each part prints on its own: a failed forecast leaves the almanac and Your Day.
pub fn page(i: &PageInputs) -> (Section, Option<String>, Assets) {
    let cfg = i.cfg;
    let units = cfg.units;
    let (lat, lon) = (
        cfg.latitude.unwrap_or_default(),
        cfg.longitude.unwrap_or_default(),
    );
    let mut articles = Vec::new();
    let mut assets: Assets = Vec::new();
    let mut line = None;

    // 1. Today and the strip.
    match &i.forecast {
        Ok(f) if !f.daily.time.is_empty() => {
            let (unit, wind_unit) = match units {
                Units::Imperial => ("F", "mph"),
                Units::Metric => ("C", "km/h"),
            };
            let d = &f.daily;
            let sky = describe(d.weather_code[0]);
            let rain = d.precipitation_probability_max[0].unwrap_or(0.0).round() as i64;
            line = Some(format!(
                "{sky} · {}/{}{unit} · {rain}% rain",
                deg(d.temperature_2m_max[0]),
                deg(d.temperature_2m_min[0]),
            ));
            let mut cells = String::new();
            for n in 0..d.time.len().min(5) {
                let icon = Icon::from_code(d.weather_code[n]);
                if !assets.iter().any(|(h, _)| h == icon.href()) {
                    assets.push((icon.href().to_owned(), icon.png().to_vec()));
                }
                let day = NaiveDate::parse_from_str(&d.time[n], "%Y-%m-%d")
                    .map(|x| x.format("%a").to_string().to_uppercase())
                    .unwrap_or_else(|_| d.time[n].clone());
                cells.push_str(&format!(
                    "<td><span class=\"wday\">{}</span><br/><img src=\"{}\" alt=\"\"/><br/>\
                     <b>{}</b> {}<br/><span class=\"word\">{}</span></td>",
                    esc(&day),
                    icon.href(),
                    deg(d.temperature_2m_max[n]),
                    deg(d.temperature_2m_min[n]),
                    esc(&strip_word(f, n, units)),
                ));
            }
            articles.push(Article {
                title: format!("Today: {sky}"),
                body: format!(
                    "<table class=\"strip\"><tr>{cells}</tr></table>\
                     <p>Right now: {now}{unit} and {now_sky}, feels like {feels}. \
                     Humidity {hum}%, wind {wind} {wind_unit}.</p>",
                    now = deg(f.current.temperature_2m),
                    now_sky = describe(f.current.weather_code).to_lowercase(),
                    feels = deg(f.current.apparent_temperature),
                    hum = f.current.relative_humidity_2m.round() as i64,
                    wind = f.current.wind_speed_10m.round() as i64,
                ),
                ..Default::default()
            });
        }
        Ok(_) => articles.push(forecast_down("no days in the forecast")),
        Err(e) => articles.push(forecast_down(e)),
    }

    // 2. The almanac.
    let today = sun::sun_day(i.date, lat, lon, i.tz);
    let yesterday = sun::sun_day(i.date - Duration::days(1), lat, lon, i.tz);
    let mut almanac = String::new();
    if let sun::SunDay::Normal { rise, set } = today {
        almanac.push_str(&format!(
            "<p>Sunrise {} · Sunset {}</p>",
            clock(rise.time(), units),
            clock(set.time(), units)
        ));
    }
    almanac.push_str(&format!(
        "<p>Daylight {}</p>",
        esc(&sun::daylight_line(&today, &yesterday))
    ));
    if let Some(frost) = i.frost.as_ref().and_then(|u| frost::line(u, i.date)) {
        almanac.push_str(&format!("<p>{}</p>", esc(&frost)));
    }
    let moon = moon::phase(i.date);
    almanac.push_str(&format!(
        "<p>{}, {}% lit · {}</p>",
        moon.name,
        moon.illumination,
        esc(&moon::next_line(i.date, i.tz, lat))
    ));
    if let Some(next) = seasons::coming_up(i.date, i.tz, lat, units) {
        almanac.push_str(&format!("<p>Coming up: {}</p>", esc(&next)));
    }
    articles.push(Article {
        title: "The almanac".into(),
        body: almanac,
        ..Default::default()
    });

    // 3. Tonight's sky.
    if let Some(sky) = showers::tonight(i.date, lat, moon.illumination) {
        articles.push(Article {
            title: "Tonight's sky".into(),
            body: format!("<p>{}</p>", esc(&sky)),
            ..Default::default()
        });
    }

    // 4. Your Day.
    if let Some(agenda) = &i.agenda {
        let mut body = day::render(agenda, cfg.day.max_items, units);
        if cfg.day.weather_note
            && let Ok(f) = &i.forecast
            && let Some(note) = day::weather_note(&agenda.entries, f, i.date, units)
        {
            body.push_str(&format!("<p class=\"rain\">{}</p>", esc(&note)));
        }
        articles.push(Article {
            title: "Your Day".into(),
            body,
            ..Default::default()
        });
    }

    if let Some(last) = articles.last_mut() {
        last.body
            .push_str(&format!("<p class=\"credit\">{}</p>", esc(CREDIT)));
    }
    let section = Section {
        title: TITLE.into(),
        kicker: (!cfg.place.is_empty()).then(|| cfg.place.clone()),
        articles,
        ..Default::default()
    };
    (section, line, assets)
}

fn forecast_down(why: impl std::fmt::Display) -> Article {
    Article {
        title: "Today".into(),
        body: format!(
            "<p class=\"note\">Forecast unavailable: {}.</p>",
            esc(&why.to_string())
        ),
        ..Default::default()
    }
}

/// The page, fetched: the forecast, the frost dates (cached after the first time) and
/// today's calendars.
pub fn section(
    cfg: &Config,
    date: NaiveDate,
    tz: Tz,
    net: &dyn Net,
) -> (Section, Option<String>, Assets) {
    let (Some(lat), Some(lon)) = (cfg.latitude, cfg.longitude) else {
        return (
            super::unavailable(TITLE, "set latitude and longitude in digest.toml"),
            None,
            Vec::new(),
        );
    };
    let inputs = PageInputs {
        cfg,
        date,
        tz,
        forecast: net
            .get(&url(lat, lon, cfg.units, tz))
            .and_then(|b| parse(&b)),
        frost: frost::usual(lat, lon, &cfg.almanac_cache(), net).ok(),
        agenda: (!cfg.day.calendars.is_empty()).then(|| day::agenda(&cfg.day, date, tz, net)),
    };
    page(&inputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_have_words() {
        assert_eq!(describe(0), "Clear");
        assert_eq!(describe(81), "Showers");
        assert_eq!(describe(1234), "Unsettled");
    }

    /// One day, 2026-10-22, with wet hours (probability 80) at `wet`.
    fn day(code: u32, low: f64, wet: &[u32]) -> Forecast {
        let hours: Vec<u32> = (0..24).collect();
        Forecast {
            current: Current {
                temperature_2m: 50.0,
                apparent_temperature: 48.0,
                relative_humidity_2m: 60.0,
                wind_speed_10m: 5.0,
                weather_code: code,
            },
            daily: Daily {
                time: vec!["2026-10-22".into()],
                weather_code: vec![code],
                temperature_2m_max: vec![60.0],
                temperature_2m_min: vec![low],
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
                weather_code: vec![Some(code); 24],
            },
        }
    }

    #[test]
    fn pm_showers() {
        assert_eq!(
            strip_word(&day(80, 45.0, &[13, 14, 15]), 0, Units::Imperial),
            "Showers, pm"
        );
    }

    #[test]
    fn am_rain() {
        assert_eq!(
            strip_word(&day(61, 45.0, &[6, 7, 11]), 0, Units::Imperial),
            "Light rain, am"
        );
    }

    #[test]
    fn all_day_no_suffix() {
        assert_eq!(
            strip_word(&day(63, 45.0, &[9, 15]), 0, Units::Imperial),
            "Rain"
        );
        assert_eq!(strip_word(&day(0, 45.0, &[]), 0, Units::Imperial), "Clear");
    }

    #[test]
    fn forecast_uses_the_papers_clock() {
        // Hourly times must share a clock with Your Day: the Mac's zone, not the place's.
        let la: Tz = "America/Los_Angeles".parse().unwrap();
        let u = url(40.71, -74.01, Units::Imperial, la);
        assert!(u.contains("&timezone=America%2FLos_Angeles&"), "{u}");
        assert!(!u.contains("timezone=auto"));
    }

    #[test]
    fn dry_words_get_no_timing() {
        // A clear or cloudy day with a wet hour stays "Clear": the suffix is for rain words.
        assert_eq!(
            strip_word(&day(0, 45.0, &[16]), 0, Units::Imperial),
            "Clear"
        );
        assert_eq!(
            strip_word(&day(3, 45.0, &[16]), 0, Units::Imperial),
            "Overcast"
        );
        assert_eq!(strip_word(&day(45, 45.0, &[7]), 0, Units::Imperial), "Fog");
    }

    #[test]
    fn frost_at_dawn() {
        assert_eq!(
            strip_word(&day(1, 31.0, &[]), 0, Units::Imperial),
            "Frost at dawn"
        );
        assert_eq!(
            strip_word(&day(1, -1.0, &[]), 0, Units::Metric),
            "Frost at dawn"
        );
        assert_eq!(
            strip_word(&day(1, 1.0, &[]), 0, Units::Metric),
            "Mostly clear"
        );
        assert_eq!(
            strip_word(&day(3, 31.0, &[]), 0, Units::Imperial),
            "Overcast"
        );
    }

    #[test]
    fn words_fit() {
        let cap = "Freezing drizzle, pm".len();
        for code in 0..100 {
            for wet in [&[13][..], &[6][..]] {
                let w = strip_word(&day(code, 45.0, wet), 0, Units::Imperial);
                assert!(w.len() <= cap, "{code}: {w}");
            }
        }
    }

    #[test]
    fn icon_mapping() {
        use Icon::*;
        for (code, icon) in [
            (0, Clear),
            (1, Clear),
            (2, Partly),
            (3, Cloudy),
            (45, Fog),
            (63, Rain),
            (51, Rain),
            (73, Snow),
            (77, Snow),
            (95, Storm),
            (81, Rain),
            (85, Snow),
        ] {
            assert_eq!(Icon::from_code(code), icon, "{code}");
        }
        assert_eq!(Icon::Rain.href(), "images/wx-rain.png");
    }

    #[test]
    fn hourly_nulls_are_tolerated() {
        // Open-Meteo sends null for an hour a model doesn't cover.
        let mut v: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/weather/forecast.json"))
                .unwrap();
        v["hourly"]["weather_code"][5] = serde_json::Value::Null;
        v["hourly"]["temperature_2m"][5] = serde_json::Value::Null;
        assert!(parse(v.to_string().as_bytes()).is_ok());
    }

    #[test]
    fn hourly_length_mismatch_rejected() {
        let mut v: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/weather/forecast.json"))
                .unwrap();
        assert!(parse(v.to_string().as_bytes()).is_ok());
        v["hourly"]["weather_code"].as_array_mut().unwrap().pop();
        assert!(parse(v.to_string().as_bytes()).is_err());
    }

    #[test]
    fn pngs_decode() {
        use Icon::*;
        for icon in [Clear, Partly, Cloudy, Fog, Rain, Snow, Storm] {
            let img = image::load_from_memory(icon.png()).unwrap();
            assert_eq!((img.width(), img.height()), (64, 64), "{icon:?}");
            assert_eq!(img.color(), image::ColorType::L8, "{icon:?}");
        }
    }
}
