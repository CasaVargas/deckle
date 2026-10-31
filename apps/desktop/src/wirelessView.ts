// SPDX-License-Identifier: GPL-3.0-or-later
// The words Settings → KOReader's calibre connection section shows (calibre bridge design §4.4).
import type { CatalogEntry, WirelessJob, WirelessReader } from "./types";

/** "2.1 GB free", "640 MB free". */
export function freeLine(bytes: number | null): string | null {
  if (bytes === null) return null;
  return bytes >= 1e9 ? `${(bytes / 1e9).toFixed(1)} GB free` : `${Math.round(bytes / 1e6)} MB free`;
}

/** "Connected", or "Last seen 30 Oct". */
export function seenLine(r: WirelessReader): string {
  if (r.connected) return "Connected";
  const d = new Date(r.last_seen);
  if (Number.isNaN(d.getTime())) return "Not connected";
  return `Last seen ${d.toLocaleDateString(undefined, { day: "numeric", month: "short" })}`;
}

/** The reader a job is for, as the page names it. */
export function jobReader(job: WirelessJob, readers: WirelessReader[]): string {
  if (job.reader.toLowerCase() === "any") return "the next reader to connect";
  return readers.find((r) => r.id === job.reader || r.name.toLowerCase() === job.reader.toLowerCase())?.name ?? job.reader;
}

/** "Send Dune.epub", "Take off Frank Herbert/Dune - Frank Herbert.epub". */
export function jobLine(job: WirelessJob): string {
  if (job.send) return `Send ${job.send.split(/[\\/]/).pop()}`;
  if (job.remove) return `Take off ${job.remove}`;
  return "Nothing";
}

/** The jobs waiting for `r` (named by id, name, or any). */
export function jobsFor(r: WirelessReader, queue: WirelessJob[]): WirelessJob[] {
  return queue.filter((j) => j.reader === r.id || j.reader.toLowerCase() === r.name.toLowerCase() || j.reader.toLowerCase() === "any");
}

/** Library books matching `text` (title or author), at most `limit`, sendable formats only. */
export function pickable(entries: CatalogEntry[], text: string, limit = 50): CatalogEntry[] {
  const q = text.trim().toLowerCase();
  return entries
    .filter((e) => !q || e.title.toLowerCase().includes(q) || e.authors.some((a) => a.toLowerCase().includes(q)))
    .slice(0, limit);
}

/** A reader's book by title and author, else its path on the reader. */
export function bookLine(b: { lpath: string; title: string | null; authors: string[] }): string {
  if (!b.title) return b.lpath;
  return b.authors.length ? `${b.title} — ${b.authors.join(", ")}` : b.title;
}
