// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { backupLine, lastReadLabel, progressLabel, readerBooks, readerLine, removeConfirm } from "./readerRooms";
import type { ReaderBook, ReaderRow } from "./types";

const book = (over: Partial<ReaderBook>): ReaderBook => ({ id: "x", title: "X", authors: [], folder: "", format: "epub", bytes: 1, percent: null, finished: false, last_read: null, removable: true, note: null, ...over });
const kobo: ReaderRow = { key: "kobo/N1", kind: "kobo", name: "Kobo", connection: "USB drive", detail: "Firmware 4.38", can_remove: true, remove_note: null };

describe("a reader's library", () => {
  const books = [
    book({ id: "a", title: "Emma", authors: ["Jane Austen"], bytes: 5, last_read: "2026-11-01T08:00:00Z" }),
    book({ id: "b", title: "dune", folder: "Sci-fi", bytes: 9, last_read: "2026-11-02T08:00:00Z" }),
    book({ id: "c", title: "Beowulf", bytes: 2 }),
  ];
  it("searches titles, authors and folders and sorts three ways", () => {
    expect(readerBooks(books, "", "recent").map((b) => b.id)).toEqual(["b", "a", "c"]);
    expect(readerBooks(books, "", "title").map((b) => b.id)).toEqual(["c", "b", "a"]);
    expect(readerBooks(books, "", "size").map((b) => b.id)).toEqual(["b", "a", "c"]);
    expect(readerBooks(books, "austen", "recent").map((b) => b.id)).toEqual(["a"]);
    expect(readerBooks(books, "sci", "recent").map((b) => b.id)).toEqual(["b"]);
  });
  it("says how far and when", () => {
    expect(progressLabel(book({ finished: true, percent: 100 }))).toBe("Finished");
    expect(progressLabel(book({ percent: 41.6 }))).toBe("42%");
    expect(progressLabel(book({ percent: 0 }))).toBe("Not started");
    expect(progressLabel(book({}))).toBe("");
    const now = new Date("2026-11-03T12:00:00Z");
    expect(lastReadLabel("2026-11-02T08:00:00Z", now)).toBe("Read Nov 2");
    expect(lastReadLabel("2025-01-09T08:00:00Z", now)).toBe("Read Jan 9, 2025");
    expect(lastReadLabel(null, now)).toBe("");
  });
  it("warns before a remove the way each reader needs", () => {
    expect(removeConfirm(kobo, 2)).toMatch(/^Remove these 2 books from the Kobo\? Its database is copied/);
    expect(removeConfirm({ ...kobo, kind: "remarkable", name: "reMarkable" }, 1)).toMatch(/trash, with their handwriting/);
    expect(removeConfirm({ ...kobo, kind: "pocketbook", name: "PocketBook" }, 1)).toMatch(/^Remove this book from the PocketBook\?/);
    expect(backupLine(kobo, null)).toMatch(/database.*Not backed up yet\.$/);
    expect(backupLine(kobo, "/x/reader-backups/kobo-N1/2026-11-03T08-00-00Z")).toMatch(/Last backup: 2026-11-03T08-00-00Z$/);
    expect(readerLine(kobo)).toBe("USB drive · Firmware 4.38");
  });
});
