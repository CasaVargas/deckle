// SPDX-License-Identifier: GPL-3.0-or-later
//! The horoscope, written locally: the same sign on the same day always reads the same, and
//! no service is asked. For entertainment, and the section says so.

use chrono::NaiveDate;

use crate::config::Config;
use crate::{Article, Section, esc};

pub const SIGNS: [(&str, &str, &str); 12] = [
    ("aries", "Aries", "Mar 21 – Apr 19"),
    ("taurus", "Taurus", "Apr 20 – May 20"),
    ("gemini", "Gemini", "May 21 – Jun 20"),
    ("cancer", "Cancer", "Jun 21 – Jul 22"),
    ("leo", "Leo", "Jul 23 – Aug 22"),
    ("virgo", "Virgo", "Aug 23 – Sep 22"),
    ("libra", "Libra", "Sep 23 – Oct 22"),
    ("scorpio", "Scorpio", "Oct 23 – Nov 21"),
    ("sagittarius", "Sagittarius", "Nov 22 – Dec 21"),
    ("capricorn", "Capricorn", "Dec 22 – Jan 19"),
    ("aquarius", "Aquarius", "Jan 20 – Feb 18"),
    ("pisces", "Pisces", "Feb 19 – Mar 20"),
];

const OPENINGS: &[&str] = &[
    "The morning favors the person who reads the whole page before deciding.",
    "Something you set aside weeks ago is ready to be picked back up.",
    "A small kindness you hand out early comes back to you before dinner.",
    "Today rewards patience more than speed.",
    "An unexpected message clears up a question you stopped asking.",
    "Your best idea arrives while you are doing something ordinary.",
    "A conversation you have been putting off goes better than you fear.",
    "The day starts slow and finishes with more than you planned.",
    "Old routines feel new if you change just one step.",
    "Someone notices the work you thought went unseen.",
    "Curiosity is your compass today; follow the side street.",
    "A tidy desk turns into a tidy mind by noon.",
    "You have more room in your schedule than it looks; claim it.",
    "A choice between two good options is still a good day.",
];

const FOCUS: &[&str] = &[
    "At home, a quick fix saves a long chore later.",
    "At work, say the simple version first and the details second.",
    "With money, the cheaper option is the better one this week.",
    "With friends, a short call beats a long text.",
    "For your health, a walk in daylight does more than coffee.",
    "In love, listen for what is meant, not only what is said.",
    "With family, an old story gets a new ending.",
    "In your projects, finish one thing before starting the next.",
    "With your time, guard the first hour of the evening.",
    "With learning, a single chapter today beats a binge on Sunday.",
];

const ADVICE: &[&str] = &[
    "Write the list down; your memory has better things to do.",
    "Say yes to the small invitation.",
    "Leave five minutes early and enjoy the quiet.",
    "Return the thing you borrowed.",
    "Trust the first draft, then sleep on the second.",
    "Let someone else choose lunch.",
    "Put your phone in another room for an hour.",
    "Ask the question everyone else is thinking.",
    "Keep the receipt.",
    "Read one more page tonight.",
];

const COLORS: &[&str] = &[
    "ink black",
    "paper white",
    "slate",
    "forest green",
    "brick red",
    "harbor blue",
    "mustard",
    "plum",
    "sand",
    "copper",
];

/// FNV-1a, so a reading is stable across runs and platforms.
fn seed(date: NaiveDate, sign: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in format!("{date}:{sign}").bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn pick<'a>(bank: &[&'a str], h: u64, salt: u32) -> &'a str {
    bank[(h.rotate_left(salt * 7) % bank.len() as u64) as usize]
}

/// One sign's reading for one day.
pub fn reading(sign: &str, date: NaiveDate) -> Option<Article> {
    let (key, name, range) = SIGNS.iter().find(|(k, _, _)| *k == sign)?;
    let h = seed(date, key);
    let lucky = (h.rotate_left(29) % 99) + 1;
    let match_sign = SIGNS[(h.rotate_left(41) % 12) as usize].1;
    Some(Article {
        url: None,
        outlet: None,
        teaser: None,
        photos: Vec::new(),
        title: (*name).into(),
        meta: Some((*range).into()),
        body: format!(
            "<p>{} {} {}</p><p class=\"small\">Lucky number {lucky} · Color: {} · Gets along with {}</p>",
            esc(pick(OPENINGS, h, 1)),
            esc(pick(FOCUS, h, 2)),
            esc(pick(ADVICE, h, 3)),
            esc(pick(COLORS, h, 4)),
            esc(match_sign),
        ),
    })
}

pub fn section(cfg: &Config, date: NaiveDate) -> Section {
    let wanted: Vec<String> = if cfg.horoscope.signs.is_empty() {
        SIGNS.iter().map(|(k, _, _)| (*k).to_owned()).collect()
    } else {
        cfg.horoscope
            .signs
            .iter()
            .map(|s| s.trim().to_lowercase())
            .collect()
    };
    let mut articles = Vec::new();
    let mut unknown = Vec::new();
    for s in &wanted {
        match reading(s, date) {
            Some(a) => articles.push(a),
            None => unknown.push(s.clone()),
        }
    }
    Section {
        weather: None,
        puzzle: None,
        key: String::new(),
        title: "Horoscope".into(),
        kicker: Some("Written by Deckle, for fun.".into()),
        articles,
        unavailable: (!unknown.is_empty())
            .then(|| format!("Unknown signs in [horoscope]: {}.", unknown.join(", "))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_per_day_and_varies_across_days() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
        let a = reading("leo", d).unwrap();
        assert_eq!(a.body, reading("leo", d).unwrap().body);
        let week: std::collections::HashSet<_> = (0..7)
            .map(|i| reading("leo", d + chrono::Days::new(i)).unwrap().body)
            .collect();
        assert!(week.len() > 1);
        assert!(reading("ophiuchus", d).is_none());
    }
}
