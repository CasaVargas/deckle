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
    /// A page drawn from your own Kindle: the books in progress, a past highlight, words you
    /// looked up (shelf design).
    Shelf,
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
        SectionKind::Shelf => "shelf",
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
    #[serde(skip_serializing_if = "crate::editor::EditorConfig::is_default")]
    pub llm: crate::editor::EditorConfig,
    pub title: String,
    /// Who the paper is for, on its card: "for Dad, in Ohio" (many papers §3).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reader: String,
    /// The Kindle this paper goes to, by its name or sync id; empty: any Kindle (many papers §4).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub kindle: String,
    /// A layout made for big type (many papers §6).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub large_print: bool,
    /// When the paper is built (many papers §5). Left out of a save while it's untouched.
    #[serde(skip_serializing_if = "ScheduleConfig::is_default")]
    pub schedule: ScheduleConfig,
    /// The face the paper's name is drawn in (masthead design §1). Left out of a save while it's
    /// the default.
    #[serde(skip_serializing_if = "crate::masthead::MastheadFont::is_default")]
    pub masthead: crate::masthead::MastheadFont,
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
    /// Newspapers the household subscribes to (publisher subscriptions design §3). A
    /// household setting: every paper sees the same list. Left out of a save while empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub subscriptions: Vec<SubscriptionConfig>,
    /// Your Day: today's events from calendars (almanac design §5). Left out of a save
    /// while it's untouched.
    #[serde(skip_serializing_if = "DayConfig::is_default")]
    pub day: DayConfig,
    /// The puzzle page (puzzle design §6). Left out of a save while it's untouched.
    #[serde(skip_serializing_if = "PuzzleConfig::is_default")]
    pub puzzle: PuzzleConfig,
    /// From Your Shelf (shelf design §6). Left out of a save while it's untouched.
    #[serde(skip_serializing_if = "ShelfConfig::is_default")]
    pub shelf: ShelfConfig,
    /// The Morgue, every edition kept (Morgue design §3). Left out of a save while untouched.
    #[serde(skip_serializing_if = "MorgueConfig::is_default")]
    pub morgue: MorgueConfig,
    /// `[[export]]`: folders each edition is also put in, e.g. iCloud Drive for an iPad (other
    /// readers design §7). Left out of a save while there are none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub export: Vec<ExportConfig>,
    /// `[remarkable]`: the paper as a PDF to a reMarkable (other readers design §9).
    #[serde(skip_serializing_if = "RemarkableConfig::is_default")]
    pub remarkable: RemarkableConfig,
    /// The frost cache (tests); the default is `almanac.json` beside `digest.toml`.
    #[serde(skip)]
    pub almanac_path: Option<PathBuf>,
    /// Stop the presses (tests); the default is `press.json` beside `digest.toml`.
    #[serde(skip)]
    pub press_path: Option<PathBuf>,
    /// Each day's puzzle (tests); the default is `puzzles.json` beside `digest.toml`.
    #[serde(skip)]
    pub puzzle_path: Option<PathBuf>,
    /// Keep the photos in color whatever the exports say (`digest build --bundle`).
    #[serde(skip)]
    pub force_color: bool,
    /// What the Kindles shared (tests); the default is `shelf.json` beside `digest.toml`.
    #[serde(skip)]
    pub shelf_path: Option<PathBuf>,
    /// Which paper this is (many papers §3): `None` for `digest.toml`'s, the main paper.
    #[serde(skip)]
    pub paper: Option<PaperRef>,
}

/// An extra paper: its slug (the file stem in `papers/`) and the folder its state lives in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaperRef {
    pub slug: String,
    pub state_dir: PathBuf,
}

/// A day of the week, for `[schedule] days`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Day {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Day {
    pub const ALL: [Day; 7] = [
        Day::Mon,
        Day::Tue,
        Day::Wed,
        Day::Thu,
        Day::Fri,
        Day::Sat,
        Day::Sun,
    ];

    /// `mon` … `sun`, as written in the config.
    pub fn key(self) -> &'static str {
        match self {
            Day::Mon => "mon",
            Day::Tue => "tue",
            Day::Wed => "wed",
            Day::Thu => "thu",
            Day::Fri => "fri",
            Day::Sat => "sat",
            Day::Sun => "sun",
        }
    }

    pub fn parse(s: &str) -> Option<Day> {
        Day::ALL
            .into_iter()
            .find(|d| d.key().eq_ignore_ascii_case(s.trim()))
    }
}

/// `[schedule]`: when the paper is built (many papers §5).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScheduleConfig {
    /// 24-hour time, e.g. "05:30"; unset: not scheduled from this file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    /// Empty: every day.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub days: Vec<Day>,
}

impl ScheduleConfig {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Some(at) = &self.at {
            crate::schedule::parse_at(at).map_err(|e| format!("[schedule] at: {e}"))?;
        }
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            llm: Default::default(),
            title: "The Deckle Daily".into(),
            masthead: crate::masthead::MastheadFont::default(),
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
                SectionKind::Shelf,
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
            subscriptions: Vec::new(),
            day: DayConfig::default(),
            puzzle: PuzzleConfig::default(),
            shelf: ShelfConfig::default(),
            morgue: MorgueConfig::default(),
            export: Vec::new(),
            remarkable: RemarkableConfig::default(),
            almanac_path: None,
            press_path: None,
            puzzle_path: None,
            force_color: false,
            shelf_path: None,
            reader: String::new(),
            kindle: String::new(),
            large_print: false,
            schedule: ScheduleConfig::default(),
            paper: None,
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

    /// The Google News topic feed. Google answers this with a 302 to a `/rss/topics/<id>`
    /// feed; the fetcher (ureq) follows redirects.
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

/// Whether a feed's stories are fetched whole: `true`, `false`, or `"lead"` (only the top story).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "FullTextRepr", into = "FullTextRepr")]
pub enum FullText {
    Yes,
    No,
    Lead,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum FullTextRepr {
    Bool(bool),
    Word(String),
}

impl TryFrom<FullTextRepr> for FullText {
    type Error = String;
    fn try_from(r: FullTextRepr) -> Result<Self, String> {
        match r {
            FullTextRepr::Bool(true) => Ok(FullText::Yes),
            FullTextRepr::Bool(false) => Ok(FullText::No),
            FullTextRepr::Word(w) if w.trim().eq_ignore_ascii_case("lead") => Ok(FullText::Lead),
            FullTextRepr::Word(w) => Err(format!(
                "unknown full_text \"{w}\"; use true, false or \"lead\""
            )),
        }
    }
}

impl From<FullText> for FullTextRepr {
    fn from(f: FullText) -> Self {
        match f {
            FullText::Yes => FullTextRepr::Bool(true),
            FullText::No => FullTextRepr::Bool(false),
            FullText::Lead => FullTextRepr::Word("lead".into()),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedConfig {
    pub name: String,
    #[serde(default)]
    pub url: String,
    /// More feeds merged into the same section.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub urls: Vec<String>,
    /// A Google News topic, merged in ahead of `url` and `urls`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<Topic>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_text: Option<FullText>,
    /// 0 means unset: [`Config::parse`] makes it 5 for a topic entry, else 10.
    #[serde(default = "default_feed_items")]
    pub max_items: usize,
}

impl FeedConfig {
    /// Every feed URL for this section: the topic's, then `url`, then `urls`, without repeats.
    pub fn sources(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let first = self.topic.map(Topic::url);
        let one = Some(self.url.trim().to_owned()).filter(|u| !u.is_empty());
        for u in first
            .into_iter()
            .chain(one)
            .chain(self.urls.iter().map(|u| u.trim().to_owned()))
        {
            if !u.is_empty() && !out.contains(&u) {
                out.push(u);
            }
        }
        out
    }

    /// How this entry fetches whole stories: its own setting, else `Lead` for a topic entry,
    /// else the global `full_text`.
    pub fn full(&self, global: bool) -> FullText {
        self.full_text.unwrap_or(match (self.topic, global) {
            (Some(_), _) => FullText::Lead,
            (None, true) => FullText::Yes,
            (None, false) => FullText::No,
        })
    }
}

fn default_feed_items() -> usize {
    0
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

/// `[morgue]`: where every edition is kept, and for how long.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MorgueConfig {
    /// A folder; `~` is the home folder, as in `[reading] inbox`.
    pub folder: String,
    /// Editions older than this many days are removed; `0` keeps everything.
    pub keep_days: u32,
}

impl Default for MorgueConfig {
    fn default() -> Self {
        MorgueConfig {
            folder: "~/Deckle/Morgue".into(),
            keep_days: 365,
        }
    }
}

impl MorgueConfig {
    pub fn is_default(&self) -> bool {
        *self == MorgueConfig::default()
    }
}

/// `[shelf]`: what From Your Shelf prints.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ShelfConfig {
    /// "On your nightstand": the books in progress.
    pub nightstand: bool,
    /// A past highlight, on Kindles that share them.
    pub highlights: bool,
    /// Words you looked up this week, on Kindles that share them.
    pub words: bool,
    /// Titles containing any of these (case-insensitive) stay off the nightstand.
    pub hide: Vec<String>,
}

impl Default for ShelfConfig {
    fn default() -> Self {
        ShelfConfig {
            nightstand: true,
            highlights: true,
            words: true,
            hide: Vec::new(),
        }
    }
}

impl ShelfConfig {
    pub fn is_default(&self) -> bool {
        *self == ShelfConfig::default()
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
    /// Take a snapshot of what's on the Kindle for From Your Shelf (shelf design §6).
    pub shelf: bool,
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
            shelf: true,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shelf: Option<bool>,
    /// What From Your Shelf calls this Kindle ("Mom's Kindle"); else the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// The actions settled for one device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SyncActions {
    pub push: bool,
    pub prune: bool,
    pub clippings: bool,
    pub readership: bool,
    pub shelf: bool,
}

impl SyncConfig {
    /// The entry for `id`, each key falling back to the entry written for the same Kindle before
    /// serial numbers (`<model>/<volume>#<serial>` → `<model>/<volume>`).
    fn entry(&self, id: &str) -> DeviceSync {
        let exact = self.devices.get(id).cloned().unwrap_or_default();
        let Some(older) = id
            .rsplit_once('#')
            .and_then(|(base, _)| self.devices.get(base))
        else {
            return exact;
        };
        DeviceSync {
            push: exact.push.or(older.push),
            prune: exact.prune.or(older.prune),
            clippings: exact.clippings.or(older.clippings),
            readership: exact.readership.or(older.readership),
            shelf: exact.shelf.or(older.shelf),
            name: exact.name.or_else(|| older.name.clone()),
        }
    }

    /// The name the owner gave this Kindle.
    pub fn device_name(&self, id: &str) -> Option<String> {
        self.entry(id).name.filter(|n| !n.trim().is_empty())
    }

    pub fn for_device(&self, id: &str) -> SyncActions {
        let d = self.entry(id);
        SyncActions {
            push: d.push.unwrap_or(self.push),
            prune: d.prune.unwrap_or(self.prune),
            clippings: d.clippings.unwrap_or(self.clippings),
            readership: d.readership.unwrap_or(self.readership),
            shelf: d.shelf.unwrap_or(self.shelf),
        }
    }

    /// Every named Kindle: (id, name).
    pub fn named(&self) -> impl Iterator<Item = (&str, &str)> {
        self.devices.iter().filter_map(|(id, d)| {
            d.name
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(|n| (id.as_str(), n))
        })
    }

    /// Names the Kindle `id` (Kindle profiles design §2); an empty name clears it. Refused when
    /// another Kindle already has the name, ignoring case.
    pub fn set_device_name(&mut self, id: &str, name: &str) -> Result<(), String> {
        let name = name.trim();
        if !name.is_empty()
            && let Some((other, _)) = self.named().find(|(other, n)| {
                n.eq_ignore_ascii_case(name) && !deckle_core::sync::same_kindle(other, id)
            })
        {
            return Err(format!(
                "another Kindle ({other}) is already called \"{name}\""
            ));
        }
        // The exact entry wins over one written before serials, so that one is left alone: other
        // Kindles of the model may still go by it.
        self.devices.entry(id.to_owned()).or_default().name =
            (!name.is_empty()).then(|| name.to_owned());
        Ok(())
    }

    /// The id of the Kindle called `name_or_id`, ignoring case; else the id itself when a Kindle
    /// with that id is configured.
    pub fn find_device(&self, name_or_id: &str) -> Option<String> {
        let want = name_or_id.trim();
        self.named()
            .find(|(_, n)| n.eq_ignore_ascii_case(want))
            .or_else(|| {
                self.devices
                    .keys()
                    .find(|id| deckle_core::sync::same_kindle(id, want))
                    .map(|id| (id.as_str(), ""))
            })
            .map(|(id, _)| id.to_owned())
    }

    /// Clippings asked for anywhere.
    pub fn any_clippings(&self) -> bool {
        self.clippings || self.devices.values().any(|d| d.clippings == Some(true))
    }
}

/// `[[subscriptions]]`: a newspaper the owner subscribes to. The password is not stored here;
/// it lives in the system keychain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscriptionConfig {
    /// A bare lowercase host: no scheme, path or `www.`.
    pub site: String,
    pub username: String,
    /// `"blox"` once detected; `None` until then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

impl SubscriptionConfig {
    /// `site` as a bare host: `https://www.ThePilot.com/` becomes `thepilot.com`. A path, query
    /// or fragment after the host is an error, as is anything that isn't a dotted host.
    pub fn normalized_site(raw: &str) -> Result<String, String> {
        let bad = |why: &str| format!("[[subscriptions]] site = {raw:?}: {why}");
        let lower = raw.trim().to_ascii_lowercase();
        let s = lower
            .strip_prefix("https://")
            .or_else(|| lower.strip_prefix("http://"))
            .unwrap_or(&lower);
        let (host, rest) = match s.find(['/', '?', '#']) {
            Some(i) => (&s[..i], &s[i..]),
            None => (s, ""),
        };
        if !rest.is_empty() && rest != "/" {
            return Err(bad("use just the site's host, like thepilot.com"));
        }
        let host = host.strip_prefix("www.").unwrap_or(host);
        let like = "use a host like thepilot.com";
        // The session's cookies and, later, the password go to `https://{site}/`: anything that
        // could make that URL name another host (`user@host`, a port, `\`) is refused outright.
        if host.is_empty()
            || !host.contains('.')
            || host.ends_with('.')
            || host.contains(['@', ':', '\\'])
            || host.contains(char::is_whitespace)
        {
            return Err(bad(like));
        }
        let parsed = url::Url::parse(&format!("https://{host}/")).map_err(|_| bad(like))?;
        if !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.port().is_some()
            || parsed.host_str() != Some(host)
        {
            return Err(bad(like));
        }
        Ok(host.to_owned())
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.username.trim().is_empty() {
            return Err(format!(
                "[[subscriptions]] site = {:?}: username is empty",
                self.site
            ));
        }
        Ok(())
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

/// One `[[export]]` folder: each edition (and, with `books`, each long read) is written there
/// after the build, plugged in or not (other readers design §7).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ExportConfig {
    /// What the app and the report call it; unique among the exports.
    pub name: String,
    /// The folder; `~` is the home folder, as in `[reading] inbox`.
    pub dir: String,
    pub format: ExportFormat,
    pub photos: Photos,
    /// With `format = "pdf"`: the screen the PDF is laid out for (PDF edition design §5).
    pub screen: crate::pdf::Screen,
    /// Editions kept in the folder; 0 keeps all.
    pub keep: usize,
    /// Also the long reads, not only the paper.
    pub books: bool,
}

impl Default for ExportConfig {
    fn default() -> Self {
        ExportConfig {
            name: String::new(),
            dir: String::new(),
            format: ExportFormat::Epub,
            photos: Photos::Color,
            screen: crate::pdf::Screen::default(),
            keep: 7,
            books: false,
        }
    }
}

/// What an export folder takes: the EPUB, the PDF edition laid out for its `screen`, or the
/// edition bundle for a native reader (edition bundle design §4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    #[default]
    Epub,
    Pdf,
    Bundle,
}

/// `[remarkable]`: the paper as a PDF to a reMarkable over its USB web interface (other readers
/// design §9). Left out of a save while it's untouched.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RemarkableConfig {
    pub enabled: bool,
    pub screen: crate::pdf::Screen,
    /// The PDF's photos; the screen's own choice when unset (color on a Paper Pro).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photos: Option<Photos>,
    /// Also the long reads, as EPUBs.
    pub books: bool,
}

impl RemarkableConfig {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The photos its PDF gets.
    pub fn photos(&self) -> Photos {
        self.photos.unwrap_or(self.screen.photos())
    }
}

/// The paper's photos: grayscale for e-ink, or as published for a color screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Photos {
    #[default]
    Gray,
    Color,
}

/// Each export has a name, unique, and a folder.
pub fn validate_exports(exports: &[ExportConfig]) -> Result<(), String> {
    for (i, e) in exports.iter().enumerate() {
        let name = e.name.trim();
        if name.is_empty() {
            return Err(format!("[[export]] #{}: give it a name", i + 1));
        }
        if e.dir.trim().is_empty() {
            return Err(format!("[[export]] {name:?}: dir is empty"));
        }
        if exports[..i].iter().any(|o| o.name.trim() == name) {
            return Err(format!("[[export]] {name:?}: two exports have this name"));
        }
    }
    Ok(())
}

/// The starter file `deckle digest init` writes.
pub const STARTER: &str = r#"# Deckle Daily — your morning paper, built on this machine.
# Edit freely; `deckle digest build` shows what a change does.

title = "The Deckle Daily"
# reader = "the whole house"      # who it's for, on its card in the Daily tab
# kindle = "Mom's Kindle"         # only this Kindle gets it (its name or sync id); unset: any
# large_print = true              # a layout for big type; set the size itself on the Kindle (Aa)
# masthead = "blackletter"        # the title's typeface: "serif", "sans", or the path to a .ttf
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
sections = ["reading", "weather", "local", "feeds", "shelf", "lottery", "horoscope", "puzzle", "onthisday"]

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
# A Google News topic (WORLD NATION BUSINESS TECHNOLOGY ENTERTAINMENT SPORTS SCIENCE HEALTH)
# prints its top story whole and the rest as briefs:
# [[feeds]]
# name = "Sports"
# topic = "SPORTS"
# urls = ["https://example.com/more.rss"]   # optional extra feeds, merged in
# full_text = "lead"              # true, false or "lead" (top story only)
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

# The Morgue: every edition kept in a folder you can see, search and back up.
# [morgue]
# folder = "~/Deckle/Morgue"      # point at iCloud Drive to keep it there
# keep_days = 365                 # 0 keeps everything

# From Your Shelf: the books in progress on your Kindle, a past highlight and the words you
# looked up this week (the last two only on Kindles that share them).
# [shelf]
# nightstand = true
# highlights = true
# words = true
# hide = []                       # titles containing any of these stay off the nightstand

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
shelf = true                      # From Your Shelf reads this Kindle (Daily tab)

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

# A newspaper you subscribe to: the paper signs in as you to read its full articles. The
# password is not stored here; it goes in the system keychain. One block per site.
# [[subscriptions]]
# site = "thepilot.com"           # just the host, no https:// or path
# username = "you@example.com"

# When the paper is built, once `deckle digest schedule` is installed.
# [schedule]
# at = "05:30"                    # 24-hour
# days = ["sat", "sun"]           # mon … sun; leave out for every day

# Also put each edition in a folder, plugged in or not: an iCloud Drive folder reaches an
# iPad's Files app (open it in Books), Dropbox reaches a Boox. Add one block per folder.
# [[export]]
# name = "iPad"
# dir = "~/Library/Mobile Documents/com~apple~CloudDocs/Deckle Daily"
# format = "epub"                 # or "pdf": the paper laid out for one screen; or "bundle":
#                                 # the paper as data, for a native reader app
# screen = "remarkable-2"         # for a PDF: remarkable-2, remarkable-paper-pro,
#                                 # remarkable-paper-pro-move, ipad, a5
# photos = "color"                # or "gray" for an e-ink reader
# keep = 7                        # editions kept in the folder; 0 keeps all
# books = false                   # also the long reads, not only the paper

# A reMarkable gets the paper as a PDF over its USB web interface: plug it in with
# Settings → Storage → USB web interface on. Deckle can't delete from it, so old papers stay.
# [remarkable]
# enabled = true
# screen = "remarkable-2"         # or remarkable-paper-pro, remarkable-paper-pro-move
# books = false                   # also the long reads, as EPUBs
"#;

/// A new extra paper (`deckle digest papers new`, the app's New paper): `{title}`, `{reader}`
/// and `{place}` are filled in as TOML strings, and `{location}` with the main paper's latitude
/// and longitude lines (empty when it has none).
pub const PAPER_STARTER: &str = r#"# A Deckle Daily paper of its own (many papers). The household settings, [sync], [reading]
# [[subscriptions]] and the email account, are set in digest.toml for every paper.

title = {title}
reader = {reader}                 # who it's for, on its card in the Daily tab
# kindle = "Mom's Kindle"         # only this Kindle gets it (its name or sync id); unset: any
# large_print = true              # a layout for big type; set the size itself on the Kindle (Aa)
place = {place}
{location}units = "imperial"                # or "metric"
keep = 3                          # editions kept on the Kindle

sections = ["weather", "local", "feeds"]

[local]
google_news = ""                  # a Google News search, e.g. "Toledo OH"
feeds = []

# [schedule]
# at = "06:00"                    # 24-hour
# days = ["sat"]                  # mon … sun; leave out for every day

# Email this paper to its own Kindle address, on the account set in digest.toml.
# [email]
# enabled = true
# to = "dad_ABC123@kindle.com"
# usb_too = false
"#;

impl Config {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut cfg: Config = toml::from_str(text).map_err(|e| e.to_string())?;
        cfg.normalize();
        cfg.normalize_subscriptions()?;
        cfg.email.validate()?;
        cfg.llm.validate()?;
        cfg.schedule.validate()?;
        validate_exports(&cfg.export)?;
        Ok(cfg)
    }

    /// Where an extra paper keeps a state file (`papers/<slug>/<name>`); `None` for the main
    /// paper, which keeps today's paths.
    pub fn state_file(&self, name: &str) -> Option<PathBuf> {
        self.paper.as_ref().map(|p| p.state_dir.join(name))
    }

    /// The build's work folder under `cache`: `digest`, or `digest/papers/<slug>`.
    pub fn work_dir(&self, cache: &Path) -> PathBuf {
        match &self.paper {
            None => cache.join("digest"),
            Some(p) => cache.join("digest").join("papers").join(&p.slug),
        }
    }

    /// `main` for `digest.toml`'s paper, else the extra paper's slug.
    pub fn slug(&self) -> &str {
        self.paper
            .as_ref()
            .map_or(crate::papers::MAIN, |p| p.slug.as_str())
    }

    /// Fills in what a person (or the app's "Add feed") left unset: a feed's `max_items` of 0
    /// becomes 5 for a topic entry, else 10. A feed with no source is kept; its section says so.
    pub fn normalize(&mut self) {
        for f in &mut self.feeds {
            if f.max_items == 0 {
                f.max_items = if f.topic.is_some() { 5 } else { 10 };
            }
        }
    }

    /// Makes each subscription's site a bare host, so a loaded site is always normalized.
    fn normalize_subscriptions(&mut self) -> Result<(), String> {
        for s in &mut self.subscriptions {
            s.site = SubscriptionConfig::normalized_site(&s.site)?;
        }
        Ok(())
    }

    /// The subscription for `host`: its site, or a subdomain of it.
    pub fn subscription(&self, host: &str) -> Option<&SubscriptionConfig> {
        let host = host.to_ascii_lowercase();
        self.subscriptions
            .iter()
            .find(|s| host == s.site || host.ends_with(&format!(".{}", s.site)))
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
    let mut cfg = cfg.clone();
    cfg.normalize_subscriptions()?;
    for s in &cfg.subscriptions {
        s.validate()?;
    }
    let cfg = &cfg;
    cfg.email.validate()?;
    cfg.llm.validate()?;
    cfg.schedule.validate()?;
    validate_exports(&cfg.export)?;
    let mut cfg = cfg.clone();
    cfg.normalize();
    let cfg = &cfg;
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => match cfg.paper {
            None => STARTER.to_owned(),
            Some(_) => String::new(),
        },
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut new = toml_edit::ser::to_document(cfg).map_err(|e| e.to_string())?;
    // Another paper's file never holds the household settings it inherits (many papers §3).
    if cfg.paper.is_some() {
        let t = new.as_table_mut();
        t.remove("sync");
        t.remove("reading");
        t.remove("subscriptions");
        if let Some(email) = t.get_mut("email").and_then(|e| e.as_table_like_mut()) {
            for key in ["from", "smtp_host", "smtp_port", "username"] {
                email.remove(key);
            }
        }
    }
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
    /// Some export wants the photos in color, so the build keeps them that way and makes a
    /// color edition beside the grayscale one.
    pub fn wants_color(&self) -> bool {
        // A bundle always carries color: graying is the reader's choice.
        self.force_color
            || self
                .export
                .iter()
                .any(|e| e.photos == Photos::Color || e.format == ExportFormat::Bundle)
            || (self.remarkable.enabled && self.remarkable.photos() == Photos::Color)
    }

    /// The edition bundles the build makes: one with the long reads, one without, as the
    /// `format = "bundle"` exports ask.
    pub fn bundles_wanted(&self) -> Vec<bool> {
        let mut out: Vec<bool> = self
            .export
            .iter()
            .filter(|e| e.format == ExportFormat::Bundle)
            .map(|e| e.books)
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The PDF editions the build makes: one per screen and photos an export folder or the
    /// reMarkable asks for.
    pub fn pdfs_wanted(&self) -> Vec<(crate::pdf::Screen, Photos)> {
        let mut out: Vec<_> = self
            .export
            .iter()
            .filter(|e| e.format == ExportFormat::Pdf)
            .map(|e| (e.screen, e.photos))
            .collect();
        if self.remarkable.enabled {
            out.push((self.remarkable.screen, self.remarkable.photos()));
        }
        out.sort_by_key(|(s, p)| (format!("{s:?}"), *p == Photos::Color));
        out.dedup();
        out
    }

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

    /// Where what the Kindles shared is kept.
    pub fn shelf_store(&self) -> PathBuf {
        self.shelf_path
            .clone()
            .unwrap_or_else(|| default_path().with_file_name("shelf.json"))
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

    fn feed(toml: &str) -> Result<FeedConfig, String> {
        Config::parse(toml).map(|c| c.feeds[0].clone())
    }

    #[test]
    fn old_feed_parses_unchanged() {
        let f = feed("[[feeds]]\nname = \"A\"\nurl = \"https://a.example/f\"\n").unwrap();
        assert_eq!(f.sources(), ["https://a.example/f"]);
        assert_eq!(f.max_items, 10);
        assert_eq!(f.full(true), FullText::Yes);
        assert_eq!(f.full(false), FullText::No);
    }

    #[test]
    fn topic_only_feed() {
        let f = feed("[[feeds]]\nname = \"Sports\"\ntopic = \"SPORTS\"\n").unwrap();
        assert_eq!(f.max_items, 5);
        assert_eq!(f.full(false), FullText::Lead);
        assert_eq!(f.sources(), [Topic::Sports.url()]);
    }

    #[test]
    fn all_three_sources_in_order() {
        let f = feed(
            "[[feeds]]\nname = \"S\"\ntopic = \"world\"\nurl = \"https://a.example/f\"\n\
             urls = [\"https://b.example/f\", \"https://a.example/f\"]\nmax_items = 7\n",
        )
        .unwrap();
        assert_eq!(
            f.sources(),
            [
                Topic::World.url(),
                "https://a.example/f".into(),
                "https://b.example/f".into()
            ]
        );
        assert_eq!(f.max_items, 7);
    }

    #[test]
    fn a_feed_with_no_source_still_loads() {
        let f = feed("[[feeds]]\nname = \"Empty\"\nurl = \" \"\nmax_items = 0\n").unwrap();
        assert!(f.sources().is_empty());
        assert_eq!(f.max_items, 10);
    }

    #[test]
    fn save_normalizes_a_zero_max_items() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("digest.toml");
        let mut cfg = Config::parse("").unwrap();
        cfg.feeds.push(FeedConfig {
            name: "New".into(),
            max_items: 0,
            ..Default::default()
        });
        save(&path, &cfg).unwrap();
        let back = Config::load(&path).unwrap();
        assert_eq!(back.feeds[0].max_items, 10);
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("max_items = 10")
        );
    }

    #[test]
    fn full_text_values() {
        let f = |v: &str| {
            feed(&format!(
                "[[feeds]]\nname = \"A\"\nurl = \"u\"\nfull_text = {v}\n"
            ))
        };
        assert_eq!(f("true").unwrap().full_text, Some(FullText::Yes));
        assert_eq!(f("false").unwrap().full_text, Some(FullText::No));
        assert_eq!(f("\"lead\"").unwrap().full_text, Some(FullText::Lead));
        assert!(f("\"sometimes\"").is_err());
    }

    #[test]
    fn save_round_trip_keeps_comments_and_new_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("digest.toml");
        std::fs::write(
            &path,
            "sections = [\"feeds\"]\n\n[[feeds]]\n# my sports\nname = \"Sports\" # desk\ntopic = \"SPORTS\"\n\
             urls = [\"https://b.example/f\"]\nfull_text = \"lead\"\n",
        )
        .unwrap();
        let mut cfg = Config::load(&path).unwrap();
        cfg.title = "Edited".into();
        save(&path, &cfg).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("# my sports") && text.contains("# desk"),
            "{text}"
        );
        assert!(text.contains("topic = \"SPORTS\""), "{text}");
        assert!(text.contains("https://b.example/f"), "{text}");
        assert!(text.contains("full_text = \"lead\""), "{text}");
        let back = Config::load(&path).unwrap();
        assert_eq!(back.feeds, cfg.feeds);
        assert_eq!(back.title, "Edited");
    }

    #[test]
    fn desktop_topic_save_round_trip() {
        // What the desktop app sends for a ticked topic (daily_save): a whole Config as JSON.
        let mut json = serde_json::to_value(Config::parse(STARTER).unwrap()).unwrap();
        json["feeds"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!(
                {"name": "Sports", "url": "", "topic": "SPORTS", "max_items": 5}
            ));
        let cfg: Config = serde_json::from_value(json).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("digest.toml");
        std::fs::write(&path, STARTER).unwrap();
        save(&path, &cfg).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# Deckle Daily"), "comments kept:\n{text}");
        assert!(text.contains("topic = \"SPORTS\""), "{text}");
        let back = Config::load(&path).unwrap();
        let f = back.feeds.iter().find(|f| f.name == "Sports").unwrap();
        assert_eq!(f.sources(), vec![Topic::Sports.url()]);
        assert_eq!(f.max_items, 5);
        assert_eq!(f.full(true), FullText::Lead);
        assert_eq!(back, cfg);
    }

    #[test]
    fn starter_parses() {
        let c = Config::parse(STARTER).unwrap();
        assert_eq!(c.feeds.len(), 1);
        assert_eq!(c.local.google_news, "New York City");
        assert_eq!(c.sections.len(), 9);
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
            ..Default::default()
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
    fn a_serial_id_falls_back_to_the_settings_written_before_serials() {
        let c = Config::parse(
            "[sync.devices.\"Kindle/Kindle\"]\nshelf = false\nname = \"Den\"\n\n\
             [sync.devices.\"Kindle/Kindle#G0B0AAAA\"]\nname = \"Mom's Kindle\"\n",
        )
        .unwrap();
        // An exact entry wins, and only for the keys it sets.
        assert_eq!(
            c.sync.device_name("Kindle/Kindle#G0B0AAAA").as_deref(),
            Some("Mom's Kindle")
        );
        assert!(!c.sync.for_device("Kindle/Kindle").shelf);
        // Another Kindle of the same model and volume inherits the older entry.
        assert!(!c.sync.for_device("Kindle/Kindle#G0B0BBBB").shelf);
        assert_eq!(
            c.sync.device_name("Kindle/Kindle#G0B0BBBB").as_deref(),
            Some("Den")
        );
        assert_eq!(c.sync.device_name("Other/Kindle"), None);
    }

    #[test]
    fn kindles_are_named_uniquely_found_by_name_and_saved_with_comments() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("digest.toml");
        std::fs::write(&path, STARTER).unwrap();
        let mut c = Config::load(&path).unwrap();
        c.sync
            .set_device_name("G000AAAA", " Mom's Kindle ")
            .unwrap();
        c.sync
            .set_device_name("Kindle/Kindle#G000BBBB", "Dad's Kindle")
            .unwrap();
        // A clash is refused, ignoring case; renaming the same Kindle isn't a clash.
        let err = c
            .sync
            .set_device_name("Kindle/Kindle#G000BBBB", "mom's kindle")
            .unwrap_err();
        assert!(err.contains("already called"), "{err}");
        c.sync
            .set_device_name("Kindle/Kindle#G000AAAA", "Mom's Kindle")
            .unwrap();
        save(&path, &c).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# Deckle Daily"), "comments stay");
        let c = Config::load(&path).unwrap();
        assert_eq!(
            c.sync.find_device("MOM'S KINDLE").as_deref(),
            Some("G000AAAA")
        );
        assert_eq!(
            c.sync.find_device("dad's kindle").as_deref(),
            Some("Kindle/Kindle#G000BBBB")
        );
        // An id finds itself, as does the same Kindle by its serial.
        assert_eq!(
            c.sync.find_device("g000bbbb").as_deref(),
            Some("Kindle/Kindle#G000BBBB")
        );
        assert_eq!(c.sync.find_device("Nobody"), None);
        // Mom's Kindle was named over MTP and over mass storage: one Kindle, two entries.
        let mut names: Vec<&str> = c.sync.named().map(|(_, n)| n).collect();
        names.sort();
        names.dedup();
        assert_eq!(names, ["Dad's Kindle", "Mom's Kindle"]);
        // An empty name clears it.
        let mut c = c;
        c.sync.set_device_name("G000AAAA", "  ").unwrap();
        assert_eq!(c.sync.device_name("G000AAAA"), None);
    }

    #[test]
    fn sync_defaults_push_and_prune_for_any_device() {
        let c = Config::parse("").unwrap();
        let want = SyncActions {
            push: true,
            prune: true,
            clippings: false,
            readership: true,
            shelf: true,
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
            shelf: true,
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
    fn subscriptions_parse_and_round_trip() {
        let text = "[[subscriptions]]\nsite = \"thepilot.com\"\nusername = \"me@example.com\"\n";
        let cfg = Config::parse(text).unwrap();
        assert_eq!(cfg.subscriptions.len(), 1);
        assert_eq!(cfg.subscriptions[0].site, "thepilot.com");
        assert_eq!(cfg.subscriptions[0].kind, None);
        assert!(cfg.subscription("www.thepilot.com").is_some());
        assert!(cfg.subscription("thepilot.com").is_some());
        assert!(cfg.subscription("notthepilot.com").is_none());
        let empty = Config::default();
        assert!(!toml::to_string(&empty).unwrap().contains("subscriptions"));
    }

    #[test]
    fn a_site_is_a_bare_host() {
        assert_eq!(
            SubscriptionConfig::normalized_site("https://www.ThePilot.com/").unwrap(),
            "thepilot.com"
        );
        assert_eq!(
            SubscriptionConfig::normalized_site("thepilot.com").unwrap(),
            "thepilot.com"
        );
        let e = SubscriptionConfig::normalized_site("thepilot.com/news").unwrap_err();
        assert!(e.contains("site"), "{e}");
        assert!(SubscriptionConfig::normalized_site("").is_err());
        assert!(SubscriptionConfig::normalized_site("localhost").is_err());
        assert!(SubscriptionConfig::normalized_site("the pilot.com").is_err());
    }

    #[test]
    fn a_site_that_could_name_another_host_is_refused() {
        for raw in [
            "thepilot.com@evil.com",
            "user:pw@thepilot.com",
            "https://thepilot.com@evil.com/",
            "thepilot.com:8443",
            "https://thepilot.com:443/",
            "thepilot.com\\@evil.com",
            "evil.com\\thepilot.com",
            "thepilot.com.",
            "the\tpilot.com",
            "thepil%6ft.com",
            "[::1]",
            "127.0.0.1:80",
        ] {
            assert!(
                SubscriptionConfig::normalized_site(raw).is_err(),
                "{raw:?} was accepted"
            );
        }
        for (raw, want) in [
            ("HTTPS://WWW.ThePilot.com/", "thepilot.com"),
            ("http://thepilot.com", "thepilot.com"),
            ("  news.thepilot.com  ", "news.thepilot.com"),
            ("my-paper.co.uk/", "my-paper.co.uk"),
        ] {
            assert_eq!(
                SubscriptionConfig::normalized_site(raw).as_deref(),
                Ok(want),
                "{raw:?}"
            );
        }
        let cfg =
            Config::parse("[[subscriptions]]\nsite = \"thepilot.com\"\nusername = \"\"\n").unwrap();
        assert!(
            cfg.subscriptions[0]
                .validate()
                .unwrap_err()
                .contains("username")
        );
    }

    #[test]
    fn a_loaded_site_is_normalized_and_bad_ones_refused() {
        let cfg = Config::parse(
            "[[subscriptions]]\nsite = \"https://WWW.ThePilot.com/\"\nusername = \"me\"\n",
        )
        .unwrap();
        assert_eq!(cfg.subscriptions[0].site, "thepilot.com");
        assert!(
            Config::parse("[[subscriptions]]\nsite = \"a.com/x\"\nusername = \"me\"\n").is_err()
        );
        assert!(
            Config::parse("[[subscriptions]]\nsite = \"a.com\"\nusername = \"m\"\nx = 1\n")
                .is_err()
        );
    }

    #[test]
    fn saving_keeps_subscriptions_and_leaves_an_empty_list_out() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("digest.toml");
        std::fs::write(&path, STARTER).unwrap();
        let mut c = Config::load(&path).unwrap();
        save(&path, &c).unwrap();
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains("\n[[subscriptions]]")
        );
        c.subscriptions.push(SubscriptionConfig {
            site: "thepilot.com".into(),
            username: "me@example.com".into(),
            kind: None,
        });
        save(&path, &c).unwrap();
        assert_eq!(Config::load(&path).unwrap().subscriptions, c.subscriptions);
        c.subscriptions[0].username = String::new();
        assert!(save(&path, &c).unwrap_err().contains("username"));
    }

    #[test]
    fn the_starter_subscriptions_block_parses_once_uncommented() {
        assert!(Config::parse(STARTER).unwrap().subscriptions.is_empty());
        let block: String = STARTER
            .lines()
            .skip_while(|l| !l.starts_with("# [[subscriptions]]"))
            .take_while(|l| l.starts_with('#'))
            .map(|l| format!("{}\n", l.strip_prefix("# ").unwrap_or(l)))
            .collect();
        assert!(block.starts_with("[[subscriptions]]"), "{block}");
        let text = STARTER.replace(
            &block
                .lines()
                .map(|l| format!("# {l}\n"))
                .collect::<String>(),
            &block,
        );
        let subs = Config::parse(&text).unwrap().subscriptions;
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].site, "thepilot.com");
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
