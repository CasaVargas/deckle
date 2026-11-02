// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { STEPS, clock, dateline, firstSave, mainAt, saveLine, scheduleLine } from "./onboarding";
import type { ReadingEntry, ScheduleStatus } from "./types";

const entry = (p: Partial<ReadingEntry>): ReadingEntry => ({
  id: "x",
  url: "https://ex.com/a",
  file: null,
  title: "A story",
  saved_at: "2026-11-01T10:00:00Z",
  source: "bookmarklet",
  state: "pending",
  tries: 0,
  printed_in: null,
  note: null,
  ...p,
});

test("four pages, the paper before the bookmarklet", () => {
  expect(STEPS.map((s) => s.key)).toEqual(["welcome", "paper", "save", "where"]);
});

test("the clock drops a leading zero and leaves anything else alone", () => {
  expect(clock("05:30")).toBe("5:30");
  expect(clock("12:05")).toBe("12:05");
  expect(clock("")).toBe("");
});

test("the schedule line follows the main paper's job and the watcher", () => {
  const off: ScheduleStatus = { at: null, watch: false, exe: null, papers: [] };
  expect(scheduleLine(null, false)).toMatch(/^Off\./);
  expect(scheduleLine(off, false)).toMatch(/^Off\./);
  const on: ScheduleStatus = { at: "05:30", watch: true, exe: "/x", papers: [{ slug: "main", at: "06:15", days: [] }] };
  expect(mainAt(on)).toBe("06:15");
  expect(scheduleLine(on, false)).toBe("Every morning at 6:15. The watcher that delivers it, and takes what you save, runs from login.");
  expect(scheduleLine({ ...on, watch: false }, false)).toMatch(/watcher isn't installed/);
  // The web app is the container: never a button, always on.
  expect(scheduleLine(off, true)).toMatch(/container/);
});

test("the first save is a bookmarklet entry since the page opened, newest first", () => {
  const since = "2026-11-01T09:30:00Z";
  expect(firstSave([], since)).toBeNull();
  expect(firstSave([entry({ saved_at: "2026-11-01T09:00:00Z" })], since)).toBeNull();
  expect(firstSave([entry({ source: "inbox" })], since)).toBeNull();
  const got = firstSave([entry({ id: "old", saved_at: "2026-11-01T09:40:00Z" }), entry({ id: "new", saved_at: "2026-11-01T09:50:00Z" })], since);
  expect(got?.id).toBe("new");
});

test("the check line names the page once it landed", () => {
  expect(saveLine(null)).toMatch(/^Try it/);
  expect(saveLine(entry({}))).toBe('"A story" is in tomorrow\'s paper.');
  expect(saveLine(entry({ title: " " }))).toBe('"https://ex.com/a" is in tomorrow\'s paper.');
});

test("the dateline reads like a paper's", () => {
  expect(dateline(new Date(2026, 10, 3, 9))).toBe("Tuesday, 3 November 2026");
});
