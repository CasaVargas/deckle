// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { libraryItems, missingNumbers, ranges, seriesKey, seriesLine, type SeriesGroup } from "./libraryView";
import type { CatalogEntry } from "./types";

const e = (title: string, over: Partial<CatalogEntry> = {}): CatalogEntry => ({
  path: `/b/${title}.epub`,
  from: "library",
  title,
  authors: ["J. Corey"],
  series: null,
  isbn: null,
  library_dir: null,
  sent_to: [],
  added: null,
  on_kindle: false,
  ...over,
});
const s = (name: string, index: number | null) => ({ name, index });

const books = [
  e("Leviathan Wakes", { series: s("The Expanse", 1), on_kindle: true }),
  e("Caliban's War", { series: s("the expanse ", 2) }),
  e("Abaddon's Gate", { series: s("The Expanse", 3) }),
  e("Cibola Burn", { series: s("The Expanse", 4) }),
  e("Persepolis Rising", { series: s("The Expanse", 7) }),
  e("Anathem", { authors: ["N. Stephenson"] }),
  e("Zorba", { authors: ["N. Kazantzakis"], on_kindle: true }),
];

describe("series", () => {
  it("names the gaps", () => {
    expect(missingNumbers([1, 2, 3, 4, 7, 2.5, null])).toEqual([5, 6]);
    expect(ranges([4, 1, 2, 3, 7])).toBe("1–4, 7");
    expect(seriesKey("  The  Expanse!")).toBe("the expanse");
  });

  it("groups a series across spellings, in reading order, with its gaps", () => {
    const items = libraryItems(books, "", "all", true);
    expect(items.map((i) => (i.kind === "series" ? `series:${i.name}` : i.book.title))).toEqual(["Anathem", "series:The Expanse", "Zorba"]);
    const g = items[1] as SeriesGroup;
    expect(g.books.map((b) => b.series?.index)).toEqual([1, 2, 3, 4, 7]);
    expect(seriesLine(g)).toBe("The Expanse, 1–4, 7 · 5 and 6 aren't in your library");
  });

  it("filters by search and by what's not on this Kindle, keeping a series' gaps", () => {
    const items = libraryItems(books, "", "not-on-kindle", true);
    expect(items.map((i) => (i.kind === "series" ? i.books.length : i.book.title))).toEqual(["Anathem", 4]);
    expect((items[1] as SeriesGroup).missing).toEqual([5, 6]);
    expect(libraryItems(books, "stephenson", "all", true).map((i) => (i.kind === "book" ? i.book.title : ""))).toEqual(["Anathem"]);
    expect(libraryItems(books, "", "all", false)).toHaveLength(7);
  });
});
