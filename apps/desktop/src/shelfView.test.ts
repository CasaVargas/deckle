// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { emailLine, statusLine } from "./shelfView";
import type { ShelfStatus } from "./types";

const NOW = new Date("2026-10-22T14:00:00Z");
const TZ = "America/New_York";

const scribe: ShelfStatus = {
  kindle: "Kindle Scribe",
  seen: "2026-10-21T20:00:00Z",
  inProgress: 2,
  highlightsShared: false,
  wordsShared: false,
  emailOn: false,
  enabled: true,
};

test("a Scribe says what it doesn't share", () => {
  expect(statusLine(scribe, NOW, TZ)).toBe(
    "From your Kindle Scribe, Wednesday: 2 books in progress. Highlights and words aren't shared by this Kindle.",
  );
});

test("a Kindle that shares them says nothing more", () => {
  const pw = { ...scribe, kindle: "Kindle Paperwhite", inProgress: 1, highlightsShared: true, wordsShared: true };
  expect(statusLine(pw, NOW, TZ)).toBe("From your Kindle Paperwhite, Wednesday: 1 book in progress.");
});

test("an old snapshot says how many days ago", () => {
  const old = { ...scribe, seen: "2026-10-12T20:00:00Z" };
  expect(statusLine(old, NOW, TZ)).toContain("From your Kindle Scribe, 10 days ago:");
});

test("before any Kindle has been plugged in", () => {
  const none = { ...scribe, kindle: null, seen: null, inProgress: 0 };
  expect(statusLine(none, NOW, TZ)).toBe("Plug your Kindle in once to fill From Your Shelf.");
});

test("emailed papers pass through Amazon", () => {
  expect(emailLine({ ...scribe, emailOn: true })).toBe("Emailed papers include this page, so it passes through Amazon.");
  expect(emailLine(scribe)).toBeNull();
});
