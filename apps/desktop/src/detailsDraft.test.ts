// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { changedFields, draftFrom, problems, toDetails, type Draft } from "./detailsDraft";
import type { CatalogEntry } from "./types";

const entry: CatalogEntry = {
  path: "/lib/a/book.epub", from: "library", title: "Leviathan Wakes", authors: ["James S. A. Corey"], series: { name: "The Expanse", index: 1 },
  isbn: null, library_dir: "/lib/a", sent_to: [], added: null, on_kindle: false,
};
const base: Draft = draftFrom(entry, { title: "ignored", authors: "ignored", asin: "B00ABCDEFG" });

test("from the library entry, with the queued book's ASIN", () =>
  expect(base).toEqual({ title: "Leviathan Wakes", authors: ["James S. A. Corey"], series: "The Expanse", index: "1", asin: "B00ABCDEFG" }));
test("from the queued book when there's no entry", () =>
  expect(draftFrom(null, { title: "T", authors: "A, B", asin: null })).toEqual({ title: "T", authors: ["A", "B"], series: "", index: "", asin: "" }));
test("a fractional series number keeps its fraction", () =>
  expect(draftFrom({ ...entry, series: { name: "S", index: 1.5 } }, null).index).toBe("1.5"));

test("nothing changed", () => expect(changedFields(base, { ...base })).toEqual([]));
test("whitespace isn't a change", () => expect(changedFields(base, { ...base, title: " Leviathan Wakes  ", authors: ["James S. A. Corey", " "] })).toEqual([]));
test("each changed field is named", () =>
  expect(changedFields(base, { ...base, title: "Leviathan", authors: [], index: "2", asin: "b00abcdefg" })).toEqual(["title", "authors", "index"]));
test("the ASIN compares without case", () => expect(changedFields(base, { ...base, asin: "" })).toEqual(["asin"]));

test("valid", () => expect(problems(base)).toEqual({}));
test("a title is needed", () => expect(problems({ ...base, title: "  " }).title).toBeTruthy());
test("a number must be zero or more", () => expect(problems({ ...base, index: "-1" }).index).toBeTruthy());
test("a number is fine empty", () => expect(problems({ ...base, index: "" })).toEqual({}));
test("a number without a series is ignored", () => expect(problems({ ...base, series: "", index: "x" })).toEqual({}));
test("an ASIN is 10 letters or digits", () => expect(problems({ ...base, asin: "B00" }).asin).toBeTruthy());

test("to the library's details", () =>
  expect(toDetails({ ...base, title: " T ", authors: [" A ", "", "B"], series: " S ", index: "" })).toEqual({ title: "T", authors: ["A", "B"], series: { name: "S", index: null } }));
test("no series", () => expect(toDetails({ ...base, series: "  ", index: "3" }).series).toBeNull());
