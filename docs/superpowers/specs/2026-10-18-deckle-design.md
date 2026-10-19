# Deckle — design

**Status:** approved by owner's "just do it" (2026-10-18), builder-checked · **Date:** 2026-10-18 · **Owner:** Jonathan (CasaVargas LLC)

Deckle is an open-source desktop app for Mac and Windows that turns any EPUB you own into a
Kindle book that reads like one bought from the Kindle Store, and puts it on your Kindle. One
window, no library, no account, nothing leaves the machine unless you turn on an LLM or the
Amazon edition lookup.

## 1. Intent and why

Sideloaded EPUBs on a Kindle are second-class: no Enhanced Typesetting, grey or missing covers
on new devices, no X-Ray, no Word Wise, no real page numbers. Every piece of the fix exists
somewhere as a Calibre plugin, forum recipe, or Python script, each with its own install story,
and they fight each other (the one that gives you X-Ray takes away your cover on a 2024 Scribe).
Deckle is the one tool that does all of it, decides the conflicts well by default, and asks
nothing of the user beyond "drop books, plug in Kindle, press go".

The owner is a reader, not an author. The audience is readers who sideload. It is open source
so the research that went into it stops being re-derived on MobileRead every year.

## 2. Goals, non-goals, anti-goals

**Goals (v1)**

1. Any valid EPUB in → KFX with Enhanced Typesetting out, sideloaded to the device, with the
   cover showing in the library view. Always. On every supported Kindle.
2. X-Ray and Word Wise on every book where the device and cover rule allow it (§5).
3. Correct metadata on device: title, author (sort-aware), series name and index, language.
4. Real page numbers where the source has them, synthesized ones otherwise (to verify, §9).
5. Zero-config happy path: install, (install Kindle Previewer when prompted), drop, go.
6. The app runs fully offline except for three opt-in network features: LLM calls, edition
   lookup (Open Library), and Wikipedia summaries. All default off for new installs; the
   owner's build turns them on. Note the *device* is not offline: an EBOK book with an ASIN
   makes the Kindle itself query Amazon for that ASIN and may sync position under it. That is
   Amazon's behaviour and is disclosed in the device line (§6); it is not a Deckle network call.
7. A CLI that does everything the GUI does, for power users, scripting, and tests.

**Non-goals (v1)**

- Library management, tagging, reading, metadata editing beyond what the pipeline needs.
- DRM removal of any kind. Input is DRM-free EPUB only.
- Send-to-Kindle email or Amazon cloud delivery. Deckle is USB/MTP only.
- Comics, manga, fixed-layout, PDFs, dictionaries.
- Producing AZW3/MOBI. If KFX conversion fails the book is reported, not downgraded (AZW3
  fallback is a candidate for v1.1).
- Linux as a first-class target. It builds and runs every stage except KFX conversion, and
  says so plainly.

**Anti-goals (would make it a failure even if it "works")**

- Any book ever lands on the device with a grey/generic cover without a visible explanation.
- Silent downgrades: a book quietly loses X-Ray or typography and the user is not told why.
- A Python runtime, Calibre, or any interpreter the user has to install. Kindle Previewer is
  the only external dependency, and only because the KFX writer is Amazon's.
- Subscriptions, accounts, telemetry.
- Shipping Amazon's binaries or data (Previewer, Word Wise databases) inside the app.

## 3. Constraints

- **Stack:** Tauri 2, Rust (stable, 2024 edition), React + TypeScript + Vite frontend. One
  Cargo workspace. Owner's default cross-platform stack; no exceptions.
- **Platforms:** macOS 14+ (Apple Silicon and Intel), Windows 11 x64. Linux best-effort.
- **License:** GPL-3.0-or-later. The KFX container knowledge comes from reading GPL-3 code
  (jhowell's kfxlib via KFX Input, jloutsch's kfxgen) and WordDumb (GPL-3). Picking GPL-3
  removes any derivative-work argument and lets us port their code where it is faster than
  re-deriving it. Wiktionary-derived Word Wise data is CC BY-SA 3.0 and is attributed in-app.
- **Kindle Previewer** is detected, never bundled. Amazon now ships only **Previewer 4**
  (4.0.3 as of 2026-10-18, signed and notarized `KindlePreviewer4.pkg`), so 4.x is the primary
  target and the fixture KPF/KDF set is generated with it. The CLI binary is
  `Kindle Previewer 4.app/Contents/MacOS/KindlePreviewer4CLI` (macOS) and uses double-dash
  flags: `<input> --convert --locale en --output <dir>`, writing `KPF/<name>.kpf`,
  `Logs/<name>_log.csv` and `Summary_Log.csv` (column "Enhanced Typesetting Status" =
  Supported/Not Supported). A leftover 3.x install is accepted if found (3.38+), with its
  single-dash flags. Known 4.x issue: SVG images can fail; Normalize rasterizes SVG as a
  mitigation. The app links the user to Amazon's download when Previewer is missing. The
  pkg payload can be installed per-user without admin by expanding it into `~/Applications`.
- **Target devices:** Kindle Paperwhite (owner's, exact generation to be read off the device
  at first connect) and Kindle Scribe 2024 (MTP-only, no writable `system/` folder). Firmware
  5.16+ assumed; the app records firmware per device and warns outside tested ranges.
- **Reference tooling to study, not ship:** Calibre KFX Output (closed, Previewer-driven),
  KFX Input/kfxlib (GPL-3; contains the complete KPF→monolithic-KFX conversion the Output
  plugin wraps, including approximate page numbers; our oracle), kfxgen (GPL-3, pure-Python KFX writer),
  WordDumb (GPL-3, X-Ray/Word Wise sidecar generator), kindle-cli (MIT, Go, PDOC/cover
  behaviour), KCC (cover thumbnail handling).

## 4. The pipeline

Every book runs the same seven stages. Each stage is a pure Rust function in `deckle-core`,
takes a work directory, and writes a JSON report the GUI renders. Stages are independently
testable from fixtures.

```
EPUB ──► Inspect ──► Normalize ──► Match ──► Convert ──► Enrich ──► Deliver ──► Verify
```

1. **Inspect.** Parse the EPUB (container, OPF, nav/NCX, spine, CSS, fonts, images, page-list).
   Extract metadata (title, creators with roles and file-as, language, identifiers incl. ISBN,
   series via `belongs-to-collection` or Calibre `calibre:series` meta, description, cover).
   Validate hard. Reject DRM, non-EPUB, or broken containers with a specific message.
2. **Normalize.** Produce a clean EPUB 3 that Previewer converts well: single cover image with
   `properties="cover-image"`, nav document with a real TOC (promote headings when the TOC is
   junk), landmarks, `page-list` (preserve, or synthesize at a fixed character stride when
   absent, see §9), strip scripts and remote resources, fix encoding and XHTML well-formedness,
   consolidate CSS, keep embedded fonts only when they are real text fonts (drop decorative
   ones that break ET), normalize series metadata, generate a deterministic book UUID.
3. **Match (opt-in network).** Obtain a candidate real ASIN without scraping Amazon: Amazon's
   PA-API needs an Associates account and is not assumed; scraping amazon.com is excluded from
   v1 (ToS-grey, brittle). v1 Match = ISBN-10 from the EPUB, or found via Open Library by
   title+author, used as the paperback ASIN (Amazon reuses ISBN-10 as the print ASIN). Whether
   the Scribe fetches a cover for a print-edition ASIN is exactly what spike 2 tests. Store the
   candidate and its confidence. Used only by the cover rule (§5). Default off. If spike 2
   fails for print ASINs, Match is removed from v1 and MTP devices get PDOC always.
4. **Convert.** Run the Kindle Previewer CLI (`KindlePreviewer4CLI <epub> --convert --locale en
   --output <dir>`, ~40 s for a novel on an M-series Mac), parse `Summary_Log.csv`, confirm
   "Enhanced Typesetting Status" is Supported and the output is KPF (MOBI means ET was refused
   and the per-book log CSV says why). The KPF is a zip: `book.kdf` (SQLite of Ion fragments),
   `book.kcb` (JSON), `resources/res/*` (images), `book.epub` (Previewer's cleaned source). Then **repackage** KPF → monolithic, unencrypted KFX: read the KDF
   (SQLite of Ion fragments inside the KPF), rewrite fragments into a KFX container with the
   correct symbol table, metadata (`cdeType`, ASIN, cover, title, author, language, book id),
   and entity table. This step is a Rust port of what kfxlib/KFX Output do. Riskiest stage;
   spike first (§10).
5. **Enrich.** Generate sidecars into `<book>.sdr/`:
   - **X-Ray** (`*.asc`, SQLite, Amazon's X-Ray schema): entities (people, terms), excerpts,
     occurrences keyed by KFX position, descriptions. Entity selection and description writing
     use the configured LLM when one is set; otherwise a heuristic extractor (capitalized
     n-gram frequency + stop lists) plus Wikipedia summaries when that opt-in is on, plus the
     book's own first-mention sentence as the final fallback. Wikipedia is the third opt-in
     network feature and defaults **off** like the other two; "X-Ray: basic" (excerpts only,
     which is also what Amazon shows for most terms) is the accepted no-network result. Position mapping comes from the
     KFX content we just wrote, so Enrich depends on Convert.
   - **Word Wise** (`*.kll`, SQLite): per-occurrence glosses for hard words at the user's chosen
     level. Data comes from Deckle's own Wiktionary-derived lemma/gloss/difficulty database,
     built offline by a data pipeline in `data/` and downloaded on first use (not bundled, to
     keep the installer small). No LLM.
   - **Thumbnail** for devices with a writable `system/thumbnails` (Paperwhite): render the
     Kindle-format thumbnail PNG so EBOK books show covers without an Amazon lookup.
6. **Deliver.** Detect connected Kindles, pick the transport: USB mass storage (plain file
   copy) or MTP (Windows: WPD via `windows-rs`; macOS/Linux: a minimal PTP/MTP client over
   libusb via `rusb`, the operation subset Kindles use). Copy the KFX, the `.sdr` folder, and
   the thumbnail if applicable, in the order the device needs (book first, sidecars after,
   thumbnail last). Export-to-folder is always available as a transport for manual copying.
7. **Verify.** Re-list the device, confirm sizes, and record what landed. The GUI shows a
   per-book result line: what features it has, and if anything was withheld, the one-line why.

## 5. The cover rule

Covers are non-negotiable (owner decision, 2026-10-18). X-Ray and Word Wise require the book
be marked **EBOK** with an ASIN; on MTP-only devices EBOK with an unknown ASIN produces a grey
cover and we cannot write the thumbnail ourselves. So, per device profile:

| Device class | Thumbnail folder writable | Decision |
|---|---|---|
| Mass-storage Kindles (Paperwhite) | yes | **EBOK + generated ASIN + Deckle-written thumbnail.** Everything, always. |
| MTP-only Kindles (Scribe 2024, Colorsoft) | no | If Match found a real ASIN **and** the real-ASIN spike (§10) passed: **EBOK + real ASIN**, device fetches the store cover, everything on. Otherwise **PDOC + embedded cover**, typography and page numbers only, X-Ray/Word Wise withheld with the reason shown. |

Per-book override is one click: "Prefer features (risk grey cover)" or "Prefer cover". A
global "Always PDOC (no Amazon traffic from the device)" switch exists for people who object
to the EBOK side effect in goal 6. The default is never changed silently. Generated ASINs use the `B0` + 8 alphanumerics shape and a
`DECK` marker so they are recognizable and never collide with a real one by accident.

## 6. Devices

- Deckle keeps a `devices.json` of every Kindle it has seen: serial, model (from the device
  info / `system/version.txt` when reachable), firmware, transport, thumbnail-writable flag,
  and the user's overrides. First sight of a device shows a one-line "Found Kindle Scribe
  (MTP). Covers via store match; features when matched." so the rules are visible.
- Two devices connected at once is supported; the user picks a target per batch.
- Firmware outside the tested set produces a warning, not a refusal.

## 7. LLM policy

- Provider is a setting: **Off** (default), **Anthropic API**, **OpenAI-compatible endpoint**
  (covers Ollama, LM Studio, vLLM), with model name and key. Owner's build uses Anthropic.
- LLM is used only in Enrich/X-Ray: pick notable entities from the book text, write 1–2
  sentence spoiler-free descriptions, disambiguate aliases ("Mr. Darcy" = "Darcy" =
  "Fitzwilliam Darcy"). Chunked, with a per-book token budget and a cost estimate shown before
  the first run on a given provider.
- Nothing else calls the LLM. Word Wise, typography, covers, and matching never do.
- Without an LLM the feature still works via the heuristic + Wikipedia path; the result line
  says "X-Ray: basic".

## 8. GUI

One window, three regions.

- **Top: drop zone / book list.** Drop EPUBs (or a folder) and each becomes a row: cover
  thumbnail, title, author, series, and a mode badge (**Full** / **Cover-only** / **Error**)
  with the reason on hover. Rows stay simple; a row menu holds the override and "Show report".
- **Bottom bar: device + action.** Connected Kindle(s) with model and transport, a target
  picker when there is more than one, and a single primary button: **Send to Kindle**.
  "Export to folder" lives in the button's secondary menu.
- **Settings drawer.** Kindle Previewer path (auto-detected, with install link), LLM provider,
  Word Wise level (1–5), Wikipedia lookups on/off, Amazon edition matching on/off,
  page-number synthesis on/off. That is the whole settings surface for v1.

Progress is per row (stage name + bar). Failures are per row with the stage and the first
useful line of Previewer's log. No modal dialogs on the happy path.

## 9. Open technical questions the builder owns

Settle these during the build; none change what gets built, only how.

- **Page numbers:** confirm Previewer carries an EPUB `page-list` nav into KPF page labels and
  the Kindle shows them for sideloaded KFX. If not, document that page numbers are
  store-only for KFX and drop goal 4 to "preserve when present".
- **KFX repackaging scope:** minimal viable container that the device accepts (which fragment
  types, which metadata, whether `$ion_symbol_table` must be rebuilt or copied). Decide by
  round-tripping an existing store KFX through kfxlib-equivalent parsing in tests.
- **Entity-to-position mapping** for X-Ray: positions are KFX content offsets; derive them from
  the KFX we wrote rather than from the EPUB text, and prove alignment on a fixture.
- **MTP subset:** which PTP operations the Scribe needs (GetDeviceInfo, OpenSession,
  GetStorageIDs, GetObjectHandles, GetObjectInfo, SendObjectInfo, SendObject, DeleteObject,
  CloseSession) and whether folder creation for `.sdr` works over SendObjectInfo with
  association type. Windows uses WPD and should be the easier side.
- **Word Wise data pipeline:** Wiktionary (kaikki.org JSON) → lemmas, short glosses, POS,
  difficulty score from a word-frequency list. Output a versioned SQLite the app downloads from
  the project's GitHub Releases. Decide the compression and the size budget (target <60 MB).
- **Heuristic X-Ray quality bar:** define a fixture book and a minimum precision for entity
  extraction so "basic" is still useful.

## 10. Risks and the spikes that retire them

Run in this order; each is a throwaway unless it passes, in which case it becomes the first
real implementation.

1. **KPF → KFX repackaging in Rust.** Convert one Gutenberg EPUB with Previewer (done:
   `pride-and-prejudice.epub` → 47 MB KPF, ET Supported, 38 s), repackage, sideload, confirm
   Enhanced Typesetting is active (Aa menu shows it) and the book opens and paginates.
   **Time box: three working sessions.** Kill criterion: no device-accepted KFX by the end of
   the third. Update 2026-10-18: the whole KPF→KFX path (KDF reader, fix-ups, container
   writer) exists in GPL-3 Python as kfxlib inside the KFX Input plugin, and it produced a
   KFX from Previewer 4 output in one second; those files are the goldens in `fixtures/`, so
   the Rust port has a byte-exact oracle. Pivot if the port still misses the box: ship an
   **embedded Python runtime (python-build-standalone) + kfxlib** behind the same Rust
   interface. That keeps Enhanced Typesetting and the no-user-installed-interpreter rule;
   an AZW3 fallback is no longer the pivot.
2. **Real ASIN on the Scribe.** Sideload an EBOK KFX stamped with a real ASIN plus our X-Ray
   sidecar. Confirm: cover appears, X-Ray opens and shows *our* entities, nothing gets replaced
   or deleted by the device after a sync. Decides whether the "matched" branch of §5 exists.
3. **Scribe MTP write from macOS.** Minimal rusb PTP client creating `.sdr` and writing files.
   Decides the transport design and whether a libmtp dependency is needed instead.
4. **X-Ray sidecar acceptance.** Hand-craft an `.asc` from the documented schema for a fixture
   book, confirm the Kindle renders it. Then automate.

Other risks: Previewer version churn (pin recommended version, detect, warn); Amazon firmware
changing sidecar handling (version-gate, keep a device test matrix in `docs/devices.md`);
KFX being undocumented (keep container code isolated in one crate with heavy fixtures).

## 11. Repository layout

```
deckle/
├── Cargo.toml                  # workspace
├── crates/
│   ├── deckle-core/            # pipeline stages, device profiles, reports
│   ├── deckle-kfx/             # KFX/Ion container read/write (isolated, fixture-heavy)
│   ├── deckle-sidecar/         # X-Ray (.asc) and Word Wise (.kll) writers
│   ├── deckle-transport/       # mass storage, MTP (rusb), WPD (windows)
│   └── deckle-cli/             # `deckle inspect|normalize|convert|enrich|send|run`
├── apps/desktop/               # Tauri 2 + React + TS + Vite
├── data/wordwise/              # offline build pipeline for the Word Wise database
├── fixtures/                   # Gutenberg EPUBs, sample KFX/KPF, sidecar goldens
├── docs/
│   ├── superpowers/specs/      # this document and its successors
│   ├── devices.md              # tested device/firmware matrix
│   └── formats/                # what we learned about KFX, .asc, .kll, thumbnails
└── .github/workflows/          # core tests on ubuntu/macos/windows; Previewer-gated
                                # conversion tests on the self-hosted Mac runner
```

## 12. Milestones

- **M0 Scaffold.** Workspace, CLI skeleton, Tauri shell that opens, CI green, fixtures in.
- **M1 Typography.** Inspect + Normalize + Convert (Previewer + repackage) + mass-storage
  Deliver. One book on the Paperwhite with Enhanced Typesetting. Spike 1 lives here.
- **M2 Covers and devices.** Device profiles, thumbnails, PDOC/EBOK rule, Match, MTP to the
  Scribe. Spikes 2 and 3. Both owner devices show covers on every book.
- **M3 X-Ray.** Sidecar writer, heuristic extractor, LLM providers, position mapping. Spike 4.
- **M4 Word Wise.** Data pipeline, download-on-first-use, `.kll` writer, level setting.
- **M5 Ship.** GUI polish, installers, README with the research write-up, first tagged
  release. macOS: Developer ID signed and notarized (owner's existing Apple developer account).
  Windows: **unsigned in v1** with a documented SmartScreen note; a code-signing cert is a
  purchase and waits for the owner's go-ahead. **No Tauri auto-updater** in v1 (it phones home);
  the app shows the current version and a link to Releases.

Each milestone ends with a verified book on a real device, not a passing test.

## 13. Open questions for the owner (do not block the build)

1. **Which Paperwhite generation?** If it is the 2024 (12th gen) model it is MTP-only like the
   Scribe, there is no mass-storage device in hand, the thumbnail-writer branch of §5 has no
   test target, and MTP (spike 3) moves into M1. The plan below assumes MTP may be needed in
   M1 and treats mass storage as the easy case either way.
2. **Amazon PA-API credentials?** If the owner has an Associates account, Match can use the
   real API for Kindle-edition ASINs instead of the print-ASIN heuristic.
3. **Windows code signing budget?** Decides whether M5 ships a signed Windows installer.

## 14. Decisions log

- 2026-10-18 Reader-focused, not author-focused. Owner.
- 2026-10-18 Mac + Windows first-class; Tauri/Rust/React. Owner.
- 2026-10-18 Simple GUI, every feature. Owner.
- 2026-10-18 Covers beat X-Ray when they conflict. Owner.
- 2026-10-18 LLM allowed, opt-in, owner's build uses it. Owner.
- 2026-10-18 GPL-3.0-or-later; no Python; Previewer 4 detected not bundled; KFX-only output in
  v1 unless spike 1 misses its time box (then AZW3 fallback is promoted); no Amazon scraping,
  Open Library + print-ASIN heuristic for Match; Wikipedia opt-in default off; Windows unsigned
  in v1; no auto-updater; Word Wise data downloaded not bundled; working name "Deckle".
  Claude's calls.
- 2026-10-19 Word Wise points at the Kindle's own English dictionary when present (read at send
  time, never stored), else installs Deckle's Wiktionary dictionary in the Chinese slot (reader
  sets Word Wise language to 中文 once); Amazon's dictionaries are never overwritten. Database
  12 MB gzipped, on GitHub Releases, sha256-pinned. Claude's call (M4 plan).
