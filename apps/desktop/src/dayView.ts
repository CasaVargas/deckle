// SPDX-License-Identifier: GPL-3.0-or-later
import type { DayConfig, DayPreview } from "./types";

/** `[day]` as digest.toml defaults it (almanac design §5). */
export const DEFAULT_DAY: DayConfig = { calendars: [], max_items: 8, weather_note: true };

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** "Today: 4 events · 1 calendar unavailable". */
export function previewLine(p: DayPreview | null): string {
  if (!p) return "";
  const today = p.events === 0 ? "Today: nothing on the calendar" : `Today: ${plural(p.events, "event", "events")}`;
  return p.errors.length ? `${today} · ${plural(p.errors.length, "calendar", "calendars")} unavailable` : today;
}

/** Why an address can't be added, or null when it can. */
export function addressError(text: string): string | null {
  const t = text.trim();
  if (!t) return "Paste the calendar's address.";
  return /^(https|webcal):\/\/\S+$/i.test(t) ? null : "Use an https:// or webcal:// address.";
}
