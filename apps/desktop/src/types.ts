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
  /** Older copies this send removed from the Kindle. */
  replaced: string[];
}

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
}

export interface PlanRow {
  id: string;
  cde: "pdoc" | "ebok";
  write_thumbnail: boolean;
  reason: string;
}

export interface Settings {
  previewer_path: string | null;
  cde: "auto" | "pdoc" | "ebok";
  pages_auto: boolean;
  llm_provider: "off" | "anthropic" | "openai";
  llm_model: string;
  llm_endpoint: string;
  llm_api_key: string;
  xray: boolean;
  word_wise: boolean;
  word_wise_level: number;
  wikipedia: boolean;
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

export type SectionKind = "reading" | "weather" | "local" | "feeds" | "lottery" | "horoscope" | "onthisday";

export interface FeedConfig {
  name: string;
  url: string;
  max_items: number;
}

/** `digest.toml`, as the backend's `deckle_digest::Config` serializes it. */
export interface DailyConfig {
  title: string;
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
  local: { google_news: string; feeds: string[]; max_items: number; max_age_hours: number };
  feeds: FeedConfig[];
  lottery: { games: string[] };
  horoscope: { signs: string[] };
  reading: { inbox: string; max_items: number; long_read_words: number };
  /** Absent while it's untouched (the backend leaves the defaults out). */
  email?: EmailConfig;
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
}

export interface PlaceRow {
  label: string;
  latitude: number;
  longitude: number;
}

export const SECTION_LABEL: Record<SectionKind, string> = {
  reading: "Your Reading",
  weather: "Weather",
  local: "Local news",
  feeds: "Your feeds",
  lottery: "Lottery",
  horoscope: "Horoscope",
  onthisday: "On This Day",
};

export const ALL_SECTIONS: SectionKind[] = ["reading", "weather", "local", "feeds", "lottery", "horoscope", "onthisday"];

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
}

export interface DeviceShelf {
  device: string;
  space: { total: number; free: number } | null;
  books: DeviceBookRow[];
}
