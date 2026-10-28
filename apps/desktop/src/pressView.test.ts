// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test, vi } from "vitest";
import { pressView, setupSavedExport, patchExport } from "./pressView";
import { newExport } from "./exportsView";
import { saveQueue } from "./saveQueue";
const folder = { ...newExport("/synced/paper", []), format: "bundle" as const };
const ready = { enabled: true, watcher_ready: true, error: null, producer_id: "producer", paper_id: "paper" };
test("only bundle exports show the default-off request switch", () => {
  expect(pressView(newExport("/epub", [])).visible).toBe(false);
  expect(pressView({ ...folder, format: "pdf" }).visible).toBe(false);
  expect(pressView(folder)).toMatchObject({ visible: true, checked: false, label: "Off" });
});
test("Ready requires confirmed enabled setup and watcher with no failure", () => {
  expect(pressView(folder, ready).label).toBe("Ready");
  expect(pressView(folder, { ...ready, error: "storage_unavailable" }).label).not.toBe("Ready");
  expect(pressView(folder, { ...ready, watcher_ready: false }).label).not.toBe("Ready");
  expect(pressView({ ...folder, press_enabled: true }).label).not.toBe("Ready");
});
test("a foreign producer explains explicit disconnect without silently taking over", () => {
  const view = pressView(folder, { ...ready, enabled: false, watcher_ready: false, error: "protocol_conflict" });
  expect(view.message).toMatch(/other computer/i);
  expect(view.message).toMatch(/disconnect/i);
  expect(view.label).not.toBe("Ready");
});
test("pending folder saves finish before setup uses its positional index", async () => {
  let release!: () => void;
  let disk = [folder];
  const gate = new Promise<void>((r) => { release = r; });
  const queue = saveQueue<typeof disk>(async (next) => { await gate; disk = next; }, () => {});
  const pending = queue.push([{ ...folder, dir: "/synced/new" }, folder]);
  const setup = vi.fn(async () => { expect(disk[1]).toEqual(folder); return ready; });
  const result = setupSavedExport(1, folder, true, async () => { await pending; return true; }, () => disk, setup);
  expect(setup).not.toHaveBeenCalled(); release();
  expect(await result).toEqual(ready);
  expect(setup).toHaveBeenCalledWith(1, true, { dir: folder.dir, format: "bundle" });
});
test("failed save or changed index never invokes setup", async () => {
  const setup = vi.fn();
  await expect(setupSavedExport(0, folder, true, async () => false, () => [folder], setup)).rejects.toThrow(/save/i);
  await expect(setupSavedExport(0, folder, true, async () => true, () => [{ ...folder, dir: "/wrong" }], setup)).rejects.toThrow(/changed/i);
  expect(setup).not.toHaveBeenCalled();
});

test("changing format opts out instead of copying activation into a different binding", () => {
  expect(patchExport({ ...folder, press_enabled: true }, { format: "pdf" }).press_enabled).toBe(false);
  expect(patchExport(folder, { press_enabled: true }).press_enabled).not.toBe(true);
  expect(patchExport({ ...folder, press_enabled: true }, { books: true }).press_enabled).toBe(true);
});
