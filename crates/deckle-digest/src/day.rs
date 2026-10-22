// SPDX-License-Identifier: GPL-3.0-or-later
//! Your Day: today's events from `.ics` calendars, files or private addresses (almanac
//! design §5). Calendar contents only ever go into the paper: nothing here logs or caches
//! them, and an address shows up in a message only as [`mask`] prints it.

use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;

use crate::config::DayConfig;
use crate::fetch::Net;
use crate::inbox::expand_home;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum When {
    AllDay,
    /// Local wall-clock times. `from_yesterday`: it began before today's midnight.
    Timed {
        start: NaiveDateTime,
        end: NaiveDateTime,
        from_yesterday: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub when: When,
    /// "Busy" for a private or confidential event.
    pub title: String,
    pub private: bool,
    pub uid: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Agenda {
    pub entries: Vec<Entry>,
    /// "https://host/…: why", one per calendar that couldn't be read.
    pub errors: Vec<String>,
    /// Some event named a time zone Deckle doesn't know; it was read as local time.
    pub guessed_zones: bool,
}

/// Windows zone names, as Outlook writes them, to IANA (CLDR's primary mapping).
pub const WINDOWS_ZONES: &[(&str, &str)] = &[
    ("Eastern Standard Time", "America/New_York"),
    ("Central Standard Time", "America/Chicago"),
    ("Mountain Standard Time", "America/Denver"),
    ("US Mountain Standard Time", "America/Phoenix"),
    ("Pacific Standard Time", "America/Los_Angeles"),
    ("Alaskan Standard Time", "America/Anchorage"),
    ("Hawaiian Standard Time", "Pacific/Honolulu"),
    ("Atlantic Standard Time", "America/Halifax"),
    ("GMT Standard Time", "Europe/London"),
    ("Greenwich Standard Time", "Atlantic/Reykjavik"),
    ("W. Europe Standard Time", "Europe/Berlin"),
    ("Central Europe Standard Time", "Europe/Budapest"),
    ("Central European Standard Time", "Europe/Warsaw"),
    ("Romance Standard Time", "Europe/Paris"),
    ("E. Europe Standard Time", "Europe/Chisinau"),
    ("FLE Standard Time", "Europe/Kiev"),
    ("AUS Eastern Standard Time", "Australia/Sydney"),
    ("Tokyo Standard Time", "Asia/Tokyo"),
    ("China Standard Time", "Asia/Shanghai"),
    ("India Standard Time", "Asia/Kolkata"),
    ("Singapore Standard Time", "Asia/Singapore"),
    ("New Zealand Standard Time", "Pacific/Auckland"),
    ("UTC", "UTC"),
];

/// How a calendar is named in messages: its file name, or `https://host/…`.
pub fn mask(spec: &str) -> String {
    let spec = spec.trim();
    if let Some(rest) = spec
        .strip_prefix("https://")
        .or_else(|| spec.strip_prefix("webcal://"))
        .or_else(|| spec.strip_prefix("http://"))
    {
        let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
        return format!("https://{host}/…");
    }
    if spec.contains("://") {
        return spec.split("://").next().unwrap_or_default().to_owned() + "://…";
    }
    spec.rsplit(['/', '\\']).next().unwrap_or(spec).to_owned()
}

/// A calendar's text: a path read from disk, or an `https`/`webcal` address fetched.
pub fn read_calendar(spec: &str, net: &dyn Net) -> Result<String, String> {
    let spec = spec.trim();
    let url = if let Some(rest) = spec.strip_prefix("webcal://") {
        Some(format!("https://{rest}"))
    } else if spec.starts_with("https://") {
        Some(spec.to_owned())
    } else if let Some((scheme, _)) = spec.split_once("://") {
        return Err(format!("{scheme} addresses are not supported"));
    } else {
        None
    };
    let bytes = match url {
        Some(url) => net.get(&url)?,
        None => std::fs::read(expand_home(spec)).map_err(|e| e.to_string())?,
    };
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

struct Prop {
    name: String,
    params: Vec<(String, String)>,
    value: String,
}

impl Prop {
    fn param(&self, key: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }
}

/// `NAME;P=V;P="V":value`, splitting at the first colon outside quotes.
fn prop(line: &str) -> Option<Prop> {
    let mut quoted = false;
    let colon = line.char_indices().find_map(|(i, c)| {
        if c == '"' {
            quoted = !quoted;
        }
        (c == ':' && !quoted).then_some(i)
    })?;
    let (head, value) = (&line[..colon], &line[colon + 1..]);
    let mut parts = head.split(';');
    let name = parts.next()?.trim().to_ascii_uppercase();
    let params = parts
        .filter_map(|p| p.split_once('='))
        .map(|(k, v)| (k.trim().to_owned(), v.trim_matches('"').to_owned()))
        .collect();
    Some(Prop {
        name,
        params,
        value: value.to_owned(),
    })
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push(' '),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out.trim().to_owned()
}

#[derive(Default)]
struct VEvent {
    props: Vec<Prop>,
}

impl VEvent {
    fn get(&self, name: &str) -> Option<&Prop> {
        self.props.iter().find(|p| p.name == name)
    }

    fn all(&self, name: &str) -> impl Iterator<Item = &Prop> {
        self.props.iter().filter(move |p| p.name == name)
    }

    fn text(&self, name: &str) -> String {
        self.get(name)
            .map(|p| unescape(&p.value))
            .unwrap_or_default()
    }
}

/// Top-level `VEVENT`s, with their `VALARM`s and other sub-components left out.
fn events(ics: &str) -> Result<Vec<VEvent>, String> {
    if !ics.contains("BEGIN:VCALENDAR") {
        return Err("not a calendar".into());
    }
    let unfolded = ics
        .replace("\r\n ", "")
        .replace("\r\n\t", "")
        .replace("\n ", "")
        .replace("\n\t", "");
    let mut out = Vec::new();
    let mut current: Option<VEvent> = None;
    let mut depth = 0usize;
    for line in unfolded.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(what) = line.strip_prefix("BEGIN:") {
            if what.eq_ignore_ascii_case("VEVENT") && current.is_none() {
                current = Some(VEvent::default());
            } else if current.is_some() {
                depth += 1;
            }
            continue;
        }
        if let Some(what) = line.strip_prefix("END:") {
            if current.is_some() && depth > 0 {
                depth -= 1;
            } else if what.eq_ignore_ascii_case("VEVENT") {
                out.extend(current.take());
            }
            continue;
        }
        if depth == 0
            && let (Some(ev), Some(p)) = (current.as_mut(), prop(line))
        {
            ev.props.push(p);
        }
    }
    Ok(out)
}

/// A date-time as written: a bare date, UTC, in a zone, or floating (local).
#[derive(Clone, Copy)]
enum Stamp {
    Date(NaiveDate),
    Utc(NaiveDateTime),
    Zoned(NaiveDateTime, Tz),
    Floating(NaiveDateTime),
}

struct Zones {
    local: Tz,
    guessed: bool,
}

impl Zones {
    fn named(&mut self, tzid: &str) -> Tz {
        let tzid = tzid.trim();
        if let Ok(tz) = tzid.parse::<Tz>() {
            return tz;
        }
        if let Some(tz) = WINDOWS_ZONES
            .iter()
            .find(|(w, _)| w.eq_ignore_ascii_case(tzid))
            .and_then(|(_, iana)| iana.parse().ok())
        {
            return tz;
        }
        self.guessed = true;
        self.local
    }

    /// Each comma-separated value of a date property.
    fn stamps(&mut self, p: &Prop) -> Vec<Stamp> {
        let date_only = p
            .param("VALUE")
            .is_some_and(|v| v.eq_ignore_ascii_case("DATE"));
        let zone = p.param("TZID").map(|z| self.named(z));
        p.value
            .split(',')
            .filter_map(|v| {
                let v = v.trim();
                if date_only || v.len() == 8 {
                    return NaiveDate::parse_from_str(v, "%Y%m%d").ok().map(Stamp::Date);
                }
                let (body, utc) = match v.strip_suffix('Z') {
                    Some(b) => (b, true),
                    None => (v, false),
                };
                let t = NaiveDateTime::parse_from_str(body, "%Y%m%dT%H%M%S").ok()?;
                Some(match (utc, zone) {
                    (true, _) => Stamp::Utc(t),
                    (false, Some(z)) => Stamp::Zoned(t, z),
                    (false, None) => Stamp::Floating(t),
                })
            })
            .collect()
    }
}

/// A wall-clock time in `tz`, skipping forward over a spring-forward gap.
fn in_zone(tz: Tz, t: NaiveDateTime) -> DateTime<Tz> {
    tz.from_local_datetime(&t)
        .earliest()
        .or_else(|| tz.from_local_datetime(&(t + Duration::hours(1))).earliest())
        .unwrap_or_else(|| tz.from_utc_datetime(&t))
}

/// The zone a stamp's recurrences are counted in, and the stamp as an instant there.
fn anchor(s: Stamp, local: Tz) -> (Tz, DateTime<Tz>) {
    match s {
        Stamp::Date(d) => (
            local,
            in_zone(local, d.and_hms_opt(0, 0, 0).expect("midnight")),
        ),
        Stamp::Utc(t) => (Tz::UTC, Utc.from_utc_datetime(&t).with_timezone(&Tz::UTC)),
        Stamp::Zoned(t, z) => (z, in_zone(z, t)),
        Stamp::Floating(t) => (local, in_zone(local, t)),
    }
}

/// `P1D`, `PT1H30M`, `P2W`: an iCalendar duration.
fn duration(text: &str) -> Option<Duration> {
    let (neg, rest) = match text.trim().strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, text.trim().trim_start_matches('+')),
    };
    let rest = rest.strip_prefix('P')?;
    let mut total = Duration::zero();
    let mut num = String::new();
    let mut in_time = false;
    for c in rest.chars() {
        match c {
            'T' => in_time = true,
            '0'..='9' => num.push(c),
            unit => {
                let n: i64 = num.parse().ok()?;
                num.clear();
                total += match (unit, in_time) {
                    ('W', _) => Duration::weeks(n),
                    ('D', _) => Duration::days(n),
                    ('H', true) => Duration::hours(n),
                    ('M', true) => Duration::minutes(n),
                    ('S', true) => Duration::seconds(n),
                    _ => return None,
                };
            }
        }
    }
    Some(if neg { -total } else { total })
}

/// RRULE's UNTIL must be UTC when the start has a zone; rewrite a local or date-only one.
fn until_in_utc(rule: &str, zone: Tz) -> String {
    rule.split(';')
        .map(|part| {
            let Some(v) = part.strip_prefix("UNTIL=") else {
                return part.to_owned();
            };
            if v.ends_with('Z') {
                return part.to_owned();
            }
            let local = if v.len() == 8 {
                NaiveDate::parse_from_str(v, "%Y%m%d")
                    .ok()
                    .and_then(|d| d.and_hms_opt(23, 59, 59))
            } else {
                NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S").ok()
            };
            match local {
                Some(t) => format!(
                    "UNTIL={}",
                    in_zone(zone, t)
                        .with_timezone(&Utc)
                        .format("%Y%m%dT%H%M%SZ")
                ),
                None => part.to_owned(),
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// One occurrence: its start instant, length, and whether it's all-day.
struct Occurrence {
    start: DateTime<Tz>,
    length: Duration,
    all_day: bool,
}

/// Every occurrence of `ev` starting in `[from, to)`, recurrences expanded.
fn occurrences(
    ev: &VEvent,
    zones: &mut Zones,
    from: DateTime<Tz>,
    to: DateTime<Tz>,
) -> Result<Vec<Occurrence>, String> {
    let Some(start) = ev
        .get("DTSTART")
        .and_then(|p| zones.stamps(p).into_iter().next())
    else {
        return Ok(Vec::new());
    };
    let all_day = matches!(start, Stamp::Date(_));
    let (zone, first) = anchor(start, zones.local);
    let end = ev
        .get("DTEND")
        .and_then(|p| zones.stamps(p).into_iter().next())
        .map(|s| anchor(s, zones.local).1);
    let length = match (end, ev.get("DURATION").and_then(|p| duration(&p.value))) {
        (Some(e), _) => e - first,
        (None, Some(d)) => d,
        (None, None) if all_day => Duration::days(1),
        (None, None) => Duration::zero(),
    };
    let rrule = ev.get("RRULE").map(|p| p.value.clone());
    let rdates: Vec<Stamp> = ev.all("RDATE").flat_map(|p| zones.stamps(p)).collect();
    if rrule.is_none() && rdates.is_empty() {
        return Ok(vec![Occurrence {
            start: first.with_timezone(&from.timezone()),
            length,
            all_day,
        }]);
    }
    let rz = rrule::Tz::Tz(zone);
    let mut set = rrule::RRuleSet::new(first.with_timezone(&rz));
    if let Some(rule) = rrule {
        let parsed: rrule::RRule<rrule::Unvalidated> = until_in_utc(&rule, zone)
            .parse()
            .map_err(|e| format!("RRULE: {e}"))?;
        set = set.rrule(
            parsed
                .validate(first.with_timezone(&rz))
                .map_err(|e| format!("RRULE: {e}"))?,
        );
    }
    for s in rdates {
        set = set.rdate(anchor(s, zones.local).1.with_timezone(&rz));
    }
    for s in ev
        .all("EXDATE")
        .flat_map(|p| zones.stamps(p))
        .collect::<Vec<_>>()
    {
        set = set.exdate(anchor(s, zones.local).1.with_timezone(&rz));
    }
    let found = set
        .after((from - length.max(Duration::zero())).with_timezone(&rz))
        .before(to.with_timezone(&rz))
        .all(500);
    Ok(found
        .dates
        .into_iter()
        .map(|t| Occurrence {
            start: t.with_timezone(&from.timezone()),
            length,
            all_day,
        })
        .collect())
}

/// Today's events in one calendar's text, and whether any zone had to be guessed.
pub fn on_date(ics: &str, date: NaiveDate, tz: Tz) -> Result<(Vec<Entry>, bool), String> {
    let evs = events(ics)?;
    let mut zones = Zones {
        local: tz,
        guessed: false,
    };
    let day_start = in_zone(tz, date.and_hms_opt(0, 0, 0).expect("midnight"));
    let day_end = in_zone(
        tz,
        (date + Duration::days(1))
            .and_hms_opt(0, 0, 0)
            .expect("midnight"),
    );
    // Instances moved or cancelled by a RECURRENCE-ID override, per UID.
    let mut moved: BTreeMap<String, HashSet<DateTime<Utc>>> = BTreeMap::new();
    for ev in &evs {
        if let Some(rid) = ev
            .get("RECURRENCE-ID")
            .and_then(|p| zones.stamps(p).into_iter().next())
        {
            moved
                .entry(ev.text("UID"))
                .or_default()
                .insert(anchor(rid, tz).1.with_timezone(&Utc));
        }
    }
    let mut out = Vec::new();
    for ev in &evs {
        if ev.text("STATUS").eq_ignore_ascii_case("CANCELLED") {
            continue;
        }
        let uid = ev.text("UID");
        let is_override = ev.get("RECURRENCE-ID").is_some();
        let class = ev.text("CLASS").to_ascii_uppercase();
        let private = class == "PRIVATE" || class == "CONFIDENTIAL";
        let title = if private {
            "Busy".to_owned()
        } else {
            ev.text("SUMMARY")
        };
        let window_from = day_start - Duration::days(1);
        let window_to = day_end + Duration::days(1);
        for o in occurrences(ev, &mut zones, window_from, window_to)? {
            if !is_override
                && moved
                    .get(&uid)
                    .is_some_and(|m| m.contains(&o.start.with_timezone(&Utc)))
            {
                continue;
            }
            let when = if o.all_day {
                let first = o.start.date_naive();
                let days = o.length.num_days().max(1);
                if !(first <= date && date < first + Duration::days(days)) {
                    continue;
                }
                When::AllDay
            } else {
                let (s, e) = (o.start, o.start + o.length);
                let overlaps = if o.length > Duration::zero() {
                    s < day_end && e > day_start
                } else {
                    s >= day_start && s < day_end
                };
                if !overlaps {
                    continue;
                }
                When::Timed {
                    start: s.naive_local(),
                    end: e.naive_local(),
                    from_yesterday: s < day_start,
                }
            };
            out.push(Entry {
                when,
                title: title.clone(),
                private,
                uid: uid.clone(),
            });
        }
    }
    sort(&mut out);
    Ok((out, zones.guessed))
}

/// All-day first, then by start time, then by title.
fn sort(entries: &mut [Entry]) {
    entries.sort_by(|x, y| {
        sort_key(x)
            .cmp(&sort_key(y))
            .then_with(|| x.title.cmp(&y.title))
    });
}

fn sort_key(e: &Entry) -> (u8, Option<NaiveDateTime>) {
    match e.when {
        When::AllDay => (0, None),
        When::Timed { start, .. } => (1, Some(start)),
    }
}

/// The error text with any trace of the address replaced by its mask.
fn scrub(err: &str, spec: &str) -> String {
    let spec = spec.trim();
    let mut out = err.to_owned();
    for s in [spec.to_owned(), spec.replacen("webcal://", "https://", 1)] {
        if !s.is_empty() {
            out = out.replace(&s, &mask(spec));
        }
    }
    out
}

/// Today's events from every configured calendar, merged and in order: all-day first,
/// then by start time.
pub fn agenda(cfg: &DayConfig, date: NaiveDate, tz: Tz, net: &dyn Net) -> Agenda {
    let mut a = Agenda::default();
    for spec in &cfg.calendars {
        match read_calendar(spec, net).and_then(|text| on_date(&text, date, tz)) {
            Ok((entries, guessed)) => {
                a.entries.extend(entries);
                a.guessed_zones |= guessed;
            }
            Err(e) => a
                .errors
                .push(format!("{}: {}", mask(spec), scrub(&e, spec))),
        }
    }
    sort(&mut a.entries);
    let mut seen = HashSet::new();
    a.entries.retain(|e| {
        let (_, start) = sort_key(e);
        e.uid.is_empty() || seen.insert((e.uid.clone(), start))
    });
    a
}
