// SPDX-License-Identifier: GPL-3.0-or-later
// The Devices tab's other readers (a Kobo, a PocketBook, a reMarkable): the pure parts of
// their library page, beside ReaderView.tsx as kindleRooms.ts is beside KindleView.
import type { ReaderBook, ReaderRow } from "./types";

export type ReaderSort = "recent" | "title" | "size";

/** The books that match `query` (title, author, folder), in `sort` order. */
export function readerBooks(books: ReaderBook[], query: string, sort: ReaderSort): ReaderBook[] {
  const q = query.trim().toLowerCase();
  const hit = (b: ReaderBook) => !q || b.title.toLowerCase().includes(q) || b.authors.some((a) => a.toLowerCase().includes(q)) || b.folder.toLowerCase().includes(q);
  const out = books.filter(hit);
  const byTitle = (a: ReaderBook, b: ReaderBook) => a.title.localeCompare(b.title, undefined, { sensitivity: "base" });
  if (sort === "title") out.sort(byTitle);
  else if (sort === "size") out.sort((a, b) => (b.bytes ?? 0) - (a.bytes ?? 0) || byTitle(a, b));
  else out.sort((a, b) => (b.last_read ?? "").localeCompare(a.last_read ?? "") || byTitle(a, b));
  return out;
}

/** "Finished", "42%", "Not started", or "" when the reader doesn't say. */
export function progressLabel(b: ReaderBook): string {
  if (b.finished) return "Finished";
  if (b.percent == null) return "";
  if (b.percent <= 0) return "Not started";
  return `${Math.round(b.percent)}%`;
}

/** "Read Nov 3" / "Read Nov 3, 2025" (another year), from an RFC 3339 time. */
export function lastReadLabel(iso: string | null, now: Date = new Date()): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  const opts: Intl.DateTimeFormatOptions = { month: "short", day: "numeric", timeZone: "UTC" };
  if (d.getUTCFullYear() !== now.getUTCFullYear()) opts.year = "numeric";
  return `Read ${d.toLocaleDateString("en-US", opts)}`;
}

/** What "Remove" will say before it goes, for this reader. */
export function removeConfirm(reader: ReaderRow, count: number): string {
  const what = count === 1 ? "this book" : `these ${count} books`;
  const base = `Remove ${what} from the ${reader.name}?`;
  if (reader.kind === "kobo")
    return `${base} Its database is copied to this computer first. The Kobo drops them from its library when you eject it. Highlights made in Kobo's own reader stay in its database; KOReader's go with the book.`;
  if (reader.kind === "remarkable")
    return `${base} They go to the tablet's trash, with their handwriting, and the tablet restarts its reader for a few seconds to show it. Back it up first if you might want them again.`;
  return `${base} The files and KOReader's notes beside them are deleted from the reader.`;
}

/** The line under the backup button. */
export function backupLine(reader: ReaderRow, last: string | null): string {
  const what =
    reader.kind === "remarkable"
      ? reader.key === "remarkable/ssh"
        ? "Each document as the tablet keeps it, handwriting included (a .tar you can put back)."
        : "Each document as a PDF with its handwriting drawn in."
      : reader.kind === "kobo"
        ? "Every book file with KOReader's notes beside it, and the Kobo's database (your places and highlights)."
        : "Every book file, with KOReader's notes beside it.";
  return last ? `${what} Last backup: ${last.split(/[\\/]/).pop()}` : `${what} Not backed up yet.`;
}

/** "Kobo · USB drive · Firmware 4.38" */
export function readerLine(r: ReaderRow): string {
  return [r.connection, r.detail].filter(Boolean).join(" · ");
}
