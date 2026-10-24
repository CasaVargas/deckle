// SPDX-License-Identifier: GPL-3.0-or-later
import { matchesMetadata } from "./libraryView";
import type { BookKind, DeviceBookRow, DeviceShelf, Spare } from "./types";

/** The Kindle tab's rooms (Books and Kindle design §7), like the Daily tab's. */
export type KindleRoom = "shelf" | "upgrades" | "highlights" | "words" | "device";

export const KINDLE_ROOMS: { key: KindleRoom; title: string }[] = [
  { key: "shelf", title: "Shelf" },
  { key: "upgrades", title: "Upgrades" },
  { key: "highlights", title: "Highlights" },
  { key: "words", title: "Words" },
  { key: "device", title: "This Kindle" },
];

export type ShelfFilter = "all" | "reading" | "not-started" | "finished" | "amazon-own" | "sideloaded" | "deckle";

export const SHELF_FILTERS: { key: ShelfFilter; title: string }[] = [
  { key: "all", title: "All" },
  { key: "reading", title: "Reading" },
  { key: "not-started", title: "Not started" },
  { key: "finished", title: "Finished" },
  { key: "amazon-own", title: "Amazon's own" },
  { key: "sideloaded", title: "Sideloaded" },
  { key: "deckle", title: "Deckle" },
];

export type ShelfSort = "recent" | "title" | "author" | "size";

export const SHELF_SORTS: { key: ShelfSort; title: string }[] = [
  { key: "recent", title: "Recently read" },
  { key: "title", title: "Title" },
  { key: "author", title: "Author" },
  { key: "size", title: "Size" },
];

export const KIND_LABEL: Record<BookKind, string> = {
  "amazon-own": "Amazon's own",
  deckle: "Deckle's books",
  paper: "Deckle papers",
  sideloaded: "Sideloaded",
};

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

export function size(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1e3))} KB`;
}

export function titleOf(b: DeviceBookRow): string {
  return b.title ?? b.file;
}

function matches(b: DeviceBookRow, f: ShelfFilter): boolean {
  switch (f) {
    case "all":
      return b.kind !== "paper";
    case "reading":
    case "not-started":
    case "finished":
      return b.kind !== "paper" && b.reading === f;
    case "deckle":
      return b.kind === "deckle" || b.kind === "paper";
    default:
      return b.kind === f;
  }
}

/** How many books each filter shows. */
export function filterCounts(books: DeviceBookRow[]): Record<ShelfFilter, number> {
  const out = {} as Record<ShelfFilter, number>;
  for (const f of SHELF_FILTERS) out[f.key] = books.filter((b) => matches(b, f.key)).length;
  return out;
}

/** The Shelf's books: filtered, searched (title, author, file name) and sorted. */
export function shelfBooks(books: DeviceBookRow[], filter: ShelfFilter, query: string, sort: ShelfSort): DeviceBookRow[] {
  const q = query.trim().toLowerCase();
  const found = books.filter((b) => matches(b, filter) && matchesMetadata(titleOf(b), [...b.authors, b.file], null, q));
  const byTitle = (a: DeviceBookRow, b: DeviceBookRow) => titleOf(a).localeCompare(titleOf(b), undefined, { sensitivity: "base" });
  const author = (b: DeviceBookRow) => b.authors[0] ?? "￿";
  const order: Record<ShelfSort, (a: DeviceBookRow, b: DeviceBookRow) => number> = {
    // Read most recently first; never-opened books after, by title.
    recent: (a, b) => (b.last_read ?? "").localeCompare(a.last_read ?? "") || byTitle(a, b),
    title: byTitle,
    author: (a, b) => author(a).localeCompare(author(b), undefined, { sensitivity: "base" }) || byTitle(a, b),
    size: (a, b) => b.size - a.size || byTitle(a, b),
  };
  return [...found].sort(order[sort]);
}

/** "today", "yesterday", "3 days ago", "in March". */
export function lastOpened(utc: string | null, now: Date): string | null {
  if (!utc) return null;
  const then = new Date(utc);
  if (Number.isNaN(then.getTime())) return null;
  const day = (d: Date) => Date.UTC(d.getFullYear(), d.getMonth(), d.getDate());
  const days = Math.round((day(now) - day(then)) / 86_400_000);
  if (days <= 0) return "today";
  if (days === 1) return "yesterday";
  if (days < 14) return `${days} days ago`;
  if (days < 60) return `${Math.round(days / 7)} weeks ago`;
  const month = then.toLocaleString("en", { month: "long" });
  return then.getFullYear() === now.getFullYear() ? `in ${month}` : `in ${month} ${then.getFullYear()}`;
}

/** The line under a book on the Shelf: how far in and when, else what it is. */
export function progressLine(b: DeviceBookRow, now: Date): string {
  const when = lastOpened(b.last_read, now);
  if (b.reading === "finished") return when ? `Finished · opened ${when}` : "Finished";
  if (b.fraction != null && b.reading === "reading") return `${Math.round(b.fraction * 100)}%${when ? ` · opened ${when}` : ""}`;
  return when ? `Opened ${when}` : "Not started";
}

/** Books the Upgrades room could offer, from the shelf's classes (the room's own count replaces it once it has looked). */
export function upgradable(books: DeviceBookRow[]): number {
  return books.filter((b) => b.class === "has-source" || b.class === "kindle-copy").length;
}

/** Deckle's books it could build better now. */
export function updatable(books: DeviceBookRow[]): DeviceBookRow[] {
  return books.filter((b) => b.updates.length > 0 && b.library_source);
}

/** The line under each room's name. `upgrades` is the Upgrades room's own count, once it has one. */
export function kindleRoomNotes(shelf: DeviceShelf | null, upgrades: number | null): Record<KindleRoom, string> {
  if (!shelf) return { shelf: "", upgrades: "", highlights: "", words: "", device: "" };
  const books = shelf.books.filter((b) => b.kind !== "paper");
  const reading = books.filter((b) => b.reading === "reading").length;
  const can = (upgrades ?? upgradable(shelf.books)) + updatable(shelf.books).length;
  const fresh = shelf.highlights.reduce((n, h) => n + h.new, 0);
  return {
    shelf: `${plural(books.length, "book")}${reading ? ` · ${reading} reading` : ""}`,
    upgrades: can ? `${can} can be upgraded` : "All current",
    highlights: fresh ? `${fresh} new` : plural(shelf.highlights.length, "book"),
    words: "Vocabulary Builder",
    device: shelf.space ? `${size(shelf.space.free)} free` : "",
  };
}

/** Why a book is on the Make room list. */
export function spareLine(s: Spare): string {
  const sdr = s.has_sdr ? " Its reading position goes with it." : "";
  switch (s.why) {
    case "finished":
      return `Finished, ${size(s.size)}.${sdr}`;
    case "duplicate":
      return `Another copy stays: ${s.keeping}.${sdr}`;
    case "old-paper":
      return `An old paper, past what the Daily keeps.`;
  }
}

/** The Make room list ticked at first: everything but finished books (those you may want to reread). */
export function defaultSpare(spare: Spare[]): Set<string> {
  return new Set(spare.filter((s) => s.why !== "finished").map((s) => s.file));
}

/** Each slice of the space bar as a percent of the whole, smallest visible at 0.5%. */
export function spaceSlices(split: { total: number; free: number; kinds: [BookKind, number, number][]; other: number }): { key: BookKind | "other" | "free"; label: string; bytes: number; pct: number }[] {
  const total = Math.max(1, split.total);
  const pct = (b: number) => (b > 0 ? Math.max(0.5, (b / total) * 100) : 0);
  return [
    ...split.kinds.map(([k, b, n]) => ({ key: k, label: `${KIND_LABEL[k]} · ${plural(n, "book")}`, bytes: b, pct: pct(b) })),
    { key: "other" as const, label: "System and other", bytes: split.other, pct: pct(split.other) },
    { key: "free" as const, label: "Free", bytes: split.free, pct: pct(split.free) },
  ].filter((s) => s.bytes > 0);
}
