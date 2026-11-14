// SPDX-License-Identifier: GPL-3.0-or-later
import type { CatalogEntry } from "./types";

/** The Books tab's library (Books and Kindle design §8): search, series and "not on this Kindle". */
export type LibraryFilter = "all" | "not-on-kindle";

export interface SeriesGroup {
  kind: "series";
  key: string;
  name: string;
  /** In reading order; unnumbered last. */
  books: CatalogEntry[];
  /** Whole numbers from 1 to the highest owned that no book has. */
  missing: number[];
  /** Distinct whole numbers owned, counting books a filter hides. */
  owned: number;
}

export type LibraryItem = SeriesGroup | { kind: "book"; book: CatalogEntry };

/** What groups series names, as the library's own grouping does: case, spacing and edge punctuation ignored. */
export function seriesKey(name: string): string {
  return name
    .normalize("NFC")
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .join(" ")
    .replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, "");
}

export function missingNumbers(indices: (number | null)[]): number[] {
  const whole = indices.filter((i): i is number => i != null && Number.isInteger(i) && i >= 0 && i <= 1000);
  if (whole.length === 0) return [];
  const have = new Set(whole);
  const out: number[] = [];
  for (let n = 1; n <= Math.max(...whole); n++) if (!have.has(n)) out.push(n);
  return out;
}

/** "1–4, 6": the whole numbers owned, as runs. */
export function ranges(nums: number[]): string {
  const sorted = [...new Set(nums)].sort((a, b) => a - b);
  const out: string[] = [];
  for (let i = 0; i < sorted.length; ) {
    let j = i;
    while (j + 1 < sorted.length && sorted[j + 1] === sorted[j] + 1) j++;
    out.push(j > i ? `${sorted[i]}–${sorted[j]}` : `${sorted[i]}`);
    i = j + 1;
  }
  return out.join(", ");
}

/** "The Expanse · you have 5 of 7"; just the name when no book has a whole number. */
export function seriesLine(g: SeriesGroup): string {
  return g.owned ? `${g.name} · you have ${g.owned} of ${g.owned + g.missing.length}` : g.name;
}

/** Distinct whole series numbers among `books`. */
export function ownedNumbers(books: CatalogEntry[]): number {
  return new Set(books.map((b) => b.series?.index).filter((i): i is number => i != null && Number.isInteger(i))).size;
}

/** One place on a series' shelf: a book, one missing number, or a run of three or more. */
export type ShelfSlot = { kind: "book"; book: CatalogEntry } | { kind: "stub"; n: number } | { kind: "fold"; from: number; to: number };

/** A series' books with its missing numbers where they'd stand: runs of three or more fold into one. */
export function shelfSlots(g: SeriesGroup): ShelfSlot[] {
  const gaps: ShelfSlot[][] = [];
  const nums = [...g.missing].sort((a, b) => a - b);
  for (let i = 0; i < nums.length; ) {
    let j = i;
    while (j + 1 < nums.length && nums[j + 1] === nums[j] + 1) j++;
    gaps.push(j - i >= 2 ? [{ kind: "fold", from: nums[i], to: nums[j] }] : nums.slice(i, j + 1).map((n) => ({ kind: "stub", n })));
    i = j + 1;
  }
  const first = (run: ShelfSlot[]) => (run[0].kind === "fold" ? run[0].from : run[0].kind === "stub" ? run[0].n : 0);
  const out: ShelfSlot[] = [];
  let k = 0;
  for (const book of g.books) {
    const idx = book.series?.index;
    while (k < gaps.length && idx != null && first(gaps[k]) < idx) out.push(...gaps[k++]);
    out.push({ kind: "book", book });
  }
  while (k < gaps.length) out.push(...gaps[k++]);
  return out;
}

export function matchesMetadata(title: string, authors: string[], series: string | null, query: string): boolean {
  const fold = (s: string) => s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
  const haystack = fold([title, ...authors, series ?? ""].join(" "));
  return fold(query).split(/\s+/).filter(Boolean).every((term) => haystack.includes(term));
}

function matches(b: CatalogEntry, q: string): boolean {
  return matchesMetadata(b.title, b.authors, b.series?.name ?? null, q);
}

const byTitle = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: "base" });

/**
 * The library as the grid shows it. A series' gaps come from every book in it; the books shown
 * are those the search and filter keep, and a series with none of them is left out.
 */
export function libraryItems(entries: CatalogEntry[], query: string, filter: LibraryFilter, bySeries: boolean): LibraryItem[] {
  const q = query.trim().toLowerCase();
  const keep = (b: CatalogEntry) => matches(b, q) && (filter === "all" || !b.on_kindle);
  const items: LibraryItem[] = [];
  const groups = new Map<string, CatalogEntry[]>();
  for (const b of entries) {
    const key = bySeries && b.series ? seriesKey(b.series.name) : "";
    if (key) groups.set(key, [...(groups.get(key) ?? []), b]);
    else if (keep(b)) items.push({ kind: "book", book: b });
  }
  for (const [key, all] of groups) {
    const shown = all.filter(keep);
    if (shown.length === 0) continue;
    const order = (b: CatalogEntry) => b.series?.index ?? Number.POSITIVE_INFINITY;
    shown.sort((a, b) => order(a) - order(b) || byTitle(a.title, b.title));
    items.push({ kind: "series", key, name: all[0].series!.name, books: shown, missing: missingNumbers(all.map((b) => b.series?.index ?? null)), owned: ownedNumbers(all) });
  }
  const name = (i: LibraryItem) => (i.kind === "series" ? i.name : i.book.title).replace(/^(the|a|an)\s+/i, "");
  return items.sort((a, b) => byTitle(name(a), name(b)));
}

/** Where an entry is, for its card's foot. */
export function fromLabel(b: CatalogEntry): string {
  return b.from === "library" ? "Deckle's library" : b.from === "calibre" ? "calibre" : "a folder";
}
