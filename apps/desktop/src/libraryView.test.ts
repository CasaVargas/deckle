// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { matchesMetadata, libraryItems, missingNumbers, ranges, seriesKey, seriesLine, shelfSlots, type SeriesGroup } from "./libraryView";
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
    expect(seriesLine(g)).toBe("The Expanse · you have 5 of 7");
  });

  it("filters by search and by what's not on this Kindle, keeping a series' gaps", () => {
    const items = libraryItems(books, "", "not-on-kindle", true);
    expect(items.map((i) => (i.kind === "series" ? i.books.length : i.book.title))).toEqual(["Anathem", 4]);
    expect((items[1] as SeriesGroup).missing).toEqual([5, 6]);
    expect(libraryItems(books, "stephenson", "all", true).map((i) => (i.kind === "book" ? i.book.title : ""))).toEqual(["Anathem"]);
    expect(libraryItems(books, "", "all", false)).toHaveLength(7);
  });
});

it("searches metadata across fields with case and accent folding", () => {
  const entry = e("Éarthsea", { authors: ["Ursula Le Guin"], series: s("Earthsea Cycle", 1) });
  expect(libraryItems([entry], "GUIN earthsea cycle", "all", false)).toHaveLength(1);
  expect(matchesMetadata("Éarthsea", ["Ursula Le Guin"], null, "guin e\u0301arthsea")).toBe(true);
  expect(matchesMetadata("Éarthsea", ["Ursula Le Guin"], null, "guin absent")).toBe(false);
});

const slotBook = (i: number | null) => e(`B${i}`, { series: i == null ? null : { name: "S", index: i } });
const group = (idx: (number | null)[], missing: number[]): SeriesGroup => ({ kind: "series", key: "s", name: "S", books: idx.map(slotBook), missing });
const shape = (g: SeriesGroup) =>
  shelfSlots(g).map((s) => (s.kind === "book" ? `b${s.book.series?.index ?? "-"}` : s.kind === "stub" ? `s${s.n}` : `f${s.from}-${s.to}`));

describe("shelfSlots", () => {
  it("puts a lone gap where its book would be", () => expect(shape(group([2], [1]))).toEqual(["s1", "b2"]));
  it("keeps two in a row as stubs", () => expect(shape(group([1, 4], [2, 3]))).toEqual(["b1", "s2", "s3", "b4"]));
  it("folds three or more", () => expect(shape(group([6], [1, 2, 3, 4, 5]))).toEqual(["f1-5", "b6"]));
  it("handles separate runs", () => expect(shape(group([2, 8], [1, 3, 4, 5, 6, 7]))).toEqual(["s1", "b2", "f3-7", "b8"]));
  it("keeps every book: fractions, duplicates, unnumbered last", () =>
    expect(shape(group([1, 2, 2, 2.5, 4, null], [3]))).toEqual(["b1", "b2", "b2", "b2.5", "s3", "b4", "b-"]));
});

describe("seriesLine", () => {
  it("is just the name when no book has a whole number", () => expect(seriesLine(group([null, 2.5], []))).toBe("S"));
});
