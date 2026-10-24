// SPDX-License-Identifier: GPL-3.0-or-later
import type { BookState, PlanRow, PreflightView, Settings } from "./types";

/** One line of What Deckle will do. */
export interface PlannedLine {
  what: string;
  text: string;
  tone: "" | "muted" | "warn";
}

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;
const list = (xs: string[]) => (xs.length <= 1 ? xs.join("") : `${xs.slice(0, -1).join(", ")} and ${xs[xs.length - 1]}`);

/** What Deckle will do to a book, from its preflight, the cover rule's plan and the settings. */
export function plannedLines(v: PreflightView, plan: PlanRow | undefined, book: Pick<BookState, "cdeOverride">, s: Pick<Settings, "typography" | "popup_notes" | "xray" | "word_wise" | "generated_cover">, preset = false): PlannedLine[] {
  const out: PlannedLine[] = [];
  if (!s.typography) out.push({ what: "Typography", text: preset ? "Off for this book’s style preset." : "Off. Turn it on in Settings → Books for curly quotes, proper dashes and ellipses.", tone: "muted" });
  else {
    const t = v.typography;
    const fixed = t ? [t.quotes && plural(t.quotes, "curly quote"), t.dashes && plural(t.dashes, "dash", "dashes"), t.ellipses && plural(t.ellipses, "ellipsis", "ellipses")].filter((x): x is string => !!x) : [];
    const parts = [fixed.length ? `${list(fixed)} fixed` : "", t?.empty_paragraphs ? `${plural(t.empty_paragraphs, "empty paragraph")} removed` : ""].filter(Boolean);
    out.push({ what: "Typography", text: parts.length ? parts.join("; ") : "Nothing to fix: the book's typography is clean", tone: "" });
  }
  if (!s.popup_notes) out.push({ what: "Pop-up notes", text: preset ? "Off for this book’s style preset; existing notes are kept." : "Off. Turn it on in Settings → Books to read footnotes without leaving the page.", tone: "muted" });
  else {
    const n = v.notes ?? 0;
    const skipped = v.notes_skipped ? `; ${plural(v.notes_skipped, "note-like link")} with no way back left as ${v.notes_skipped === 1 ? "it is" : "they are"}` : "";
    out.push({ what: "Pop-up notes", text: n ? `${plural(n, "note")} become pop-up notes${skipped}` : `No footnotes found${skipped}`, tone: "" });
  }
  const cover = {
    own: "Its own cover",
    generated: "Deckle's typographic cover",
    picked: "The cover you picked from Open Library",
    none: s.generated_cover ? "None, and Deckle couldn't make one" : "None. Turn on typographic covers in Settings → Books, or pick one under Cover.",
  }[v.cover];
  out.push({ what: "Cover", text: cover, tone: v.cover === "none" ? "warn" : "" });
  const cde = book.cdeOverride ?? plan?.cde;
  if (!cde) out.push({ what: "Mode", text: "Chosen when a Kindle is connected", tone: "muted" });
  else
    out.push({
      what: "Mode",
      text: book.cdeOverride ? `${cde.toUpperCase()}, as you set it` : `${cde.toUpperCase()}: ${plan!.reason}`,
      tone: "",
    });
  if (cde === "ebok") {
    const on = [s.xray && "X-Ray", s.word_wise && "Word Wise"].filter(Boolean);
    out.push({ what: "X-Ray and Word Wise", text: on.length ? `${on.join(" and ")}, built for this book` : preset ? "Both off for this book’s style preset" : "Both off in Settings", tone: on.length ? "" : "muted" });
  } else if (cde === "pdoc") out.push({ what: "X-Ray and Word Wise", text: "Not on a personal document (PDOC): send it as EBOK for them", tone: "muted" });
  if (v.toc_synthesized) out.push({ what: "Contents", text: "The book has none, so Deckle makes one from its headings", tone: "" });
  if (v.images_downscaled) out.push({ what: "Pictures", text: `${plural(v.images_downscaled, "picture")} made smaller for the Kindle`, tone: "" });
  for (const w of v.warnings) out.push({ what: "Note", text: w, tone: "warn" });
  return out;
}

/** A Kindle page for the preview frame: the book's styles over a Kindle-like base, nothing loaded from anywhere, cut at the first page. */
export function kindlePage(html: string, css: string): string {
  const safeCss = css.replace(/<\/style/gi, "<\\/style");
  return `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; style-src 'unsafe-inline'">
<style>
html, body { margin: 0; background: #fbfaf6; color: #1b1b1b; }
body { box-sizing: border-box; height: 100vh; padding: 34px 30px; font-family: Bookerly, "Iowan Old Style", "Palatino Linotype", Palatino, Georgia, serif; font-size: 16px; line-height: 1.55; text-align: justify; hyphens: auto; -webkit-hyphens: auto; }
img, svg { max-width: 100%; height: auto; }
p { margin: 0; text-indent: 1.4em; }
h1, h2, h3 { text-align: center; line-height: 1.25; }
</style><style>${safeCss}</style>
<style>html, body { overflow: hidden !important; max-height: 100vh; }</style></head><body>${html}</body></html>`;
}
