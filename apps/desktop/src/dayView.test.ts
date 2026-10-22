// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, test } from "vitest";
import { DEFAULT_DAY, addressError, previewLine, withoutCalendar } from "./dayView";

describe("Your Day card", () => {
  test("preview line counts today's events", () => {
    expect(previewLine({ events: 4, labels: [], errors: [] })).toBe("Today: 4 events");
    expect(previewLine({ events: 1, labels: [], errors: [] })).toBe("Today: 1 event");
    expect(previewLine({ events: 0, labels: [], errors: [] })).toBe("Today: nothing on the calendar");
  });

  test("preview line mentions calendars that couldn't be read", () => {
    expect(previewLine({ events: 2, labels: [], errors: ["https://x/…: timeout"] })).toBe(
      "Today: 2 events · 1 calendar unavailable",
    );
    expect(previewLine({ events: 0, labels: [], errors: ["a", "b"] })).toBe(
      "Today: nothing on the calendar · 2 calendars unavailable",
    );
  });

  test("no preview yet is blank", () => {
    expect(previewLine(null)).toBe("");
  });

  test("only https and webcal addresses are accepted", () => {
    expect(addressError("https://calendar.google.com/x/basic.ics")).toBeNull();
    expect(addressError("webcal://p01-caldav.icloud.com/published/2/abc")).toBeNull();
    expect(addressError("  ")).toBe("Paste the calendar's address.");
    expect(addressError("http://example.com/cal.ics")).toBe("Use an https:// or webcal:// address.");
    expect(addressError("calendar.google.com")).toBe("Use an https:// or webcal:// address.");
  });

  test("defaults match digest.toml", () => {
    expect(DEFAULT_DAY).toEqual({ calendars: [], max_items: 8, weather_note: true });
  });
});

describe("removing a calendar", () => {
  test("drops that row's label right away, so no row shows another's", () => {
    const p = { events: 3, labels: ["Home.ics", "https://a.example/…", "Work.ics"], errors: ["https://a.example/…: timed out"] };
    expect(withoutCalendar(p, 1)).toEqual({ events: 3, labels: ["Home.ics", "Work.ics"], errors: ["https://a.example/…: timed out"] });
    expect(withoutCalendar(null, 0)).toBeNull();
  });
});
