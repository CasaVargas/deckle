// SPDX-License-Identifier: GPL-3.0-or-later
import type { BookDetails, BookState, CatalogEntry } from "./types";

/** The book details editor's form (Books tab redesign §5), as the person typed it. */
export interface Draft {
  title: string;
  authors: string[];
  series: string;
  /** The series number as typed; checked by `problems`. */
  index: string;
  /** Kept with the queued book, not the library copy. */
  asin: string;
}

export type Field = keyof Draft;

const ASIN = /^[A-Z0-9]{10}$/;

/** The form for a book: the library entry's details when it has one, else the queued book's. */
export function draftFrom(entry: CatalogEntry | null, book: Pick<BookState, "title" | "authors" | "asin"> | null): Draft {
  const authors = entry?.authors ?? (book?.authors ?? "").split(",");
  const index = entry?.series?.index;
  return {
    title: entry?.title ?? book?.title ?? "",
    authors: authors.map((a) => a.trim()).filter(Boolean),
    series: entry?.series?.name ?? "",
    index: index != null ? String(index) : "",
    asin: book?.asin ?? "",
  };
}

const norm = (d: Draft) => ({
  title: d.title.trim(),
  authors: d.authors.map((a) => a.trim()).filter(Boolean).join("\n"),
  series: d.series.trim(),
  index: d.index.trim(),
  asin: d.asin.trim().toUpperCase(),
});

/** The fields that differ from the saved ones, in form order. */
export function changedFields(saved: Draft, now: Draft): Field[] {
  const a = norm(saved);
  const b = norm(now);
  return (["title", "authors", "series", "index", "asin"] as Field[]).filter((f) => a[f] !== b[f]);
}

/** What stops a save, by field, in plain words. */
export function problems(d: Draft): Partial<Record<Field, string>> {
  const out: Partial<Record<Field, string>> = {};
  if (!d.title.trim()) out.title = "A book needs a title.";
  const index = d.index.trim();
  if (d.series.trim() && index) {
    const n = Number(index);
    if (!Number.isFinite(n) || n < 0) out.index = "A number like 1, 2 or 2.5.";
  }
  const asin = d.asin.trim().toUpperCase();
  if (asin && !ASIN.test(asin)) out.asin = "An ASIN is 10 letters or digits.";
  return out;
}

/** The details written to the library copy. */
export function toDetails(d: Draft): BookDetails {
  const name = d.series.trim();
  const index = d.index.trim();
  return {
    title: d.title.trim(),
    authors: d.authors.map((a) => a.trim()).filter(Boolean),
    series: name ? { name, index: index ? Number(index) : null } : null,
  };
}
