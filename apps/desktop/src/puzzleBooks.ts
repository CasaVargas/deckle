// SPDX-License-Identifier: GPL-3.0-or-later
// The Puzzles tab's pure parts (Puzzles tab design §1): the request a kind starts from, what
// its options mean, and the lines of copy worked out from a book. Beside the tab, as
// cookbookRooms.ts is beside the Cookbook. (Not puzzlesView.ts: on a Mac or Windows that name
// is the same file as PuzzlesView.tsx.)
import type { MadePuzzleBook, PuzzleBookRequest, PuzzleBundleTarget, PuzzleKind, PuzzleKindKey, PuzzlesConfig } from "./types";

/** The screens a book can be laid out for, as `--screen` names them. */
export const SCREENS: { key: string; name: string }[] = [
  { key: "kindle-scribe", name: "Kindle Scribe" },
  { key: "remarkable-2", name: "reMarkable 2" },
  { key: "remarkable-paper-pro", name: "reMarkable Paper Pro" },
  { key: "remarkable-paper-pro-move", name: "reMarkable Paper Pro Move" },
  { key: "ipad", name: "iPad" },
  { key: "a5", name: "A5 paper" },
];

export const DIFFICULTIES = ["easy", "medium", "hard"] as const;

/** A fresh request for `kind`, keeping what carries over from `prev` (count, screen, title). */
export function requestFor(kind: PuzzleKind, prev?: PuzzleBookRequest | null): PuzzleBookRequest {
  return {
    kind: kind.key,
    count: prev?.count ?? 10,
    size: kind.default_size,
    difficulty: prev?.difficulty ?? "medium",
    title: "",
    screen: prev?.screen ?? "kindle-scribe",
    words: "",
    quotes: "",
    picture: null,
    seed: null,
    date: "",
    theme: "none",
    interests: [],
    subject: "",
    clue_style: "plain",
  };
}

/** The sizes a kind's select offers: every size it takes, the default marked. */
export function sizeChoices(kind: PuzzleKind): { size: number; label: string }[] {
  if (!kind.sizes) return [];
  const [min, max] = kind.sizes;
  const out: { size: number; label: string }[] = [];
  for (let s = min; s <= max; s++) {
    const label = `${s} × ${s}${s === kind.default_size ? " (usual)" : kind.key === "crossword" && s === 5 ? " (the Mini)" : ""}`;
    out.push({ size: s, label });
  }
  return out;
}

/** What the own-list box asks for, for a kind that takes one; `null` for none. */
export function ownListPrompt(kind: PuzzleKind): { label: string; placeholder: string; help: string } | null {
  switch (kind.takes) {
    case "words":
      return { label: "Your own words", placeholder: "one a line\nrouter\nkernel\npixel", help: "They lead each puzzle; Deckle's list fills the rest. At least 8 that fit the grid, or leave it empty." };
    case "clued":
      return { label: "Your own words and clues", placeholder: "word: clue, one a line\ncargo: Rust's package registry\nbryce: Young, the Panthers' quarterback", help: "3 to 6 letters each, with a clue. Each puzzle takes as many as fit and marks them as theme answers." };
    case "quotes":
      return { label: "Your own quotes", placeholder: "quote — who said it, one a line\nReader, I married him. — Charlotte Brontë", help: "24 to 120 letters each. Leave it empty for Deckle's own, all public domain." };
    default:
      return null;
  }
}

/** Whether the request can be made: what's wrong when it can't. */
export function problem(r: PuzzleBookRequest, kind: PuzzleKind): string | null {
  if (!(r.count >= 1 && r.count <= 200)) return "Puzzles must be from 1 to 200.";
  if (kind.sizes && (r.size < kind.sizes[0] || r.size > kind.sizes[1])) return `Size must be from ${kind.sizes[0]} to ${kind.sizes[1]}.`;
  if (r.date.trim() && !/^\d{4}-\d{2}-\d{2}$/.test(r.date.trim())) return "The date must be YYYY-MM-DD.";
  if (r.theme === "subject" && !r.subject.trim()) return "Name a subject for the AI to write about.";
  return null;
}

/** "10 crosswords, 11×11, medium · Kindle Scribe": a made book's line. */
export function bookLine(b: MadePuzzleBook): string {
  const screen = SCREENS.find((s) => s.key === b.request.screen)?.name ?? b.request.screen;
  return `${b.subtitle} · for the ${screen}`;
}

/** "2.1 MB" */
export function mb(bytes: number): string {
  return `${(bytes / 1e6).toFixed(1)} MB`;
}

/** The progress line while a book is made. */
export function makingLine(kind: PuzzleKindKey, made: number, of: number): string {
  if (of === 0) return "Starting…";
  const noun = kind === "ladder" ? "page" : "puzzle";
  return `Making ${noun} ${Math.min(made + 1, of)} of ${of}…`;
}

/** "On iPad's folder, for the iPad": where a book went as a bundle, by the export's name when one still
 *  points at that folder, else the folder itself; null when it hasn't. */
export function ipadLine(b: MadePuzzleBook, targets: PuzzleBundleTarget[]): string | null {
  if (!b.bundles?.length) return null;
  const names = b.bundles.map((path) => {
    const dir = path.replace(/[\\/][^\\/]+$/, "");
    return targets.find((t) => t.dir === dir)?.name ?? dir;
  });
  const uniq = names.filter((n, i) => names.indexOf(n) === i);
  const list = uniq.length <= 1 ? uniq.join("") : `${uniq.slice(0, -1).join(", ")} and ${uniq[uniq.length - 1]}`;
  return `On ${list}'s ${uniq.length === 1 ? "folder" : "folders"}, for the iPad`;
}

/** "2 of 10 solved on the iPad", or null before a reader wrote. */
export function solvedLine(b: MadePuzzleBook): string | null {
  const s = b.solved_ipad;
  if (!s) return null;
  return `${s[0]} of ${s[1]} solved on the iPad`;
}

/** A paper's Sunday recipe as the card starts it: the backend's defaults. */
export const SUNDAY_DEFAULT: PuzzlesConfig = { sunday: false, kind: "crossword", count: 10, size: 0, difficulty: "medium", theme: "week", clues: "plain" };

/** "10 crosswords from the week's paper, beside Sunday's paper": the recipe in a line. */
export function sundayLine(p: PuzzlesConfig, kinds: PuzzleKind[]): string {
  const kind = kinds.find((k) => k.key === p.kind);
  const noun = p.count === 1 ? (kind?.name ?? p.kind) : p.kind === "ladder" ? "pages of word ladders" : `${kind?.name ?? p.kind}s`;
  const theme: Record<PuzzlesConfig["theme"], string> = { none: "", interests: " from your interests", week: " from the week's paper", vocab: " from the words you looked up", cookbook: " from your cookbook" };
  const themed = p.kind === "crossword" || p.kind === "search" ? theme[p.theme] : "";
  return `${p.count} ${noun}${themed}, beside Sunday's paper`;
}

/** The line under Send: where the book goes. */
export function sendLine(deviceName: string | null): string {
  return deviceName ? `Copies it to ${deviceName}, into Documents.` : "Plug a Kindle in to send it there, or save a copy to a folder for a reMarkable or an iPad.";
}

// ---- Themes and clues (Puzzles tab design §2) ----
import type { ClueStyle, PuzzleSources, PuzzleTheme } from "./types";

export interface ThemeChip {
  key: PuzzleTheme;
  label: string;
  /** Why it can't be picked, when it can't. */
  off: string | null;
  /** "41 words", "AI" */
  tag: string | null;
}

/** The theme chips for a kind that takes words, with what each has to offer right now. */
export function themeChips(kind: PuzzleKind, sources: PuzzleSources | null): ThemeChip[] {
  if (kind.takes !== "words" && kind.takes !== "clued") return [];
  const s = sources?.sources;
  const interests = s ? s.interests.reduce((n, [, , c]) => n + c, 0) : 0;
  const count = (n: number) => (n ? `${n} words` : null);
  return [
    { key: "none", label: "No theme", off: null, tag: null },
    { key: "own", label: "Your own list", off: null, tag: null },
    { key: "interests", label: "Your interests", off: s && s.interests.length === 0 ? "no interests in your papers yet" : s && interests === 0 ? "no headlines kept yet" : null, tag: count(interests) },
    { key: "week", label: "This week's paper", off: s && s.week === 0 ? "no headlines kept yet" : null, tag: count(s?.week ?? 0) },
    { key: "vocab", label: "Words you looked up", off: s && s.vocab === 0 ? "no Kindle has shared its vocabulary yet" : null, tag: count(s?.vocab ?? 0) },
    { key: "cookbook", label: "Your cookbook", off: s && s.cookbook === 0 ? "no recipes saved yet" : null, tag: count(s?.cookbook ?? 0) },
    { key: "subject", label: "A subject", off: sources && !sources.ai ? "needs the AI: Settings → AI" : null, tag: "AI" },
  ];
}

/** The interests to tick, grouped by paper only when there is more than one paper. */
export function interestRows(sources: PuzzleSources | null): { name: string; label: string; words: number }[] {
  const rows = sources?.sources.interests ?? [];
  const papers = new Set(rows.map(([p]) => p));
  return rows.map(([paper, name, words]) => ({ name, label: papers.size > 1 ? `${name} (${paper})` : name, words }));
}

export const CLUE_STYLES: { key: ClueStyle; label: string; ai: boolean }[] = [
  { key: "plain", label: "Plain", ai: false },
  { key: "trivia", label: "Trivia", ai: true },
  { key: "cryptic", label: "Cryptic", ai: true },
  { key: "kid", label: "For a kid", ai: true },
];

/** The line under the clue styles. */
export function clueLine(style: ClueStyle, ai: string | null): string {
  if (style === "plain") return "Clues from the headlines, your list, or Wiktionary. With the AI on, it can rewrite them in a style.";
  if (!ai) return "Styled clues need the AI. Choose where it runs in Settings → AI.";
  return `The AI rewrites every clue, on ${ai}. A puzzle it can't reach keeps its plain clues.`;
}
