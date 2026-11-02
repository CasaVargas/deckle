// SPDX-License-Identifier: GPL-3.0-or-later
// The Puzzles tab's pure parts (Puzzles tab design §1): the request a kind starts from, what
// its options mean, and the lines of copy worked out from a book. Beside the tab, as
// cookbookRooms.ts is beside the Cookbook. (Not puzzlesView.ts: on a Mac or Windows that name
// is the same file as PuzzlesView.tsx.)
import type { MadePuzzleBook, PuzzleBookRequest, PuzzleKind, PuzzleKindKey } from "./types";

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

/** The line under Send: where the book goes. */
export function sendLine(deviceName: string | null): string {
  return deviceName ? `Copies it to ${deviceName}, into Documents.` : "Plug a Kindle in to send it there, or save a copy to a folder for a reMarkable or an iPad.";
}
