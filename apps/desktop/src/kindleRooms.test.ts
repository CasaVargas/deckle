// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { better, canBeBetter, connectedBy, sentOn, continueReading, coverLine, freesLine, spareGroups, highlightsOf, onKindle, defaultSpare, kindWord, lastOpened, progressLine, shelfBooks, showCounts, spaceSlices, spareLine, tabCounts, worthALook } from "./kindleRooms";
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
  it("counts each Show choice, leaving the paper out", () => {
    expect(showCounts(books, "anywhere")).toEqual({ all: 3, reading: 1, "not-started": 1, finished: 1 });
    expect(showCounts(books, "amazon-own")).toEqual({ all: 1, reading: 0, "not-started": 0, finished: 1 });
  });

  it("shows papers only from Deckle, under All", () => {
    expect(shelfBooks(books, "all", "deckle", "", "title").map((b) => b.title)).toEqual(["Circe", "The Deckle Daily"]);
    expect(shelfBooks(books, "not-started", "deckle", "", "title").map((b) => b.title)).toEqual(["Circe"]);
  });

  it("sorts by recently read, then searches by author", () => {
    expect(shelfBooks(books, "all", "anywhere", "", "recent").map((b) => b.title)).toEqual(["Anathem", "Babel", "Circe"]);
    expect(shelfBooks(books, "all", "anywhere", "", "size").map((b) => b.title)).toEqual(["Babel", "Circe", "Anathem"]);
    expect(shelfBooks(books, "all", "anywhere", "kuang", "title").map((b) => b.title)).toEqual(["Babel"]);
  });

  it("says how far in and when", () => {
    const now = new Date("2026-10-23T12:00:00Z");
    expect(progressLine(books[0], now)).toBe("40% · opened 3 days ago");
    expect(progressLine(books[1], now)).toBe("Finished · opened 7 weeks ago");
    expect(progressLine(books[2], now)).toBe("Not started");
    expect(lastOpened("2026-03-02T10:00:00Z", now)).toBe("in March");
    expect(lastOpened("2025-03-02T10:00:00Z", now)).toBe("in March 2025");
  });

  it("names what a book is in plain words", () => {
    expect(kindWord(book({ cde: "EBOK" }))).toBe("Kindle book");
    expect(kindWord(book({ cde: "PDOC" }))).toBe("Document");
    expect(kindWord(books[1])).toBe("Amazon");
    expect(kindWord(books[3])).toBe("Paper");
    expect(kindWord(book({ format: "pdf" }))).toBe("PDF");
    expect(coverLine(books[0])).toBe("40% · AZW3");
    expect(coverLine(books[1])).toBe("Finished · Amazon");
    expect(coverLine(book({ cde: "PDOC" }))).toBe("Unread · Document");
    expect(coverLine(books[3])).toBe("Paper");
  });

  it("marks what Upgrades can do better", () => {
    expect(canBeBetter(books[0])).toBe(true);
    expect(canBeBetter(books[1])).toBe(false);
    expect(canBeBetter(book({ class: "current", updates: [{ older: true, feature: null, text: "t" }], library_source: "/l" }))).toBe(true);
    expect(canBeBetter(book({ class: "current", updates: [{ older: true, feature: null, text: "t" }] }))).toBe(false);
  });

  it("continues the books being read, most recent first, four at most", () => {
    const reading = ["2026-10-01", "2026-10-20", "2026-09-01", "2026-10-10", "2026-10-15"].map((d, i) =>
      book({ file: `${i}`, title: `R${i}`, reading: "reading", last_read: `${d}T10:00:00Z` }),
    );
    expect(continueReading([...reading, books[1], books[3]]).map((b) => b.title)).toEqual(["R1", "R4", "R3", "R0"]);
  });
});

describe("Worth a look and the tabs", () => {
  const shelf: DeviceShelf = {
    device: "Kindle",
    space: { total: 8e9, free: 2.1e9 },
    books,
    highlights: [
      { key: "a", title: "Piranesi", author: "", file: "", total: 9, new: 5, markdown: "" },
      { key: "b", title: "The Overstory", author: "", file: "", total: 3, new: 2, markdown: "" },
      { key: "c", title: "Circe", author: "", file: "", total: 4, new: 1, markdown: "" },
      { key: "d", title: "Old", author: "", file: "", total: 4, new: 0, markdown: "" },
    ],
    highlights_error: null,
    split: null,
    spare: [
      { file: "d1", title: "D", size: 24e6, has_sdr: true, why: "duplicate", keeping: "x" },
      { file: "f1", title: "F", size: 300e6, has_sdr: true, why: "finished" },
      { file: "f2", title: "G", size: 100e6, has_sdr: false, why: "finished" },
    ],
    backups: [],
    unread_positions: 0,
  };

  it("says what's worth a look, and where to fix it", () => {
    expect(worthALook(shelf, null)).toEqual([
      { key: "upgrades", title: "1 book can be upgraded", body: "X-Ray, Word Wise and page numbers, where Deckle can add them.", action: "See upgrades", room: "upgrades" },
      { key: "highlights", title: "8 new highlights", body: "From Piranesi, The Overstory and 1 more book.", action: "Save as Markdown", room: "highlights" },
      { key: "space", title: "424.0 MB you could free", body: "1 duplicate and 2 finished books.", action: "Make room", room: "device" },
    ]);
  });

  it("says nothing when there's nothing to say", () => {
    expect(worthALook({ ...shelf, books: [books[1]], highlights: [], spare: [] }, 0)).toEqual([]);
  });

  it("counts for the tabs, preferring the Upgrades room's own count", () => {
    expect(tabCounts(shelf, null)).toEqual({ upgrades: 1, highlights: "8 new" });
    expect(tabCounts(shelf, 4)).toEqual({ upgrades: 4, highlights: "8 new" });
    expect(tabCounts(null, null)).toEqual({ upgrades: 0, highlights: "" });
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
  expect(shelfBooks(rows, "all", "anywhere", "GUIN earthsea", "title")).toHaveLength(1);
  expect(shelfBooks(rows, "all", "anywhere", "guin absent", "title")).toHaveLength(0);
});

describe("a book's drawer", () => {
  it("offers the one thing Deckle can do better", () => {
    expect(better(book({ class: "current", updates: [{ older: true, feature: null, text: "Built by an older Deckle." }], library_source: "/l" }))).toMatchObject({ kind: "update", action: "Update", body: "Built by an older Deckle." });
    expect(better(book({ class: "has-source" }))?.body).toMatch(/^Its original is on this computer\./);
    expect(better(book({ class: "kindle-copy" }))?.body).toMatch(/^Deckle can rebuild it/);
    expect(better(book({ class: "current", library_source: "/l", cde: "PDOC" }))?.kind).toBe("as-kindle-book");
    expect(better(book({ class: "current", library_source: "/l", cde: "EBOK" }))).toBeNull();
    expect(better(books[1])).toBeNull();
  });

  it("says what it is on the Kindle in plain words", () => {
    expect(onKindle(book({ cde: "PDOC", from_deckle: true, library_source: "/l", size: 4.2e6 }))).toEqual([
      ["Shows up as", "Document, with its own cover"],
      ["X-Ray", "Not yet"],
      ["Word Wise", "Not yet"],
      ["Size", "4.2 MB"],
      ["Sent", "By Deckle · in your library"],
    ]);
    const locked = onKindle(book({ class: "locked", duplicate: true }));
    expect(locked[1]).toEqual(["X-Ray", "No: locked by DRM, which Deckle never opens"]);
    expect(locked.at(-1)).toEqual(["Duplicate", "Another copy of it is on this Kindle"]);
    expect(onKindle(books[1])[0]).toEqual(["Shows up as", "Book from Amazon"]);
  });

  it("finds a book's highlights by title", () => {
    const h = { key: "k", title: "anathem ", author: "", file: "", total: 2, new: 1, markdown: "" };
    expect(highlightsOf(books[0], [h])).toBe(h);
    expect(highlightsOf(books[2], [h])).toBeNull();
  });
});

describe("This Kindle, redesigned", () => {
  const spare: Spare[] = [
    { file: "f", title: "F", size: 300e6, has_sdr: true, why: "finished" },
    { file: "d1", title: "D", size: 24e6, has_sdr: true, why: "duplicate", keeping: "x" },
    { file: "p", title: "P", size: 12e6, has_sdr: false, why: "old-paper" },
    { file: "d2", title: "E", size: 22e6, has_sdr: false, why: "duplicate", keeping: "y" },
  ];

  it("groups Make room by why, with totals, finished last", () => {
    expect(spareGroups(spare).map((g) => [g.title, g.total, g.items.map((s) => s.file)])).toEqual([
      ["Duplicates", 46e6, ["d1", "d2"]],
      ["Old papers", 12e6, ["p"]],
      ["Finished, over 10 MB", 300e6, ["f"]],
    ]);
    expect(spareGroups([])).toEqual([]);
  });

  it("says what the ticked ones free", () => {
    expect(freesLine(spare, defaultSpare(spare))).toBe("3 ticked · frees 58.0 MB");
    expect(freesLine(spare, new Set())).toBe("Nothing ticked");
  });

  it("says how it's connected", () => {
    expect(connectedBy("mass_storage")).toBe("Connected over USB as a drive");
    expect(connectedBy("mtp")).toBe("Connected over USB as a media device");
  });
});

describe("unplugged", () => {
  it("dates a send, with the year when it isn't this one", () => {
    const now = new Date("2026-10-26T12:00:00Z");
    expect(sentOn("2026-10-12T09:00:00Z", now)).toBe("12 Oct");
    expect(sentOn("2025-03-02T09:00:00Z", now)).toBe("2 Mar 2025");
    expect(sentOn("nonsense", now)).toBe("");
  });
});
