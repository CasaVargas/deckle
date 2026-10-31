// SPDX-License-Identifier: GPL-3.0-or-later
// The words Settings → Library → Reading data into calibre shows (calibre bridge design §5.4).
import type { WritebackChange, WritebackColumns, WritebackStatus } from "./types";

export const FIELDS: { key: keyof WritebackColumns; label: string; hint: string }[] = [
  { key: "progress", label: "Progress", hint: "how far through, as a percent (an integer or decimal column)" },
  { key: "read", label: "Read", hint: "finished or not (a yes/no column)" },
  { key: "last_read", label: "Last read", hint: "when it was last opened (a date column)" },
  { key: "rating", label: "Stars", hint: "KOReader's stars (a rating or integer column)" },
];

/** A value as calibre shows it: 62%, Yes, ★★★★, 30 Oct 2026. */
export function shown(field: WritebackChange["field"], v: string | null): string {
  if (v === null || v === "") return "empty";
  if (field === "progress") return `${v}%`;
  if (field === "read") return v === "true" ? "Yes" : "No";
  if (field === "rating") {
    const n = Number(v);
    // A rating column keeps half-stars, 0–10.
    return Number.isFinite(n) ? "★".repeat(Math.round(n / 2)) || "none" : v;
  }
  const d = new Date(v);
  return Number.isNaN(d.getTime()) ? v : d.toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
}

/** "Dune: Progress 30% → 62%". A rating in an integer column is plain stars. */
export function changeLine(c: WritebackChange, ratingIsColumn = true): string {
  const label = FIELDS.find((f) => f.key === c.field)?.label ?? c.column;
  const show = (v: string | null) => (c.field === "rating" && !ratingIsColumn && v ? "★".repeat(Number(v)) : shown(c.field, v));
  return `${c.title}: ${label} ${show(c.old)} → ${show(c.new)}`;
}

/** Nothing mapped yet. */
export function noColumns(c: WritebackColumns): boolean {
  return !c.progress && !c.read && !c.last_read && !c.rating;
}

/** Why it can't write yet, or null. */
export function blocker(s: WritebackStatus): string | null {
  if (!s.library) return "No calibre library found. Choose its folder under Originals.";
  if (!s.calibredb) return "calibre's calibredb wasn't found. Install calibre, or give its path below.";
  if (noColumns(s.writeback.columns)) return "Choose a column for at least one, or create the usual ones.";
  return null;
}
