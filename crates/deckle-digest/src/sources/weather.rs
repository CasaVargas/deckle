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

pub fn url(lat: f64, lon: f64, units: Units) -> String {
    let (t, w) = match units {
        Units::Imperial => ("fahrenheit", "mph"),
        Units::Metric => ("celsius", "kmh"),
    };
    format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}\
         &current=temperature_2m,apparent_temperature,relative_humidity_2m,wind_speed_10m,weather_code\
         &daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,sunrise,sunset\
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
        96 | 99 => "Thunderstorms with hail",
        _ => "Unsettled",
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
            esc(&moon.next)
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

    #[test]
    fn clock_is_twelve_hour() {
        assert_eq!(clock("2026-10-20T07:28"), "7:28 AM");
        assert_eq!(clock("2026-10-20T18:36"), "6:36 PM");
    }
}
