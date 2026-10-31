// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { daysLine, deliveryGroups, destinations, exportFolderHint, goesLine, stopLine, toggleDay } from "./dailyRooms";
import { newExport } from "./exportsView";
import type { DailyConfig, ReadershipView } from "./types";

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

test("press days toggle in week order, and say themselves", () => {
  expect(toggleDay([], "fri")).toEqual(["fri"]);
  expect(toggleDay(["fri", "mon"], "wed")).toEqual(["mon", "wed", "fri"]);
  expect(toggleDay(["mon", "wed"], "mon")).toEqual(["wed"]);
  expect(daysLine([])).toBe("Every day");
  expect(daysLine(["mon", "tue", "wed", "thu", "fri", "sat", "sun"])).toBe("Every day");
  expect(daysLine(["fri", "mon", "tue", "wed", "thu"])).toBe("Weekdays");
  expect(daysLine(["sun", "sat"])).toBe("Weekends");
  expect(daysLine(["mon", "wed", "fri"])).toBe("Mon, Wed and Fri");
  expect(daysLine(["sat"])).toBe("Sat");
});

test("where the paper goes, in one line", () => {
  const email = { enabled: true, to: "me@kindle.com", from: "", smtp_host: "", smtp_port: 587, username: "", usb_too: false };
  expect(goesLine(cfg())).toBe("To your Kindle over USB");
  expect(goesLine(cfg({ email }))).toBe("To your Kindle by email");
  expect(goesLine(cfg({ email: { ...email, usb_too: true } }))).toBe("To your Kindle by email and over USB");
  expect(goesLine(cfg({ email: { ...email, enabled: false } }))).toBe("To your Kindle over USB");
  const rm = { enabled: true, screen: "remarkable-2" as const, books: false };
  expect(goesLine(cfg({ remarkable: rm, export: [newExport("/Users/me/iCloud/Deckle Daily", [])] }))).toBe(
    "To your Kindle over USB, and to your reMarkable and Deckle Daily",
  );
});

test("where you usually stop, once there are three editions read", () => {
  const view = (opened: number, stopSection: string | null, enabled = true) =>
    ({ enabled, report: { window: 14, opened, medianPercent: 60, stopSection, sections: [] }, noticed: [] }) as unknown as ReadershipView;
  expect(stopLine(null)).toBeNull();
  expect(stopLine(view(5, "Sports"))).toBe("You usually stop in Sports");
  expect(stopLine(view(2, "Sports"))).toBeNull();
  expect(stopLine(view(5, null))).toBeNull();
  expect(stopLine(view(5, "Sports", false))).toBeNull();
});

test("Delivery: the Kindle by USB and email, then other readers with a row per folder", () => {
  const plain = deliveryGroups(cfg());
  expect(plain.kindle.map((r) => [r.key, r.status, r.on])).toEqual([
    ["usb", "Keeps 3 papers", null],
    ["email", "Off", false],
  ]);
  expect(plain.others.map((r) => r.key)).toEqual(["remarkable", "readers", "reader_server", "wireless"]);
  const full = deliveryGroups(
    cfg({
      keep: 1,
      email: { enabled: true, to: "", from: "", smtp_host: "", smtp_port: 587, username: "", usb_too: false },
      remarkable: { enabled: true, screen: "remarkable-paper-pro", books: false },
      export: [newExport("/Users/me/iCloud/Deckle Daily", []), { ...newExport("/x/Boox", []), format: "pdf", photos: "gray" }],
      reader_server: { enabled: true, port: 47220, username: "deckle" },
      wireless: { enabled: true, port: 9091, discovery: true, library_name: "Deckle", paper: true },
    }),
  );
  expect(full.kindle.map((r) => r.status)).toEqual(["Keeps 1 paper", "No address yet"]);
  expect(full.others.map((r) => [r.key, r.name, r.status, r.on])).toEqual([
    ["remarkable", "reMarkable", "reMarkable Paper Pro", true],
    ["folder:0", "Deckle Daily", "EPUB · color", null],
    ["folder:1", "Boox", "PDF · gray", null],
    ["readers", "Kobo and PocketBook", "Automatic", null],
    ["reader_server", "KOReader over Wi-Fi", "Port 47220", true],
    ["wireless", "KOReader's calibre connection", "Port 9091", true],
  ]);
});

test("on the web, the export-folder hint speaks of the server's folders", () => {
  expect(exportFolderHint(false)).toBe("iCloud Drive for an iPad, Dropbox for a Kobo or a Boox");
  expect(exportFolderHint(true)).toBe("A folder under /data your reader syncs from");
  expect(destinations(cfg(), true).find((r) => r.key === "exports")?.how).toBe(exportFolderHint(true));
  expect(destinations(cfg()).find((r) => r.key === "exports")?.how).toBe(exportFolderHint(false));
});
