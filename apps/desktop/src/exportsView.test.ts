// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { exportParts, folderName, hasFolder, iCloudDrive, isKoboDropbox, newExport, remarkablePart } from "./exportsView";

const icloud = "/Users/me/Library/Mobile Documents/com~apple~CloudDocs/Deckle Daily";

test("a new export is named after its folder, in color, keeping a week", () => {
  expect(newExport(icloud, [])).toEqual({ name: "Deckle Daily", dir: icloud, format: "epub", photos: "color", screen: "remarkable-2", keep: 7, books: false });
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

test("a reMarkable's part of the result", () => {
  expect(remarkablePart(undefined)).toBeNull();
  expect(remarkablePart({ remarkable_queued: true })).toMatch(/Waiting for your reMarkable/);
  expect(remarkablePart({ remarkable_queued: true, remarkable: { uploaded: ["a.pdf"], errors: [] } })).toBe("Sent to your reMarkable.");
  expect(remarkablePart({ remarkable: { uploaded: [], errors: ["no answer"] } })).toBe("reMarkable: no answer");
});

test("Dropbox's Kobo folder gets a gray KEPUB named Kobo", () => {
  const mac = "/Users/me/Dropbox/Apps/Rakuten Kobo";
  expect(newExport(mac, [])).toMatchObject({ name: "Kobo", dir: mac, format: "kepub", photos: "gray" });
  expect(newExport("C:\\Users\\me\\Dropbox\\Apps\\Rakuten Kobo\\", []).format).toBe("kepub");
  expect(newExport(mac, [newExport(mac, [])]).name).toBe("Kobo 2");
});

test("only the Rakuten Kobo app folder counts as a Kobo's", () => {
  expect(isKoboDropbox("/Users/me/Dropbox/Apps/Rakuten Kobo/")).toBe(true);
  expect(isKoboDropbox("/Users/me/Dropbox/Apps/rakuten kobo")).toBe(true);
  expect(isKoboDropbox("/Users/me/Dropbox/Apps/Rakuten Kobo/Old")).toBe(false);
  expect(isKoboDropbox("/Users/me/Dropbox/Kobo")).toBe(false);
  expect(newExport("/Users/me/Dropbox/Boox", []).format).toBe("epub");
});
