// SPDX-License-Identifier: GPL-3.0-or-later
import type { BookState, PlanRow, Settings } from "./types";

/** One small status label on a book row. */
export type Chip = { text: string; tone: "plain" | "ok" | "warn" | "fail"; title?: string };

/** What sending will do, in plain words: the cover rule's plan plus the features that are on. */
export function beforeChip(book: Pick<BookState, "mode" | "reason">, plan: PlanRow | undefined, settings: Pick<Settings, "xray" | "word_wise">): Chip {
  if (book.mode === "error") return { text: "Can't convert", tone: "fail", title: book.reason };
  if (book.mode === "ready") return { text: "Sent as is", tone: "plain", title: book.reason };
  const title = plan?.reason ?? book.reason;
  if (!plan || plan.cde === "pdoc") return { text: "Keeps cover", tone: "plain", title };
  const features = [settings.xray && "X-Ray", settings.word_wise && "Word Wise"].filter(Boolean).join(" + ") || "Kindle book";
  return { text: plan.write_thumbnail ? `${features} + cover` : features, tone: "plain", title };
}

/** A finished book's line: where it went, the features built, and any warnings. */
export function doneChips(book: BookState): Chip[] {
  const verb = book.exported ? "Exported" : "Sent";
  const first = book.mode === "ready" ? `${verb} as is` : book.pages ? `${verb} · ${book.pages} pages` : verb;
  const chips: Chip[] = [{ text: first, tone: "ok" }];
  if (book.xray) chips.push({ text: "X-Ray", tone: "plain" });
  if (book.wordWise) chips.push({ text: "Word Wise", tone: "plain" });
  const n = book.runWarnings.length;
  if (n > 0) chips.push({ text: `${n} warning${n > 1 ? "s" : ""}`, tone: "warn" });
  return chips;
}
