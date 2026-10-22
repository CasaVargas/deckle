// SPDX-License-Identifier: GPL-3.0-or-later
import type { HighlightsReport } from "./types";

/** One line after a Write: what went into the folder, or what couldn't. */
export function writtenSummary(r: HighlightsReport): string {
  if (r.failed.length > 0) return `Couldn't write ${r.failed.map(([, e]) => e).join("; ")}`;
  const n = r.written.reduce((sum, w) => sum + w.new, 0);
  if (n === 0) return "Nothing new since the last time.";
  const where = r.written.length === 1 ? r.written[0].file : `${r.written.length} files`;
  return `${n} new highlight${n === 1 ? "" : "s"} in ${where}.`;
}
