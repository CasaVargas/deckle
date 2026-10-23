// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { BookRow, DailyConfig, DailyState, DayConfig, DayPreview, DeviceRow, DeviceShelf, EditionPreview, EmailStatus, HighlightsReport, HighlightsStatus, LibraryStatus, NextPaper, PlaceRow, PlanRow, PreviewerStatus, ProgressEvent, ReadershipView, ReadingEntry, RunResult, RunTarget, ScheduleStatus, Settings, ShelfStatus } from "./types";

export interface RunRequest {
  id: string;
  path: string;
  cde: string | null;
  asin: string | null;
}

export const api = {
  appVersion: () => invoke<string>("app_version"),
  takePreloadedBooks: () => invoke<string[]>("take_preloaded_books"),
  takePreloadedTarget: () => invoke<string | null>("take_preloaded_target"),
  startTab: () => invoke<"books" | "daily" | "kindle">("start_tab"),
  inspectBooks: (paths: string[]) => invoke<BookRow[]>("inspect_books", { paths }),
  /** Web addresses, .webloc/.url link files or saved pages, each built into a book (slow: fetches). */
  importArticles: (targets: string[]) => invoke<BookRow[]>("import_articles", { targets }),
  listDevices: () => invoke<DeviceRow[]>("list_devices"),
  previewerStatus: () => invoke<PreviewerStatus>("previewer_status"),
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<void>("set_settings", { settings }),
  runBooks: (books: RunRequest[], target: RunTarget) =>
    invoke<void>("run_books", { books, target }),
  planBooks: (books: RunRequest[], target: RunTarget) =>
    invoke<PlanRow[]>("plan_books", { books, target }),
  setDeviceOptions: (key: string, cdeOverride: "pdoc" | "ebok" | null, verifiedRealAsin: boolean) =>
    invoke<void>("set_device_options", { key, cdeOverride, verifiedRealAsin }),
  reveal: (path: string) => invoke<void>("reveal", { path }),

  // The library and the Kindle tab
  libraryStatus: () => invoke<LibraryStatus>("library_status"),
  librarySet: (root: string | null, autoAdd: boolean) => invoke<LibraryStatus>("library_set", { root, autoAdd }),
  /** Slow over MTP (a few seconds). */
  deviceBooks: (target: RunTarget) => invoke<DeviceShelf>("device_books", { target }),
  deviceRemove: (target: RunTarget, file: string) => invoke<string[]>("device_remove", { target, file }),
  highlightsStatus: () => invoke<HighlightsStatus>("highlights_status"),
  highlightsSet: (folder: string | null, clippings: boolean) => invoke<HighlightsStatus>("highlights_set", { folder, clippings }),
  /** Reads the Kindle again (its own session) and writes what's new into the folder. */
  highlightsWrite: (target: RunTarget) => invoke<HighlightsReport>("highlights_write", { target }),
  readershipReport: () => invoke<ReadershipView>("readership_report"),
  readershipApply: (id: string) => invoke<ReadershipView>("readership_apply", { id }),
  readershipDismiss: (id: string) => invoke<ReadershipView>("readership_dismiss", { id }),
  readershipSet: (on: boolean) => invoke<ReadershipView>("readership_set", { on }),
  shelfStatus: () => invoke<ShelfStatus>("shelf_status"),
  shelfForget: () => invoke<ShelfStatus>("shelf_forget"),
  shelfSet: (on: boolean) => invoke<ShelfStatus>("shelf_set", { on }),
  /** Opens an https link in the default browser (webview links do not). */
  openLink: (url: string) => openUrl(url),
  onProgress: (cb: (e: ProgressEvent) => void): Promise<UnlistenFn> =>
    listen<ProgressEvent>("book-progress", (ev) => cb(ev.payload)),
  onRunFinished: (cb: () => void): Promise<UnlistenFn> => listen("run-finished", () => cb()),

  // The Daily tab
  dailyState: () => invoke<DailyState>("daily_state"),
  dailySetup: () => invoke<DailyState>("daily_setup"),
  dailySave: (config: DailyConfig) => invoke<DailyState>("daily_save", { config }),
  dailyDayPreview: (day: DayConfig) => invoke<DayPreview>("daily_day_preview", { day }),
  geocode: (query: string) => invoke<PlaceRow[]>("geocode", { query }),
  readingList: () => invoke<ReadingEntry[]>("reading_list"),
  readingSave: (url: string) => invoke<string>("reading_save", { url }),
  readingRemove: (id: string) => invoke<boolean>("reading_remove", { id }),
  bookmarklet: () => invoke<string>("bookmarklet"),
  editionPreview: () => invoke<EditionPreview | null>("edition_preview"),
  dailyRun: (send: boolean) => invoke<void>("daily_run", { send }),
  onDailyFinished: (cb: (r: RunResult) => void): Promise<UnlistenFn> =>
    listen<RunResult>("daily-finished", (ev) => cb(ev.payload)),
  scheduleGet: () => invoke<ScheduleStatus>("schedule_get"),
  scheduleSet: (at: string | null) => invoke<ScheduleStatus>("schedule_set", { at }),
  dailyNext: () => invoke<NextPaper>("daily_next"),
  dailyNextRefresh: () => invoke<NextPaper>("daily_next_refresh"),
  pressStop: (on: boolean) => invoke<NextPaper>("press_stop", { on }),
  emailStatus: () => invoke<EmailStatus>("email_status"),
  /** Write-only: the keychain keeps it, nothing reads it back. */
  emailSetPassword: (password: string) => invoke<EmailStatus>("email_set_password", { password }),
  emailTest: () => invoke<string>("email_test"),
};
