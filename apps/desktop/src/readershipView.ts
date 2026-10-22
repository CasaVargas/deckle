// SPDX-License-Identifier: GPL-3.0-or-later
import type { ReadershipReport } from "./types";

/** Bars under 30% are muted; a section without a share yet has none. */
export function barTone(share: number | null): "strong" | "muted" | "none" {
  if (share === null) return "none";
  return share < 30 ? "muted" : "strong";
}

export function headline(r: ReadershipReport): string {
  return `Opened ${r.opened} of ${r.window}`;
}

export function howFar(r: ReadershipReport): string {
  return r.medianPercent === null ? "—" : `About ${r.medianPercent}% in`;
}
