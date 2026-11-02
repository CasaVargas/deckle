// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { readsRtl, withDirection } from "./comicDirection";

const cbz = { path: "/c/Manga.cbz", comic_rtl: false };
const manga = { path: "/c/Marked.cbz", comic_rtl: true };

describe("comic reading direction", () => {
  it("is null for a book that isn't a comic", () => {
    expect(readsRtl({ path: "/b/Novel.epub", comic_rtl: null }, { "/b/Novel.epub": "rtl" })).toBeNull();
  });

  it("follows the book until the owner chooses", () => {
    expect(readsRtl(cbz, {})).toBe(false);
    expect(readsRtl(manga, {})).toBe(true);
  });

  it("puts the owner's choice first", () => {
    expect(readsRtl(cbz, { [cbz.path]: "rtl" })).toBe(true);
    expect(readsRtl(manga, { [manga.path]: "ltr" })).toBe(false);
  });

  it("keeps only choices that differ from the book", () => {
    const set = withDirection({}, cbz, true);
    expect(set).toEqual({ [cbz.path]: "rtl" });
    expect(withDirection(set, cbz, false)).toEqual({});
    expect(withDirection({}, manga, false)).toEqual({ [manga.path]: "ltr" });
  });

  it("leaves other books' choices alone", () => {
    expect(withDirection({ "/x.cbr": "rtl" }, cbz, true)).toEqual({ "/x.cbr": "rtl", [cbz.path]: "rtl" });
  });
});
