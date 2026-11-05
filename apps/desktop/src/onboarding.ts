// SPDX-License-Identifier: GPL-3.0-or-later
// The welcome (welcome design §4): its pages, the morning-schedule line, and the check that the
// first bookmarklet save landed. Pure, so it's tested without the window.
import type { Key, T } from "./i18n";
import type { ReadingEntry, ScheduleStatus } from "./types";

export const STEPS = [
  { key: "welcome", title: "welcome.step.welcome" },
  { key: "paper", title: "welcome.step.paper" },
  { key: "save", title: "welcome.step.save" },
  { key: "extras", title: "welcome.step.extras" },
  { key: "where", title: "welcome.step.where" },
] as const satisfies readonly { key: string; title: Key }[];
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
export function scheduleLine(status: ScheduleStatus | null, web: boolean, t: T): string {
  if (web) return t("schedule.web");
  const at = mainAt(status);
  if (!at) return t("schedule.off");
  if (!status?.watch) return t("schedule.noWatcher", { time: clock(at) });
  return t("schedule.on", { time: clock(at) });
}

/** The first bookmarklet save made at or after `since` (an ISO instant), the newest if several. */
export function firstSave(entries: ReadingEntry[], since: string): ReadingEntry | null {
  const from = Date.parse(since);
  const landed = entries.filter((e) => e.source === "bookmarklet" && Date.parse(e.saved_at) >= from);
  if (landed.length === 0) return null;
  landed.sort((a, b) => Date.parse(b.saved_at) - Date.parse(a.saved_at));
  return landed[0];
}

/** The miniature paper's dateline: "Tuesday, 3 November 2026" (en-GB), "mardi 3 novembre 2026" (fr-FR). */
export function dateline(now: Date, dateLocale = "en-GB"): string {
  return now.toLocaleDateString(dateLocale, { weekday: "long", day: "numeric", month: "long", year: "numeric" });
}

/** The check line on page 3 (§4.3). */
export function saveLine(save: ReadingEntry | null, t: T): string {
  if (!save) return t("welcome.save.try");
  const what = save.title?.trim() || save.url || t("welcome.save.thatPage");
  return t("welcome.save.landed", { title: what });
}
