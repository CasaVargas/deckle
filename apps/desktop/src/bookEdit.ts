// SPDX-License-Identifier: GPL-3.0-or-later
// Editing a book's title, authors and series anywhere it is (details editing): before it's
// sent, on a Kindle (rebuilt with them), on a Kobo or PocketBook, or a reMarkable's name.
import type { BookEdit, DeviceBookRow } from "./types";

/** Authors typed one per line, or with " & " or ";" between them. */
export function authorsFromText(text: string): string[] {
  return text
    .split(/\n|;| & /)
    .map((a) => a.trim())
    .filter(Boolean);
}

export function authorsText(authors: string[]): string {
  return authors.join(" & ");
}

/** What's wrong with an edit, or null. */
export function editProblem(e: BookEdit): string | null {
  if (!e.title.trim()) return "A book needs a title.";
  if (e.series && e.series.index != null && (!Number.isFinite(e.series.index) || e.series.index < 0)) return "The series number should be a number, like 2 or 2.5.";
  return null;
}

/** The edit from the form's fields: an empty series name drops the series. */
export function editFrom(title: string, authors: string, series: string, index: string): BookEdit {
  const n = index.trim() === "" ? null : Number(index.trim());
  return {
    title: title.trim(),
    authors: authorsFromText(authors),
    series: series.trim() ? { name: series.trim(), index: n } : null,
  };
}

/** "Dune by Frank Herbert": what the queue and notices call an edited book. */
export function editLabel(e: BookEdit): string {
  return e.authors.length ? `${e.title} by ${authorsText(e.authors)}` : e.title;
}

/**
 * A book Amazon delivered (bought, or sent to Kindle by email) stays in the owner's Amazon
 * account: removing the file takes only the download. Null for any other book.
 */
export function amazonRemoveNote(book: Pick<DeviceBookRow, "kind" | "class">): string | null {
  if (book.kind !== "amazon-own" && book.class !== "locked") return null;
  return "This came from Amazon (bought, or sent to Kindle by email), so Amazon keeps it in your account: this removes only the copy on the Kindle, and it can still show under All with a download icon. To remove it for good, use Remove from library on the Kindle, or Manage Your Content and Devices on Amazon.";
}
