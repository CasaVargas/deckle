// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { byOrigin, defaultTicked, leftAsIs, partwayLine, sourceLine, sourcesLine, upgradeLabel } from "./upgradesView";
import type { UpgradeRow } from "./types";

const row = (p: Partial<UpgradeRow>): UpgradeRow => ({
  file: "Middlemarch.azw3",
  title: "Middlemarch",
  authors: ["George Eliot"],
  format: "azw3",
  class: "has-source",
  source: "/Users/me/Calibre Library/George Eliot/Middlemarch (1)/Middlemarch - George Eliot.epub",
  origin: "calibre",
  fraction: null,
  partway: false,
  gains: ["Enhanced Typesetting", "Page numbers", "X-Ray", "Word Wise"],
  cover: null,
  ...p,
});

test("books you're partway through aren't ticked at first", () => {
  const rows = [row({}), row({ file: "Dracula.azw3", partway: true, fraction: 0.12 })];
  expect([...defaultTicked(rows)]).toEqual(["Middlemarch.azw3"]);
  expect(partwayLine(rows[1])).toBe("You're 12% in. The new copy opens at the start, so note your place first.");
  expect(partwayLine(rows[0])).toBeNull();
});

test("a book rebuilt from the Kindle's copy says so", () => {
  expect(sourceLine(row({ source: null, origin: "kindle-copy" }))).toBe(
    "No original here: rebuilt from the Kindle's own AZW3, checked before it replaces anything",
  );
});

test("where the original is", () => {
  expect(sourceLine(row({}))).toBe("calibre · EPUB · on the Kindle as AZW3");
  expect(sourceLine(row({ origin: "folder", source: "/Books/x.epub", format: "mobi" }))).toBe("a folder · EPUB · on the Kindle as MOBI");
});

test("what's left alone, in one line", () => {
  expect(leftAsIs({})).toBeNull();
  expect(leftAsIs({ "has-source": 3, current: 2 })).toBeNull();
  expect(leftAsIs({ "amazon-own": 158, locked: 2, "fixed-layout": 1, "kindle-copy": 7, "no-source": 2, pdf: 4 })).toBe(
    "Left as they are: 158 store books, which already have all of this · 2 locked by DRM, which Deckle never opens · 1 fixed-layout book · 2 KFX with no original on this computer · 4 PDFs.",
  );
});

test("where Deckle looks", () => {
  expect(sourcesLine({ find_in_calibre: true, calibre: "/Users/me/Calibre Library", folders: ["/a", "/b"] })).toBe(
    "Looking for originals in your calibre library (/Users/me/Calibre Library) and 2 folders. Change this in Settings → General.",
  );
  expect(sourcesLine({ find_in_calibre: true, calibre: null, folders: [] })).toContain("isn't looking for originals anywhere");
  expect(upgradeLabel(0)).toBe("Upgrade");
  expect(upgradeLabel(1)).toBe("Upgrade 1 book");
  expect(upgradeLabel(28)).toBe("Upgrade 28 books");
});

test("rows are grouped by where the new copy comes from, calibre first", () => {
  const groups = byOrigin([row({ file: "k", origin: "kindle-copy" }), row({ file: "c", origin: "calibre" }), row({ file: "c2", origin: "calibre" })]);
  expect(groups.map((g) => [g.origin, g.rows.length])).toEqual([
    ["calibre", 2],
    ["kindle-copy", 1],
  ]);
  expect(groups[1].title).toBe("Rebuilt from the Kindle's own copy");
});
