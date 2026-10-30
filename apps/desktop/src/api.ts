// SPDX-License-Identifier: GPL-3.0-or-later
import { invoke, listen } from "./transport";
import { platform } from "./platform";
import type { Capabilities, PressSetupStatus, ExportConfig, CookbookBook, CookbookConfig, CookbookDelivery, CookbookIllustrated, CookbookIllustrateEvent, CookbookPatch, CookbookPicture, CookbookSaved, CookbookSend, RecipePreview, BookSearchReport, BackupReport, BookDetails, CatalogView, CoverChoice, CoverOffer, PreflightView, RestoreReport, Watched, BookRow, DailyConfig, DailyState, DayConfig, DayPreview, DeviceRow, DeviceShelf, EditionPreview, EmailStatus, SessionState, SiteStatus, HighlightsReport, HighlightsStatus, LibraryStatus, NextPaper, PapersView, PlaceRow, PlanRow, PreviewerStatus, ProgressEvent, MorgueDetail, MorgueHit, MorgueMonth, PutBack, ReadershipView, Rebuild, ReadingEntry, RunResult, RunTarget, ScheduleStatus, SentKindle, Settings, Suggestions, ShelfStatus, SourcesStatus, UpgradeOf, UpgradesView } from "./types";

export interface RunRequest {
  id: string;
  path: string;
  cde: string | null;
  asin: string | null;
  upgrade_of?: UpgradeOf | null;
  cover?: CoverRequest;
}

export type CoverRequest = { kind: "generated" } | { kind: "image"; path: string } | null;

/** The cover a send asks for: the picked image's path, without the thumbnail the app shows. */
export function coverRequest(c: CoverChoice | null): CoverRequest {
  if (!c || c.kind === "own") return null;
  return c.kind === "image" ? { kind: "image", path: c.path } : { kind: c.kind };
}

export const api = {
  appVersion: () => invoke<string>("app_version"),
  capabilities: () => invoke<Capabilities>("capabilities"),
  takePreloadedBooks: () => invoke<string[]>("take_preloaded_books"),
  takePreloadedTarget: () => invoke<string | null>("take_preloaded_target"),
  // `--daily` and `--kindle` start on their tabs; the Cookbook tab is never a start tab.
  startTab: () => invoke<"books" | "daily" | "kindle">("start_tab"),
  inspectBooks: (paths: string[]) => invoke<BookRow[]>("inspect_books", { paths }),
  /** The Books tab's library; `device` is the footer's Kindle, for "on this Kindle". */
  librarySearch: (query: string) => invoke<BookSearchReport>("library_search", { query }),
  libraryCatalog: (device: string | null) => invoke<CatalogView>("library_catalog", { device }),
  /** Small covers, as `[path, data URL]`. */
  libraryCovers: (paths: string[]) => invoke<[string, string | null][]>("library_covers", { paths }),
  /** What Deckle will do to a book, with the cover picked for it (Before you send). */
  bookPreflight: (path: string, cover: CoverChoice | null) => invoke<PreflightView>("book_preflight", { path, cover: coverRequest(cover) }),
  coverCandidates: (title: string, author: string, isbn: string | null) => invoke<CoverOffer[]>("cover_candidates", { title, author, isbn }),
  /** Fetches a picked Open Library cover; resolves to its path on this computer. */
  coverFetch: (large: string) => invoke<string>("cover_fetch", { large }),
  libraryFixDetails: (dir: string, details: BookDetails) => invoke<void>("library_fix_details", { dir, details }),
  libraryReadDetails: (dir: string) => invoke<BookDetails>("library_read_details", { dir }),
  librarySentKindles: () => invoke<SentKindle[]>("library_sent_kindles"),
  /** Online: only with Fill missing details on, or when the person `asked`. `null`: no record. */
  librarySuggestDetails: (dir: string, asked: boolean) => invoke<Suggestions | null>("library_suggest_details", { dir, asked }),
  libraryWatchSet: (dir: string | null) => invoke<string | null>("library_watch_set", { dir }),
  libraryWatchScan: () => invoke<Watched>("library_watch_scan"),
  /** The files bound, in order, into one book; resolves to its row (#22). */
  bindBooks: (paths: string[], title: string, author: string | null) => invoke<BookRow>("bind_books", { paths, title, author }),
  /** Web addresses, .webloc/.url link files or saved pages, each built into a book (slow: fetches). */
  importArticles: (targets: string[]) => invoke<BookRow[]>("import_articles", { targets }),
  listDevices: () => invoke<DeviceRow[]>("list_devices"),
  nameKindle: (syncId: string, name: string) => invoke<void>("name_kindle", { syncId, name }),
  previewerStatus: () => invoke<PreviewerStatus>("previewer_status"),
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<void>("set_settings", { settings }),
  /** Changes only the cookbook's image key in the settings file. */
  setIllustrationsKey: (key: string) => invoke<void>("set_illustrations_key", { key }),
  runBooks: (books: RunRequest[], target: RunTarget) =>
    invoke<void>("run_books", { books, target }),
  /** Books Deckle didn't send, sorted, with their originals; reads every book once (slow over MTP the first time). */
  deviceUpgrades: (target: RunTarget) => invoke<UpgradesView>("device_upgrades", { target }),
  /** Rebuilds the Kindle's own DRM-free copies of these books as EPUBs, for books with no original here. */
  deviceRebuild: (target: RunTarget, files: string[]) => invoke<Rebuild[]>("device_rebuild", { target, files }),
  /** Undoes an upgrade: the kept copy goes back, Deckle's goes. */
  devicePutBack: (target: RunTarget, file: string) => invoke<PutBack>("device_put_back", { target, file }),
  librarySources: () => invoke<SourcesStatus>("library_sources"),
  librarySourcesSet: (findInCalibre: boolean, calibre: string | null, folders: string[]) =>
    invoke<SourcesStatus>("library_sources_set", { findInCalibre, calibre, folders }),
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
  /** Removes each file over one session; `[file, error]` for each that couldn't go. */
  deviceRemoveMany: (target: RunTarget, files: string[]) => invoke<[string, string][]>("device_remove_many", { target, files }),
  /** Every DRM-free book and its .sdr into a new folder under `into` (default: kindle-backups/). */
  deviceBackup: (target: RunTarget, into: string | null) => invoke<BackupReport>("device_backup", { target, into }),
  deviceRestore: (target: RunTarget, dir: string) => invoke<RestoreReport>("device_restore", { target, dir }),
  highlightsStatus: () => invoke<HighlightsStatus>("highlights_status"),
  highlightsSet: (folder: string | null, clippings: boolean) => invoke<HighlightsStatus>("highlights_set", { folder, clippings }),
  /** Reads the Kindle again (its own session) and writes what's new into the folder. */
  highlightsWrite: (target: RunTarget) => invoke<HighlightsReport>("highlights_write", { target }),
  /** Every looked-up word as an Anki CSV at `to`; resolves to how many words. */
  wordsAnki: (target: RunTarget, to: string) => invoke<number>("words_anki", { target, to }),
  readershipReport: (paper: string | null = null) => invoke<ReadershipView>("readership_report", { paper }),
  readershipApply: (id: string, paper: string | null = null) => invoke<ReadershipView>("readership_apply", { id, paper }),
  readershipDismiss: (id: string, paper: string | null = null) => invoke<ReadershipView>("readership_dismiss", { id, paper }),
  readershipSet: (on: boolean) => invoke<ReadershipView>("readership_set", { on }),
  interestsAccept: (key: string, paper: string | null = null) => invoke<ReadershipView>("interests_accept", { key, paper }),
  interestsDismiss: (key: string, paper: string | null = null) => invoke<ReadershipView>("interests_dismiss", { key, paper }),
  morgueMonth: (year: number, month: number) => invoke<MorgueMonth>("morgue_month", { year, month }),
  morgueSearch: (query: string) => invoke<MorgueHit[]>("morgue_search", { query }),
  morgueShow: (date: string) => invoke<MorgueDetail | null>("morgue_show", { date }),
  /** Queues the edition for the Kindle again; resolves to the queued file name. */
  morgueSend: (date: string) => invoke<string>("morgue_send", { date }),
  morgueOpen: (date: string) => invoke<void>("morgue_open", { date }),
  shelfStatus: () => invoke<ShelfStatus>("shelf_status"),
  shelfForget: () => invoke<ShelfStatus>("shelf_forget"),
  shelfSet: (on: boolean) => invoke<ShelfStatus>("shelf_set", { on }),
  /** Opens an https link in the default browser (webview links do not). */
  openLink: (url: string) => platform.openUrl(url),
  onProgress: (cb: (e: ProgressEvent) => void): Promise<() => void> =>
    listen<ProgressEvent>("book-progress", cb),
  onRunFinished: (cb: () => void): Promise<() => void> => listen("run-finished", () => cb()),

  // The Daily tab
  dailyState: (paper: string | null = null) => invoke<DailyState>("daily_state", { paper }),
  /** Your papers: every paper's card, main first, and the household's named Kindles. */
  papersList: () => invoke<PapersView>("papers_list"),
  /** The bundled masthead faces that can't draw `title`, by key. */
  mastheadMissing: (title: string) => invoke<string[]>("masthead_missing", { title }),
  /** Writes papers/<slug>.toml; resolves to the slug. */
  paperCreate: (title: string, reader: string) => invoke<string>("paper_create", { title, reader }),
  dailySetup: () => invoke<DailyState>("daily_setup"),
  pressSetup: (paper: string | null, exportIndex: number, enabled: boolean, expected?: Pick<ExportConfig, "dir" | "format">) =>
    invoke<PressSetupStatus>("press_setup", { paper, exportIndex, enabled, expected: expected ?? null }),
  dailySave: (config: DailyConfig, paper: string | null = null) => invoke<DailyState>("daily_save", { config, paper }),
  dailyDayPreview: (day: DayConfig) => invoke<DayPreview>("daily_day_preview", { day }),
  geocode: (query: string) => invoke<PlaceRow[]>("geocode", { query }),
  readingList: () => invoke<ReadingEntry[]>("reading_list"),
  readingSave: (url: string) => invoke<string>("reading_save", { url }),
  readingRemove: (id: string) => invoke<boolean>("reading_remove", { id }),
  bookmarklet: () => invoke<string>("bookmarklet"),
  recipeBookmarklet: () => invoke<string>("recipe_bookmarklet"),
  editionPreview: (paper: string | null = null) => invoke<EditionPreview | null>("edition_preview", { paper }),
  dailyRun: (send: boolean, paper: string | null = null) => invoke<void>("daily_run", { send, paper }),
  onDailyFinished: (cb: (r: RunResult) => void): Promise<() => void> =>
    listen<RunResult>("daily-finished", cb),
  scheduleGet: () => invoke<ScheduleStatus>("schedule_get"),
  scheduleSet: (at: string | null, paper: string | null = null) => invoke<ScheduleStatus>("schedule_set", { at, paper }),
  dailyNext: (paper: string | null = null) => invoke<NextPaper>("daily_next", { paper }),
  dailyNextRefresh: (paper: string | null = null) => invoke<NextPaper>("daily_next_refresh", { paper }),
  pressStop: (on: boolean, paper: string | null = null) => invoke<NextPaper>("press_stop", { on, paper }),
  emailStatus: (paper: string | null = null) => invoke<EmailStatus>("email_status", { paper }),
  /** Write-only: the keychain keeps it, nothing reads it back. */
  emailSetPassword: (password: string) => invoke<EmailStatus>("email_set_password", { password }),
  emailTest: (paper: string | null = null) => invoke<string>("email_test", { paper }),
  subscriptionsStatus: () => invoke<SiteStatus[]>("subscriptions_status"),
  /** Write-only: the keychain keeps it, nothing reads it back. */
  subscriptionSetPassword: (site: string, password: string) => invoke<SiteStatus[]>("subscription_set_password", { site, password }),
  subscriptionTest: (site: string) => invoke<SessionState>("subscription_test", { site }),
  subscriptionForget: (site: string) => invoke<SiteStatus[]>("subscription_forget", { site }),

  // The Cookbook tab. Preview reads a page and writes nothing; add is the one call that commits.
  cookbookPreview: (target: string) => invoke<RecipePreview>("cookbook_preview", { target }),
  cookbookAdd: (target: string, chapter: string | null, note: string | null, picture: CookbookPicture | null) =>
    invoke<CookbookSaved>("cookbook_add", { target, chapter, note, picture }),
  cookbookBook: () => invoke<CookbookBook>("cookbook_book"),
  cookbookUpdate: (id: string, patch: CookbookPatch) => invoke<CookbookSaved>("cookbook_update", { id, patch }),
  cookbookRemove: (id: string) => invoke<void>("cookbook_remove", { id }),
  cookbookSaveConfig: (cfg: CookbookConfig) => invoke<void>("cookbook_save_config", { cfg }),
  cookbookSendNow: () => invoke<CookbookSend>("cookbook_send_now"),
  /** Builds the book; resolves to the EPUB's path. */
  cookbookPreviewBook: () => invoke<string>("cookbook_preview_book"),
  cookbookDelivery: () => invoke<CookbookDelivery>("cookbook_delivery"),
  cookbookSetKindle: (id: string, on: boolean) => invoke<void>("cookbook_set_kindle", { id, on }),
  /** Draws one recipe's picture (`id`) or every recipe's (`all`); progress comes as `cookbook-illustrate` events. */
  cookbookIllustrate: (id: string | null, all: boolean, redraw: boolean) => invoke<CookbookIllustrated>("cookbook_illustrate", { id, all, redraw }),
  cookbookIllustrateStop: () => invoke<void>("cookbook_illustrate_stop"),
  onCookbookIllustrate: (cb: (e: CookbookIllustrateEvent) => void): Promise<() => void> =>
    listen<CookbookIllustrateEvent>("cookbook-illustrate", cb),
};
