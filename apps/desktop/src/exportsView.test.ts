// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { exportParts, folderName, hasFolder, iCloudDrive, newExport } from "./exportsView";

const icloud = "/Users/me/Library/Mobile Documents/com~apple~CloudDocs/Deckle Daily";

test("a new export is named after its folder, in color, keeping a week", () => {
  expect(newExport(icloud, [])).toEqual({ name: "Deckle Daily", dir: icloud, format: "epub", photos: "color", keep: 7, books: false });
});

test("a taken name gets a number", () => {
  const first = newExport(icloud, []);
  const second = newExport("/Volumes/Other/Deckle Daily", [first]);
  expect(second.name).toBe("Deckle Daily 2");
  expect(newExport("/x/Deckle Daily", [first, second]).name).toBe("Deckle Daily 3");
});

test("folder names work with either separator and a root falls back", () => {
  expect(folderName("C:\\Users\\me\\Dropbox\\Boox\\")).toBe("Boox");
  expect(folderName("/")).toBe("");
  expect(newExport("/", []).name).toBe("Folder");
});

test("the same folder isn't added twice", () => {
  const e = newExport(icloud, []);
  expect(hasFolder([e], `${icloud}/`)).toBe(true);
  expect(hasFolder([e], "/elsewhere")).toBe(false);
});

test("each export is one sentence of the result", () => {
  expect(exportParts(undefined)).toEqual([]);
  expect(
    exportParts([
      { name: "iPad", dir: "/a", written: ["/a/x.epub"], pruned: [] },
      { name: "Boox", dir: "/b", written: [], pruned: [], error: "/b: no such folder" },
    ]),
  ).toEqual(["Put in iPad.", "Not put in Boox: /b: no such folder."]);
});

test("iCloud Drive is under the home folder", () => {
  expect(iCloudDrive("/Users/me/")).toBe("/Users/me/Library/Mobile Documents/com~apple~CloudDocs");
});
