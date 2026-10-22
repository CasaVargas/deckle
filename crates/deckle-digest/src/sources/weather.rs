// SPDX-License-Identifier: GPL-3.0-or-later
//! Weather from Open-Meteo (free, no key), plus the moon from [`crate::moon`].

use chrono::{NaiveDate, NaiveDateTime};
use serde::Deserialize;

use crate::config::{Config, Units};
use crate::fetch::Net;
use crate::{Article, Section, esc};

const TITLE: &str = "Weather";

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
    pub temperature_2m: Vec<f64>,
    pub weather_code: Vec<u32>,
}

pub fn url(lat: f64, lon: f64, units: Units) -> String {
    let (t, w) = match units {
        Units::Imperial => ("fahrenheit", "mph"),
        Units::Metric => ("celsius", "kmh"),
    };
    format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}\
         &current=temperature_2m,apparent_temperature,relative_humidity_2m,wind_speed_10m,weather_code\
         &daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,sunrise,sunset\
         &hourly=precipitation_probability,temperature_2m,weather_code\
         &temperature_unit={t}&wind_speed_unit={w}&timezone=auto&forecast_days=5"
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
    if [
        h.precipitation_probability.len(),
        h.temperature_2m.len(),
        h.weather_code.len(),
    ]
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
    if wet.is_empty() || wet.iter().any(|h| *h < 12) && wet.iter().any(|h| *h >= 12) {
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

fn clock(iso: &str) -> String {
    NaiveDateTime::parse_from_str(iso, "%Y-%m-%dT%H:%M")
        .map(|t| t.format("%-I:%M %p").to_string())
        .unwrap_or_else(|_| iso.to_owned())
}

fn deg(v: f64) -> String {
    format!("{}°", v.round() as i64)
}

/// The Weather section and the one-line summary for the front page and cover.
pub fn section(cfg: &Config, date: NaiveDate, net: &dyn Net) -> (Section, Option<String>) {
    let (Some(lat), Some(lon)) = (cfg.latitude, cfg.longitude) else {
        return (
            super::unavailable(TITLE, "set latitude and longitude in digest.toml"),
            None,
        );
    };
    match net.get(&url(lat, lon, cfg.units)).and_then(|b| parse(&b)) {
        Ok(f) => from_forecast(&f, cfg, date),
        Err(e) => (super::unavailable(TITLE, e), None),
    }
}

pub fn from_forecast(f: &Forecast, cfg: &Config, date: NaiveDate) -> (Section, Option<String>) {
    let unit = match cfg.units {
        Units::Imperial => "F",
        Units::Metric => "C",
    };
    let wind_unit = match cfg.units {
        Units::Imperial => "mph",
        Units::Metric => "km/h",
    };
    let d = &f.daily;
    let mut articles = Vec::new();
    let mut line = None;
    if !d.time.is_empty() {
        let rain = d.precipitation_probability_max[0].unwrap_or(0.0).round() as i64;
        let sky = describe(d.weather_code[0]);
        line = Some(format!(
            "{sky} · {}/{}{unit} · {rain}% rain",
            deg(d.temperature_2m_max[0]),
            deg(d.temperature_2m_min[0]),
        ));
        articles.push(Article {
            teaser: None,
            photos: Vec::new(),
            title: format!("Today: {sky}"),
            meta: (!cfg.place.is_empty()).then(|| cfg.place.clone()),
            body: format!(
                "<p class=\"big\">{hi}<span class=\"lo\"> / {lo}{unit}</span></p>\
                 <p>{rain}% chance of rain. Sunrise {rise}, sunset {set}.</p>\
                 <p>Right now: {now}{unit} and {now_sky}, feels like {feels}. \
                 Humidity {hum}%, wind {wind} {wind_unit}.</p>",
                hi = deg(d.temperature_2m_max[0]),
                lo = deg(d.temperature_2m_min[0]),
                rise = clock(&d.sunrise[0]),
                set = clock(&d.sunset[0]),
                now = deg(f.current.temperature_2m),
                now_sky = describe(f.current.weather_code).to_lowercase(),
                feels = deg(f.current.apparent_temperature),
                hum = f.current.relative_humidity_2m.round() as i64,
                wind = f.current.wind_speed_10m.round() as i64,
            ),
        });
    }
    if d.time.len() > 1 {
        let mut rows = String::new();
        for i in 1..d.time.len() {
            let day = NaiveDate::parse_from_str(&d.time[i], "%Y-%m-%d")
                .map(|x| x.format("%A").to_string())
                .unwrap_or_else(|_| d.time[i].clone());
            rows.push_str(&format!(
                "<tr><td class=\"day\">{}</td><td>{}</td><td class=\"num\">{}/{}</td><td class=\"num\">{}%</td></tr>",
                esc(&day),
                describe(d.weather_code[i]),
                deg(d.temperature_2m_max[i]),
                deg(d.temperature_2m_min[i]),
                d.precipitation_probability_max[i].unwrap_or(0.0).round() as i64,
            ));
        }
        articles.push(Article {
            teaser: None,
            photos: Vec::new(),
            title: "The days ahead".into(),
            meta: None,
            body: format!("<table class=\"forecast\">{rows}</table>"),
        });
    }
    let moon = crate::moon::phase(date);
    articles.push(Article {
        teaser: None,
        photos: Vec::new(),
        title: format!("The moon: {}", moon.name),
        meta: None,
        body: format!(
            "<p>{}% illuminated, {:.0} days into its cycle. {}</p>",
            moon.illumination,
            moon.age_days,
            esc(&crate::moon::next_line(
                date,
                crate::tz::local_tz(),
                cfg.latitude.unwrap_or_default()
            ))
        ),
    });
    let section = Section {
        key: String::new(),
        title: TITLE.into(),
        kicker: Some("Forecast by Open-Meteo".into()),
        articles,
        unavailable: None,
    };
    (section, line)
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
                temperature_2m: vec![50.0; 24],
                weather_code: vec![code; 24],
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

    #[test]
    fn clock_is_twelve_hour() {
        assert_eq!(clock("2026-10-20T07:28"), "7:28 AM");
        assert_eq!(clock("2026-10-20T18:36"), "6:36 PM");
    }
}
