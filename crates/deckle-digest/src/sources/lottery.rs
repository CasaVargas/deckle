// SPDX-License-Identifier: GPL-3.0-or-later
//! Latest Powerball and Mega Millions draws from New York's open data portal (no key).

use chrono::NaiveDateTime;
use serde::Deserialize;

use crate::config::Config;
use crate::fetch::Get;
use crate::{Article, Section, esc};

const TITLE: &str = "Lottery";
const POWERBALL: &str =
    "https://data.ny.gov/resource/d6yy-54nr.json?$order=draw_date%20DESC&$limit=1";
const MEGA_MILLIONS: &str =
    "https://data.ny.gov/resource/5xaw-6ayf.json?$order=draw_date%20DESC&$limit=1";

#[derive(Debug, Deserialize)]
struct Draw {
    draw_date: String,
    winning_numbers: String,
    #[serde(default)]
    mega_ball: Option<String>,
    #[serde(default)]
    multiplier: Option<String>,
    #[serde(default)]
    double_play_winning_numbers: Option<String>,
}

fn latest(json: &[u8]) -> Result<Draw, String> {
    let draws: Vec<Draw> = serde_json::from_slice(json).map_err(|e| e.to_string())?;
    draws.into_iter().next().ok_or_else(|| "no draws".into())
}

fn drawn_on(iso: &str) -> String {
    NaiveDateTime::parse_from_str(iso, "%Y-%m-%dT%H:%M:%S%.f")
        .map(|t| format!("Drawing of {}", t.format("%A, %B %-d")))
        .unwrap_or_else(|_| iso.to_owned())
}

fn balls(numbers: &[&str], special: Option<(&str, &str)>) -> String {
    let mut out = String::from("<p class=\"balls\">");
    for n in numbers {
        out.push_str(&format!("<span class=\"ball\">{}</span> ", esc(n)));
    }
    if let Some((label, n)) = special {
        out.push_str(&format!(
            "<span class=\"ball special\">{}</span> <span class=\"label\">{}</span>",
            esc(n),
            esc(label)
        ));
    }
    out.push_str("</p>");
    out
}

/// Powerball: six numbers, the last is the Powerball.
pub fn powerball(json: &[u8]) -> Result<Article, String> {
    let d = latest(json)?;
    let nums: Vec<&str> = d.winning_numbers.split_whitespace().collect();
    let Some((pb, white)) = nums.split_last() else {
        return Err("no numbers".into());
    };
    let mut body = balls(white, Some(("Powerball", pb)));
    if let Some(m) = &d.multiplier {
        body.push_str(&format!("<p>Power Play {}x</p>", esc(m)));
    }
    if let Some(dp) = &d.double_play_winning_numbers {
        let dp: Vec<&str> = dp.split_whitespace().collect();
        if let Some((pb, white)) = dp.split_last() {
            body.push_str("<p class=\"label\">Double Play</p>");
            body.push_str(&balls(white, Some(("Powerball", pb))));
        }
    }
    Ok(Article {
        teaser: None,
        title: "Powerball".into(),
        meta: Some(drawn_on(&d.draw_date)),
        body,
    })
}

/// Mega Millions: five numbers plus the Mega Ball.
pub fn mega_millions(json: &[u8]) -> Result<Article, String> {
    let d = latest(json)?;
    let nums: Vec<&str> = d.winning_numbers.split_whitespace().collect();
    let mb = d.mega_ball.as_deref().unwrap_or("?");
    let mut body = balls(&nums, Some(("Mega Ball", mb)));
    if let Some(m) = &d.multiplier {
        body.push_str(&format!("<p>Megaplier {}x</p>", esc(m)));
    }
    Ok(Article {
        teaser: None,
        title: "Mega Millions".into(),
        meta: Some(drawn_on(&d.draw_date)),
        body,
    })
}

pub fn section(cfg: &Config, get: &mut Get<'_>) -> Section {
    let mut articles = Vec::new();
    let mut errors = Vec::new();
    for game in &cfg.lottery.games {
        let result = match game.as_str() {
            "powerball" => get(POWERBALL).and_then(|b| powerball(&b)),
            "megamillions" => get(MEGA_MILLIONS).and_then(|b| mega_millions(&b)),
            other => Err(format!("unknown game {other:?}")),
        };
        match result {
            Ok(a) => articles.push(a),
            Err(e) => errors.push(format!("{game}: {e}")),
        }
    }
    if articles.is_empty() {
        return super::unavailable(
            TITLE,
            if errors.is_empty() {
                "no games in [lottery]".into()
            } else {
                errors.join("; ")
            },
        );
    }
    Section {
        title: TITLE.into(),
        kicker: Some("Always check results with the official lottery.".into()),
        articles,
        unavailable: (!errors.is_empty()).then(|| format!("Missing: {}.", errors.join("; "))),
    }
}
