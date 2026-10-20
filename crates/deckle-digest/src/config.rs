// SPDX-License-Identifier: GPL-3.0-or-later
//! `digest.toml`: what goes in the paper, in what order.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SectionKind {
    Weather,
    Local,
    /// Every `[[feeds]]` entry, each as its own section, in file order.
    Feeds,
    Lottery,
    Horoscope,
    Onthisday,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Units {
    #[default]
    Imperial,
    Metric,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub title: String,
    pub place: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub units: Units,
    /// Editions kept on the device after a run.
    pub keep: usize,
    /// Feed items older than this are left out.
    pub max_age_hours: u32,
    pub sections: Vec<SectionKind>,
    pub local: LocalConfig,
    pub feeds: Vec<FeedConfig>,
    pub lottery: LotteryConfig,
    pub horoscope: HoroscopeConfig,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            title: "The Deckle Daily".into(),
            place: String::new(),
            latitude: None,
            longitude: None,
            units: Units::Imperial,
            keep: 3,
            max_age_hours: 36,
            sections: vec![
                SectionKind::Weather,
                SectionKind::Local,
                SectionKind::Feeds,
                SectionKind::Lottery,
                SectionKind::Horoscope,
                SectionKind::Onthisday,
            ],
            local: LocalConfig::default(),
            feeds: Vec::new(),
            lottery: LotteryConfig::default(),
            horoscope: HoroscopeConfig::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LocalConfig {
    /// A Google News search, e.g. "Moore County NC". Empty: none.
    pub google_news: String,
    /// Local paper RSS/Atom feeds, merged with the search.
    pub feeds: Vec<String>,
    pub max_items: usize,
    /// Local news moves slower than the wires: a longer window than `max_age_hours`.
    pub max_age_hours: u32,
}

impl Default for LocalConfig {
    fn default() -> Self {
        LocalConfig {
            google_news: String::new(),
            feeds: Vec::new(),
            max_items: 12,
            max_age_hours: 72,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedConfig {
    pub name: String,
    pub url: String,
    #[serde(default = "default_feed_items")]
    pub max_items: usize,
}

fn default_feed_items() -> usize {
    10
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LotteryConfig {
    /// `powerball`, `megamillions`.
    pub games: Vec<String>,
}

impl Default for LotteryConfig {
    fn default() -> Self {
        LotteryConfig {
            games: vec!["powerball".into(), "megamillions".into()],
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HoroscopeConfig {
    /// Lowercase sign names. Empty: all twelve.
    pub signs: Vec<String>,
}

/// The starter file `deckle digest init` writes.
pub const STARTER: &str = r#"# Deckle Daily — your morning paper, built on this machine.
# Edit freely; `deckle digest build` shows what a change does.

title = "The Deckle Daily"
place = "New York, NY"            # shown on the masthead and the weather page
latitude = 40.71                  # for the forecast (Open-Meteo, no key)
longitude = -74.01
units = "imperial"                # or "metric"
keep = 3                          # editions kept on the Kindle
max_age_hours = 36                # older feed items are left out

# Order of the paper. Remove a name to drop that section.
sections = ["weather", "local", "feeds", "lottery", "horoscope", "onthisday"]

[local]
google_news = "New York City"     # a Google News search; "" for none
feeds = []                        # local paper RSS feeds, merged in
max_items = 12
max_age_hours = 72                # local news moves slower

# Each [[feeds]] entry becomes its own section, in this order.
[[feeds]]
name = "Hacker News"
url = "https://hnrss.org/frontpage"
max_items = 10

[lottery]
games = ["powerball", "megamillions"]

[horoscope]
signs = []                        # e.g. ["leo", "virgo"]; empty prints all twelve
"#;

impl Config {
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// `~/Library/Application Support/deckle/digest.toml`, `%APPDATA%\deckle\digest.toml`, or
/// `$XDG_CONFIG_HOME/deckle/digest.toml` (`~/.config`).
pub fn default_path() -> PathBuf {
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Application Support"))
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home().map(|h| h.join(".config")))
    };
    base.unwrap_or_else(std::env::temp_dir)
        .join("deckle")
        .join("digest.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_parses() {
        let c = Config::parse(STARTER).unwrap();
        assert_eq!(c.feeds.len(), 1);
        assert_eq!(c.local.google_news, "New York City");
        assert_eq!(c.sections.len(), 6);
    }

    #[test]
    fn empty_file_is_defaults() {
        assert_eq!(Config::parse("").unwrap(), Config::default());
    }

    #[test]
    fn typo_is_an_error() {
        assert!(Config::parse("titel = \"x\"").is_err());
        assert!(Config::parse("sections = [\"wether\"]").is_err());
    }
}
