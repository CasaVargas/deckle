// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { editionMissedOn, notSent, runFailed, summary } from "./runResult";

test("entries the push left unsent come from the Kindle's push and the reader's", () => {
  const report = {
    outcome: { flush: { delivered: ["/K/documents/deckle-daily-2026-11-12.kfx"], failed: [["Emma.kfx", "the Kindle refused it"]] } },
    reader: { flush: { delivered: [], failed: [["broken.kepub.epub", "reading broken.epub: Is a directory"]] } },
  };
  expect(notSent(report)).toEqual([
    { name: "Emma.kfx", why: "the Kindle refused it" },
    { name: "broken.kepub.epub", why: "reading broken.epub: Is a directory" },
  ]);
  expect(notSent({ outcome: { flush: { delivered: [] } } })).toEqual([]);
  expect(notSent(null)).toEqual([]);
});

test("the run box follows digest run's exit: red when the run failed", () => {
  expect(runFailed({ ok: true, report: {}, log: "" })).toBe(false);
  expect(runFailed({ ok: false, report: { edition_sent: false }, log: "" })).toBe(true);
  expect(runFailed({ ok: false, report: null, log: "boom" })).toBe(true);
});

test("a push that stopped after the edition went isn't shown as failed", () => {
  // digest run exits 0 then: the edition went before the Kindle left.
  const report = {
    edition_sent: true,
    outcome: {
      edition_sent: true,
      flush: { delivered: ["/K/documents/deckle-daily-2026-11-12.kfx"], failed: [["b.kfx", "the Kindle is no longer connected"]], stopped: true },
    },
  };
  expect(runFailed({ ok: true, report, log: "" })).toBe(false);
  expect(editionMissedOn({ ok: true, report, log: "" })).toEqual([]);
  expect(notSent(report)).toEqual([{ name: "b.kfx", why: "the Kindle is no longer connected" }]);
});

test("a reader with another book unsent isn't named as missing the paper", () => {
  // The Kindle missed the paper; the Kobo wasn't sent it and has an unrelated entry unsent.
  const report = {
    edition_sent: true,
    outcome: {
      edition_missed: true,
      flush: { delivered: ["/K/documents/Emma.kfx"], failed: [["deckle-daily-2026-11-12.kfx", "the Kindle refused it"]] },
    },
    reader: { flush: { delivered: [], failed: [["broken.kepub.epub", "reading broken.epub: Is a directory"]] } },
  };
  expect(editionMissedOn({ ok: false, report, log: "" })).toEqual(["kindle"]);
});

test("a paper the Kindle refused while the Kobo took it isn't a clean success", () => {
  // digest run exits non-zero: a connected reader hasn't got the edition.
  const report = {
    build: { title: "The Daily" },
    edition_sent: true,
    outcome: { edition_missed: true, flush: { delivered: [], failed: [["deckle-daily-2026-11-12.kfx", "the Kindle refused it"]] } },
    reader: { edition_sent: true, flush: { delivered: ["/K/Deckle/deckle-daily-2026-11-12.kepub.epub"], failed: [] } },
  };
  const r = { ok: false, report, log: "" };
  expect(runFailed(r)).toBe(true);
  expect(editionMissedOn(r)).toEqual(["kindle"]);
  expect(summary(r, true)).not.toContain("Sent to your Kindle");
  // The other way round.
  const kobo = {
    ...report,
    outcome: { edition_sent: true, flush: { delivered: ["/K/documents/deckle-daily-2026-11-12.kfx"], failed: [] } },
    reader: { edition_missed: true, flush: { delivered: [], failed: [["deckle-daily-2026-11-12.kepub.epub", "locked"]] } },
  };
  expect(editionMissedOn({ ok: false, report: kobo, log: "" })).toEqual(["reader"]);
  expect(summary({ ok: false, report: kobo, log: "" }, true)).toContain("Sent to your Kindle.");
});

const sent = (outcome: any, extra: any = {}) => ({ ok: true, log: "", report: { build: { title: "The Daily" }, outcome, ...extra } });

test("the summary says sent to your Kindle only when the edition went to it", () => {
  const went = { edition_sent: true, flush: { delivered: ["/K/documents/deckle-daily-2026-11-12.kfx"], failed: [] } };
  expect(summary(sent(went, { edition_sent: true }), true)).toBe("The Daily — Sent to your Kindle.");
  // Another book went; the edition didn't.
  const other = { flush: { delivered: ["/K/documents/Emma.kfx"], failed: [["deckle-daily-2026-11-12.kfx", "refused"]] } };
  expect(summary(sent(other), true)).not.toContain("Sent to your Kindle");
  // It went to a Kobo; no Kindle took it.
  const kobo = sent({ flush: { delivered: ["/K/documents/Emma.kfx"], failed: [] } }, { edition_sent: true, reader: { edition_sent: true } });
  expect(summary(kobo, true)).not.toContain("Sent to your Kindle");
  expect(summary(sent({ queued: true }), true)).toBe("The Daily — Waiting for your Kindle: it goes over when you plug it in.");
  expect(summary({ ok: true, log: "", report: { title: "The Daily" } }, false)).toBe("The Daily — Preview built. Saved articles stay saved until you send.");
});
