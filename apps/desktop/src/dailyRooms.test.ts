// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { destinations, roomNotes } from "./dailyRooms";
import { newExport } from "./exportsView";
import type { DailyConfig, NextPaper, ReadingEntry, WeekState } from "./types";

const cfg = (p: Partial<DailyConfig> = {}): DailyConfig => ({
  title: "T",
  place: "",
  latitude: null,
  longitude: null,
  units: "imperial",
  keep: 3,
  max_age_hours: 36,
  full_text: true,
  photos: true,
  max_photos: 4,
  sections: ["reading", "weather", "local"],
  local: { google_news: "", feeds: [], max_items: 12, max_age_hours: 72 },
  feeds: [],
  lottery: { games: [] },
  horoscope: { signs: [] },
  reading: { inbox: "", max_items: 10, long_read_words: 4000 },
  ...p,
});

const next = (at: string | null, week: WeekState[], stopped: string | null = null): NextPaper => ({
  press: { at, next: at ? "2026-10-24T05:30:00-04:00" : null, edition: "2026-10-24", stopped, since: "2026-10-23T05:30:00-04:00" },
  paper: "The Deckle Daily",
  place: "",
  date: "2026-10-24",
  number: 296,
  weather: true,
  lead: null,
  tiles: [],
  checked: null,
  plan: [],
  off: [],
  pages: 0,
  week: week.map((state, i) => ({ date: `2026-10-${String(17 + i).padStart(2, "0")}`, state })),
});

const entry = (id: string, state: ReadingEntry["state"]): ReadingEntry =>
  ({ id, url: null, file: null, title: null, saved_at: "", source: "", state, tries: 0 }) as ReadingEntry;

test("a new paper goes to the Kindle and nowhere else", () => {
  const rows = destinations(cfg());
  expect(rows.map((r) => r.key)).toEqual(["kindle", "email", "remarkable", "exports", "readers"]);
  expect(rows.filter((r) => r.on).map((r) => r.key)).toEqual(["kindle", "readers"]);
  expect(rows.find((r) => r.key === "kindle")?.status).toBe("Keeps 3 editions");
  expect(rows.find((r) => r.key === "exports")?.status).toBe("None");
});

test("each place says what it's set to", () => {
  const one = newExport("/Users/me/iCloud/Deckle Daily", []);
  const rows = destinations(
    cfg({
      keep: 1,
      email: { enabled: true, to: "me@kindle.com", from: "", smtp_host: "", smtp_port: 587, username: "", usb_too: false },
      remarkable: { enabled: true, screen: "remarkable-paper-pro", books: false },
      export: [one],
    }),
  );
  const status = Object.fromEntries(rows.map((r) => [r.key, r.status]));
  expect(status).toEqual({ kindle: "Keeps 1 edition", email: "me@kindle.com", remarkable: "reMarkable Paper Pro", exports: "Deckle Daily", readers: "Automatic" });
  expect(destinations(cfg({ export: [one, newExport("/x/Boox", [one])] })).find((r) => r.key === "exports")?.status).toBe("2 folders");
});

test("the rooms' notes", () => {
  const week: WeekState[] = [{ state: "read", percent: 80 }, { state: "notOpened" }, { state: "read", percent: 20 }, { state: "noPaper" }];
  const notes = roomNotes(
    cfg({ remarkable: { enabled: true, screen: "remarkable-2", books: false } }),
    next("05:30", week),
    [entry("a", "pending"), entry("b", "printed"), entry("c", "pending")],
    false,
  );
  expect(notes).toEqual({
    today: "Press at 05:30",
    editions: "2 of the last 4 read",
    reading: "2 saved for tomorrow",
    delivery: "Kindle and 1 more",
    paper: "3 sections",
  });
});

test("the notes before the paper is known, and with the presses stopped or unsaved changes", () => {
  expect(roomNotes(cfg(), null, [], true)).toEqual({ today: "", editions: "", reading: "Nothing saved", delivery: "Kindle", paper: "Unsaved changes" });
  expect(roomNotes(cfg(), next("05:30", [], "2026-10-24"), [], false).today).toBe("Presses stopped");
  expect(roomNotes(cfg(), next(null, []), [], false).today).toBe("No press time set");
});
