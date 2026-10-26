// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { changedFields, draftFrom, normalIsbn, problems, toDetails, type Draft } from "./detailsDraft";
import type { BookDetails, CatalogEntry } from "./types";

const entry: CatalogEntry = {
  path: "/lib/a/book.epub", from: "library", title: "Leviathan Wakes", authors: ["James S. A. Corey"], series: { name: "The Expanse", index: 1 },
  isbn: null, library_dir: "/lib/a", sent_to: [], added: null, on_kindle: false,
};
const empty = { description: "", tags: [], publisher: "", published: "", language: "", isbn: "", titleSort: "", authorSort: "" };
const base: Draft = draftFrom(entry, { title: "ignored", authors: "ignored", asin: "B00ABCDEFG" });
const read: BookDetails = {
  title: "Leviathan Wakes", authors: ["James S. A. Corey"], series: { name: "The Expanse", index: 1 },
  description: "<p>Blurb</p>", tags: ["SF", " "], publisher: "Orbit", published: "2011", language: "en", isbn: "9780316129084", title_sort: "Leviathan Wakes", author_sort: "Corey, James S. A.",
};

test("from the library entry, with the queued book's ASIN", () =>
  expect(base).toEqual({ title: "Leviathan Wakes", authors: ["James S. A. Corey"], series: "The Expanse", index: "1", ...empty, asin: "B00ABCDEFG" }));
test("from the queued book when there's no entry", () =>
  expect(draftFrom(null, { title: "T", authors: "A, B", asin: null })).toEqual({ title: "T", authors: ["A", "B"], series: "", index: "", ...empty, asin: "" }));
test("a fractional series number keeps its fraction", () =>
  expect(draftFrom({ ...entry, series: { name: "S", index: 1.5 } }, null).index).toBe("1.5"));
test("what the copy's OPF says wins", () =>
  expect(draftFrom({ ...entry, title: "Stale" }, null, read)).toMatchObject({ title: "Leviathan Wakes", description: "<p>Blurb</p>", tags: ["SF"], published: "2011", isbn: "9780316129084", authorSort: "Corey, James S. A." }));
test("an OPF with no series clears the entry's", () => expect(draftFrom(entry, null, { ...read, series: null }).series).toBe(""));

test("nothing changed", () => expect(changedFields(base, { ...base })).toEqual([]));
test("whitespace isn't a change", () => expect(changedFields(base, { ...base, title: " Leviathan Wakes  ", authors: ["James S. A. Corey", " "] })).toEqual([]));
test("each changed field is named", () =>
  expect(changedFields(base, { ...base, title: "Leviathan", authors: [], index: "2", tags: ["SF"], titleSort: "x", asin: "b00abcdefg" })).toEqual(["title", "authors", "index", "tags", "titleSort"]));
test("the ASIN compares without case", () => expect(changedFields(base, { ...base, asin: "" })).toEqual(["asin"]));
test("an ISBN's hyphens aren't a change", () =>
  expect(changedFields({ ...base, isbn: "9780316129084" }, { ...base, isbn: "978-0-316-12908-4" })).toEqual([]));

test("valid", () => expect(problems(base)).toEqual({}));
test("a title is needed", () => expect(problems({ ...base, title: "  " }).title).toBeTruthy());
test("a number must be zero or more", () => expect(problems({ ...base, index: "-1" }).index).toBeTruthy());
test("a number is fine empty", () => expect(problems({ ...base, index: "" })).toEqual({}));
test("a number without a series is ignored", () => expect(problems({ ...base, series: "", index: "x" })).toEqual({}));
test("an ASIN is 10 letters or digits", () => expect(problems({ ...base, asin: "B00" }).asin).toBeTruthy());
test("published is a year or a date", () => {
  expect(problems({ ...base, published: "2020" })).toEqual({});
  expect(problems({ ...base, published: "2020-09-15" })).toEqual({});
  expect(problems({ ...base, published: "Sept 2020" }).published).toBeTruthy();
});
test("a language is a code", () => {
  expect(problems({ ...base, language: "en-GB" })).toEqual({});
  expect(problems({ ...base, language: "English" }).language).toBeTruthy();
});
test("an ISBN is 10 or 13 digits", () => {
  expect(problems({ ...base, isbn: "0-306-40615-x" })).toEqual({});
  expect(problems({ ...base, isbn: "12345" }).isbn).toBeTruthy();
});
test("normal ISBNs", () => {
  expect(normalIsbn("978-1-63557-563-7")).toBe("9781635575637");
  expect(normalIsbn("0 306 40615 x")).toBe("030640615X");
  expect(normalIsbn("97816")).toBeNull();
});

test("to the library's details: every field, emptied ones as empty", () =>
  expect(toDetails({ ...base, title: " T ", authors: [" A ", "", "B"], series: " S ", index: "", tags: ["x", " "], isbn: "978-1-63557-563-7" })).toEqual({
    title: "T", authors: ["A", "B"], series: { name: "S", index: null },
    description: "", tags: ["x"], publisher: "", published: "", language: "", isbn: "9781635575637", title_sort: "", author_sort: "",
  }));
test("no series", () => expect(toDetails({ ...base, series: "  ", index: "3" }).series).toBeNull());
