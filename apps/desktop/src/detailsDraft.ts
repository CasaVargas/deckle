// SPDX-License-Identifier: GPL-3.0-or-later
import type { BookDetails, BookState, CatalogEntry } from "./types";

/** The book details editor's form (Books tab redesign §5), as the person typed it. */
export interface Draft {
  title: string;
  authors: string[];
  series: string;
  /** The series number as typed; checked by `problems`. */
  index: string;
  description: string;
  tags: string[];
  publisher: string;
  published: string;
  language: string;
  isbn: string;
  titleSort: string;
  authorSort: string;
  /** Kept with the queued book, not the library copy. */
  asin: string;
}

export type Field = keyof Draft;

const FIELDS: Field[] = ["title", "authors", "series", "index", "description", "tags", "publisher", "published", "language", "isbn", "titleSort", "authorSort", "asin"];
const ASIN = /^[A-Z0-9]{10}$/;
const DATE = /^\d{4}(-\d{2}(-\d{2})?)?$/;
const LANGUAGE = /^[A-Za-z]{2,3}(-[A-Za-z0-9]{2,8})*$/;

/** An ISBN-10 or -13 without hyphens or spaces, or null when it isn't one (as the library checks). */
export function normalIsbn(s: string): string | null {
  const n = s.replace(/[-\s]/g, "").toUpperCase();
  return /^\d{13}$/.test(n) || /^\d{9}[\dX]$/.test(n) ? n : null;
}

/**
 * The form for a book: what its library copy's OPF says when that's been read, else its library
 * entry, else the queued book.
 */
export function draftFrom(entry: CatalogEntry | null, book: Pick<BookState, "title" | "authors" | "asin"> | null, read?: BookDetails | null): Draft {
  const authors = read?.authors ?? entry?.authors ?? (book?.authors ?? "").split(",");
  const series = read ? read.series : entry?.series;
  const index = series?.index;
  return {
    title: read?.title ?? entry?.title ?? book?.title ?? "",
    authors: authors.map((a) => a.trim()).filter(Boolean),
    series: series?.name ?? "",
    index: index != null ? String(index) : "",
    description: read?.description ?? "",
    tags: (read?.tags ?? []).map((t) => t.trim()).filter(Boolean),
    publisher: read?.publisher ?? "",
    published: read?.published ?? "",
    language: read?.language ?? "",
    isbn: read?.isbn ?? entry?.isbn ?? "",
    titleSort: read?.title_sort ?? "",
    authorSort: read?.author_sort ?? "",
    asin: book?.asin ?? "",
  };
}

const list = (xs: string[]) => xs.map((x) => x.trim()).filter(Boolean).join("\n");
const value = (d: Draft, f: Field): string => {
  if (f === "authors" || f === "tags") return list(d[f]);
  if (f === "asin") return d.asin.trim().toUpperCase();
  if (f === "isbn") return normalIsbn(d.isbn) ?? d.isbn.trim();
  return d[f].trim();
};

/** The fields that differ from the saved ones, in form order. */
export function changedFields(saved: Draft, now: Draft): Field[] {
  return FIELDS.filter((f) => value(saved, f) !== value(now, f));
}

/** What stops a save, by field, in plain words (the library refuses the same things). */
export function problems(d: Draft): Partial<Record<Field, string>> {
  const out: Partial<Record<Field, string>> = {};
  if (!d.title.trim()) out.title = "A book needs a title.";
  const index = d.index.trim();
  if (d.series.trim() && index) {
    const n = Number(index);
    if (!Number.isFinite(n) || n < 0) out.index = "A number like 1, 2 or 2.5.";
  }
  if (d.published.trim() && !DATE.test(d.published.trim())) out.published = "A year, or a date like 2020-09-15.";
  if (d.language.trim() && !LANGUAGE.test(d.language.trim())) out.language = "A code like en or en-GB.";
  if (d.isbn.trim() && !normalIsbn(d.isbn)) out.isbn = "10 or 13 digits.";
  const asin = d.asin.trim().toUpperCase();
  if (asin && !ASIN.test(asin)) out.asin = "An ASIN is 10 letters or digits.";
  return out;
}

/** The details written to the library copy: every field, so an emptied one is removed. */
export function toDetails(d: Draft): BookDetails {
  const name = d.series.trim();
  const index = d.index.trim();
  return {
    title: d.title.trim(),
    authors: d.authors.map((a) => a.trim()).filter(Boolean),
    series: name ? { name, index: index ? Number(index) : null } : null,
    description: d.description.trim(),
    tags: d.tags.map((t) => t.trim()).filter(Boolean),
    publisher: d.publisher.trim(),
    published: d.published.trim(),
    language: d.language.trim(),
    isbn: d.isbn.trim() ? normalIsbn(d.isbn) ?? d.isbn.trim() : "",
    title_sort: d.titleSort.trim(),
    author_sort: d.authorSort.trim(),
  };
}
