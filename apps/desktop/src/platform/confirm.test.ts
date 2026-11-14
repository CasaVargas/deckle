// SPDX-License-Identifier: GPL-3.0-or-later
// platform.confirm resolves to the answer, never to a Promise that reads as yes (jloutsch/deckle#88).
import { afterEach, expect, test, vi } from "vitest";

const nativeConfirm = vi.hoisted(() => vi.fn<(message: string, options?: unknown) => Promise<boolean>>());
vi.mock("@tauri-apps/plugin-dialog", () => ({ confirm: nativeConfirm, open: vi.fn(), save: vi.fn() }));

import { desktop } from "./desktop";
import { web } from "./web";

afterEach(() => {
  nativeConfirm.mockReset();
  vi.unstubAllGlobals();
});

test("desktop_confirm_is_false_when_the_dialog_is_cancelled", async () => {
  nativeConfirm.mockResolvedValue(false);
  await expect(desktop.confirm("Remove these 2 books from the Kobo?")).resolves.toBe(false);
  expect(nativeConfirm).toHaveBeenCalledWith("Remove these 2 books from the Kobo?", { kind: "warning" });
});

test("desktop_confirm_is_true_when_the_dialog_is_accepted", async () => {
  nativeConfirm.mockResolvedValue(true);
  await expect(desktop.confirm("Replace it?")).resolves.toBe(true);
});

test("a_cancelled_confirm_stops_the_removal", async () => {
  nativeConfirm.mockResolvedValue(false);
  const remove = vi.fn();
  // the guard ReaderView.remove and PuzzlesView.send use
  if (await desktop.confirm("Remove this book from the PocketBook?")) remove();
  expect(remove).not.toHaveBeenCalled();
});

test("web_confirm_returns_the_browsers_answer", async () => {
  const confirm = vi.fn().mockReturnValueOnce(false).mockReturnValueOnce(true);
  vi.stubGlobal("window", { confirm });
  await expect(web.confirm("Replace it?")).resolves.toBe(false);
  await expect(web.confirm("Replace it?")).resolves.toBe(true);
  expect(confirm).toHaveBeenCalledWith("Replace it?");
});
