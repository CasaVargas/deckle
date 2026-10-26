# The edition bundle: the paper as data, for native readers

Status: accepted, 2026-10-23. No issue yet. The owner delegated the open questions; they are ruled in §12.

**License of this document:** this specification, and the JSON Schema that implements it (`docs/formats/edition-bundle.schema.json`), are offered under CC-BY-4.0 and MIT respectively, in addition to the repository's GPL-3.0. Anyone may write a program that reads or writes edition bundles, open or closed, without taking on the GPL. Deckle's own code that writes them stays GPL-3.0.

## 1. Why

Today every reader gets a finished file. The Kindle, Kobo and PocketBook get an EPUB (or KFX, or KEPUB), and a reMarkable gets a PDF. A file is the right thing for e-readers, but it caps what a native app can do. It can't:

- lay the front page out for the screen in hand;
- make a photo move, or let the Pencil fill in the crossword;
- know which story you finished;
- send any of that back.

An **edition bundle** is the same edition as data: the stories, photos, front page plan, weather, puzzle and almanac, in a documented format. The Mac builds it beside the EPUB and puts it in a folder, iCloud Drive for an iPad. A native reader draws the paper from it.

The bundle is the boundary between Deckle, which is free and open (GPL-3.0), and any reader built on top of it, including a paid iPad app in its own repository. A reader shares no code with Deckle. It reads a documented format, the way a reading app reads EPUB.

## 2. Goals

- **Complete:** everything the EPUB edition prints is in the bundle, so a reader that only knows this spec can draw the whole paper.
- **Structured where a reader gains from structure:**
  - the front page plan (lead, tiles);
  - every story's headline, meta line, teaser, source link, word count and photos;
  - the weather (current conditions, the five-day strip, sun and moon);
  - the puzzle (the grid with its answers, the clues, the ladder, yesterday's answers).
- **Renderable without structure.** Every story also carries its body as the same XHTML the EPUB uses, so a reader that doesn't know a section's kind still draws it.
- **Self-contained and offline.** Photos and icons are inside the bundle, and a reader needs no network.
- **Safe to sync.** A bundle that iCloud Drive has synced only partly is never mistaken for a whole one (§5).
- **Versioned and forward-compatible.** Readers ignore fields they don't know, and a breaking change raises the major version (§10).
- **Two-way.** A reader may write back what the owner read, solved and marked (§8). Deckle reads that into the readership report, the puzzle check, the highlights export and the Morgue, as it does a Kindle's.

## 3. Non-goals

- **Not a replacement for the EPUB.** E-readers keep getting files; the bundle is an extra output.
- **No Deckle server or account.** Bundles move through the owner's own folders (iCloud Drive, Dropbox, a USB stick).
- **Not a book format.** Books (long reads, Inbox books) stay EPUB. A bundle may point to a long read's EPUB (§6.3), but it doesn't describe books.
- **No reader logic.** How a reader lays out a page, animates a photo or checks a crossword is the reader's business. The spec only says what the data means.

## 4. Where bundles come from

An export folder gets a third format:

```toml
[[export]]
name = "iPad"
dir = "~/Library/Mobile Documents/com~apple~CloudDocs/Deckle Daily"
format = "bundle"                # epub | pdf | bundle
keep = 14
```

- The bundle is written after the build, as the EPUB and PDF exports are. `keep` prunes old bundles, counting only Deckle's own names (`deckle-daily-YYYY-MM-DD.deckle`).
- Photos are always kept in color in a bundle. Graying them is the reader's choice, so `photos` doesn't apply to `format = "bundle"`.
- `digest build --bundle <dir>` writes one by hand, for testing a reader.

## 5. On disk

A bundle is a **folder** named `deckle-daily-YYYY-MM-DD.deckle`:

```text
deckle-daily-2026-10-23.deckle/
  edition.json          the edition (§6); written last
  cover.jpg             the generated front page, as the EPUB's cover
  media/
    3f9a1c0e5b7d.jpg    photos, named by the first 12 hex digits of their SHA-256
    9b1e44a0c2f3.png    icons (the weather's), likewise
  epub/
    deckle-daily-2026-10-23.epub        the same edition as an EPUB, for "open in Books"
    deckle-reads-2026-10-23-….epub      long reads, when the export has `books = true`
```

- **Why a folder, not a zip:** iCloud Drive syncs a folder file by file, so tomorrow's bundle reuses nothing but uploads only what it has. It can also hand a reader the photos one at a time. On iPadOS and macOS a reader declares `.deckle` as a package type, so the Files app shows it as one item.
- **Written safely:** everything is written into `.deckle-daily-YYYY-MM-DD.deckle.tmp/` and the folder renamed into place at the end.
  - `edition.json` is the last file in it, and it lists every other file with its size and SHA-256 (§6.4).
  - A reader treats a bundle as ready only when `edition.json` parses and every file it lists is present at the listed size. Until then the bundle is "arriving".
  - That covers iCloud Drive, which may deliver the files in any order and the folder's rename before its contents.
- **Media names are content hashes**, so a photo that appears in two stories, or in two days' bundles, is stored once per bundle and is identical across bundles.

## 6. `edition.json`

UTF-8 JSON, no comments. Every field not marked optional is present. Times are RFC 3339; dates are `YYYY-MM-DD`.

### 6.1 Top level

```json
{
  "format": "deckle-edition",
  "version": "1.0",
  "generator": "Deckle 0.4.0",
  "edition": { … },
  "front": { … },
  "sections": [ … ],
  "files": [ … ]
}
```

- `format` is always `"deckle-edition"`.
- `version` is `"<major>.<minor>"` (§10).
- `generator` is informational.

### 6.2 `edition`

```json
{
  "paper": "The Deckle Daily",
  "date": "2026-10-23",
  "number": 296,
  "place": "New York, NY",
  "timezone": "America/New_York",
  "built_at": "2026-10-23T09:31:12Z",
  "weather_line": "Clear · 67°/44°F · 0% rain",
  "cover": "cover.jpg"
}
```

`weather_line` is optional. `timezone` is the paper's own clock, the one the edition date and Your Day are in.

### 6.3 `sections`

Each section, in the paper's order:

```json
{
  "key": "feeds:Hacker News",
  "kind": "feeds",
  "title": "Hacker News",
  "kicker": null,
  "unavailable": null,
  "stories": [ … ],
  "weather": null,
  "puzzle": null
}
```

- **`key`** is the section's identity across editions (`config::section_key`): the key the readership report and the page plan use.
- **`kind`** is one of `reading`, `weather`, `local`, `feeds`, `shelf`, `lottery`, `horoscope`, `puzzle`, `onthisday`. Another place's local news is a `local` section with its own key (`local:<name>`). A reader that doesn't know a kind draws the section from its stories' `body`.
- **`kicker`** and **`unavailable`** are optional strings. `unavailable` says why the section is empty today.
- **`weather`** is set only on the `weather` section, and **`puzzle`** only on the `puzzle` section (§6.5, §6.6).

Each story:

```json
{
  "id": "2026-10-23/feeds:Hacker News/3",
  "title": "SimCity, but with Permits",
  "meta": "Oct 20, 6:29 PM",
  "teaser": "A city builder where …",
  "url": "https://permit.petercai.com/",
  "outlet": "permit.petercai.com",
  "words": 1840,
  "body": { "format": "xhtml", "text": "<p>…</p><div class=\"figure lead\"><img src=\"media/3f9a1c0e5b7d.jpg\" alt=\"\"/></div>…" },
  "photos": [
    { "file": "media/3f9a1c0e5b7d.jpg", "width": 2400, "height": 1536, "lead": true, "motion": null,
      "caption": "Stall holders at the freight shed on Saturday.", "credit": "Jane Doe / The Pilot",
      "alt": "A crowd under a steel roof", "focus": { "x": 0.62, "y": 0.41 }, "color": "#8a6f52" }
  ],
  "long_read": null,
  "byline": "By Ana Ruiz",
  "published": "2026-10-22T18:29:00+00:00",
  "minutes": 8,
  "prominence": "front",
  "partial": false,
  "pull_quote": "“We have waited forty years for somebody to do something with that yard,” said one stall holder."
}
```

- **`id`:** `<date>/<section key>/<index from 1>`. It's stable for the edition and is how §8 refers to a story.
- **`meta`, `teaser`, `url` and `outlet`** are optional. `url` is the story's own page, when it has one.
- **`words`** is the body's word count, for reading time.
- **`body.text`:** an XHTML fragment, the same one the EPUB prints, with each photo's `src` the bundle-relative path of its file.
  - Elements and classes are those of the EPUB edition (`render.rs`).
  - A reader may style them as it likes, and must not run anything in them. There are no scripts.
- **`photos`:** in body order. `lead` marks the story's lead photo. Photos are color JPEGs. In 1.0 they were at most 1000 px wide. From 1.1 they're up to 2400 px on the long side, never enlarged, at JPEG quality 85.
- **`photos[].motion`:** always `null`. A reader makes moving pictures from the still (§7).
- **Added in 1.1** (Books and Kindle design §9). Each is optional, and `null` when it isn't known:
  - `photos[].caption`, `credit` and `alt`, from the story's page;
  - `photos[].focus`, where the subject is (0–1 from the top left), for crops and parallax;
  - `photos[].color`, the photo's average color;
  - `byline`, `published` (as the page or feed gives it, usually RFC 3339) and `minutes` (230 words a minute);
  - `prominence`: `lead`, `front` or `inside`, as Deckle's front page placed the story;
  - `partial`: the text looks cut off by a paywall;
  - `pull_quote`: a sentence of 60–200 characters, on stories of 600 words or more.
- **Added in 1.2:** `access` on every story: `open`, `subscription` (fetched through the owner's own subscription) or `walled` (withheld by the site). Section `kind` gains `subscribed`. Both are additive.
- **`long_read`:** for a piece printed as its own book, the path of its EPUB under `epub/`, when the export carries books. Otherwise `null`.

### 6.4 `front` and `files`

```json
"front": {
  "lead": "2026-10-23/local/1",
  "tiles": ["2026-10-23/feeds:Hacker News/1", "2026-10-23/local/2"]
},
"files": [
  { "path": "cover.jpg", "size": 182311, "sha256": "…" },
  { "path": "media/3f9a1c0e5b7d.jpg", "size": 90412, "sha256": "…" }
]
```

`front` is the front page Deckle chose (`front::front_plan`). A reader may follow it or make its own. `files` lists every file in the bundle but `edition.json` itself.

### 6.5 `weather`

```json
{
  "units": "imperial",
  "current": { "temperature": 67, "feels_like": 57, "humidity": 26, "wind": 14, "condition": "Clear", "icon": "media/….png" },
  "days": [
    { "date": "2026-10-23", "high": 67, "low": 44, "rain": 0, "condition": "Clear", "icon": "media/….png" }
  ],
  "sun": { "rise": "2026-10-23T07:16:00-04:00", "set": "2026-10-23T18:08:00-04:00" },
  "moon": { "phase": "first quarter", "illumination": 0.68, "next_full": "2026-10-26" },
  "source": "Open-Meteo"
}
```

- **Units:** temperatures are in the paper's units, and wind is mph or km/h with them.
- **`rain`:** the day's highest chance of rain, 0–100.
- **`condition`:** Deckle's own words for the weather code. `icon` is optional.
- **Optional, when the paper prints them:** the almanac's other lines (frost dates, meteor showers, Your Day). They are in the section's stories either way.

### 6.6 `puzzle`

```json
{
  "number": 12,
  "grid": ["#BAN#", "ERROR", "EERIE", "LEAST", "#DYE#"],
  "numbers": [[0,1,1],[0,2,2],[0,3,3],[1,0,4],[1,4,5],[2,0,6],[3,0,7],[4,1,8]],
  "clues": [
    { "number": 1, "dir": "across", "clue": "To prohibit", "answer": "BAN", "theme": false }
  ],
  "ladder": { "from": "MOON", "to": "GLOW", "steps": 4, "answer": ["MOON", "…", "GLOW"] },
  "yesterday": { "grid": ["…"], "ladder": ["…"] }
}
```

- **`grid`:** five rows, `#` for a black square, upper-case letters for the answers.
- **`numbers`:** `[row, column, number]` for each numbered square.
- **Answers are in the bundle**, so a reader can check a solve on the device. A reader shouldn't show them before the owner asks.
- **`ladder`** and **`yesterday`** are optional.

## 7. Moving pictures

**As built (1.1):** moving pictures are parallax, which the reader makes at view time from the still and its `focus`. `photos[].motion` stays `null`, and Deckle writes no clips. What follows is kept for a later version.

`photos[].motion` could name a short, silent, looping clip that shows the photo moving:

```json
"motion": { "file": "media/….mp4", "seconds": 3.5, "kind": "footage" }
```

- `kind` is `"footage"`, cut from the story's own video, or `"parallax"`, made from the still by depth estimation.
- **Rule: news photos never get generated motion.** A clip that shows a real event moving in a way it didn't is a fabrication. `"generated"` is a valid `kind` only on decoration: the masthead, section art, the horoscope. Never on a story photo.
- Readers of 1.0 ignore the field, so adding it later is a minor version (§10).

## 8. What a reader sends back

A reader may write what the owner did with an edition into the same folder, under `.deckle-returns/<reader>/`, one file per edition and reader:

```text
Deckle Daily/
  deckle-daily-2026-10-23.deckle/
  .deckle-returns/
    ipad-7F3A…/
      2026-10-23.json
```

```json
{
  "format": "deckle-returns",
  "version": "1.0",
  "reader": { "id": "ipad-7F3A…", "name": "Jon's iPad", "app": "…" },
  "edition": "2026-10-23",
  "updated_at": "2026-10-23T08:02:11Z",
  "read": [
    { "story": "2026-10-23/local/1", "fraction": 1.0, "seconds": 140, "last": "2026-10-23T07:41:00Z" }
  ],
  "puzzle": {
    "grid": ["#BAN#", "ERR R", "     ", "     ", "#    "],
    "done": false,
    "ladder": ["MOON", "MOAN", "", "", "GLOW"]
  },
  "marks": [
    { "story": "2026-10-23/local/1", "quote": "the mill stood by the river", "note": "Ask Dad", "at": "2026-10-23T07:40:12Z" }
  ]
}
```

- The reader owns its own folder and replaces its file whole (written to a temporary file, then renamed). Deckle only ever reads it.
- **What Deckle does with it:**
  - **`read`** feeds the readership report, by section key, alongside a Kindle's.
  - **`puzzle`** feeds tomorrow's "yesterday's answers": "you solved 9 of 10".
  - **`marks`** go to the highlights export and to the Morgue's highlight counts, like a Kindle's clippings.
- Deckle reads returns at the next build and on `digest push`. A return for an edition the Morgue doesn't hold is ignored.

## 9. Changes to the build

- **`Article` gains `url: Option<String>` and `outlet: Option<String>`.** Both are known where stories are fetched, but today only `meta` and the body's "Source:" line carry them. The EPUB doesn't change.
- **The weather and puzzle sections keep their structured inputs beside the XHTML** they render (`Section.weather`, `Section.puzzle`), so the bundle writer doesn't parse HTML back.
- **`deckle-digest::bundle`** writes a bundle from `Gathered` and the build's files. `export` gets `format = "bundle"`.
- **`deckle-morgue`** keeps no bundles. A bundle in an export folder is the owner's, pruned by `keep`.
- **Returns** are read by a new `deckle-digest::returns` module and merged into the readership state, the puzzle store and the highlights export (phase 2, §11).

## 10. Versioning

- `version` is `"<major>.<minor>"`.
- **Minor versions only add.** New optional fields and new `kind`s. A reader of 1.0 reads 1.x by ignoring what it doesn't know.
- **A major version** is a field removed or its meaning changed. A reader refuses a major version it doesn't know and says "update the app".
- **Deckle writes one version at a time.** If a major version is ever needed, `format = "bundle"` keeps writing the old major until a `bundle_version` key asks for the new one.
- **The JSON Schema** (`docs/formats/edition-bundle.schema.json`, MIT) is the reference for each version, and the writer's tests validate against it.

## 11. Phasing

1. **The bundle (M), built** (§14):
   - `Article.url`/`outlet`;
   - structured weather and puzzle on their sections;
   - the writer, `format = "bundle"` and `digest build --bundle`;
   - the schema and the tests.
2. **Returns (M):** reading `.deckle-returns/` into readership, the puzzle and highlights.
3. **Motion (version 1.1, M–L):**
   - footage loops cut from a story's own video (needs the video found when the story is fetched, and a decision on the sources that allow it, §12);
   - parallax clips made on the Mac, or left to the reader.

The readers themselves are not this repository's work.

## 12. Rulings (the owner's open questions)

The owner delegated these; each is ruled as below and can be reopened on the PR.

- **Long reads go inside the bundle**, under `epub/`, when the export has `books = true`. Tapping a long read in a reader should open it, and a bundle has to stay self-contained offline (§2).
- **Puzzle answers stay in the clear.** It's a 5×5 mini, the reader is the owner's own app, and hashed cells would cost every reader complexity for no real secrecy. Readers keep answers hidden until asked.
- **Motion (version 1.1) starts with parallax only.** Cutting loops from publishers' videos raises a reuse question per source, so `kind: "footage"` stays in the format but Deckle won't produce it until a list of sources that allow reuse is agreed in its own spec. `"parallax"` is made from photos already in the paper, and `"generated"` stays decoration-only (§7).
- **Returns from several readers are merged:**
  - **read:** the furthest `fraction` and the summed `seconds` per story;
  - **puzzle:** per cell, the most recent reader's letter, and `done` if any reader is done;
  - **marks:** the union, with the same story and quote counted once.
- **The license grant stands:** the spec under CC-BY-4.0 and the schema under MIT, beside the GPL.
  - The owner's own app doesn't need the grant: as copyright holder they can use their format any way they like. The grant is for other people's readers.
  - It costs little, since reading a documented file format was never something the GPL could stop.
  - Once published, a CC-BY or MIT grant can't be withdrawn for copies already out. That is fine for a file format, whose value is that others can rely on it.

## 13. Tests the build must add

- **A fixture edition's bundle:**
  - validates against the schema;
  - holds every section, story and photo of the EPUB built from the same `Gathered`;
  - lists every file with the right size and hash;
  - names photos by content, with a photo shared by two stories stored once.
- **Weather and puzzle:** the structured data matches what the XHTML prints (the five days, the grid's letters, the clue texts).
- **Safety:** a bundle still being written (the `.tmp` folder, or `edition.json` missing or listing an absent file) is never pruned as an edition and never counts as ready.
- **Exports:** `format = "bundle"` writes and prunes only `deckle-daily-*.deckle` folders.
- **Returns (phase 2):**
  - a return file merges into readership by section key and into the puzzle's solve;
  - two readers' returns merge as §12 says;
  - a malformed return is ignored with a note, never a failed build.

## 14. Build notes, phase 1

- **Where it's written:** the build writes one bundle per `books` value its exports ask for, under `bundle/paper/` or `bundle/books/` beside the EPUB (`BuildReport.bundles`). An export copies it in whole through a hidden `.….deckle.tmp` folder and renames it into place.
- **`Article.url` and `Article.outlet`** are set where stories are fetched:
  - a feed item: its link and source;
  - a full story: the resolved page and its outlet;
  - a saved article: the saved address;
  - a web article sent as a book: its address, never a saved page's `file:` path.
  
  Sections Deckle writes itself (horoscope, lottery, On This Day, the puzzle and weather pages) have none.
- **Structured data rides on the section.** `Section.weather` is filled by the weather page as it lays out the strip, and `Section.puzzle` by the puzzle page, with today's puzzle and yesterday's. The bundle writer never parses HTML back.
- **Color:** any `format = "bundle"` export makes the build keep photos in color (`Config::wants_color`), so the bundle's photos are always color. `digest build --bundle` sets `force_color` for the same reason.
- **Icons:** the weather icons' EPUB paths (`images/wx-*.png`) are rewritten to their `media/` names in each story's XHTML and in the `weather` data.
- **Clue numbers** are worked out from the grid by crossword rules: a square starts a word across or down.
- **Story words** are counted from the XHTML's text, outside its tags.
- **Moon:** `illumination` is the fraction lit, 0–1. The almanac's own line prints the percent.
- **The schema** (`docs/formats/edition-bundle.schema.json`, JSON Schema 2020-12) is validated in the tests with `jsonschema` 0.33, a dev-dependency with default features off, so nothing is fetched.
- **The app:** export folders gain **As: Bundle, for a reader app**. The photos setting is ignored for a bundle.
- **Kept for phase 2:** reading `.deckle-returns/`.

## Optional AI editor extension (#53)

A section may include `ai_brief`, an array of one to three plain-text sentences generated
by the opt-in Daily editor. It is omitted when absent. Native readers should label it
"AI-written section brief" and render it above the section's stories. EPUB and PDF do the
same. Horoscope sections use their existing kicker to identify AI-written entertainment.
Deduplicated stories are already removed before the bundle is assembled. Default editions
remain unchanged; the schema accepts this optional addition to version 1.1.
