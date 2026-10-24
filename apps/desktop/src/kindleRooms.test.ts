// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { defaultSpare, filterCounts, kindleRoomNotes, lastOpened, progressLine, shelfBooks, spaceSlices, spareLine } from "./kindleRooms";
import type { DeviceBookRow, DeviceShelf, Spare } from "./types";

function book(over: Partial<DeviceBookRow>): DeviceBookRow {
  return {
    file: "x.azw3",
    size: 1_000_000,
    format: "azw3",
    title: "X",
    authors: [],
    cde: null,
    from_deckle: false,
    xray: false,
    word_wise: false,
    cover: null,
    library_source: null,
    updates: [],
    duplicate: false,
    class: "kindle-copy",
    kind: "sideloaded",
    fraction: null,
    reading: "not-started",
    last_read: null,
    ...over,
  };
}

const books = [
  book({ file: "a.azw3", title: "Anathem", authors: ["Stephenson"], fraction: 0.4, reading: "reading", last_read: "2026-10-20T10:00:00Z", size: 5 }),
  book({ file: "b.kfx", title: "Babel", authors: ["Kuang"], kind: "amazon-own", class: "amazon-own", reading: "finished", fraction: 1, last_read: "2026-09-01T10:00:00Z", size: 50 }),
  book({ file: "c.kfx", title: "Circe", authors: ["Miller"], kind: "deckle", class: "current", size: 9 }),
  book({ file: "deckle-daily-2026-10-23.kfx", title: "The Deckle Daily", kind: "paper", class: "current" }),
];

describe("the Shelf", () => {
  it("counts each filter, leaving the paper out of all but Deckle", () => {
    const c = filterCounts(books);
    expect(c.all).toBe(3);
    expect(c.reading).toBe(1);
    expect(c["not-started"]).toBe(1);
    expect(c.finished).toBe(1);
    expect(c["amazon-own"]).toBe(1);
    expect(c.deckle).toBe(2);
  });

  it("sorts by recently read, then searches by author", () => {
    expect(shelfBooks(books, "all", "", "recent").map((b) => b.title)).toEqual(["Anathem", "Babel", "Circe"]);
    expect(shelfBooks(books, "all", "", "size").map((b) => b.title)).toEqual(["Babel", "Circe", "Anathem"]);
    expect(shelfBooks(books, "all", "kuang", "title").map((b) => b.title)).toEqual(["Babel"]);
  });

  it("says how far in and when", () => {
    const now = new Date("2026-10-23T12:00:00Z");
    expect(progressLine(books[0], now)).toBe("40% · opened 3 days ago");
    expect(progressLine(books[1], now)).toBe("Finished · opened 7 weeks ago");
    expect(progressLine(books[2], now)).toBe("Not started");
    expect(lastOpened("2026-03-02T10:00:00Z", now)).toBe("in March");
    expect(lastOpened("2025-03-02T10:00:00Z", now)).toBe("in March 2025");
  });
});

describe("the rooms' notes", () => {
  it("count books, upgrades and free space", () => {
    const shelf: DeviceShelf = {
      device: "Kindle",
      space: { total: 8e9, free: 2.1e9 },
      books,
      highlights: [],
      highlights_error: null,
      split: null,
      spare: [],
      backups: [],
      unread_positions: 0,
    };
    const notes = kindleRoomNotes(shelf, null);
    expect(notes.shelf).toBe("3 books · 1 reading");
    expect(notes.upgrades).toBe("1 can be upgraded");
    expect(notes.device).toBe("2.1 GB free");
    expect(kindleRoomNotes(shelf, 0).upgrades).toBe("All current");
  });
});

describe("This Kindle", () => {
  const spare: Spare[] = [
    { file: "big.azw3", title: "Big", size: 20e6, has_sdr: true, why: "finished" },
    { file: "twin.azw3", title: "Twin", size: 2e6, has_sdr: false, why: "duplicate", keeping: "twin.kfx" },
    { file: "deckle-daily-2026-10-01.kfx", title: "Paper", size: 3e6, has_sdr: false, why: "old-paper" },
  ];

  it("ticks spare copies and old papers, not finished books", () => {
    expect([...defaultSpare(spare)]).toEqual(["twin.azw3", "deckle-daily-2026-10-01.kfx"]);
    expect(spareLine(spare[0])).toBe("Finished, 20.0 MB. Its reading position goes with it.");
    expect(spareLine(spare[1])).toBe("Another copy stays: twin.kfx.");
  });

  it("splits the space bar, free last", () => {
    const s = spaceSlices({ total: 100, free: 40, kinds: [["amazon-own", 30, 3], ["sideloaded", 15, 1]], other: 15 });
    expect(s.map((x) => [x.key, x.pct])).toEqual([
      ["amazon-own", 30],
      ["sideloaded", 15],
      ["other", 15],
      ["free", 40],
    ]);
    expect(s[0].label).toBe("Amazon's own · 3 books");
  });
});

it("matches Kindle titles and authors across terms, ignoring accents", () => {
  const rows = [book({ title: "Éarthsea", authors: ["Ursula Le Guin"] })];
  expect(shelfBooks(rows, "all", "GUIN earthsea", "title")).toHaveLength(1);
  expect(shelfBooks(rows, "all", "guin absent", "title")).toHaveLength(0);
});
