// SPDX-License-Identifier: GPL-3.0-or-later
import type { MorgueDay } from "./types";

/**
 * The month as calendar cells, Monday first: an edition's day, `null` for a day in the month with
 * none, and "blank" for padding before the 1st and after the last day (whole weeks).
 */
export function monthGrid(year: number, month: number, days: MorgueDay[]): (MorgueDay | null | "blank")[] {
  const byDate = new Map(days.map((d) => [d.date, d]));
  const first = new Date(Date.UTC(year, month - 1, 1));
  const lead = (first.getUTCDay() + 6) % 7; // Monday = 0
  const count = new Date(Date.UTC(year, month, 0)).getUTCDate();
  const cells: (MorgueDay | null | "blank")[] = Array(lead).fill("blank");
  for (let d = 1; d <= count; d++) {
    const key = `${year}-${String(month).padStart(2, "0")}-${String(d).padStart(2, "0")}`;
    cells.push(byDate.get(key) ?? null);
  }
  while (cells.length % 7 !== 0) cells.push("blank");
  return cells;
}

/** "Morgue: 214 editions, 590 MB" (MB = 1024², rounded). */
export function sizeLine(editions: number, bytes: number): string {
  const mb = Math.round(bytes / (1024 * 1024));
  return `Morgue: ${editions} edition${editions === 1 ? "" : "s"}, ${mb} MB`;
}
