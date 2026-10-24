# The masthead font, configurable — design

**Status:** proposed · **Date:** 2026-10-23 · **Owner:** Jonathan (CasaVargas LLC)
**Asked by:** a Kindle Paperwhite 10th gen user on r/kindle: "I would like to change the font of the header/title."
**Builds on:** `2026-10-20-daily-digest-design.md` (the cover, the PDF edition)

The paper's name ("The Morning Deckle") is drawn in UnifrakturMaguntia, a blackletter face, on
the cover and at the top of the PDF edition. It can't be changed. This makes it a choice.

## 1. Decisions

- **One key, `masthead`**, at the top of `digest.toml`, beside `title`. Four kinds of value:
  - `"blackletter"` (the default, today's look: UnifrakturMaguntia);
  - `"serif"` (Old Standard Bold, the cover's own body face);
  - `"sans"` (Inter Bold, from `deckle-cover`);
  - a path to a **`.ttf` file** you choose (`~` is the home folder). TrueType only: the PDF
    edition embeds TrueType outlines, and one rule for the cover and the PDF is simpler than two.
- **It only changes the masthead**, the paper's name on the cover and on the PDF's first page.
  The date band, the ears and the headlines stay in Old Standard. A long read's "Deckle Reads"
  cover keeps the blackletter: it is Deckle's mark, not the paper's name.
- **Inside the EPUB the title is the reader's font**, because a Kindle sets its own font. This
  setting can't reach it, and the docs say so.
- **A bad choice never fails a paper.** A missing, unreadable or non-TrueType file, or a font that
  lacks a letter of the paper's name, falls back to the blackletter and adds a one-line notice
  to the run ("masthead: … using the blackletter"), like a source that is "unavailable today".
- **Left out of a save while it is the default**, like the other optional keys.
- **In the app:** The paper → *Masthead font*: Blackletter, Serif, Sans, or *Choose a font…*
  (a `.ttf` picker). The chosen file's name shows beside the select.

## 2. Where it lives

- `config.rs`: `Config.masthead: MastheadFont` (serialized as the strings above).
- `masthead.rs` (new): `MastheadFont`, `Masthead` (the resolved face: `&'static [u8]`), and
  `MastheadFont::resolve(title)`. Custom font bytes are read once per path and kept for the life
  of the process (`FontRef<'static>`, as the bundled faces are).
- `Edition.masthead: Masthead`, set in `gather_with`, so the cover, the PDF and the bundle all
  see the same face. Tests build an `Edition` with `Masthead::default()`.
- `cover.rs` and `pdf.rs` read `edition.masthead` where they used the bundled blackletter.

## 3. Tests

- The default config round-trips and is left out of a save.
- `"serif"`, `"sans"` and a real `.ttf` path resolve; a missing file, an `.otf` and a font
  without the title's letters fall back with a notice.
- The cover renders for each face and differs from the blackletter one.
- The PDF embeds the chosen face (its font name is not UnifrakturMaguntia).
- The app's draft round-trips `masthead`.

## 4. Not in this change

- Changing the body, headline or date-band faces.
- A font for the EPUB's own title page (the reader's font wins).
- OpenType/CFF (`.otf`) and font collections (`.ttc`).
