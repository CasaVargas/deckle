// SPDX-License-Identifier: GPL-3.0-or-later
import type { Origin, SourcesStatus, UpgradeClass, UpgradeRow } from "./types";

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** The books ticked at first: all but those you're partway through (Books and Kindle design §12, ruling 1). */
export function defaultTicked(rows: UpgradeRow[]): Set<string> {
  return new Set(rows.filter((r) => !r.partway).map((r) => r.file));
}

/** Where the original is and what's on the Kindle now: "calibre · EPUB · on the Kindle as AZW3". */
export function sourceLine(r: UpgradeRow): string {
  if (r.source == null) return `No original here: rebuilt from the Kindle's own ${r.format.toUpperCase()}, checked before it replaces anything`;
  const ext = r.source.split(".").pop()?.toUpperCase() ?? "";
  return [r.origin === "calibre" ? "calibre" : "a folder", ext, `on the Kindle as ${r.format.toUpperCase()}`].filter(Boolean).join(" · ");
}

/** For a book you're partway through. */
export function partwayLine(r: UpgradeRow): string | null {
  if (!r.partway || r.fraction == null) return null;
  return `You're ${Math.round(r.fraction * 100)}% in. The new copy opens at the start, so note your place first.`;
}

/** The books Deckle leaves alone, and why, as one line; null when there are none. */
export function leftAsIs(counts: Partial<Record<UpgradeClass, number>>): string | null {
  const parts = leftAlone(counts).parts;
  return parts.length ? `Left as they are: ${parts.join(" · ")}.` : null;
}

/** The books Deckle leaves alone (Kindle tab redesign §5): how many, and each kind with why. */
export function leftAlone(counts: Partial<Record<UpgradeClass, number>>): { total: number; parts: string[] } {
  const parts: string[] = [];
  const n = (c: UpgradeClass) => counts[c] ?? 0;
  if (n("amazon-own")) parts.push(`${plural(n("amazon-own"), "store book")}, which already have all of this`);
  if (n("locked")) parts.push(`${n("locked")} locked by DRM, which Deckle never opens`);
  if (n("fixed-layout")) parts.push(`${plural(n("fixed-layout"), "fixed-layout book")}`);
  if (n("old-mobi")) parts.push(`${plural(n("old-mobi"), "older MOBI book")} Deckle can't rebuild`);
  if (n("no-source")) parts.push(`${n("no-source")} KFX with no original on this computer`);
  if (n("pdf")) parts.push(`${plural(n("pdf"), "PDF")}`);
  if (n("unread")) parts.push(`${n("unread")} not read yet`);
  const kinds: UpgradeClass[] = ["amazon-own", "locked", "fixed-layout", "old-mobi", "no-source", "pdf", "unread"];
  return { total: kinds.reduce((sum, c) => sum + n(c), 0), parts };
}

const list = (xs: string[]) => (xs.length <= 1 ? xs.join("") : `${xs.slice(0, -1).join(", ")} and ${xs[xs.length - 1]}`);

/** What an upgrade adds, in plain words: "Gains X-Ray, Word Wise and page numbers". */
export function gainsLine(gains: string[]): string {
  if (gains.length === 0) return "";
  const words = gains.map((g) => (g === "Page numbers" ? "page numbers" : g === "Cover" ? "a cover" : g === "Enhanced Typesetting" ? "Enhanced Typesetting" : g));
  return `Gains ${list(words)}`;
}

/** The Upgrades heading: how many could be better. */
export function upgradesHeading(n: number): string {
  return n === 0 ? "Every book Deckle can improve is current" : `${plural(n, "book")} could be better`;
}

/** The foot bar's line: "3 of 4 selected". */
export function selectedLine(chosen: number, of: number): string {
  return `${chosen} of ${of} selected`;
}

/** Where Deckle looks for originals, for the panel's foot. */
export function sourcesLine(s: SourcesStatus): string {
  const places: string[] = [];
  if (s.find_in_calibre && s.calibre) places.push(`your calibre library (${s.calibre})`);
  if (s.folders.length) places.push(plural(s.folders.length, "folder"));
  if (!places.length) return "Deckle isn't looking for originals anywhere. Add a calibre library or folders in Settings → General.";
  return `Looking for originals in ${places.join(" and ")}. Change this in Settings → General.`;
}

/** "Upgrade 3 books". */
export function upgradeLabel(n: number): string {
  return n === 0 ? "Upgrade" : `Upgrade ${plural(n, "book")}`;
}

const ORIGIN_TITLE: Record<Origin, string> = {
  calibre: "From your calibre library",
  folder: "From your folders",
  "kindle-copy": "Rebuilt from the Kindle's own copy",
};

/** The rows grouped by where the new copy comes from (Books and Kindle design §7), in that order; empty groups left out. */
export function byOrigin(rows: UpgradeRow[]): { origin: Origin; title: string; rows: UpgradeRow[] }[] {
  return (["calibre", "folder", "kindle-copy"] as Origin[])
    .map((origin) => ({ origin, title: ORIGIN_TITLE[origin], rows: rows.filter((r) => r.origin === origin) }))
    .filter((g) => g.rows.length > 0);
}
