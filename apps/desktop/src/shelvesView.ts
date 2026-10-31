// SPDX-License-Identifier: GPL-3.0-or-later
// The words Settings → Library → Shelves from calibre shows (calibre bridge design §6.4).
import type { ShelfDiff, ShelfRule, ShelfTarget } from "./types";

export const KINDS: { key: string; label: string; placeholder: string }[] = [
  { key: "tag", label: "Tag", placeholder: "Sci-Fi" },
  { key: "series", label: "Series", placeholder: "Dune, or * for one shelf a series" },
  { key: "author", label: "Author", placeholder: "Jane Austen" },
  { key: "column", label: "Column", placeholder: "#shelf=Bedside" },
  { key: "search", label: "Saved search", placeholder: "To read" },
];

/** "Tag Sci-Fi", "Each series", "Saved search To read". */
export function ruleLine(r: ShelfRule): string {
  const [kind, ...rest] = r.from.split(":");
  const value = rest.join(":");
  if (kind === "series" && value === "*") return "Each series, a shelf of its own";
  const label = KINDS.find((k) => k.key === kind)?.label ?? kind;
  const target = r.name ? `→ ${r.name}` : "";
  const only = r.readers.includes("all") ? "" : ` (only ${r.readers.join(", ")})`;
  return `${label} ${value} ${target}${only}`.replace(/\s+/g, " ").trim();
}

/** A rule from the add form; a reason when it isn't one. */
export function makeRule(kind: string, value: string, name: string): ShelfRule | string {
  const v = value.trim();
  if (!v) return "Give a value.";
  if (kind === "column" && !v.replace(/^#/, "").includes("=")) return "A column rule is #label=value.";
  const each = kind === "series" && v === "*";
  if (!each && !name.trim()) return "Give the shelf a name.";
  return { name: each ? "" : name.trim(), from: `${kind}:${v}`, readers: ["all"] };
}

/** "2 new shelves, 3 books added, 1 taken off", or "Up to date". */
export function diffLine(d: ShelfDiff): string {
  const added = Object.values(d.add).reduce((n, b) => n + b.length, 0);
  const removed = Object.values(d.remove).reduce((n, b) => n + b.length, 0);
  const parts = [
    d.create.length ? `${d.create.length} new ${d.create.length === 1 ? "shelf" : "shelves"}` : "",
    added ? `${added} ${added === 1 ? "book" : "books"} added` : "",
    removed ? `${removed} taken off` : "",
    d.delete.length ? `${d.delete.length} removed` : "",
  ].filter(Boolean);
  return parts.length ? parts.join(", ") : "Up to date";
}

/** Kobo shelves or KOReader collections. */
export function targetName(t: ShelfTarget): string {
  return t.kind === "kobo" ? "Kobo shelves" : "KOReader collections";
}

/** The Kobo waits for the owner's yes. */
export function needsYes(t: ShelfTarget, koboOk: string[]): boolean {
  return t.kind === "kobo" && !koboOk.includes(t.key) && diffLine(t.diff) !== "Up to date";
}
