// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test, vi } from "vitest";
import { debounce, saveQueue } from "./saveQueue";

/** A save that waits until the test lets it finish. */
function gated() {
  const saved: number[] = [];
  const gates: (() => void)[] = [];
  const save = (c: number) =>
    new Promise<void>((resolve) => {
      saved.push(c);
      gates.push(resolve);
    });
  const open = async () => {
    gates.shift()?.();
    await new Promise((r) => setTimeout(r, 0));
  };
  return { saved, save, open };
}

test("one save at a time, and only the newest of what waited", async () => {
  const g = gated();
  const settled = vi.fn();
  const q = saveQueue(g.save, () => {}, settled);
  const first = q.push(1);
  q.push(2);
  q.push(3);
  const last = q.push(4);
  expect(g.saved).toEqual([1]);
  expect(q.busy()).toBe(true);
  await g.open();
  expect(g.saved).toEqual([1, 4]);
  await g.open();
  await Promise.all([first, last]);
  expect(q.busy()).toBe(false);
  expect(settled).toHaveBeenCalledTimes(1);
});

test("a failed save is reported and the next one still runs", async () => {
  const failed = vi.fn();
  const saved: number[] = [];
  const q = saveQueue(
    async (c: number) => {
      saved.push(c);
      if (c === 1) throw new Error("disk full");
    },
    failed,
  );
  await Promise.all([q.push(1), q.push(2)]);
  expect(saved).toEqual([1, 2]);
  expect(failed).toHaveBeenCalledTimes(1);
  expect(String(failed.mock.calls[0][0])).toContain("disk full");
});

test("typing saves once it pauses, and leaving the field saves at once", () => {
  vi.useFakeTimers();
  const f = vi.fn();
  const d = debounce(f, 500);
  d.call();
  vi.advanceTimersByTime(300);
  d.call();
  vi.advanceTimersByTime(300);
  expect(f).not.toHaveBeenCalled();
  vi.advanceTimersByTime(200);
  expect(f).toHaveBeenCalledTimes(1);
  d.call();
  d.flush();
  expect(f).toHaveBeenCalledTimes(2);
  d.flush();
  expect(f).toHaveBeenCalledTimes(2);
  vi.useRealTimers();
});
