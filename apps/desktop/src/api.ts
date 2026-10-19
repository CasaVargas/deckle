// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { BookRow, DeviceRow, PlanRow, PreviewerStatus, ProgressEvent, RunTarget, Settings } from "./types";

export const api = {
  appVersion: () => invoke<string>("app_version"),
  takePreloadedBooks: () => invoke<string[]>("take_preloaded_books"),
  takePreloadedTarget: () => invoke<string | null>("take_preloaded_target"),
  inspectBooks: (paths: string[]) => invoke<BookRow[]>("inspect_books", { paths }),
  listDevices: () => invoke<DeviceRow[]>("list_devices"),
  previewerStatus: () => invoke<PreviewerStatus>("previewer_status"),
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<void>("set_settings", { settings }),
  runBooks: (books: { id: string; path: string; cde: string | null }[], target: RunTarget) =>
    invoke<void>("run_books", { books, target }),
  planBooks: (books: { id: string; path: string; cde: string | null }[], target: RunTarget) =>
    invoke<PlanRow[]>("plan_books", { books, target }),
  setDeviceOptions: (key: string, cdeOverride: "pdoc" | "ebok" | null, verifiedRealAsin: boolean) =>
    invoke<void>("set_device_options", { key, cdeOverride, verifiedRealAsin }),
  reveal: (path: string) => invoke<void>("reveal", { path }),
  onProgress: (cb: (e: ProgressEvent) => void): Promise<UnlistenFn> =>
    listen<ProgressEvent>("book-progress", (ev) => cb(ev.payload)),
  onRunFinished: (cb: () => void): Promise<UnlistenFn> => listen("run-finished", () => cb()),
};
