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

function list(nums: number[]): string {
  if (nums.length <= 2) return nums.join(" and ");
  return `${nums.slice(0, -1).join(", ")} and ${nums[nums.length - 1]}`;
}

/** "The Expanse, 1–4 · 5 isn't in your library". */
export function seriesLine(g: SeriesGroup): string {
  const whole = g.books.map((b) => b.series?.index).filter((i): i is number => i != null && Number.isInteger(i));
  const head = whole.length ? `${g.name}, ${ranges(whole)}` : g.name;
  if (g.missing.length === 0) return head;
  const many = g.missing.length > 1;
  return `${head} · ${list(g.missing)} ${many ? "aren't" : "isn't"} in your library`;
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
    items.push({ kind: "series", key, name: all[0].series!.name, books: shown, missing: missingNumbers(all.map((b) => b.series?.index ?? null)) });
  }
  const name = (i: LibraryItem) => (i.kind === "series" ? i.name : i.book.title).replace(/^(the|a|an)\s+/i, "");
  return items.sort((a, b) => byTitle(name(a), name(b)));
}

/** Where an entry is, for its card's foot. */
export function fromLabel(b: CatalogEntry): string {
  return b.from === "library" ? "Deckle's library" : b.from === "calibre" ? "calibre" : "a folder";
}
