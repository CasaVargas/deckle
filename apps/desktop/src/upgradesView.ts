// SPDX-License-Identifier: GPL-3.0-or-later
import type { SourcesStatus, UpgradeClass, UpgradeRow } from "./types";

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** The books ticked at first: all but those you're partway through (Books and Kindle design §12, ruling 1). */
export function defaultTicked(rows: UpgradeRow[]): Set<string> {
  return new Set(rows.filter((r) => !r.partway).map((r) => r.file));
}

/** Where the original is and what's on the Kindle now: "calibre · EPUB · on the Kindle as AZW3". */
export function sourceLine(r: UpgradeRow): string {
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
  const parts: string[] = [];
  const n = (c: UpgradeClass) => counts[c] ?? 0;
  if (n("amazon-own")) parts.push(`${plural(n("amazon-own"), "store book")}, which already have all of this`);
  if (n("locked")) parts.push(`${n("locked")} locked by DRM, which Deckle never opens`);
  if (n("fixed-layout")) parts.push(`${plural(n("fixed-layout"), "fixed-layout book")}`);
  const noOriginal = n("kindle-copy") + n("no-source");
  if (noOriginal) parts.push(`${noOriginal} with no original on this computer`);
  if (n("pdf")) parts.push(`${plural(n("pdf"), "PDF")}`);
  if (n("unread")) parts.push(`${n("unread")} not read yet`);
  return parts.length ? `Left as they are: ${parts.join(" · ")}.` : null;
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
