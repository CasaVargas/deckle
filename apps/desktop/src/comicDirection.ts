// SPDX-License-Identifier: GPL-3.0-or-later
import type { BookRow, ReadingDirection } from "./types";

type Directions = Record<string, ReadingDirection>;

/** Whether the comic reads right to left when sent (#280): the owner's choice in settings, else
 * what the book says by itself. null: not a comic. */
export function readsRtl(book: Pick<BookRow, "path" | "comic_rtl">, directions: Directions): boolean | null {
  if (book.comic_rtl === null) return null;
  const set = directions[book.path];
  return set ? set === "rtl" : book.comic_rtl;
}

/** The settings' directions with this comic reading as asked. A choice the book already makes by
 * itself isn't kept, so a fixed ComicInfo.xml is followed again. */
export function withDirection(directions: Directions, book: Pick<BookRow, "path" | "comic_rtl">, rtl: boolean): Directions {
  const next = { ...directions };
  if (rtl === book.comic_rtl) delete next[book.path];
  else next[book.path] = rtl ? "rtl" : "ltr";
  return next;
}
