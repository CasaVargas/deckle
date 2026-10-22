# Weather, almanac and your day — design

**Status:** direction approved by owner (2026-10-22, "Deckle Daily ideas" canvas, board 7,
"Weather, almanac and your day"); document pending review · **Date:** 2026-10-22 ·
**Owner:** Jonathan (CasaVargas LLC) · **Builds on:** `2026-10-20-daily-digest-design.md`
(the Weather section, `moon.rs`) and `2026-10-22-readership-report-design.md` (section keys).

The paper's Weather section becomes a full page, **Weather & Almanac**. It has a five-day strip,
an almanac box (sun, daylight, frost, the moon and its name), a short "tonight's sky" note, and
**Your Day**: today's events from a calendar you choose, with a warning when rain lands on one.

## 1. Intent and why

Today the Weather section is a forecast table and one moon line. It reads like an API dump. A
real paper's weather page is a small pleasure: the frost date, the Hunter's Moon on Sunday, the
Orionids after midnight. Almost all of that can be worked out on the Mac with no service and no
key, which is Deckle's whole stance. Your Day turns the paper from "the world this morning"
into "*your* morning". It's the one thing a news app can't print, and it fits the page next to
the weather it depends on.

**Success:** on 2026-10-22 in Linden Falls the page shows the following:

- Five days Thursday to Monday with highs, lows and short words ("Showers, pm", "Frost at dawn").
- Sunrise 7:28 AM and sunset 6:31 PM, with daylight of 11 h 03 m, 2 min less than yesterday.
- "First frost usually Oct 29 here".
- "Waxing gibbous, 78% lit · Full moon Sunday, the Hunter's Moon".
- A paragraph on the Orionids.
- Four events from the calendar, then "Rain likely around 4 PM, during School pickup."

Everything except the forecast and the one-time frost lookup is computed locally.

## 2. Goals, non-goals, anti-goals

**Goals**

1. **One page, one section.** The Weather section is restyled in place. Its config name stays
   `weather` and its readership key stays `weather` (readership design §4), so reading history
   carries over. The title becomes **Weather & Almanac**.
2. **Five-day strip:** today and the next four days. Each day shows its name, an icon, the high
   and low, and one or two words (§4.1).
3. **Right now:** the current temperature, sky, feels-like, humidity and wind, as today.
4. **The almanac box:** sunrise, sunset, daylight with the change from yesterday, the frost line
   (§4.3), the moon phase and % lit, the next full or new moon with its day and traditional
   name, and a *Coming up* line for an equinox, solstice or clock change within 7 days.
5. **Tonight's sky:** a meteor shower paragraph when one is active (§4.5). Omitted otherwise.
6. **Your Day:** today's events from one or more calendars (`.ics` file or `https` address),
   plus at most one weather note tying rain or snow to an event (§5).
7. **Degrade by part, not by page.** If the forecast fails, the almanac, sky and Your Day still
   print with a one-line "Forecast unavailable: …". If the calendar fails, everything else
   prints with "Calendar unavailable: …". The section is only `unavailable` when there's no
   latitude and longitude.
8. **Front page and cover:** the weather line is unchanged.
9. **Daily tab:** a **Your Day** card under the location settings: add a calendar file
   (picker) or address, remove it, and a switch for the weather note. CLI parity comes through
   `digest.toml` and `deckle digest build`, like every other section.

**Non-goals (this increment)**

- **Planets, eclipses, ISS passes, aurora.** Each needs an ephemeris or a live feed. Later.
- **Reading the macOS Calendar or Outlook directly** (EventKit, Graph). Files and addresses
  only. Every major calendar can publish a private `.ics` address.
- **Writing to the calendar**, reminders or tasks (`VTODO`), and attendee responses (Deckle
  can't know which attendee is you, so declined events still show).
- **Pollen, air quality, UV, tides.** Possible later from Open-Meteo, but not on this page now.
- **Hourly forecast tables.** Hourly data is fetched only to word the strip and the rain note.

**Anti-goals**

- The page is wrong about the sky. A full moon printed on the wrong day, or sunrise more than
  2 minutes off, is a bug, not an approximation (§6 pins the accuracy).
- Your Day leaks. Calendar contents go into the paper and nowhere else. They are never logged,
  never cached outside the edition, and never sent to any service. **The one exception is email
  delivery**, which sends the paper (and so the agenda) to Amazon. The Your Day card says so in
  one line when email is on.
- A private event's title is printed. `CLASS:PRIVATE` and `CLASS:CONFIDENTIAL` events show as
  "Busy".
- A recurring event (most of a real calendar) is missing, or appears on the wrong day across a
  time-zone or clock change.
- Advice the paper can't back up ("bring a coat"). The rain note states the forecast, nothing
  more.

## 3. Constraints

- Rust, in `crates/deckle-digest`. The section is built in `sources/weather.rs` (split into
  modules as it grows; see §8). The sun, moon and sky are new pure modules with no I/O.
- **Network:** the existing Open-Meteo forecast call gains `hourly=precipitation_probability,
  temperature_2m,weather_code` for the same 5 days. That's still one request. The frost
  climatology is one call to Open-Meteo's historical API, made once per location and cached
  (§4.3). Calendar addresses use the existing `Net` with its timeout and size limits. Nothing
  else touches the network.
- The edition's "today" is the edition date in the Mac's local time zone, as now. Calendar times
  with a `TZID` are converted to that zone, floating times are taken as local, and `DATE`-only
  values are all-day.
- **E-ink:** grayscale, no colour meaning. Every icon has its words beside it, so a missing icon
  loses nothing. Icons must render on the Scribe 2024 and on the Paperwhite (checked on both,
  §6).
- **Units** follow `units`: °F, mph and 12-hour times for imperial; °C, km/h and 24-hour times
  for metric. *(Builder's call is made here: today's code prints 12-hour times for both; metric
  moves to 24-hour.)*
- Readership's heading match (`layout_from_kfx`) keys on the section title the gatherer passes,
  so the new title needs no special handling. A test pins that the key stays `weather`.

## 4. The page

Top to bottom, as the mockup shows: the header, the five-day strip, then two columns where the
reader allows (the almanac box, then Your Day), and a footer credit. On a narrow screen the
columns stack. The footer reads: "Forecast: Open-Meteo · Sky and moon worked out on your Mac".

### 4.1 The five-day strip

- Days come from `daily.time[0..5]`, shown as `THU`, `FRI` and so on, with today first.
- The word starts from `describe(code)` (the existing WMO table), then two refinements:
  - **Timing:** when the day's wet hours (hourly precipitation probability ≥ 50%) all fall
    before noon, add ", am". When they all fall from noon on, add ", pm". So "Showers, pm".
  - **Frost:** when the low is at or below 0 °C (32 °F) and the code is 0–2 (clear to partly
    cloudy), the word is "Frost at dawn".
  - Words stay short enough for a fifth of the page. Builder's check: none longer than "Freezing
    drizzle, pm". Shorten the table if needed.
- Icons: one small grayscale image each for clear, partly cloudy, cloudy, fog, rain, snow and
  storm, mapped from the WMO code. The builder picks how to make them. They are embedded in the
  EPUB like the photos, with `alt=""`.

### 4.2 Sun and daylight

Sunrise, sunset and day length come from a local solar calculation (the NOAA algorithm, with
refraction and the sun's disc, so -0.833° altitude). Results are in local time. The change from
yesterday is shown as "N min more/less", or "about the same" under 1 minute. Polar day and
polar night print "The sun doesn't set today" and "The sun doesn't rise today".
*(Builder's call: local, not Open-Meteo's sunrise. The almanac must work when the forecast
fails.)*

### 4.3 The frost line

- **Source:** Open-Meteo's historical archive, daily `temperature_2m_min`, 1991-01-01 to
  2020-12-31 (a 30-year climate normal), for the configured point. Fetched once per location,
  then cached in `almanac.json` next to `digest.toml`. The cache key is lat/lon rounded to
  0.1°. A failed fetch is retried on the next build, and the line is left out until it works.
- **Computation**, for each year:
  - The **first autumn frost** is the first day after midsummer (Jul 1 north of the equator,
    Jan 1 south) with a low ≤ 0 °C.
  - The **last spring frost** is the last such day before midsummer.
- The median day-of-year of each is the "usual" date. It counts only if frost happened in at
  least 15 of the 30 years. Otherwise the place has no usual frost, and the line never shows.
- **When it shows:**
  - "First frost usually Oct 29 here" from 45 days before that date until it.
  - "Last frost usually Apr 12 here" from 45 days before that date until it.
  - At no other time.

### 4.4 The moon

- The phase name and % lit stay as in `moon.rs`.
- **Next full or new moon:** computed with Meeus' phase algorithm (*Astronomical Algorithms*,
  ch. 49) and converted to local time. The mean-synodic estimate can land on the wrong day.
  It's printed as a day name within 6 days ("Full moon Sunday"), "tonight" or "tomorrow" when
  that applies, and a date beyond 6 days.
- **Traditional names** (northern hemisphere only; omitted south of the equator):

  | Month | Name |
  | --- | --- |
  | Jan | Wolf |
  | Feb | Snow |
  | Mar | Worm |
  | Apr | Pink |
  | May | Flower |
  | Jun | Strawberry |
  | Jul | Buck |
  | Aug | Sturgeon |
  | Sep | Corn |
  | Oct | Hunter's |
  | Nov | Beaver |
  | Dec | Cold |

  Two overrides apply:
  - The full moon nearest the September equinox is the **Harvest Moon**, and the next one is
    the **Hunter's Moon**, whatever their months.
  - A second full moon in one calendar month is a **Blue Moon**.
- The existing `moon::phase` "next" text is replaced by this. `moon.rs` keeps its tests and
  gains the new ones (§6).

### 4.5 Tonight's sky

- A built-in table of the major annual meteor showers covers:
  - Quadrantids, Lyrids, Eta Aquariids, Southern Delta Aquariids, Perseids, Draconids,
    Orionids, Leonids, Geminids and Ursids.
  - For each: the active window, peak date, peak rate per hour, the radiant's compass direction
    and the best hours.
  - Dates are the IMO's typical calendar dates. A shower's peak drifts by a day at most between
    years, so it's fine for this paper.
- **Active today:** one short paragraph. When two are active, the one nearer its peak wins.
  - Before the peak: "peaks Tuesday night".
  - At the peak: "peaks tonight".
  - After the peak: "past their peak but still worth a look".
  - Then where to look and when, and "give your eyes 20 minutes".
- When the moon is more than 60% lit, add "a bright moon will wash out the fainter ones".
- Showers whose radiant never rises at the configured latitude are skipped (the Eta Aquariids
  barely show far north, for example). Use the radiant's declination: skip when `dec <
  lat − 90°` (north) or `dec > lat + 90°` (south).

### 4.6 Coming up

There is at most one line, the nearest event within 7 days:
- an equinox or solstice, with its local date (Meeus ch. 27), as "Winter solstice Monday:
  the shortest day";
- a clock change in the Mac's time zone, as "Clocks go back an hour Sunday at 2 AM", found
  by comparing the zone's UTC offset day by day.

## 5. Your Day

**Config** (`digest.toml`; the starter gets it commented out):

```toml
[day]
calendars = []                 # .ics files or private https addresses; empty = no Your Day
max_items = 8
weather_note = true            # "Rain likely around 4 PM, during School pickup."
```

**Reading:** each calendar is read at build time.
- A path is read from disk.
- An `https` address is fetched through `Net`. The common `webcal://` scheme is rewritten to
  `https://`.
- Any other scheme is refused with a message.
- Calendars are parsed as iCalendar (RFC 5545) `VEVENT`s, including recurrence:
  - `RRULE`, `RDATE` and `EXDATE`.
  - `RECURRENCE-ID` overrides replace the instance they name.
- `STATUS:CANCELLED` events are dropped. A `VTIMEZONE` or IANA `TZID` is honoured.
- Events from all calendars are merged, and exact duplicates (same UID and start) are
  collapsed.

**Which events:** those overlapping the edition date's local midnight-to-midnight. An event that
started yesterday and runs into today shows as "until 10 AM". Multi-day all-day events show on
each day.

**Printed:**
- All-day events first ("All day"), then timed events by start time.
- Each shows its start time and title (`SUMMARY`). Location and description are left out.
- Private and confidential events print as "Busy".
- Up to `max_items`, then "and N more".
- No events at all prints "Nothing on the calendar today."

**The weather note** (when `weather_note` is on and the forecast loaded):
- Take the first timed event whose span includes an hour with precipitation probability ≥ 50%.
- Print one line: "Rain likely around 4 PM, during School pickup." Use "Snow" when that hour's
  code is a snow code.
- No such event means no line. All-day events never trigger it.
- A private event is named as "your 4 PM event", never by its title.

**Daily tab card "Your Day":**
- The list of calendars: a file name, or a shortened address with its secret part masked as
  `…`.
- **Add file…** opens a picker filtered to `.ics`. **Add address** opens a field.
- Each calendar has a remove button.
- A switch: *Mention rain on your events*.
- A preview line: "Today: 4 events" (or the error) from a dry read.
- When email delivery is on: "Emailed papers include your agenda, so it passes through Amazon."

**The address is a secret.** A private calendar address grants read access to the calendar, so
it is never logged. Error messages show only its host. *(Builder's call: it stays in
`digest.toml` like the other settings, not the keychain. The file is the user's own config on
their own Mac, and the email password is in the keychain only because it grants send
access.)*

## 6. Testing

- **Sun:** for New York, Anchorage and Sydney on a solstice, an equinox and a clock-change day,
  sunrise and sunset must fall within 2 minutes of NOAA's published tables. Tromsø in June and
  December must hit the polar cases.
- **Moon:** every full and new moon of 2026 must fall on the correct local date in New York and
  in Sydney (against USNO's published list). Harvest and Hunter's must be correct for 2025, 2026
  and 2027. The Blue Moon of August 2023 must be found.
- **Equinoxes and solstices:** 2024–2030 to the minute, ±5. **Clock change:** America/New_York
  and Europe/London for 2026.
- **Frost:** a synthetic 30-year series yields the expected medians. Under 15 frost years means
  no line. Southern-hemisphere midsummer is honoured. The 45-day window edges are tested.
- **Strip words:** ", am", ", pm" and "Frost at dawn" each come from a fixture forecast. Length
  stays within the cap.
- **Showers:** each window and peak is phrased correctly. Moonlit wording appears above 60%.
  The latitude skip applies.
- **Calendar:** a fixture `.ics` set must handle each of these:
  - a weekly `RRULE` with an `EXDATE` and a moved instance (`RECURRENCE-ID`);
  - a `TZID` event across a US clock change;
  - floating and all-day events, and a multi-day all-day event;
  - an event that crosses midnight;
  - `CANCELLED`, private and confidential events;
  - duplicate UIDs across two files;
  - `webcal://` rewriting, and a refused `file://`.
  The address never appears in error text.
- **Weather note:** it picks the first wet event, says "Snow" when it should, stays silent when
  nothing is wet, and doesn't name private events.
- **Degrading:** forecast down means the almanac and Your Day still print. Calendar down means
  the rest prints. No location means the section is unavailable (as today).
- **Readership:** the key stays `weather` and the heading still matches.
- **Golden page:** one fixed date, place, forecast and calendar render to a checked-in XHTML
  snapshot.
- **On the owner's Kindles:** build an edition with a real calendar address. The page must read
  well on the Scribe 2024 and the Paperwhite, the icons must render, and the columns must stack
  sensibly. Then check the Daily tab card visually in the running app.

## 7. Docs

- `docs/daily.md`: "The weather page" (what's computed where, the frost cache, how to get a
  private `.ics` address from Google, iCloud and Outlook, and the email caveat).
- CHANGELOG.
- The starter `digest.toml`.

## 8. Left to the builder

- Module layout: for example `sources/weather.rs` for the forecast and strip, `sun.rs`,
  `sky.rs` (showers and seasons), and `day.rs` (calendar), keeping `moon.rs`.
- The iCalendar and recurrence crates, or a hand parser. Recurrence must be complete enough
  for §6.
- Icon drawing, CSS for the two-column box, and the exact sky-paragraph wording within §4.5.
- Tauri command names.
