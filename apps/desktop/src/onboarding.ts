// SPDX-License-Identifier: GPL-3.0-or-later
// The welcome (welcome design §4): its pages, the morning-schedule line, and the check that the
// first bookmarklet save landed. Pure, so it's tested without the window.
import type { ReadingEntry, ScheduleStatus } from "./types";

export const STEPS = [
  { key: "welcome", title: "Welcome" },
  { key: "paper", title: "Your paper" },
  { key: "save", title: "Save to paper" },
  { key: "where", title: "Where things are" },
] as const;
export type StepKey = (typeof STEPS)[number]["key"];

/** "05:30" → "5:30"; anything else as it came. */
export function clock(at: string): string {
  const m = /^(\d{1,2}):(\d{2})$/.exec(at.trim());
  return m ? `${Number(m[1])}:${m[2]}` : at;
}

/** The main paper's press time as the installed job (else its file) has it. */
export function mainAt(status: ScheduleStatus | null): string | null {
  if (!status) return null;
  return status.papers?.[0]?.at ?? status.at ?? null;
}

/** What page 2 says under the schedule control (§4.1). */
export function scheduleLine(status: ScheduleStatus | null, web: boolean): string {
  if (web) return "The container prints the paper on its schedule, and its watcher is always on.";
  const at = mainAt(status);
  if (!at) return "Off. Nothing prints until it's on.";
  if (!status?.watch) return `Every morning at ${clock(at)}, but the watcher isn't installed. Turn the schedule on again.`;
  return `Every morning at ${clock(at)}. The watcher that delivers it, and takes what you save, runs from login.`;
}

/** The first bookmarklet save made at or after `since` (an ISO instant), the newest if several. */
export function firstSave(entries: ReadingEntry[], since: string): ReadingEntry | null {
  const from = Date.parse(since);
  const landed = entries.filter((e) => e.source === "bookmarklet" && Date.parse(e.saved_at) >= from);
  if (landed.length === 0) return null;
  landed.sort((a, b) => Date.parse(b.saved_at) - Date.parse(a.saved_at));
  return landed[0];
}

/** The miniature paper's dateline: "Tuesday, 3 November 2026". */
export function dateline(now: Date): string {
  return now.toLocaleDateString("en-GB", { weekday: "long", day: "numeric", month: "long", year: "numeric" });
}

/** The check line on page 3 (§4.3). */
export function saveLine(save: ReadingEntry | null): string {
  if (!save) return "Try it: open any article in your browser and click Save to paper.";
  const what = save.title?.trim() || save.url || "That page";
  return `"${what}" is in tomorrow's paper.`;
}
