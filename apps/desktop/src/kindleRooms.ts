// SPDX-License-Identifier: GPL-3.0-or-later
import { matchesMetadata } from "./libraryView";
import type { BookKind, DeviceBookRow, DeviceShelf, HighlightRow, Spare } from "./types";

/** The Kindle tab's rooms (Books and Kindle design §7), like the Daily tab's. */
export type KindleRoom = "shelf" | "upgrades" | "highlights" | "words" | "device";

export const KINDLE_ROOMS: { key: KindleRoom; title: string }[] = [
  { key: "shelf", title: "Shelf" },
  { key: "upgrades", title: "Upgrades" },
  { key: "highlights", title: "Highlights" },
  { key: "words", title: "Words" },
  { key: "device", title: "This Kindle" },
];

/** The Shelf's Show control (Kindle tab redesign §3): how far in. */
export type ShelfShow = "all" | "reading" | "not-started" | "finished";

export const SHELF_SHOWS: { key: ShelfShow; title: string }[] = [
  { key: "all", title: "All" },
  { key: "reading", title: "Reading" },
  { key: "not-started", title: "Unread" },
  { key: "finished", title: "Finished" },
];

/** The Shelf's From menu: where a book came from. */
export type ShelfFrom = "anywhere" | "amazon-own" | "sideloaded" | "deckle";

export const SHELF_FROMS: { key: ShelfFrom; title: string }[] = [
  { key: "anywhere", title: "From anywhere" },
  { key: "amazon-own", title: "From Amazon" },
  { key: "sideloaded", title: "Sideloaded" },
  { key: "deckle", title: "From Deckle" },
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

function matches(b: DeviceBookRow, show: ShelfShow, from: ShelfFrom): boolean {
  // Papers only show under From Deckle, and only in All: they have no reading state worth a filter.
  if (b.kind === "paper") return from === "deckle" && show === "all";
  if (from !== "anywhere" && b.kind !== from) return false;
  return show === "all" || b.reading === show;
}

/** How many books each Show choice has, within a From choice. */
export function showCounts(books: DeviceBookRow[], from: ShelfFrom): Record<ShelfShow, number> {
  const out = {} as Record<ShelfShow, number>;
  for (const s of SHELF_SHOWS) out[s.key] = books.filter((b) => matches(b, s.key, from)).length;
  return out;
}

/** The Shelf's books: shown, from, searched (title, author, file name) and sorted. */
export function shelfBooks(books: DeviceBookRow[], show: ShelfShow, from: ShelfFrom, query: string, sort: ShelfSort): DeviceBookRow[] {
  const q = query.trim().toLowerCase();
  const found = books.filter((b) => matches(b, show, from) && matchesMetadata(titleOf(b), [...b.authors, b.file], null, q));
  const byTitle = (a: DeviceBookRow, b: DeviceBookRow) => titleOf(a).localeCompare(titleOf(b), undefined, { sensitivity: "base" });
  const author = (b: DeviceBookRow) => b.authors[0] ?? "\uffff";
  const order: Record<ShelfSort, (a: DeviceBookRow, b: DeviceBookRow) => number> = {
    // Read most recently first; never-opened books after, by title.
    recent: (a, b) => (b.last_read ?? "").localeCompare(a.last_read ?? "") || byTitle(a, b),
    title: byTitle,
    author: (a, b) => author(a).localeCompare(author(b), undefined, { sensitivity: "base" }) || byTitle(a, b),
    size: (a, b) => b.size - a.size || byTitle(a, b),
  };
  return [...found].sort(order[sort]);
}

/** What a book is, in plain words: Kindle book, Document, Amazon, Paper, or its format. */
export function kindWord(b: DeviceBookRow): string {
  if (b.kind === "amazon-own") return "Amazon";
  if (b.kind === "paper") return "Paper";
  if (b.cde === "EBOK") return "Kindle book";
  if (b.cde === "PDOC") return "Document";
  return b.format.toUpperCase();
}

/** The line under a cover: how far in, then what it is. */
export function coverLine(b: DeviceBookRow): string {
  const far = b.reading === "finished" ? "Finished" : b.reading === "reading" && b.fraction != null ? `${Math.round(b.fraction * 100)}%` : "Unread";
  return b.kind === "paper" ? kindWord(b) : `${far} · ${kindWord(b)}`;
}

/** Whether a cover wears the Upgrade mark: Upgrades can do something for it. */
export function canBeBetter(b: DeviceBookRow): boolean {
  return b.class === "has-source" || b.class === "kindle-copy" || (b.updates.length > 0 && !!b.library_source);
}

/** Continue reading: what's being read, most recently opened first, four at most. */
export function continueReading(books: DeviceBookRow[]): DeviceBookRow[] {
  return books
    .filter((b) => b.kind !== "paper" && b.reading === "reading")
    .sort((a, b) => (b.last_read ?? "").localeCompare(a.last_read ?? ""))
    .slice(0, 4);
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

/** One Worth a look card on the Shelf (§3): what, a line on it, and the tab that fixes it. */
export interface Nudge {
  key: "upgrades" | "highlights" | "space";
  title: string;
  body: string;
  action: string;
  room: KindleRoom;
}

const names = (xs: string[]) => (xs.length <= 1 ? xs.join("") : `${xs.slice(0, -1).join(", ")} and ${xs[xs.length - 1]}`);

/** The Shelf's Worth a look cards; only the ones with something to say. `upgrades` is the Upgrades room's own count, once it has one. */
export function worthALook(shelf: DeviceShelf, upgrades: number | null): Nudge[] {
  const out: Nudge[] = [];
  const can = (upgrades ?? upgradable(shelf.books)) + updatable(shelf.books).length;
  if (can > 0)
    out.push({ key: "upgrades", title: `${plural(can, "book")} can be upgraded`, body: "X-Ray, Word Wise and page numbers, where Deckle can add them.", action: "See upgrades", room: "upgrades" });
  const fresh = shelf.highlights.filter((h) => h.new > 0);
  const n = fresh.reduce((sum, h) => sum + h.new, 0);
  if (n > 0) {
    const shown = fresh.slice(0, 2).map((h) => h.title);
    const more = fresh.length - shown.length;
    out.push({ key: "highlights", title: `${n} new highlight${n === 1 ? "" : "s"}`, body: `From ${names(more > 0 ? [...shown, plural(more, "more book")] : shown)}.`, action: "Save as Markdown", room: "highlights" });
  }
  const free = shelf.spare.reduce((sum, s) => sum + s.size, 0);
  if (free > 0) {
    const count = (why: Spare["why"]) => shelf.spare.filter((s) => s.why === why).length;
    const parts = [count("duplicate") && plural(count("duplicate"), "duplicate"), count("finished") && plural(count("finished"), "finished book"), count("old-paper") && plural(count("old-paper"), "old paper")].filter((x): x is string => !!x);
    out.push({ key: "space", title: `${size(free)} you could free`, body: `${names(parts)}.`.replace(/^./, (c) => c.toUpperCase()), action: "Make room", room: "device" });
  }
  return out;
}

/** The counts the tabs carry: the Upgrades badge and the Highlights note. */
export function tabCounts(shelf: DeviceShelf | null, upgrades: number | null): { upgrades: number; highlights: string } {
  if (!shelf) return { upgrades: 0, highlights: "" };
  const fresh = shelf.highlights.reduce((n, h) => n + h.new, 0);
  return { upgrades: (upgrades ?? upgradable(shelf.books)) + updatable(shelf.books).length, highlights: fresh ? `${fresh} new` : "" };
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

/** What the drawer's Reading tab offers (Kindle tab redesign §4): the one thing Deckle can do better, if any. */
export type Better =
  | { kind: "update"; title: string; body: string; action: string }
  | { kind: "upgrade"; title: string; body: string; action: string }
  | { kind: "as-kindle-book"; title: string; body: string; action: string };

export function better(b: DeviceBookRow): Better | null {
  if (b.updates.length > 0 && b.library_source)
    return { kind: "update", title: "Deckle can build this better now", body: b.updates.map((u) => u.text).join(" "), action: "Update" };
  if (b.class === "has-source" || b.class === "kindle-copy")
    return {
      kind: "upgrade",
      title: "Deckle can upgrade this",
      body: `${b.class === "has-source" ? "Its original is on this computer." : "Deckle can rebuild it from the Kindle’s own copy."} Upgrading gives it what a store book has, where Deckle can add it: X-Ray, Word Wise and page numbers. Your place and highlights are kept.`,
      action: "Upgrade…",
    };
  if (b.library_source && b.cde === "PDOC")
    return { kind: "as-kindle-book", title: "Send it as a Kindle book", body: "It went over as a document, so it has no X-Ray or Word Wise. Sent as a Kindle book, it can have both.", action: "Send as a Kindle book" };
  return null;
}

/** The drawer's On the Kindle list, in plain words. */
export function onKindle(b: DeviceBookRow): [string, string][] {
  const shows =
    b.kind === "amazon-own" ? "Book from Amazon" : b.kind === "paper" ? "Deckle paper" : b.cde === "EBOK" ? "Kindle book" : b.cde === "PDOC" ? "Document, with its own cover" : `${b.format.toUpperCase()} file`;
  const why: Partial<Record<DeviceBookRow["class"], string>> = { locked: "locked by DRM, which Deckle never opens", "fixed-layout": "a fixed layout, left as it is" };
  const feature = (on: boolean) => (on ? "Yes" : why[b.class] ? `No: ${why[b.class]}` : "Not yet");
  const rows: [string, string][] = [
    ["Shows up as", shows],
    ["X-Ray", feature(b.xray)],
    ["Word Wise", feature(b.word_wise)],
    ["Size", size(b.size)],
    ["Sent", b.from_deckle ? (b.library_source ? "By Deckle · in your library" : "By Deckle") : "Not by Deckle"],
  ];
  if (b.duplicate) rows.push(["Duplicate", "Another copy of it is on this Kindle"]);
  return rows;
}

/** This book's highlights, matched by title. */
export function highlightsOf(b: DeviceBookRow, rows: HighlightRow[]): HighlightRow | null {
  const t = titleOf(b).trim().toLowerCase();
  return rows.find((h) => h.title.trim().toLowerCase() === t) ?? null;
}

/** The mode an update keeps: the one it was sent in. */
export function sentMode(b: DeviceBookRow): "pdoc" | "ebok" | null {
  return b.cde === "EBOK" ? "ebok" : b.cde === "PDOC" ? "pdoc" : null;
}

/** Make room's groups (Kindle tab redesign §6): duplicates, old papers, then finished books, each with its total. */
export function spareGroups(spare: Spare[]): { why: Spare["why"]; title: string; total: number; items: Spare[] }[] {
  const titles: [Spare["why"], string][] = [
    ["duplicate", "Duplicates"],
    ["old-paper", "Old papers"],
    ["finished", "Finished, over 10 MB"],
  ];
  return titles
    .map(([why, title]) => {
      const items = spare.filter((s) => s.why === why);
      return { why, title, total: items.reduce((n, s) => n + s.size, 0), items };
    })
    .filter((g) => g.items.length > 0);
}

/** "5 ticked · frees 82.0 MB". */
export function freesLine(spare: Spare[], ticked: Set<string>): string {
  const chosen = spare.filter((s) => ticked.has(s.file));
  if (chosen.length === 0) return "Nothing ticked";
  return `${chosen.length} ticked · frees ${size(chosen.reduce((n, s) => n + s.size, 0))}`;
}

/** How a Kindle is connected, in plain words. */
export function connectedBy(kind: "mass_storage" | "mtp"): string {
  return kind === "mtp" ? "Connected over USB as a media device" : "Connected over USB as a drive";
}

/** "12 Oct", or "12 Oct 2025" in another year. */
export function sentOn(at: string, now: Date): string {
  const d = new Date(at);
  if (Number.isNaN(d.getTime())) return "";
  const day = d.toLocaleDateString("en-GB", { day: "numeric", month: "short", timeZone: "UTC" });
  return d.getUTCFullYear() === now.getUTCFullYear() ? day : `${day} ${d.getUTCFullYear()}`;
}
