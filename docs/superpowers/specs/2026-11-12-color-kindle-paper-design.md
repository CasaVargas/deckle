# The paper in color on a color Kindle — design

**Status:** for a report that the Daily's photos were gray on a Kindle Scribe Colorsoft, sent as
the Kindle book and as the PDF · **Date:** 2026-11-12 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-11-01-fixed-layout-design.md` (the screens table, #281), the PDF edition
design (the Scribe's PDF)

A Kindle with a color screen (a Colorsoft, a Scribe Colorsoft) gets the Daily with its photos in
color, whether it takes the Kindle book or the Scribe's PDF. Nothing changes for a house with only
gray Kindles.

## 1. What was wrong

The paper's photos were grayed for every Kindle:

- The edition's EPUB, the one that becomes the Kindle book, was always written gray
  (`write_edition`). A color copy was only made for an export folder, a bundle or a color
  reMarkable (`Config::wants_color`), and the photos were fetched gray unless one of those asked.
- The Kindle's PDF was fixed at `(Screen::KindleScribe, Photos::Gray)` (`Config::KINDLE_PDF`).

Deckle already knew a Colorsoft is in color (`deckle_core::screens`, used for comics); the paper
never asked.

## 2. Decision

- **One paper for every Kindle, in color when a color Kindle is known.** Graying is a plain luma
  conversion (`article::gray_jpeg`), which a gray Kindle does on its own when it shows a color
  photo, so a gray Kindle loses nothing but some bytes. Rejected: a color twin of the Kindle book
  and of the PDF, chosen per Kindle at push like the book/PDF twins. It costs a second Previewer
  run each morning and a second set of queue names, for a difference a gray Kindle can't show.
- **"Known" means the device registry** (`devices.json`): a Kindle the app or a push has seen, its
  screen from its serial when the tables know it, else from its name
  (`screens::for_profile`). A Kindle never seen yet gets the gray paper until it's seen once.
- **`[sync] photos`** overrides it: `"color"` or `"gray"` for every Kindle; unset follows the
  registry. For a Colorsoft whose name and serial the tables don't know, or a color Kindle that's
  gone.
- **Every Kindle output follows it:** the edition's EPUB (the Kindle book, the email, the
  Morgue's copy), its long reads, and the Kindle's PDF (`Config::kindle_pdf`). The photos are
  fetched in color when it's on (`wants_color`).

## 3. Not in this

- **Per-paper targeting.** A paper set to one Kindle (`kindle = "…"`) still follows every known
  Kindle. Registry keys and sync ids differ in shape; matching them is its own change.
- **The cover and the puzzle pictures** stay gray: they're drawn in black and white.

## 4. Checks

- Config: `kindle_photos()` follows `color_kindle` and `[sync] photos`; `wants_color()` and
  `pdfs_wanted()` include the color Kindle PDF; `[sync] photos` round-trips and is left out of
  the file when unset.
- Screens: a profile is found by serial before name; a Scribe Colorsoft over MTP is in color.
- Build: with a color Kindle, the edition's EPUB keeps a color photo.
- On the device: a Scribe Colorsoft shows the photos in color, as the book and as the PDF.
