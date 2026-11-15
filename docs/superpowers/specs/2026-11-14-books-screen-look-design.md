# Books screen: the look

Mockup: the brainstorm screen "A, with C's folded gaps and bigger covers" (`books-look-v2.html`,
kept under `.superpowers/brainstorm/`, not committed). Picked over B (paper and shelves) and C
(cover gallery) on 2026-11-14.

## Why

The Books screen is the first thing Deckle shows, and its owner doesn't like how it looks. The
layout is fine (library left, To send right, from the 2026-10-26 Books tab redesign); the
dressing isn't. On a real library the screen is mostly empty space: every missing volume of a
series is a full cover-sized dashed box (Hokusai Manga shows five of them beside one real cover),
and the empty To send panel is a dead grey column with a disabled button.

## Goals

- The same screen, better dressed, in both light and dark (it follows the Mac's setting).
- Covers carry the screen: a bit bigger, with real depth (shadow, a spine edge).
- Missing volumes stop dominating: one or two missing show as thin numbered stubs; a run of
  three or more shows as one tile ("1–5 · not in your library").
- Headings in a book serif.
- An empty To send reads as a place to drop books, with the Kindle status above Send.

## Non-goals and anti-goals

- No layout change: no new panels, nothing moved between screens, To send stays a panel (C's
  floating pill was rejected: it hides where sending happens).
- No new features, settings or network calls; the README is unchanged apart from screenshot alt
  text if it names something renamed (nothing is).
- Anti-goal: anything that looks like a different app from the other tabs. Tokens and type are
  shared, so Daily, Cookbook, Puzzles and Devices pick up the serif and colours and must still
  look right.

## Approaches considered

- **A, refined dark** (today's palette done properly): least change, works in both modes.
- **B, paper and shelves**: on-brand with the printed paper, but a bigger shift; its shelf
  ledges may come later.
- **C, cover gallery**: covers first, captions on hover, To send as a floating pill.
- **Picked: A, plus C's folded gaps and bigger covers.** A fixes what looks bad without changing
  how Deckle feels; C's two pieces fix the empty-space problem A alone leaves.

## The design

- **Type.** Headings (`h1`, `.brand`, `.tray-head h2`, `.lib-series-line`) use
  `"Iowan Old Style", "Palatino", ui-serif, Georgia, serif`, weight 500, slight negative
  tracking. Iowan Old Style ships with macOS; Windows and Linux fall back to Palatino or Georgia.
  No font is bundled (bundling Newsreader stays a separate decision).
- **Covers.** `.lib-grid` columns go from `minmax(118px, 1fr)` to about 136px; covers get a
  two-layer shadow (soft and close) and a 3px darker spine strip on the left edge; corners 3/4px.
  Title and author under the cover as now.
- **Series line.** "Fixture Series · you have 1 of 2", the series name in the foreground colour,
  the count muted.
- **Missing volumes.** Shown in number order among the covers (today they follow the books). A
  run of one or two consecutive missing numbers: a stub per number, cover-high, ~16px wide,
  faintly striped like page edges, the number at its foot, and the existing "No. n isn't in your
  library" as its tooltip. A run of three or more: one cover-sized tile, "1–5" in the heading
  serif over "not in your library", tooltip listing the numbers.
- **To send, empty.** The panel's empty state becomes a dashed, rounded drop area filling the
  space: an icon, "Nothing to send yet" in the foreground colour, and "Pick covers on the left, or
  drop EPUBs, PDFs and Word files here" muted. The Kindle status line sits in a soft box above
  Send; Send keeps its disabled look until something is queued.
- **Colours.** The existing tokens stay; the light and dark palettes are tuned only where the
  mockup needed it (segmented-control fill, a quieter border in dark). Terracotta stays the one
  accent.

## Left to the builder

- Exact sizes and shadows: match the mockup by eye in the running app, both modes, at the
  default 960×640 window and full screen.
- Where the gap-run grouping lives: `libraryView.ts` is the natural home (it already computes
  `missing`), with a unit test for runs (1; 1,2; 1–5; 1,3–7).
- Verification: screenshots of the Books tab in light and dark, empty and with a queue, plus a
  look at the other four tabs for anything the shared type change broke.
- CHANGELOG: one line under Unreleased ("The Books screen looks better: …").
