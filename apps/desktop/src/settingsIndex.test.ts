// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { INDEX, PAGES, onlineLine, onlineOn, pageNotes, pageSummary, searchSettings, setupChecks, setupHeading } from "./settingsIndex";
import type { DeviceRow, Settings } from "./types";

const kindle = { key: "k", kind: "mass_storage", root: "/V", bus: 0, address: 0, model_hint: "Kindle Paperwhite", thumbnails_writable: true, cde_override: null, verified_real_asin: false, firmware: null, compat: [], sync_id: "s", name: "Paperwhite" } as DeviceRow;

describe("Search settings", () => {
  it("finds a setting by the words people type", () => {
    expect(searchSettings("asin").map((e) => e.label)).toEqual(["Match the store cover", "Keeps Deckle's X-Ray for real ASINs"]);
    expect(searchSettings("API key").map((e) => e.label)).toEqual(["API key"]);
    expect(searchSettings("calibre")[0].page).toBe("library");
    expect(searchSettings("clippings")[0].page).toBe("highlights");
    expect(searchSettings("casa vargas").map((e) => e.page)).toEqual(["about"]);
    expect(searchSettings("beltr")[0].label).toBe("Made by Casa Vargas");
    expect(searchSettings("previewer")[0].page).toBe("sending");
  });
  it("needs every word, and nothing for nothing", () => {
    expect(searchSettings("word wise dictionary").map((e) => e.label)).toEqual(["How many hints"]);
    expect(searchSettings("  ")).toEqual([]);
    expect(searchSettings("zebra")).toEqual([]);
  });
  it("only names pages that exist", () => {
    const ids = new Set(PAGES.map((p) => p.id));
    expect(INDEX.every((e) => ids.has(e.page))).toBe(true);
  });
});

describe("Overview", () => {
  const all = {
    previewer: { found: true, path: "/p", version: "3.88", download_url: "" },
    library: { root: "/Users/me/Books/Deckle", auto_add: true, books: 214 },
    sources: { find_in_calibre: true, calibre: "/c", folders: ["/a", "/b"] },
    highlights: "/Users/me/Notes/Highlights",
    devices: [kindle],
  };

  it("checks what's set up", () => {
    expect(setupChecks(all).map((c) => [c.key, c.mark, c.state])).toEqual([
      ["previewer", "ok", "found, so books can be converted"],
      ["library", "ok", "214 books in Deckle"],
      ["originals", "ok", "looking in calibre and 2 folders"],
      ["highlights", "ok", "written to Highlights"],
      ["kindles", "ok", "Paperwhite plugged in"],
    ]);
    expect(setupHeading(setupChecks(all))).toBe("Everything Deckle needs is set up.");
  });

  it("on the web, a missing Previewer is the reason, not a to-do", () => {
    const missing = { found: false, path: null, version: null, download_url: "" };
    const checks = setupChecks({ previewer: missing, library: null, sources: null, highlights: undefined, devices: [], web: true });
    expect(checks[0]).toEqual({ key: "previewer", title: "Kindle Previewer", state: "Kindle book conversion needs Kindle Previewer, which doesn't run on Linux.", mark: "optional", page: "sending" });
    expect(setupHeading(checks)).toBe("Everything Deckle needs is set up.");
  });

  it("says what's missing, and only the Previewer is a must", () => {
    const checks = setupChecks({ previewer: { found: false, path: null, version: null, download_url: "" }, library: { root: null, auto_add: false, books: 0 }, sources: { find_in_calibre: false, calibre: null, folders: [] }, highlights: null, devices: [] });
    expect(checks.map((c) => [c.key, c.mark, c.fix ?? ""])).toEqual([
      ["previewer", "todo", "Choose…"],
      ["library", "optional", "Choose folder…"],
      ["originals", "optional", "Set up…"],
      ["highlights", "optional", "Choose folder…"],
      ["kindles", "optional", ""],
    ]);
    expect(setupHeading(checks)).toBe("1 thing needs doing before Deckle can send books.");
    expect(pageNotes(checks)).toEqual({ sending: "to do" });
    expect(pageNotes(checks, ["Wikipedia summaries", "Open Library details"])).toEqual({ sending: "to do", online: "2 on" });
  });

  it("leaves out what hasn't loaded yet", () => {
    expect(setupChecks({ ...all, library: null, sources: null, highlights: undefined }).map((c) => c.key)).toEqual(["previewer", "kindles"]);
  });

  it("names what goes online", () => {
    expect(onlineOn({ metadata_fill: true, amazon_match: false, wikipedia: true, llm_provider: "anthropic" })).toEqual(["Open Library details", "Wikipedia summaries", "the AI model (Anthropic)"]);
    expect(onlineLine(["Open Library details", "Wikipedia summaries", "the AI model (Anthropic)"])).toBe(
      "3 features go online: Open Library details, Wikipedia summaries and the AI model (Anthropic). Everything else stays on this Mac.",
    );
    expect(onlineLine(["Wikipedia summaries"])).toBe("1 feature goes online: Wikipedia summaries. Everything else stays on this Mac.");
    expect(onlineLine([])).toBe("Nothing optional goes online. Everything happens on this Mac.");
  });
});

describe("page cards", () => {
  const s = { cde: "auto", pages_auto: true, typography: true, popup_notes: false, generated_cover: true, xray: true, llm_provider: "anthropic", wikipedia: true, word_wise: true, word_wise_level: 3 } as unknown as Settings;
  it("say each page's state in a line", () => {
    expect(pageSummary("sending", s, null, [], "0.7.0")).toBe("Automatic");
    expect(pageSummary("library", s, { root: "/b", auto_add: true, books: 2 }, [], "0.7.0")).toBe("2 books · adds every book you send");
    expect(pageSummary("library", s, { root: null, auto_add: false, books: 0 }, [], "0.7.0")).toBe("Off: books you send aren't kept");
    expect(pageSummary("books", s, null, [], "0.7.0")).toBe("Page numbers, tidy typography and covers when missing");
    expect(pageSummary("reading", s, null, [], "0.7.0")).toBe("Word Wise, 3 of 5 · X-Ray with an AI model and Wikipedia");
    expect(pageSummary("reading", { ...s, word_wise: false, xray: false }, null, [], "0.7.0")).toBe("All off");
    expect(pageSummary("reading", { ...s, chapter_recaps: true }, null, [], "0.7.0")).toBe("Word Wise, 3 of 5 · X-Ray with an AI model and Wikipedia · Chapter recaps");
    expect(pageSummary("reading", { ...s, chapter_recaps: true, llm_provider: "off", word_wise: false, xray: false }, null, [], "0.7.0")).toBe("All off");
    expect(pageSummary("reading", { ...s, book_summary: true, word_wise: false, xray: false }, null, [], "0.7.0")).toBe("Book summary");
    expect(pageSummary("reading", { ...s, chapter_recaps: true, book_summary: true, word_wise: false, xray: false }, null, [], "0.7.0")).toBe("Chapter recaps and a summary");
    expect(pageSummary("kindles", s, null, [kindle], "0.7.0")).toBe("Paperwhite");
    expect(pageSummary("online", { ...s, metadata_fill: false, amazon_match: false }, null, [], "0.7.0")).toBe("2 features on");
    expect(pageSummary("about", s, null, [], "0.7.0")).toBe("Deckle 0.7.0");
  });
});
