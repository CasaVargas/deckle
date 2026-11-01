// SPDX-License-Identifier: GPL-3.0-or-later
/** `ready`: a finished KFX, sent as is. */
export type Mode = "full" | "cover" | "error" | "ready";
export type RunStatus = "idle" | "queued" | "running" | "done" | "failed";

export interface BookRow {
  id: string;
  path: string;
  title: string;
  authors: string;
  series: string | null;
  cover: string | null;
  mode: Mode;
  reason: string;
  warnings: string[];
  error: string | null;
}

export interface BookState extends BookRow {
  status: RunStatus;
  stage: string | null;
  delivered: string | null;
  pages: number | null;
  runWarnings: string[];
  runError: string | null;
  /** What shipped (from the final progress event). */
  shipped: { cde: string; asin: string; reason: string; matched: string | null } | null;
  /** X-Ray sidecar summary (from the final progress event). */
  xray: { people: number; terms: number; source: "llm" | "wikipedia" | "basic" } | null;
  /** Word Wise sidecar summary (from the final progress event). */
  wordWise: WordWiseSummary | null;
  /** Per-book override of the settings' cde. */
  cdeOverride: "pdoc" | "ebok" | null;
  /** The book's Amazon ASIN, typed by the owner: EBOK then gets the store cover. */
  asin: string | null;
  showReport: boolean;
  /** The last run went to a folder (Export to folder…), not a Kindle. */
  exported: boolean;
  /** Older copies this send removed from the Kindle. */
  replaced: string[];
  /** The Kindle's own copy this send upgrades: replaced, and kept for Put back. */
  upgradeOf: UpgradeOf | null;
  /** The cover picked in Before you send; null: the book's own. */
  coverPick: CoverChoice | null;
}

/** A cover picked on the Books tab (Books and Kindle design §8). */
export type CoverChoice = { kind: "own" } | { kind: "generated" } | { kind: "image"; path: string; thumb: string };

/** A book in the Books tab's library: Deckle's own, calibre's or a folder's. */
export interface CatalogEntry {
  path: string;
  from: "library" | "calibre" | "folder";
  title: string;
  authors: string[];
  series: { name: string; index: number | null } | null;
  isbn: string | null;
  /** Its folder in Deckle's library: its details can be fixed. */
  library_dir: string | null;
  sent_to: string[];
  added: string | null;
  /** On the Kindle picked in the footer. */
  on_kindle: boolean;
}

export interface CatalogView {
  entries: CatalogEntry[];
  library: boolean;
  watch: string | null;
  warnings: string[];
}

/** The first page of chapter one, for the page preview. */
export interface Chapter {
  href: string;
  title: string | null;
  html: string;
  css: string;
}

/** What Deckle will do to a book (Before you send). */
export interface PreflightView {
  typography: { quotes: number; dashes: number; ellipses: number; empty_paragraphs: number } | null;
  notes: number | null;
  notes_skipped: number;
  cover: "own" | "generated" | "picked" | "none";
  cover_url: string | null;
  images_downscaled: number;
  toc_synthesized: boolean;
  warnings: string[];
  before: Chapter | null;
  after: Chapter | null;
}

export interface CoverOffer {
  thumb: string;
  large: string;
}

export interface Watched {
  added: string[];
  failed: [string, string][];
}

export interface BookDetails {
  title: string;
  authors: string[];
  series: { name: string; index: number | null } | null;
  // The rest: left out or null leaves the book's own; empty removes it (Books tab redesign §5).
  description?: string | null;
  tags?: string[] | null;
  publisher?: string | null;
  /** A year, or a date like 2020-09-15. */
  published?: string | null;
  /** A language tag; empty keeps the book's own. */
  language?: string | null;
  isbn?: string | null;
  title_sort?: string | null;
  author_sort?: string | null;
}

/** What Open Library has for a library book's empty fields (Books tab redesign §6). */
export interface Suggested {
  authors: string[];
  series: { name: string; index: number | null } | null;
  description: string | null;
  tags: string[];
  publisher: string | null;
  published: string | null;
  language: string | null;
}

export interface Suggestions {
  /** "isbn": the book's own edition; "search": the work found by title and author. */
  source: "isbn" | "search";
  suggested: Suggested;
}

/** A Kindle the library has sent to, for the Kindle tab when it isn't plugged in (Kindle tab redesign §7). */
export interface SentKindle {
  key: string;
  name: string;
  last: string;
  books: { path: string; title: string; authors: string[]; at: string; cde: string | null }[];
}

/** A book on a Kindle that a send replaces with Deckle's build of its original (Books and Kindle design §6). */
export interface UpgradeOf {
  /** The Kindle's profile key. */
  device: string;
  file: string;
  title: string;
  origin: Origin | null;
}

/** Where an upgrade's EPUB came from. */
export type Origin = "calibre" | "folder" | "kindle-copy";

export interface WordWiseSummary {
  hints: number;
  level: number;
  /** `device`: the Kindle's own English dictionary; `deckle`: Deckle's, in the 中文 slot. */
  dictionary: "device" | "deckle";
  downloaded: boolean;
}

export interface DeviceRow {
  key: string;
  kind: "mass_storage" | "mtp";
  root: string | null;
  bus: number;
  address: number;
  model_hint: string;
  thumbnails_writable: boolean;
  cde_override: "pdoc" | "ebok" | null;
  verified_real_asin: boolean;
  /** Last known firmware; an MTP Kindle's is recorded by its first send. */
  firmware: string | null;
  /** Known issues for this model, transport and firmware (warnings, never refusals). */
  compat: string[];
  /** The id the Daily and the outbox know this Kindle by. */
  sync_id: string;
  /** The name its owner gave it, if any. */
  name: string | null;
}

export interface PlanRow {
  id: string;
  cde: "pdoc" | "ebok";
  write_thumbnail: boolean;
  reason: string;
}

export interface Settings {
  book_presets: Record<string, StylePreset>;
  previewer_path: string | null;
  cde: "auto" | "pdoc" | "ebok";
  pages_auto: boolean;
  llm_provider: "off" | "anthropic" | "openai";
  llm_model: string;
  llm_endpoint: string;
  llm_api_key: string;
  /** The key the cookbook's illustrations send to their image endpoint; empty uses OPENAI_API_KEY. Never X-Ray's. */
  illustrations_api_key: string;
  xray: boolean;
  word_wise: boolean;
  word_wise_level: number;
  wikipedia: boolean;
  /** An AI-written recap at the end of each chapter (issue #29); needs the LLM settings. */
  chapter_recaps: boolean;
  /** A summary page after the last chapter, made from the recaps (issue #29); needs the LLM settings. */
  book_summary: boolean;
  amazon_match: boolean;
  /** A typographic cover for a book that has none (issue #27). */
  generated_cover: boolean;
  /** Fill empty title, author, series, description and subjects from Open Library (network). */
  metadata_fill: boolean;
  /** Curly quotes, dashes, ellipses and empty-paragraph cleanup (issue #30). */
  typography: boolean;
  /** Footnotes and endnotes to Kindle pop-up notes (issue #31). */
  popup_notes: boolean;
}

export interface PreviewerStatus {
  found: boolean;
  path: string | null;
  version: string | null;
  download_url: string;
}

/** What this install can do: the web app has no keychain, updater, file manager or system jobs. */
export interface Capabilities {
  web: boolean;
  previewer: boolean;
  usb: boolean;
  keychain: boolean;
  updater: boolean;
  reveal: boolean;
  /** "install": a system job runs the edition; "config": the daemon reads `[schedule]`. */
  schedule: "install" | "config";
}

export interface ProgressEvent {
  id: string;
  status: "running" | "done" | "failed";
  stage: string | null;
  delivered: string | null;
  pages: number | null;
  warnings: string[];
  error: string | null;
  cde?: "PDOC" | "EBOK" | null;
  asin?: string | null;
  reason?: string | null;
  matched?: string | null;
  xray?: { people: number; terms: number; source: "llm" | "wikipedia" | "basic" } | null;
  word_wise?: WordWiseSummary | null;
  /** Older copies removed from the Kindle (Kindle-updates design §4). */
  replaced?: string[];
}

export type RunTarget =
  | { kind: "device"; root: string }
  | { kind: "mtp"; bus: number; address: number }
  | { kind: "folder"; path: string };

export const STAGES = ["inspect", "normalize", "convert", "repackage", "enrich", "deliver"] as const;
export const STAGE_LABEL: Record<string, string> = {
  inspect: "Reading",
  normalize: "Cleaning",
  convert: "Kindle Previewer",
  repackage: "Building KFX",
  enrich: "X-Ray & Word Wise",
  deliver: "Copying",
  match: "Matching edition",
  fill: "Looking up details",
};

// ---- The Daily tab (Daily design §5c) ----

export type SectionKind = "reading" | "weather" | "local" | "interests" | "feeds" | "subscribed" | "shelf" | "lottery" | "horoscope" | "puzzle" | "onthisday";

/** One `[[interests]]` entry (interests design §3). */
export interface InterestConfig {
  name: string;
  query?: string;
  also?: string[];
  not?: string[];
  feeds?: string[];
  max_items: number;
  full_text?: boolean | "lead";
  max_age_hours?: number;
}

/** `[learning]`: what the paper notices about your reading (interests design §5). */
export interface LearningConfig {
  enabled: boolean;
  window_days: number;
  min_stories: number;
  nudges: boolean;
}

export interface FeedConfig {
  name: string;
  url: string;
  urls?: string[];
  topic?: string;
  full_text?: boolean | "lead";
  max_items: number;
}

/** `digest.toml`, as the backend's `deckle_digest::Config` serializes it. */
export interface DailyConfig {
  llm?: DailyEditorConfig;
  title: string;
  /** Who the paper is for. Absent while empty. */
  reader?: string;
  /** The Kindle it goes to, by name or sync id. Absent: any Kindle. */
  kindle?: string;
  /** A layout for big type. Absent while off. */
  large_print?: boolean;
  /** `[schedule]`: when it's built. Absent while untouched. */
  schedule?: { at?: string; days?: Day[] };
  /** The title's typeface: absent for the blackletter, else "serif", "sans" or a .ttf path. */
  masthead?: string;
  place: string;
  latitude: number | null;
  longitude: number | null;
  units: "imperial" | "metric";
  keep: number;
  max_age_hours: number;
  full_text: boolean;
  photos: boolean;
  max_photos: number;
  sections: SectionKind[];
  local: { google_news: string; feeds: string[]; max_items: number; max_age_hours: number; places?: PlaceNews[] };
  /** `[[interests]]`; absent while there are none. */
  interests?: InterestConfig[];
  feeds: FeedConfig[];
  /** `[learning]`; absent while untouched. */
  learning?: LearningConfig;
  lottery: { games: string[] };
  horoscope: { signs: string[] };
  reading: { inbox: string; max_items: number; long_read_words: number };
  /** Absent while it's untouched (the backend leaves the defaults out). */
  email?: EmailConfig;
  /** `[[subscriptions]]`: newspapers the household subscribes to. Absent while empty. No passwords here. */
  subscriptions?: SubscriptionConfig[];
  /** `[[subscribed]]`: this paper's subscribed publisher groups. Absent while empty. */
  subscribed?: SubscribedConfig[];
  /** `[day]`: Your Day on the weather page. Absent while untouched. */
  day?: DayConfig;
  /** `[morgue]`: where every edition is kept. Absent while untouched. */
  morgue?: { folder: string; keep_days: number };
  /** `[[export]]`: folders each edition also goes to (other readers design §7). Absent while empty. */
  export?: ExportConfig[];
  /** `[remarkable]`: the paper as a PDF to a reMarkable (other readers design §9). Absent while untouched. */
  remarkable?: RemarkableConfig;
  /** `[reader_server]`: the OPDS catalog and KOReader sync (KOReader hub design §A). Absent while untouched. */
  reader_server?: ReaderServerConfig;
  /** `[wireless]`: calibre's wireless connection for KOReader (calibre bridge design §4). Absent while untouched. */
  wireless?: WirelessConfig;
}

/** `[wireless]`: served by the watcher on the home network. The password isn't here. */
export interface WirelessConfig {
  enabled: boolean;
  port: number;
  discovery: boolean;
  library_name: string;
  /** A reader that connects gets the newest paper and keeps the last three. */
  paper: boolean;
}

export const DEFAULT_WIRELESS: WirelessConfig = { enabled: false, port: 9090, discovery: true, library_name: "Deckle", paper: true };

/** A book a reader reported over the wireless connection. */
export interface OnReader {
  lpath: string;
  uuid: string | null;
  title: string | null;
  authors: string[];
  last_modified: string | null;
  size: number | null;
}

/** A reader that has connected. */
export interface WirelessReader {
  /** Its storage id: what a send names it by. */
  id: string;
  name: string;
  kind: string;
  connected: boolean;
  last_seen: string;
  free: number | null;
  books: OnReader[];
}

/** One waiting job: a book to send (`send`), or one to take off (`remove`). */
export interface WirelessJob {
  id: string;
  reader: string;
  send?: string;
  remove?: string;
  build?: boolean;
  added: string;
}

export interface WirelessView {
  /** `[wireless] enabled`. */
  enabled: boolean;
  password_set: boolean;
  /** `address:port`, to type into a reader that doesn't find Deckle itself. */
  address: string;
  discovery: boolean;
  readers: WirelessReader[];
  queue: WirelessJob[];
  log: string[];
}

/** `[reader_server]`: served by the watcher on the home network. The password isn't here. */
export interface ReaderServerConfig {
  enabled: boolean;
  port: number;
  username: string;
}

export const DEFAULT_READER_SERVER: ReaderServerConfig = { enabled: false, port: 47220, username: "deckle" };

/** The reader server card's state: whether a password is set (never the password) and the URLs. */
export interface ReaderServerStatus {
  password_set: boolean;
  opds: string;
  sync: string;
}

/** A screen the PDF edition is laid out for (PDF edition design §5). */
export type Screen = "remarkable-2" | "remarkable-paper-pro" | "remarkable-paper-pro-move" | "ipad" | "a5";

export const SCREEN_LABEL: Record<Screen, string> = {
  "remarkable-2": "reMarkable 2",
  "remarkable-paper-pro": "reMarkable Paper Pro",
  "remarkable-paper-pro-move": "reMarkable Paper Pro Move",
  ipad: "iPad",
  a5: "A5 page",
};

/** `[remarkable]`: plugged in with its USB web interface on. */
export interface RemarkableConfig {
  enabled: boolean;
  screen: Screen;
  /** Absent: the screen's own choice (color on a Paper Pro). */
  photos?: "color" | "gray";
  books: boolean;
}

export interface PressSetupStatus {
  enabled: boolean;
  watcher_ready: boolean;
  error: string | null;
  producer_id: string | null;
  paper_id: string | null;
}

/** One `[[export]]` folder: an iCloud Drive folder for an iPad, a Dropbox one for a Boox or a
 * Kobo. */
export interface ExportConfig {
  /** Activated only by explicit request setup. */
  press_enabled?: boolean;
  name: string;
  dir: string;
  format: "epub" | "kepub" | "pdf" | "bundle";
  photos: "color" | "gray";
  /** With a PDF: the screen it's laid out for. */
  screen: Screen;
  /** Editions kept in the folder; 0 keeps all. */
  keep: number;
  /** Also the long reads, not only the paper. */
  books: boolean;
}

/** What one export folder got in a run (`deckle_digest::export::ExportReport`). */
export interface ExportReport {
  name: string;
  dir: string;
  written: string[];
  pruned: string[];
  error?: string;
}

/** `[[local.places]]`: another place's local news, its own section after Local (#102). */
export interface PlaceNews {
  name: string;
  google_news: string;
  feeds: string[];
  /** Stories; `[local] max_items` when absent. */
  max_items?: number;
}

/** Calendars for Your Day (almanac design §5). Addresses are secrets: show `DayPreview.labels`. */
export interface DayConfig {
  calendars: string[];
  max_items: number;
  weather_note: boolean;
}

export interface DayPreview {
  events: number;
  /** Each calendar as it may be shown: a file name, or `https://host/…`. */
  labels: string[];
  errors: string[];
}

/** `[email]`: Send-to-Kindle over the owner's SMTP (email design §5). No password here. */
export interface EmailConfig {
  enabled: boolean;
  to: string;
  from: string;
  smtp_host: string;
  smtp_port: number;
  username: string;
  usb_too: boolean;
}

export const DEFAULT_EMAIL: EmailConfig = {
  enabled: false,
  to: "",
  from: "",
  smtp_host: "",
  smtp_port: 587,
  username: "",
  usb_too: false,
};

/** Whether a password is saved: the password itself never reaches the window. */
export interface EmailStatus {
  configured: boolean;
  has_password: boolean;
}

/** `[[subscriptions]]`: a newspaper account. The password lives in the keychain. */
export interface SubscriptionConfig {
  site: string;
  username: string;
  kind?: string | null;
}

/** `[[subscribed]]`: a subscribed publisher's sections, printed as a group. Paper-file setting. */
export interface SubscribedConfig {
  site: string;
  name?: string | null;
  sections: string[];
  max_items?: number | null;
  max_age_hours?: number | null;
}

/** Where a site's session stands: serde's externally-tagged form of the Rust enum. */
export type SessionState =
  | "NotTried"
  | { SignedIn: { service: string | null } }
  | { LoginFailed: string }
  | "NoAdapter"
  | { NoPassword: string }
  | { EntitlementUnknown: string };

/** A subscription as the card shows it. Never carries the password. */
export interface SiteStatus {
  site: string;
  username: string;
  kind: string | null;
  has_password: boolean;
  state: SessionState;
  jar_note: string | null;
}

export interface DailyState {
  config_path: string;
  exists: boolean;
  config: DailyConfig | null;
  error: string | null;
}

export interface ReadingEntry {
  id: string;
  url: string | null;
  file: string | null;
  title: string | null;
  saved_at: string;
  source: string;
  state: "pending" | "printed" | "book" | "failed";
  tries: number;
  printed_in: string | null;
  note: string | null;
}

export interface EditionPreview {
  title: string;
  file: string;
  cover: string | null;
  outbox: number;
}

export interface RunResult {
  ok: boolean;
  report: any;
  log: string;
}

export interface ScheduleStatus {
  at: string | null;
  watch: boolean;
  exe: string | null;
  /** Every paper's installed edition job, main first. */
  papers?: { slug: string; at: string | null; days: Day[] }[];
}

/** `[schedule] days`. */
export type Day = "mon" | "tue" | "wed" | "thu" | "fri" | "sat" | "sun";

/** A card in Your papers (many papers §7). `main` is digest.toml's paper. */
export interface PaperCard {
  slug: string;
  title: string;
  reader: string;
  sections: string[];
  /** A Kindle's name or sync id; empty for any Kindle. */
  kindle: string;
  email_to: string | null;
  usb_too: boolean;
  at: string | null;
  days: Day[];
  large_print: boolean;
  installed: boolean;
}

export interface KnownKindle {
  sync_id: string;
  name: string;
}

export interface PapersView {
  papers: PaperCard[];
  notes: string[];
  kindles: KnownKindle[];
}

export interface PlaceRow {
  label: string;
  latitude: number;
  longitude: number;
}

export const SECTION_LABEL: Record<SectionKind, string> = {
  reading: "Your Reading",
  weather: "Weather & almanac",
  local: "Local news",
  interests: "Your interests",
  feeds: "Your feeds",
  subscribed: "Subscribed papers",
  shelf: "From Your Shelf",
  lottery: "Lottery",
  horoscope: "Horoscope",
  puzzle: "Puzzle",
  onthisday: "On This Day",
};

export const ALL_SECTIONS: SectionKind[] = ["reading", "weather", "local", "interests", "feeds", "subscribed", "shelf", "lottery", "horoscope", "puzzle", "onthisday"];

export const SIGNS = ["aries", "taurus", "gemini", "cancer", "leo", "virgo", "libra", "scorpio", "sagittarius", "capricorn", "aquarius", "pisces"];

/** library.json, as Settings shows it (library design §3). */
export interface LibraryStatus {
  root: string | null;
  auto_add: boolean;
  books: number;
}

/** One book in a Kindle's documents/ (library design §4). */
export interface DeviceBookRow {
  file: string;
  size: number;
  format: string;
  title: string | null;
  authors: string[];
  cde: "PDOC" | "EBOK" | null;
  from_deckle: boolean;
  xray: boolean;
  word_wise: boolean;
  cover: string | null;
  /** The library's source file, for Re-send and Upgrade. */
  library_source: string | null;
  /** Why Deckle could build it better now (Kindle-updates design §5). */
  updates: { older: boolean; feature: "X-Ray" | "Word Wise" | null; text: string }[];
  duplicate: boolean;
  /** Its upgrade class (Books and Kindle design §3). */
  class: UpgradeClass;
  kind: BookKind;
  /** How far in, 0–1. */
  fraction: number | null;
  reading: ReadingState;
  /** When it was last read, UTC. */
  last_read: string | null;
}

/** What a book on a Kindle is, for the space it takes and the Shelf's filters. */
export type BookKind = "amazon-own" | "deckle" | "paper" | "sideloaded";
export type ReadingState = "not-started" | "reading" | "finished";

/** The space on a Kindle by kind; `other` is the system, dictionaries and fonts. */
export interface SpaceSplit {
  total: number;
  free: number;
  /** `[kind, bytes, books]`, largest first. */
  kinds: [BookKind, number, number][];
  other: number;
}

/** A book that could go to make room. */
export type Spare = { file: string; title: string; size: number; has_sdr: boolean } & ({ why: "finished" } | { why: "duplicate"; keeping: string } | { why: "old-paper" });

export interface Backup {
  dir: string;
  device: string;
  at: string;
  books: number;
  size: number;
}

export interface BackupReport {
  dir: string;
  copied: string[];
  /** `[file, why]` */
  skipped: [string, string][];
  failed: [string, string][];
}

export interface RestoreReport {
  restored: string[];
  already: string[];
  failed: [string, string][];
}

export interface DeviceShelf {
  device: string;
  space: { total: number; free: number } | null;
  books: DeviceBookRow[];
  /** The Kindle's highlights by book, as the next export would write them. */
  highlights: HighlightRow[];
  highlights_error: string | null;
  /** This Kindle's room (design §7). */
  split: SpaceSplit | null;
  spare: Spare[];
  backups: Backup[];
  /** Reading positions left unread past the per-connection cap. */
  unread_positions: number;
}

/** One book's highlights (highlights export design §6). */
export interface HighlightRow {
  key: string;
  title: string;
  author: string;
  /** The file's name in the highlights folder. */
  file: string;
  total: number;
  new: number;
  /** The whole new file, or the block that would be appended; empty when nothing is new. */
  markdown: string;
}

export interface HighlightsStatus {
  folder: string | null;
  /** `[sync] clippings`: the morning schedule writes highlights on plug-in. */
  clippings: boolean;
}

export interface HighlightsReport {
  written: { file: string; new: number; created: boolean }[];
  /** `[file, error]` */
  failed: [string, string][];
  parse_error?: { line: number; message: string };
}

/** One section of the readership report (readership design §6). */
export interface ReadershipSection {
  key: string;
  name: string;
  /** Percent of opened editions where you got to it; null until it appeared in 5. */
  share: number | null;
  seenIn: number;
}

export interface ReadershipSuggestion {
  id: string;
  kind: "drop" | "moveUp" | "stop";
  key: string;
  title: string;
  why: string;
}

export interface ReadershipReport {
  window: number;
  opened: number;
  medianPercent: number | null;
  stopSection: string | null;
  sections: ReadershipSection[];
  suggestions: ReadershipSuggestion[];
}

export interface ReadershipView {
  /** `[sync] readership` in digest.toml. */
  enabled: boolean;
  report: ReadershipReport;
  /** What Deckle noticed about your reading (interests design §5.4). */
  noticed: Noticed[];
}

/** A subject that recurred in what you read; one click makes it an interest. */
export interface Noticed {
  key: string;
  name: string;
  why: string;
  stories: number;
  days: number;
  signal: "tap" | "highlight" | "shelf";
  query: string | null;
}

/** A day with a kept edition (Morgue design §4). `date` is YYYY-MM-DD. */
export interface MorgueDay {
  date: string;
  paper: string;
  lead: string | null;
}

export interface MorgueMonth {
  days: MorgueDay[];
  editions: number;
  bytes: number;
  folder: string;
  /** "Tomorrow, 05:30": when the next paper is made, if a schedule is installed. */
  nextBuild: string | null;
  /** The date that label is for, YYYY-MM-DD. */
  nextBuildDate: string | null;
}

/** An edition with stories matching a search: (section title, story title) each. */
export interface MorgueHit {
  date: string;
  paper: string;
  stories: [string, string][];
}

export interface MorgueDetail {
  heading: string;
  weather: string | null;
  lead: { section: string; title: string; teaser: string | null } | null;
  read: string | null;
  highlights: string | null;
  onKindle: string | null;
  epub: string;
}

/** What the last Kindle shared with From Your Shelf (shelf design §7). */
export interface ShelfStatus {
  /** The Kindle plugged in last; null before any. */
  kindle: string | null;
  /** When, as an ISO instant. */
  seen: string | null;
  inProgress: number;
  highlightsShared: boolean;
  wordsShared: boolean;
  /** Emailed papers pass through Amazon. */
  emailOn: boolean;
  /** `[sync] shelf` in digest.toml. */
  enabled: boolean;
}

/** The press (tomorrow's paper design §4): `next` and `since` are ISO instants with offsets. */
export interface Press {
  at: string | null;
  next: string | null;
  /** YYYY-MM-DD of the paper the preview shows. */
  edition: string;
  stopped: string | null;
  since: string;
}

export interface Tile {
  key: string;
  kicker: string;
  title: string | null;
  reason: string | null;
  dashed: boolean;
}

export interface PagePlanRow {
  key: string;
  kind: SectionKind;
  /** The `[[feeds]]` index of a feed row. */
  feed: number | null;
  /** The `[[local.places]]` index of another place's row. */
  place: number | null;
  /** The `[[subscribed]]` group and slug of a subscribed paper's row. */
  subscribed?: { site: string; slug: string };
  /** The `[[interests]]` index of an interest row. */
  interest: number | null;
  name: string;
  detail: string;
  budget: string;
  /** The editable budget (`max_items`); null for fixed sections. */
  items: number | null;
}

export type WeekState =
  | { state: "read"; percent: number }
  | { state: "notOpened" }
  | { state: "sent" }
  | { state: "noPaper" };

export interface WeekDay {
  date: string;
  state: WeekState;
}

export interface NextPaper {
  press: Press;
  paper: string;
  place: string;
  date: string;
  number: number;
  weather: boolean;
  lead: Tile | null;
  tiles: Tile[];
  checked: string | null;
  plan: PagePlanRow[];
  off: PagePlanRow[];
  pages: number;
  week: WeekDay[];
}

/** What a book on a Kindle is, for upgrading (Books and Kindle design §3). */
export type UpgradeClass =
  | "current"
  | "older"
  | "has-source"
  | "kindle-copy"
  | "old-mobi"
  | "no-source"
  | "amazon-own"
  | "locked"
  | "fixed-layout"
  | "pdf"
  | "other"
  | "unread";

/** A book the Kindle tab can upgrade from its original on this computer. */
export interface UpgradeRow {
  file: string;
  title: string;
  authors: string[];
  format: string;
  class: UpgradeClass;
  /** The original; null for a book rebuilt from the Kindle's copy when it's upgraded. */
  source: string | null;
  origin: Origin;
  fraction: number | null;
  partway: boolean;
  gains: string[];
  cover: string | null;
}

export interface UpgradeHistoryRow {
  old_file: string;
  new_file: string | null;
  title: string;
  origin: Origin | null;
  at: string;
}

/** One book the rebuild tried: its EPUB, or why not. */
export interface Rebuild {
  file: string;
  source: string | null;
  error: string | null;
}

/** Where originals are looked for (Settings → General → Library). */
export interface SourcesStatus {
  find_in_calibre: boolean;
  calibre: string | null;
  folders: string[];
  /** The folder whose new EPUBs are added to the library. */
  watch?: string | null;
}

export interface UpgradesView {
  device: string;
  rows: UpgradeRow[];
  counts: Partial<Record<UpgradeClass, number>>;
  history: UpgradeHistoryRow[];
  sources: SourcesStatus;
  warnings: string[];
}

export interface PutBack {
  restored: string;
  removed: string[];
}

/** Search combines live device inventories with explicitly labelled send history. */
export interface BookSearchReport {
  results: {
    title: string;
    authors: string[];
    series: { name: string; index: number | null } | null;
    locations: { kind: "library" | "connected" | "recorded_send"; device: string | null; name: string; path: string }[];
  }[];
  warnings: string[];
}

export type StylePreset = "academic" | "novel" | "manual";

export interface DailyEditorConfig {
  enabled: boolean;
  provider: "openai" | "anthropic";
  model: string;
  endpoint: string;
}

// The Cookbook tab (cookbook design §8.2). These mirror the DTOs in src-tauri/src/cookbook.rs.

/** Which picture a recipe, or the whole book, prints. `default` is a recipe following the book. */
export type CookbookPicture = "site" | "illustration" | "none" | "default";

export interface CookbookChapter {
  id: string;
  name: string;
  /** Match keywords; edited in cookbook.toml only. */
  match: string[];
}

export interface CookbookConfig {
  title: string;
  subtitle: string;
  picture: Exclude<CookbookPicture, "default">;
  chapters: CookbookChapter[];
  illustrations: { enabled: boolean; endpoint: string; model: string };
}

/** A fetched page, as it would be filed. Nothing is written until it is added. */
export interface RecipePreview {
  title: string;
  site_name: string;
  source_url: string;
  description: string | null;
  yield: string | null;
  /** Minutes. */
  prep: number | null;
  cook: number | null;
  total: number | null;
  ingredients: number;
  steps: number;
  /** Read from recipe data (true) or from the page text (false). */
  structured: boolean;
  chapter: string;
  chapter_name: string;
  /** Already in the book: adding says "Updated". */
  exists: boolean;
  /** A JPEG data URL. */
  picture: string | null;
}

export interface CookbookRecipe {
  id: string;
  title: string;
  site_title: string;
  title_override: string | null;
  chapter: string;
  home: string;
  home_name: string;
  added: string;
  is_new: boolean;
  structured: boolean;
  site_name: string;
  url: string;
  note: string | null;
  picture: CookbookPicture;
  printed: Exclude<CookbookPicture, "default">;
  has_illustration: boolean;
  thumb: string | null;
  ingredients: string[];
}

export interface CookbookSaved extends CookbookRecipe {
  stored: "added" | "updated";
  picture_note: string | null;
}

export interface CookbookChapterRows {
  id: string;
  name: string;
  match: string[];
  recipes: CookbookRecipe[];
}

export interface CookbookBook {
  config: CookbookConfig;
  chapters: CookbookChapterRows[];
  recipes: number;
  printed_chapters: number;
  newest: { id: string; title: string; added: string } | null;
  /** A JPEG data URL: the cover as it will print. */
  cover: string | null;
  pending_build: boolean;
  build_error: string | null;
  config_path: string;
}

export interface CookbookPatch {
  title_override?: string;
  chapter?: string;
  picture?: CookbookPicture;
  note?: string;
}

export interface CookbookSend {
  build: string;
  result: "sent" | "up_to_date" | "off" | "all_current" | "queued";
  kindle: string | null;
  message: string;
  built: number | null;
}

export type CookbookKindleState = "up_to_date" | "behind" | "never_sent" | "off";

export interface CookbookKindle {
  name: string;
  id: string;
  status: string;
  state: CookbookKindleState;
  behind: number | null;
}

export interface CookbookDelivery {
  kindles: CookbookKindle[];
  watcher_installed: boolean;
  error: string | null;
}

export interface CookbookIllustrateEvent {
  n: number;
  total: number;
  id: string;
  title: string;
  result: "start" | "drawn" | "kept_photo";
}

export interface CookbookIllustrated {
  total: number;
  drawn: number;
  kept: number;
  already: number;
  stopped: boolean;
}

/** A KOReader plugin on the reader (`deckle_core::koreader_plugins::Plugin`). */
export interface KoPlugin {
  name: string;
  fullname: string;
  description?: string | null;
  version?: string | null;
  builtin: boolean;
  enabled: boolean;
  /** Where Deckle installed it from. */
  source?: { from: string; release?: string | null; installed: string };
  /** On (true) or off after KOReader restarts with the Deckle plugin. */
  pending?: boolean;
}

/** Settings → KOReader (KOReader plugins design, phase 4). */
export interface KoreaderView {
  device: string | null;
  root: string | null;
  listing: { data_dir: string; plugins: KoPlugin[]; patches: string[]; patches_disabled: boolean } | null;
  deckle_version: string;
  /** Why the Deckle plugin can't be installed now. */
  deckle_blocked: string | null;
  note: string | null;
}

/** What an install would do, before it's done. */
export interface KoreaderPreview {
  name: string;
  fullname: string;
  version: string | null;
  replaces: boolean;
  from: string;
  release: string | null;
  /** The release Deckle reviewed: a Recommended pin, or the Deckle plugin. */
  reviewed: boolean;
}

/** A plugin on Deckle's Recommended list, pinned to a reviewed release. */
export interface StorePick {
  repo: string;
  name: string;
  kind: string;
  why: string;
  license: string;
  release: { tag: string; asset: string; sha256: string };
}

/** A repository Discover found (not reviewed). */
export interface StoreFound {
  repo: string;
  description: string | null;
  stars: number;
  pushed: string;
  license: string | null;
  url: string;
}

/** A newer release of an installed plugin. */
export interface StoreUpdate {
  name: string;
  repo: string;
  installed: string | null;
  latest: string;
  pinned: boolean;
}

/** Which calibre column each piece of reading data goes in, by label without `#` (calibre bridge design §5). */
export interface WritebackColumns {
  progress: string | null;
  read: string | null;
  last_read: string | null;
  rating: string | null;
  /** A long-text column, written as Markdown. */
  highlights: string | null;
}

export interface WritebackConfig {
  enabled: boolean;
  calibredb: string | null;
  server: string | null;
  username: string | null;
  columns: WritebackColumns;
  /** Highlights also into calibre's own annotations. */
  annotations: boolean;
  confirmed: Record<string, string>;
  last_run: string | null;
  last_result: string[];
}

export interface WritebackChange {
  uuid: string;
  book_id: number;
  title: string;
  field: "progress" | "read" | "last_read" | "rating" | "highlights";
  column: string;
  old: string | null;
  new: string;
}

export interface WritebackConflict {
  title: string;
  field: WritebackChange["field"];
  column: string;
  calibre: string;
  reader: string;
}

export interface WritebackToConfirm {
  key: string;
  reader: string;
  title: string;
  authors: string[];
  uuid: string;
  calibre_title: string;
}

/** One book's highlights for calibre's own annotations. */
export interface WritebackAnnotations {
  uuid: string;
  book_id: number;
  title: string;
  format: string;
  count: number;
}

export interface WritebackPlan {
  changes: WritebackChange[];
  conflicts: WritebackConflict[];
  to_confirm: WritebackToConfirm[];
  unmatched: string[];
  annotations: WritebackAnnotations[];
}

export interface WritebackStatus {
  library: string | null;
  calibredb: string | null;
  writeback: WritebackConfig;
  /** Each field's possible columns, by label. */
  choices: Partial<Record<keyof WritebackColumns, string[]>>;
  plan: WritebackPlan | null;
  error: string | null;
  /** This calibredb can import annotations (calibre 9.18 or later). */
  annotations_supported: boolean;
}

/** A change to the write-back settings; a field left out stays. */
export interface WritebackPatch {
  enabled?: boolean;
  columns?: WritebackColumns;
  annotations?: boolean;
  server?: string;
  username?: string;
  calibredb?: string;
}

/** A rule that makes a shelf from calibre (calibre bridge design §6.1). */
export interface ShelfRule {
  /** The shelf's name; empty for `series:*`, which names one per series. */
  name: string;
  /** `tag:…`, `series:…`, `author:…`, `column:#label=value` or `search:…`. */
  from: string;
  /** `all`, or reader keys. */
  readers: string[];
}

export interface ShelvesConfig {
  rules: ShelfRule[];
  kobo_ok: string[];
  auto: boolean;
}

export interface ShelfDiff {
  create: string[];
  delete: string[];
  add: Record<string, string[]>;
  remove: Record<string, string[]>;
}

export interface ShelfTarget {
  kind: "kobo" | "koreader";
  key: string;
  diff: ShelfDiff;
  applied: boolean;
  note: string | null;
}

export interface ShelvesView {
  config: ShelvesConfig;
  /** The plugged-in reader: its key and name. */
  reader: [string, string] | null;
  targets: ShelfTarget[];
  /** Each shelf the rules make, with how many calibre books. */
  sets: [string, number][];
  error: string | null;
}
