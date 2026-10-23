// SPDX-License-Identifier: GPL-3.0-or-later
import type { ShelfStatus } from "./types";

const DAY = 86_400_000;

/** The calendar day of an instant in `tz` (the viewer's zone by default), as UTC midnight. */
function dayOf(d: Date, tz?: string): number {
  return Date.parse(d.toLocaleDateString("en-CA", { timeZone: tz }));
}

/** "Wednesday" within the last week, else "10 days ago" (calendar days). `tz` is for tests. */
function when(seen: string, now: Date, tz?: string): string {
  const then = new Date(seen);
  const days = Math.round((dayOf(now, tz) - dayOf(then, tz)) / DAY);
  if (days > 6) return `${days} days ago`;
  return then.toLocaleDateString("en-US", { weekday: "long", timeZone: tz });
}

/** The line under the Shelf card's title (shelf design §7). */
export function statusLine(s: ShelfStatus, now: Date = new Date(), tz?: string): string {
  if (!s.kindle || !s.seen) return "Plug your Kindle in once to fill From Your Shelf.";
  const books = `${s.inProgress} ${s.inProgress === 1 ? "book" : "books"} in progress.`;
  const line = `From your ${s.kindle}, ${when(s.seen, now, tz)}: ${books}`;
  return s.highlightsShared || s.wordsShared ? line : `${line} Highlights and words aren't shared by this Kindle.`;
}

/** The warning for papers that go by email, which pass through Amazon. */
export function emailLine(s: ShelfStatus): string | null {
  return s.emailOn ? "Emailed papers include this page, so it passes through Amazon." : null;
}
