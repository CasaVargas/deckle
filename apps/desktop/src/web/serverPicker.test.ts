// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { pickerView, saveTarget, type Listing } from "./serverFiles";

const listing = (path: string, parent: string | null): Listing => ({
  path,
  parent,
  entries: [
    { name: "notes.md", path: `${path}/notes.md`, dir: false, size: 10 },
    { name: "Books", path: `${path}/Books`, dir: true, size: 0 },
    { name: "book.EPUB", path: `${path}/book.EPUB`, dir: false, size: 20 },
    { name: "archive", path: `${path}/archive`, dir: true, size: 0 },
  ],
});

test("server_picker_lists_dirs_first_and_climbs_to_parent_not_above_root", () => {
  const roots = ["/data", "/config"];
  // Inside a root: folders first, files filtered by extension, up to the parent.
  const inside = pickerView(listing("/data/Books", "/data"), roots, { mode: "file", extensions: ["epub"] });
  expect(inside.rows.map((r) => r.name)).toEqual(["Books", "archive", "book.EPUB"]);
  expect(inside.up).toBe("/data");
  // At a root: up goes back to the list of roots, never to "/".
  const atRoot = pickerView(listing("/data", "/"), roots, { mode: "file" });
  expect(atRoot.up).toBe("roots");
  // One root only: nothing above it.
  expect(pickerView(listing("/data", null), ["/data"], { mode: "file" }).up).toBeNull();
  // A parent outside every root is never offered.
  expect(pickerView(listing("/elsewhere/x", "/elsewhere"), roots, { mode: "file" }).up).toBeNull();
  // Folder mode lists folders only.
  expect(pickerView(listing("/data/Books", "/data"), roots, { mode: "folder" }).rows.map((r) => r.name)).toEqual(["Books", "archive"]);
});

test("save_target_prefers_data_exports", () => {
  expect(saveTarget(["/config", "/data"], "Kindle words.csv")).toBe("/data/Deckle/Exports/Kindle words.csv");
  expect(saveTarget(["/tmp/x/"], "a.csv")).toBe("/tmp/x/Deckle/Exports/a.csv");
  expect(saveTarget([], "a.csv")).toBeNull();
});
