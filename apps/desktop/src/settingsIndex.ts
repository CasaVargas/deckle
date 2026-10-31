// SPDX-License-Identifier: GPL-3.0-or-later
import { PREVIEWER_REASON } from "./web/gates";
import type { DeviceRow, LibraryStatus, PreviewerStatus, Settings, SourcesStatus } from "./types";

/** The Settings pages (Settings redesign §2). */
export type SettingsPage = "overview" | "sending" | "library" | "books" | "reading" | "kindles" | "koreader" | "highlights" | "online" | "about";

export const PAGES: { id: SettingsPage; title: string }[] = [
  { id: "overview", title: "Overview" },
  { id: "sending", title: "Sending" },
  { id: "library", title: "Library" },
  { id: "books", title: "Books" },
  { id: "reading", title: "Reading aids" },
  { id: "kindles", title: "Kindles" },
  { id: "koreader", title: "KOReader" },
  { id: "highlights", title: "Highlights & reading" },
  { id: "online", title: "Online & privacy" },
  { id: "about", title: "About" },
];

/** One setting, for Search settings: its label, its page, and words people might type for it. */
export interface Entry {
  label: string;
  page: SettingsPage;
  words: string;
}

export const INDEX: Entry[] = [
  { label: "Kindle Previewer", page: "sending", words: "previewer amazon convert kpf location path" },
  { label: "Library folder", page: "library", words: "library folder keep copy" },
  { label: "Add every book I send", page: "library", words: "library auto add" },
  { label: "Find originals in calibre", page: "library", words: "calibre originals upgrades source" },
  { label: "Find originals in folders", page: "library", words: "folders originals upgrades source epub" },
  { label: "Watch a folder", page: "library", words: "watch downloads folder inbox" },
  { label: "How books show up", page: "sending", words: "default mode cover first features first ebok pdoc document kindle book automatic" },
  { label: "Page numbers", page: "books", words: "pages numbers store" },
  { label: "Make a cover when missing", page: "books", words: "cover generated typographic" },
  { label: "Match the store cover", page: "books", words: "amazon cover asin store edition online" },
  { label: "Fill missing details", page: "books", words: "open library metadata details author series description online" },
  { label: "Tidy typography", page: "books", words: "typography quotes dashes ellipses" },
  { label: "Pop-up footnotes", page: "books", words: "footnotes notes popup" },
  { label: "X-Ray", page: "reading", words: "xray characters people places terms" },
  { label: "Wikipedia summaries", page: "reading", words: "wikipedia online" },
  { label: "AI model for X-Ray", page: "reading", words: "ai llm model anthropic claude openai ollama lm studio online" },
  { label: "API key", page: "reading", words: "api key token anthropic openai" },
  { label: "Word Wise", page: "reading", words: "word wise hints vocabulary" },
  { label: "How many hints", page: "reading", words: "word wise level slider hints dictionary" },
  { label: "Highlights folder", page: "highlights", words: "highlights markdown clippings notes folder" },
  { label: "Write highlights when a Kindle connects", page: "highlights", words: "highlights connect clippings schedule morning" },
  { label: "Note how far I read the paper", page: "highlights", words: "readership reading paper daily progress" },
  { label: "From Your Shelf", page: "highlights", words: "shelf reading daily highlight words" },
  { label: "Mode on this Kindle", page: "kindles", words: "kindle device mode ebok pdoc per kindle" },
  { label: "Keeps Deckle's X-Ray for real ASINs", page: "kindles", words: "asin xray real verified" },
  { label: "KOReader plugins", page: "koreader", words: "koreader plugins install update remove kobo pocketbook boox patches store" },
  { label: "Plugin store", page: "koreader", words: "koreader plugin store recommended discover github install online" },
  { label: "The Deckle plugin", page: "koreader", words: "koreader deckle plugin fetch paper wifi automatic" },
  { label: "Updates", page: "about", words: "update version restart" },
  { label: "What Deckle sends online", page: "online", words: "online privacy network internet send" },
  { label: "Checking for updates", page: "online", words: "update github online privacy" },
  { label: "Credits", page: "about", words: "credits licence license wiktionary" },
  { label: "Made by Casa Vargas", page: "about", words: "casa vargas studio developer support buy beltr onescribe" },
];

/** Search settings: the entries matching every word of `query` (label, words or page title). */
export function searchSettings(query: string): Entry[] {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (terms.length === 0) return [];
  const title = (p: SettingsPage) => PAGES.find((x) => x.id === p)!.title;
  return INDEX.filter((e) => {
    const hay = `${e.label} ${e.words} ${title(e.page)}`.toLowerCase();
    return terms.every((t) => hay.includes(t));
  });
}

/** One row of Overview's setup checklist (§4). */
export interface Check {
  key: "previewer" | "library" | "originals" | "highlights" | "kindles";
  title: string;
  state: string;
  /** ok: set up; todo: Deckle can't work without it; optional: off by choice, with a way to turn it on. */
  mark: "ok" | "todo" | "optional";
  /** The button that fixes or sets it up. */
  fix?: string;
  page: SettingsPage;
}

const base = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;
const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;
const list = (xs: string[]) => (xs.length <= 1 ? xs.join("") : `${xs.slice(0, -1).join(", ")} and ${xs[xs.length - 1]}`);

export function setupChecks(a: { previewer: PreviewerStatus | null; library: LibraryStatus | null; sources: SourcesStatus | null; highlights: string | null | undefined; devices: DeviceRow[]; web?: boolean }): Check[] {
  const out: Check[] = [];
  out.push(
    // The web app's server can't run it: the reason, with nothing to choose.
    a.web && a.previewer && !a.previewer.found
      ? { key: "previewer", title: "Kindle Previewer", state: PREVIEWER_REASON, mark: "optional", page: "sending" }
      : !a.previewer
      ? { key: "previewer", title: "Kindle Previewer", state: "looking for it…", mark: "ok", page: "sending" }
      : a.previewer.found
        ? { key: "previewer", title: "Kindle Previewer", state: "found, so books can be converted", mark: "ok", page: "sending" }
        : { key: "previewer", title: "Kindle Previewer", state: "not found: Deckle can't convert books without it", mark: "todo", fix: "Choose…", page: "sending" },
  );
  if (a.library)
    out.push(
      a.library.root
        ? { key: "library", title: "Library", state: `${plural(a.library.books, "book")} in ${base(a.library.root)}`, mark: "ok", page: "library" }
        : { key: "library", title: "Library", state: "off: books you send aren't kept", mark: "optional", fix: "Choose folder…", page: "library" },
    );
  if (a.sources) {
    const where = [a.sources.find_in_calibre && a.sources.calibre ? "calibre" : "", a.sources.folders.length ? plural(a.sources.folders.length, "folder") : ""].filter(Boolean);
    out.push(
      where.length
        ? { key: "originals", title: "Originals", state: `looking in ${list(where)}`, mark: "ok", page: "library" }
        : { key: "originals", title: "Originals", state: "looking nowhere: Upgrades can only rebuild from the Kindle's copy", mark: "optional", fix: "Set up…", page: "library" },
    );
  }
  if (a.highlights !== undefined)
    out.push(
      a.highlights
        ? { key: "highlights", title: "Highlights", state: `written to ${base(a.highlights)}`, mark: "ok", page: "highlights" }
        : { key: "highlights", title: "Highlights", state: "no folder chosen, so highlights stay on the Kindle", mark: "optional", fix: "Choose folder…", page: "highlights" },
    );
  out.push(
    a.devices.length
      ? { key: "kindles", title: "Kindles", state: `${list(a.devices.map((d) => d.name || d.model_hint))} plugged in`, mark: "ok", page: "kindles" }
      : { key: "kindles", title: "Kindles", state: "none plugged in", mark: "optional", page: "kindles" },
  );
  return out;
}

/** The checklist's heading. */
export function setupHeading(checks: Check[]): string {
  const todo = checks.filter((c) => c.mark === "todo").length;
  return todo === 0 ? "Everything Deckle needs is set up." : `${plural(todo, "thing")} need${todo === 1 ? "s" : ""} doing before Deckle can send books.`;
}

/** The optional features that are on and go online, by name. */
export function onlineOn(s: Pick<Settings, "metadata_fill" | "amazon_match" | "wikipedia" | "llm_provider">): string[] {
  const provider = s.llm_provider === "anthropic" ? "Anthropic" : "an OpenAI-compatible provider";
  return [s.metadata_fill && "Open Library details", s.amazon_match && "store-cover matching", s.wikipedia && "Wikipedia summaries", s.llm_provider !== "off" && `the AI model (${provider})`].filter(
    (x): x is string => !!x,
  );
}

/** Overview's line on what goes online. */
export function onlineLine(names: string[]): string {
  if (names.length === 0) return "Nothing optional goes online. Everything happens on this Mac.";
  return `${plural(names.length, "feature")} go${names.length === 1 ? "es" : ""} online: ${list(names)}. Everything else stays on this Mac.`;
}

/** The notes beside the sidebar's pages: what needs doing there, and how many online features are on. */
export function pageNotes(checks: Check[], online: string[] = []): Partial<Record<SettingsPage, string>> {
  const out: Partial<Record<SettingsPage, string>> = {};
  for (const c of checks.filter((c) => c.mark === "todo")) out[c.page] = "to do";
  if (online.length) out.online = `${online.length} on`;
  return out;
}

const MODE_WORD: Record<Settings["cde"], string> = { auto: "Automatic", pdoc: "Documents, cover first", ebok: "Kindle books, features first" };

/** Each page's card on Overview: its state in one line. */
export function pageSummary(page: SettingsPage, s: Settings, library: LibraryStatus | null, devices: DeviceRow[], version: string): string {
  switch (page) {
    case "sending":
      return MODE_WORD[s.cde];
    case "library":
      return library?.root ? `${plural(library.books, "book")}${library.auto_add ? " · adds every book you send" : ""}` : "Off: books you send aren't kept";
    case "highlights":
      return "Your highlights, reading of the paper, and From Your Shelf";
    case "books": {
      const on = [s.pages_auto && "page numbers", s.typography && "tidy typography", s.popup_notes && "pop-up footnotes", s.generated_cover && "covers when missing"].filter((x): x is string => !!x);
      return on.length ? list(on).replace(/^./, (c) => c.toUpperCase()) : "Books go as they are";
    }
    case "reading": {
      const ww = s.word_wise ? (s.word_wise_level === 5 ? "Word Wise, every hint" : `Word Wise, ${s.word_wise_level} of 5`) : "";
      const xr = s.xray ? `X-Ray${s.llm_provider !== "off" ? " with an AI model" : ""}${s.wikipedia ? " and Wikipedia" : ""}` : "";
      return [ww, xr].filter(Boolean).join(" · ") || "Both off";
    }
    case "kindles":
      return devices.length ? list(devices.map((d) => d.name || d.model_hint)) : "None plugged in";
    case "online": {
      const on = onlineOn(s).length;
      return on ? `${plural(on, "feature")} on` : "Nothing optional goes online";
    }
    case "about":
      return `Deckle ${version}`;
    default:
      return "";
  }
}
