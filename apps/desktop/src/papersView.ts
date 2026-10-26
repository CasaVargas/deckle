// SPDX-License-Identifier: GPL-3.0-or-later
// Your papers on the Daily tab (many papers design §7): the lines on each paper's card.
import type { Day, KnownKindle, PaperCard } from "./types";

const DAY_NAME: Record<Day, string> = { mon: "Mon", tue: "Tue", wed: "Wed", thu: "Thu", fri: "Fri", sat: "Sat", sun: "Sun" };
const DAY_PLURAL: Record<Day, string> = {
  mon: "Mondays",
  tue: "Tuesdays",
  wed: "Wednesdays",
  thu: "Thursdays",
  fri: "Fridays",
  sat: "Saturdays",
  sun: "Sundays",
};
export const DAYS: Day[] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

/** "Saturdays", "Mon and Wed", "Mon, Wed and Fri"; empty for every day. */
export function daysText(days: Day[]): string {
  const ds = DAYS.filter((d) => days.includes(d));
  if (ds.length === 0 || ds.length === 7) return "";
  if (ds.length === 1) return DAY_PLURAL[ds[0]];
  const names = ds.map((d) => DAY_NAME[d]);
  return `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

/** "USB at 05:30", "USB, Saturdays at 07:00", "Email at 06:00", "Email and USB at 06:00", or "Not scheduled". */
export function deliveryLine(card: Pick<PaperCard, "at" | "days" | "email_to" | "usb_too">): string {
  if (!card.at) return "Not scheduled";
  const how = card.email_to ? (card.usb_too ? "Email and USB" : "Email") : "USB";
  const days = daysText(card.days);
  return days ? `${how}, ${days} at ${card.at}` : `${how} at ${card.at}`;
}

/** The Kindle a paper goes to: its name, "Any Kindle", or what the file says when no Kindle has that name. */
export function kindleLine(card: Pick<PaperCard, "kindle">, kindles: KnownKindle[]): string {
  const want = card.kindle.trim();
  if (!want) return "Any Kindle";
  const k = kindles.find((k) => k.name.toLowerCase() === want.toLowerCase() || k.sync_id === want);
  return k ? k.name : `${want} (not named yet)`;
}

export const typeLine = (card: Pick<PaperCard, "large_print">): string => (card.large_print ? "Large print" : "Regular");

/** With two or more papers, one line for each paper that goes to whichever Kindle connects first. */
export function anyKindleHint(cards: Pick<PaperCard, "title" | "kindle">[]): string | null {
  if (cards.length < 2) return null;
  const any = cards.filter((c) => !c.kindle.trim()).map((c) => c.title);
  if (any.length === 0) return null;
  const names = any.length === 1 ? any[0] : `${any.slice(0, -1).join(", ")} and ${any[any.length - 1]}`;
  return `${names} ${any.length === 1 ? "goes" : "go"} to whichever Kindle connects first.`;
}

/** The file name a paper's title makes, as `papers::slug_from_title` does: "The Kid Courier" → "kid-courier". */
export function slugFromTitle(title: string): string {
  let words = title
    .split(/[^A-Za-z0-9]+/)
    .filter(Boolean)
    .map((w) => w.toLowerCase());
  if (words[0] === "the" && words.length > 1) words = words.slice(1);
  return words
    .join("-")
    .replace(/^[0-9-]+/, "")
    .slice(0, 32)
    .replace(/-+$/, "");
}

/** "for Maya" when the paper names its reader. */
export const readerLine = (card: Pick<PaperCard, "reader">): string => (card.reader.trim() ? `for ${card.reader.trim()}` : "");
