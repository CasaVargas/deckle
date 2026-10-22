// SPDX-License-Identifier: GPL-3.0-or-later
//! `digest.toml`: what goes in the paper, in what order.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SectionKind {
    /// Your saved articles (save-to-paper design §5).
    Reading,
    Weather,
    Local,
    /// Every `[[feeds]]` entry, each as its own section, in file order.
    Feeds,
    Lottery,
    Horoscope,
    /// The mini crossword and word ladder (puzzle design).
    Puzzle,
    Onthisday,
}

/// The key a section goes by across editions (readership design §4): the kind's name, or
/// `feeds:<name>` for each `[[feeds]]` entry and `local:<name>` for each `[[local.places]]` one.
pub fn section_key(kind: SectionKind, feed: Option<&str>) -> String {
    let name = match kind {
        SectionKind::Reading => "reading",
        SectionKind::Weather => "weather",
        SectionKind::Local => match feed {
            // another place's local news (#102)
            Some(place) => return format!("local:{place}"),
            None => "local",
        },
        SectionKind::Feeds => return format!("feeds:{}", feed.unwrap_or_default()),
        SectionKind::Lottery => "lottery",
        SectionKind::Horoscope => "horoscope",
        SectionKind::Puzzle => "puzzle",
        SectionKind::Onthisday => "onthisday",
    };
    name.to_owned()
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
    /// Fetch each story's full text so nothing needs a browser (design §5a).
    pub full_text: bool,
    /// With full text: the story's photos, as grayscale JPEGs.
    pub photos: bool,
    /// Photos kept per story.
    pub max_photos: usize,
    pub sections: Vec<SectionKind>,
    pub local: LocalConfig,
    pub feeds: Vec<FeedConfig>,
    pub lottery: LotteryConfig,
    pub horoscope: HoroscopeConfig,
    pub reading: ReadingConfig,
    /// What `digest watch` does when a Kindle connects (auto-sync design §3).
    pub sync: SyncConfig,
    /// Send-to-Kindle email (email design §5). Left out of a save while it's untouched.
    #[serde(skip_serializing_if = "EmailConfig::is_default")]
    pub email: EmailConfig,
    /// Your Day: today's events from calendars (almanac design §5). Left out of a save
    /// while it's untouched.
    #[serde(skip_serializing_if = "DayConfig::is_default")]
    pub day: DayConfig,
    /// The puzzle page (puzzle design §6). Left out of a save while it's untouched.
    #[serde(skip_serializing_if = "PuzzleConfig::is_default")]
    pub puzzle: PuzzleConfig,
    /// The frost cache (tests); the default is `almanac.json` beside `digest.toml`.
    #[serde(skip)]
    pub almanac_path: Option<PathBuf>,
    /// Stop the presses (tests); the default is `press.json` beside `digest.toml`.
    #[serde(skip)]
    pub press_path: Option<PathBuf>,
    /// Each day's puzzle (tests); the default is `puzzles.json` beside `digest.toml`.
    #[serde(skip)]
    pub puzzle_path: Option<PathBuf>,
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
            full_text: true,
            photos: true,
            max_photos: 4,
            sections: vec![
                SectionKind::Reading,
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
            reading: ReadingConfig::default(),
            sync: SyncConfig::default(),
            email: EmailConfig::default(),
            day: DayConfig::default(),
            puzzle: PuzzleConfig::default(),
            almanac_path: None,
            press_path: None,
            puzzle_path: None,
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
    /// `[[local.places]]`: other towns' local news, each its own section after Local (#102).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub places: Vec<PlaceNews>,
}

/// Another place's local news: a relative's town, a vacation spot (local places design §1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaceNews {
    /// The section's title in the paper.
    pub name: String,
    /// A Google News search. Empty: none.
    #[serde(default)]
    pub google_news: String,
    /// Local paper RSS/Atom feeds, merged with the search.
    #[serde(default)]
    pub feeds: Vec<String>,
    /// Stories kept; `[local] max_items` when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_items: Option<usize>,
}

impl Default for LocalConfig {
    fn default() -> Self {
        LocalConfig {
            google_news: String::new(),
            feeds: Vec::new(),
            max_items: 12,
            max_age_hours: 72,
            places: Vec::new(),
        }
    }
}

/// A Google News topic, the source of a newspaper topic section.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum Topic {
    World,
    Nation,
    Business,
    Technology,
    Entertainment,
    Sports,
    Science,
    Health,
}

impl Topic {
    pub const ALL: [Topic; 8] = [
        Topic::World,
        Topic::Nation,
        Topic::Business,
        Topic::Technology,
        Topic::Entertainment,
        Topic::Sports,
        Topic::Science,
        Topic::Health,
    ];

    /// Case-insensitive, whitespace-trimmed lookup by slug.
    pub fn parse(s: &str) -> Option<Topic> {
        let s = s.trim();
        Topic::ALL
            .into_iter()
            .find(|t| t.slug().eq_ignore_ascii_case(s))
    }

    pub fn slug(self) -> &'static str {
        match self {
            Topic::World => "WORLD",
            Topic::Nation => "NATION",
            Topic::Business => "BUSINESS",
            Topic::Technology => "TECHNOLOGY",
            Topic::Entertainment => "ENTERTAINMENT",
            Topic::Sports => "SPORTS",
            Topic::Science => "SCIENCE",
            Topic::Health => "HEALTH",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Topic::World => "World",
            Topic::Nation => "Nation",
            Topic::Business => "Business",
            Topic::Technology => "Technology",
            Topic::Entertainment => "Entertainment",
            Topic::Sports => "Sports",
            Topic::Science => "Science",
            Topic::Health => "Health",
        }
    }

    pub fn url(self) -> String {
        format!(
            "https://news.google.com/rss/headlines/section/topic/{}?hl=en-US&gl=US&ceid=US:en",
            self.slug()
        )
    }
}

impl TryFrom<String> for Topic {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        Topic::parse(&s).ok_or_else(|| {
            let all: Vec<&str> = Topic::ALL.iter().map(|t| t.slug()).collect();
            format!("unknown topic \"{s}\"; use one of {}", all.join(", "))
        })
    }
}

impl From<Topic> for String {
    fn from(t: Topic) -> String {
        t.slug().to_string()
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DayConfig {
    /// `.ics` files or private `https://` (or `webcal://`) addresses. Empty: no Your Day.
    pub calendars: Vec<String>,
    pub max_items: usize,
    /// "Rain likely around 4 PM, during School pickup."
    pub weather_note: bool,
}

impl Default for DayConfig {
    fn default() -> Self {
        DayConfig {
            calendars: Vec::new(),
            max_items: 8,
            weather_note: true,
        }
    }
}

impl DayConfig {
    pub fn is_default(&self) -> bool {
        *self == DayConfig::default()
    }
}

/// `[puzzle]`: what the puzzle page prints.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PuzzleConfig {
    pub ladder: bool,
    /// Theme answers from yesterday's headlines, clued as the headline with the word blanked.
    pub theme: bool,
}

impl Default for PuzzleConfig {
    fn default() -> Self {
        PuzzleConfig {
            ladder: true,
            theme: true,
        }
    }
}

impl PuzzleConfig {
    pub fn is_default(&self) -> bool {
        *self == PuzzleConfig::default()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HoroscopeConfig {
    /// Lowercase sign names. Empty: all twelve.
    pub signs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ReadingConfig {
    /// Links and saved pages dropped here are saved for the next edition.
    pub inbox: String,
    /// Saved articles per edition; the rest wait.
    pub max_items: usize,
    /// Longer articles become their own book.
    pub long_read_words: usize,
    /// The queue file (tests); the default is `reading.jsonl` beside `digest.toml`.
    #[serde(skip)]
    pub queue_path: Option<PathBuf>,
    /// Saved entries to print inside the paper whatever their length: their book failed to
    /// convert, so the paper must not point to it (`digest run` sets this and rebuilds).
    #[serde(skip)]
    pub inline: Vec<String>,
}

impl Default for ReadingConfig {
    fn default() -> Self {
        ReadingConfig {
            inbox: "~/Deckle/Inbox".into(),
            max_items: 10,
            long_read_words: 4000,
            queue_path: None,
            inline: Vec::new(),
        }
    }
}

/// `[sync]`: the actions run on plug-in, with per-device overrides keyed by sync id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SyncConfig {
    pub push: bool,
    pub prune: bool,
    /// Write the Kindle's highlights as Markdown into the highlights folder (#60).
    pub clippings: bool,
    /// Note how far each edition was read, from the Kindle's `.yjf` (readership design).
    pub readership: bool,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub devices: BTreeMap<String, DeviceSync>,
}

impl Default for SyncConfig {
    fn default() -> Self {
        SyncConfig {
            push: true,
            prune: true,
            clippings: false,
            readership: true,
            devices: BTreeMap::new(),
        }
    }
}

/// `[sync.devices."<id>"]`: what's set here wins over `[sync]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DeviceSync {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prune: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clippings: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readership: Option<bool>,
}

/// The actions settled for one device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SyncActions {
    pub push: bool,
    pub prune: bool,
    pub clippings: bool,
    pub readership: bool,
}

impl SyncConfig {
    pub fn for_device(&self, id: &str) -> SyncActions {
        let d = self.devices.get(id).cloned().unwrap_or_default();
        SyncActions {
            push: d.push.unwrap_or(self.push),
            prune: d.prune.unwrap_or(self.prune),
            clippings: d.clippings.unwrap_or(self.clippings),
            readership: d.readership.unwrap_or(self.readership),
        }
    }

    /// Clippings asked for anywhere.
    pub fn any_clippings(&self) -> bool {
        self.clippings || self.devices.values().any(|d| d.clippings == Some(true))
    }
}

/// `[email]`: the paper to a Send-to-Kindle address over the owner's own SMTP account (email
/// design §5). The password is never here: it's in the system keychain (`secret`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EmailConfig {
    pub enabled: bool,
    /// `…@kindle.com` or `…@free.kindle.com`, nothing else.
    pub to: String,
    pub from: String,
    pub smtp_host: String,
    /// 587 (STARTTLS) or 465 (implicit TLS): there is no plain-text SMTP.
    pub smtp_port: u16,
    pub username: String,
    /// Also queue the paper for USB when the email went out.
    pub usb_too: bool,
}

impl Default for EmailConfig {
    fn default() -> Self {
        EmailConfig {
            enabled: false,
            to: String::new(),
            from: String::new(),
            smtp_host: String::new(),
            smtp_port: 587,
            username: String::new(),
            usb_too: false,
        }
    }
}

impl EmailConfig {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// A typo can't send the paper anywhere but a Kindle, or over plain text.
    pub fn validate(&self) -> Result<(), String> {
        let to = self.to.trim().to_ascii_lowercase();
        let kindle = ["@kindle.com", "@free.kindle.com"]
            .iter()
            .any(|d| to.len() > d.len() && to.ends_with(d));
        if !to.is_empty() && !kindle {
            return Err(format!(
                "[email] to = {:?}: it must be a Send-to-Kindle address (…@kindle.com or \
                 …@free.kindle.com)",
                self.to
            ));
        }
        if !matches!(self.smtp_port, 587 | 465) {
            return Err(format!(
                "[email] smtp_port = {}: use 587 (STARTTLS) or 465 (TLS); plain-text SMTP isn't \
                 supported",
                self.smtp_port
            ));
        }
        Ok(())
    }

    /// What's missing before an email can go out, as the config's key names.
    pub fn missing(&self) -> Vec<&'static str> {
        [
            ("to", &self.to),
            ("from", &self.from),
            ("smtp_host", &self.smtp_host),
            ("username", &self.username),
        ]
        .into_iter()
        .filter(|(_, v)| v.trim().is_empty())
        .map(|(k, _)| k)
        .collect()
    }
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
full_text = true                  # fetch whole stories, so nothing opens a browser
photos = true                     # with their photos, in grayscale for e-ink
max_photos = 4                    # per story

# Order of the paper. Remove a name to drop that section.
sections = ["reading", "weather", "local", "feeds", "lottery", "horoscope", "puzzle", "onthisday"]

[reading]
inbox = "~/Deckle/Inbox"          # point at iCloud Drive to save from an iPhone
max_items = 10                    # saved articles per edition; the rest wait
long_read_words = 4000            # longer pieces become their own book

[local]
google_news = "New York City"     # a Google News search; "" for none
feeds = []                        # local paper RSS feeds, merged in
max_items = 12
max_age_hours = 72                # local news moves slower

# Other places' local news, each its own section after Local: a relative's town, a trip.
# [[local.places]]
# name = "Asheville, NC"          # the section's title
# google_news = "Asheville NC"
# feeds = []

# Each [[feeds]] entry becomes its own section, in this order.
[[feeds]]
name = "Hacker News"
url = "https://hnrss.org/frontpage"
max_items = 10

[lottery]
games = ["powerball", "megamillions"]

[horoscope]
signs = []                        # e.g. ["leo", "virgo"]; empty prints all twelve

# The puzzle page: a mini crossword, a word ladder, and yesterday's answers.
# [puzzle]
# ladder = true
# theme = true                    # clues from yesterday's headlines, the word blanked

# Your Day on the weather page: today's events from your calendars. Use .ics files or a
# calendar's private address (it's a secret: anyone with it can read the calendar).
# [day]
# calendars = ["~/Calendars/Home.ics", "webcal://p01-caldav.icloud.com/published/2/…"]
# max_items = 8
# weather_note = true             # "Rain likely around 4 PM, during School pickup."

# What `deckle digest watch` does when a Kindle is plugged in.
[sync]
push = true                       # deliver waiting editions
prune = true                      # then keep only the newest `keep` on the Kindle
clippings = false                 # write highlights as Markdown (folder: deckle highlights export --to)
readership = true                 # note how far you read the paper (Daily tab)

# Per Kindle, by the "sync id" `deckle devices` shows:
# [sync.devices."G0000000000"]
# prune = false

# Email the paper to your Kindle each morning (opt-in). The password is not stored here:
# run `deckle digest email password` (or use the Daily tab), which saves it in the system
# keychain. Add the "from" address to Amazon's Approved Personal Document E-mail List.
# [email]
# enabled = false
# to = "you_ABC123@kindle.com"
# from = "you@example.com"
# smtp_host = "smtp.gmail.com"
# smtp_port = 587            # 587 = STARTTLS, 465 = implicit TLS; anything else is refused
# username = "you@example.com"
# usb_too = false            # also queue the paper for USB when the email succeeds
"#;

impl Config {
    pub fn parse(text: &str) -> Result<Self, String> {
        let cfg: Config = toml::from_str(text).map_err(|e| e.to_string())?;
        cfg.email.validate()?;
        Ok(cfg)
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// Writes `cfg` into the `digest.toml` at `path`, keeping what a person wrote: comments, key
/// order and every value that didn't change stay as they are. A missing file starts from
/// [`STARTER`] (Daily design §5c).
pub fn save(path: &Path, cfg: &Config) -> Result<(), String> {
    cfg.email.validate()?;
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => STARTER.to_owned(),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let new = toml_edit::ser::to_document(cfg).map_err(|e| e.to_string())?;
    merge_table(doc.as_table_mut(), new.as_table());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, doc.to_string()).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// `old` takes `new`'s keys and values; a value that changed keeps its comments, one that
/// didn't is left alone, and keys `new` lacks (cleared options) are removed.
fn merge_table(old: &mut toml_edit::Table, new: &toml_edit::Table) {
    use toml_edit::Item;
    let gone: Vec<String> = old
        .iter()
        .map(|(k, _)| k.to_owned())
        .filter(|k| !new.contains_key(k))
        .collect();
    for k in gone {
        old.remove(&k);
    }
    for (k, item) in new.iter() {
        match (old.get_mut(k), item) {
            (Some(Item::Table(o)), Item::Table(n)) => merge_table(o, n),
            (Some(Item::Table(o)), Item::Value(toml_edit::Value::InlineTable(n))) => {
                merge_table(o, &n.clone().into_table())
            }
            (Some(Item::Value(o)), Item::Value(n)) => {
                if o.to_string().trim() != n.to_string().trim() {
                    let decor = o.decor().clone();
                    *o = n.clone();
                    *o.decor_mut() = decor;
                }
            }
            // [[feeds]]: in the new order, each entry merged into the old one of the same name
            // so its comments stay; new entries come whole, dropped ones go. (The serializer
            // writes the new list as an inline array of inline tables.)
            (Some(Item::ArrayOfTables(o)), n) if tables_of(n).is_some() => {
                let name = |t: &toml_edit::Table| {
                    t.get("name").and_then(|v| v.as_str()).map(str::to_owned)
                };
                // Tables print in document-position order: hand the old slots out in the new order.
                let mut slots: Vec<_> = o.iter().filter_map(|t| t.position()).collect();
                slots.sort_unstable();
                let mut slots = slots.into_iter();
                let mut merged = toml_edit::ArrayOfTables::new();
                for nt in tables_of(n).unwrap_or_default() {
                    let mut t = match o
                        .iter()
                        .find(|ot| name(ot).is_some() && name(ot) == name(&nt))
                    {
                        Some(ot) => ot.clone(),
                        None => nt.clone(),
                    };
                    merge_table(&mut t, &nt);
                    t.set_position(slots.next());
                    merged.push(t);
                }
                *o = merged;
            }
            // New keys: take the new one whole.
            (_, n) => {
                old.insert(k, n.clone());
            }
        }
    }
}

/// `item` as a list of tables, when it is one: an array of tables, or an array of inline tables.
fn tables_of(item: &toml_edit::Item) -> Option<Vec<toml_edit::Table>> {
    match item {
        toml_edit::Item::ArrayOfTables(a) => Some(a.iter().cloned().collect()),
        toml_edit::Item::Value(toml_edit::Value::Array(a)) => a
            .iter()
            .map(|v| v.as_inline_table().map(|t| t.clone().into_table()))
            .collect(),
        _ => None,
    }
}

/// `~/Library/Application Support/deckle/digest.toml`, `%APPDATA%\deckle\digest.toml`, or
/// `$XDG_CONFIG_HOME/deckle/digest.toml` (`~/.config`).
impl Config {
    /// Where the frost dates are cached.
    pub fn almanac_cache(&self) -> PathBuf {
        self.almanac_path
            .clone()
            .unwrap_or_else(|| default_path().with_file_name("almanac.json"))
    }

    /// Where a stopped morning is kept (tomorrow's paper design §4.2).
    pub fn press_state(&self) -> PathBuf {
        self.press_path
            .clone()
            .unwrap_or_else(|| default_path().with_file_name("press.json"))
    }

    /// Where each day's puzzle is kept.
    pub fn puzzle_store(&self) -> PathBuf {
        self.puzzle_path
            .clone()
            .unwrap_or_else(|| default_path().with_file_name("puzzles.json"))
    }
}

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
    fn readership_defaults_on_and_follows_device_overrides() {
        let c = Config::parse("[sync.devices.\"G000\"]\nreadership = false\n").unwrap();
        assert!(c.sync.readership);
        assert!(!c.sync.for_device("G000").readership);
        assert!(c.sync.for_device("other").readership);
        assert!(STARTER.contains(
            "readership = true                 # note how far you read the paper (Daily tab)"
        ));
    }

    #[test]
    fn section_keys() {
        assert_eq!(section_key(SectionKind::Onthisday, None), "onthisday");
        assert_eq!(section_key(SectionKind::Feeds, Some("HN")), "feeds:HN");
    }

    #[test]
    fn saving_keeps_comments_inside_feeds_when_one_is_dropped_or_moved() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("digest.toml");
        std::fs::write(
            &path,
            "sections = [\"feeds\"]\n\n[[feeds]]\nname = \"A\" # morning tech\nurl = \"https://a.example/feed\"\n\n\
             [[feeds]]\n# keep\nname = \"B\"\nurl = \"https://b.example/feed\"\nmax_items = 3 # three is plenty\n\n\
             [[feeds]]\nname = \"C\"\nurl = \"https://c.example/feed\"\n",
        )
        .unwrap();
        let mut cfg = Config::load(&path).unwrap();
        cfg.feeds.retain(|f| f.name != "A");
        cfg.feeds.swap(0, 1); // C, B
        save(&path, &cfg).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("morning tech"), "{text}");
        assert!(text.contains("# keep"), "{text}");
        assert!(text.contains("# three is plenty"), "{text}");
        assert!(
            text.find("name = \"C\"").unwrap() < text.find("name = \"B\"").unwrap(),
            "{text}"
        );
        let back = Config::load(&path).unwrap();
        assert_eq!(
            back.feeds
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            ["C", "B"]
        );
    }

    #[test]
    fn starter_parses() {
        let c = Config::parse(STARTER).unwrap();
        assert_eq!(c.feeds.len(), 1);
        assert_eq!(c.local.google_news, "New York City");
        assert_eq!(c.sections.len(), 8);
        assert_eq!(c.sections[0], SectionKind::Reading);
        assert_eq!(c.reading.long_read_words, 4000);
    }

    #[test]
    fn empty_file_is_defaults() {
        assert_eq!(Config::parse("").unwrap(), Config::default());
    }

    #[test]
    fn saving_keeps_comments_and_untouched_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("digest.toml");
        std::fs::write(&path, STARTER).unwrap();
        let mut c = Config::load(&path).unwrap();
        c.title = "The Carthage Courier".into();
        c.feeds.push(FeedConfig {
            name: "NPR".into(),
            url: "https://feeds.npr.org/1001/rss.xml".into(),
            max_items: 8,
        });
        c.latitude = None;
        save(&path, &c).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# Deckle Daily — your morning paper, built on this machine."));
        assert!(text.contains(r#"title = "The Carthage Courier""#), "{text}");
        assert!(
            text.contains(r#"units = "imperial"                # or "metric""#),
            "an untouched line keeps its comment:\n{text}"
        );
        assert!(!text.contains("latitude"), "a cleared value is removed");
        assert_eq!(Config::load(&path).unwrap(), c, "round-trips");
    }

    #[test]
    fn saving_without_a_file_starts_from_the_starter() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new/digest.toml");
        let c = Config::parse(STARTER).unwrap();
        save(&path, &c).unwrap();
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .starts_with("# Deckle Daily")
        );
    }

    #[test]
    fn sync_defaults_push_and_prune_for_any_device() {
        let c = Config::parse("").unwrap();
        let want = SyncActions {
            push: true,
            prune: true,
            clippings: false,
            readership: true,
        };
        assert_eq!(c.sync.for_device("G000"), want);
        assert_eq!(
            Config::parse(STARTER).unwrap().sync.for_device("G000"),
            want
        );
    }

    #[test]
    fn a_device_overrides_only_what_it_sets() {
        let c = Config::parse(
            "[sync]\nprune = false\n\n[sync.devices.\"G000\"]\npush = false\nprune = true\n\n\
             [sync.devices.\"Kindle/Kindle\"]\nclippings = true\n",
        )
        .unwrap();
        let actions = |push, prune, clippings| SyncActions {
            push,
            prune,
            clippings,
            readership: true,
        };
        assert_eq!(c.sync.for_device("G000"), actions(false, true, false));
        assert_eq!(
            c.sync.for_device("Kindle/Kindle"),
            actions(true, false, true)
        );
        assert_eq!(c.sync.for_device("other"), actions(true, false, false));
        assert!(c.sync.any_clippings());
    }

    #[test]
    fn a_sync_typo_is_an_error() {
        assert!(Config::parse("[sync]\npsuh = true").is_err());
        assert!(Config::parse("[sync.devices.\"G\"]\nprnue = false").is_err());
    }

    #[test]
    fn saving_keeps_a_device_override() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("digest.toml");
        let text = format!("{STARTER}\n[sync.devices.\"G000\"]\nprune = false   # mine\n");
        std::fs::write(&path, &text).unwrap();
        let mut c = Config::load(&path).unwrap();
        c.title = "The Carthage Courier".into();
        save(&path, &c).unwrap();
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.contains("prune = false   # mine"), "{saved}");
        assert_eq!(Config::load(&path).unwrap(), c);
    }

    #[test]
    fn email_is_off_by_default() {
        let e = Config::parse("").unwrap().email;
        assert_eq!(e, EmailConfig::default());
        assert!(!e.enabled);
        assert_eq!(e.smtp_port, 587);
        assert!(!e.usb_too, "one copy of the paper, not two (design §11)");
        assert_eq!(
            Config::parse(STARTER).unwrap().email,
            EmailConfig::default(),
            "the starter's [email] block is commented out"
        );
    }

    #[test]
    fn the_starters_email_block_parses_once_uncommented() {
        let block: String = STARTER
            .lines()
            .skip_while(|l| !l.starts_with("# [email]"))
            .take_while(|l| l.starts_with('#'))
            .map(|l| format!("{}\n", l.strip_prefix("# ").unwrap_or(l)))
            .collect();
        assert!(block.starts_with("[email]"), "{block}");
        let text = STARTER.replace(
            &block
                .lines()
                .map(|l| format!("# {l}\n"))
                .collect::<String>(),
            &block,
        );
        let e = Config::parse(&text).unwrap().email;
        assert!(!e.enabled);
        assert_eq!(e.to, "you_ABC123@kindle.com");
        assert_eq!(e.smtp_host, "smtp.gmail.com");
        assert_eq!(e.smtp_port, 587);
        assert!(!e.usb_too);
    }

    #[test]
    fn email_only_goes_to_a_kindle_address() {
        let with = |to: &str| Config::parse(&format!("[email]\nto = \"{to}\"\n"));
        assert!(with("me_1@kindle.com").is_ok());
        assert!(with("me_1@free.kindle.com").is_ok());
        assert!(with("Me_1@Kindle.COM").is_ok());
        assert!(with("").is_ok(), "unset is fine until it's enabled");
        for bad in [
            "me@gmail.com",
            "me@kindle.com.evil.net",
            "me@notkindle.com",
            "kindle.com",
        ] {
            let e = with(bad).unwrap_err();
            assert!(e.contains("@kindle.com"), "{bad}: {e}");
        }
    }

    #[test]
    fn email_needs_tls_ports() {
        let with = |port: u16| Config::parse(&format!("[email]\nsmtp_port = {port}\n"));
        assert!(with(587).is_ok());
        assert!(with(465).is_ok());
        for bad in [25, 2525, 0] {
            let e = with(bad).unwrap_err();
            assert!(e.contains("587") && e.contains("465"), "{bad}: {e}");
        }
        assert!(Config::parse("[email]\npasword = \"x\"").is_err());
    }

    #[test]
    fn saving_refuses_a_bad_email_table_and_keeps_a_good_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("digest.toml");
        std::fs::write(&path, STARTER).unwrap();
        let mut c = Config::load(&path).unwrap();
        c.email.to = "me@gmail.com".into();
        assert!(save(&path, &c).is_err());
        c.email.to = "me_1@kindle.com".into();
        c.email.enabled = true;
        save(&path, &c).unwrap();
        assert_eq!(Config::load(&path).unwrap().email, c.email);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("password ="), "{text}");
    }

    #[test]
    fn typo_is_an_error() {
        assert!(Config::parse("titel = \"x\"").is_err());
        assert!(Config::parse("sections = [\"wether\"]").is_err());
    }

    #[test]
    fn topic_parse_is_case_insensitive() {
        assert_eq!(Topic::parse(" sports "), Some(Topic::Sports));
        assert_eq!(Topic::parse("golf"), None);
    }

    #[test]
    fn topic_urls() {
        assert_eq!(
            Topic::Technology.url(),
            "https://news.google.com/rss/headlines/section/topic/TECHNOLOGY?hl=en-US&gl=US&ceid=US:en"
        );
        let urls: std::collections::BTreeSet<String> = Topic::ALL.iter().map(|t| t.url()).collect();
        assert_eq!(urls.len(), 8);
    }

    #[test]
    fn topic_serde_round_trip() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct Holder {
            topic: Topic,
        }
        let h: Holder = toml::from_str("topic = \"sports\"").unwrap();
        assert_eq!(h.topic, Topic::Sports);
        assert_eq!(toml::to_string(&h).unwrap().trim(), "topic = \"SPORTS\"");
        let err = toml::from_str::<Holder>("topic = \"nonsense\"")
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown topic \"nonsense\""), "{err}");
        for t in Topic::ALL {
            assert!(err.contains(t.slug()), "{err}");
        }
    }
}
