// SPDX-License-Identifier: GPL-3.0-or-later
// What a Send's result says besides the edition (device delivery safety design): the outbox
// entries that didn't go, and whether the run box shows the run as failed.
import { exportParts, remarkablePart } from "./exportsView";
import type { RunResult } from "./types";

/** The line under the Send or Preview button: the edition, then where it went. */
export function summary(r: RunResult, send: boolean): string {
  if (!r.ok && !r.report) return r.log || "Something went wrong.";
  const build = send ? r.report?.build : r.report;
  const parts: string[] = [];
  if (build?.title) parts.push(build.title);
  if (send) {
    const run = r.report?.run;
    const outcome = r.report?.outcome;
    if (r.report?.emailed) parts.push(`Emailed to ${r.report.emailed}.`);
    if (r.report?.email_error) parts.push(`Email failed: ${r.report.email_error}. Sent over USB instead.`);
    parts.push(...exportParts(r.report?.exports));
    const rm = remarkablePart(r.report);
    if (rm) parts.push(rm);
    if (run?.error) parts.push(`Not converted: ${run.error}`);
    // The edition itself, to the Kindle: not another book that went, nor the paper on a Kobo.
    else if (outcome?.edition_sent) parts.push("Sent to your Kindle.");
    else if (outcome?.queued) parts.push("Waiting for your Kindle: it goes over when you plug it in.");
  } else {
    parts.push("Preview built. Saved articles stay saved until you send.");
  }
  return parts.join(" — ");
}

export interface NotSent {
  name: string;
  why: string;
}

/** Outbox entries the push left unsent, to the Kindle and to a Kobo or PocketBook: (name, why). */
export function notSent(report: any): NotSent[] {
  const lists = [report?.outcome?.flush?.failed, report?.reader?.flush?.failed];
  return lists
    .flatMap((l) => (Array.isArray(l) ? l : []))
    .filter((f) => Array.isArray(f) && f.length >= 2)
    .map(([name, why]) => ({ name: String(name), why: String(why) }));
}

/**
 * The readers that didn't get this run's edition while another did: the Kindle (`outcome`) and a
 * Kobo or PocketBook (`reader`), each by its `edition_missed`: the edition itself is among what
 * didn't go there, not just any book. `digest run` then exits with an error.
 */
export function editionMissedOn(r: RunResult): ("kindle" | "reader")[] {
  if (r.ok || r.report?.edition_sent !== true) return [];
  // `digest run` says it per device, from the same check that fails the run.
  const missed = (o: any) => o?.edition_missed === true;
  const out: ("kindle" | "reader")[] = [];
  if (missed(r.report?.outcome)) out.push("kindle");
  if (missed(r.report?.reader)) out.push("reader");
  return out;
}

/**
 * Red when `digest run` failed: its edition didn't go, a push stopped before it went, or a
 * connected reader didn't get it while another did. Another entry left unsent doesn't fail it.
 */
export function runFailed(r: RunResult): boolean {
  return !r.ok;
}
