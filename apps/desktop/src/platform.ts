// SPDX-License-Identifier: GPL-3.0-or-later
// Everything the front end asks of its host besides commands (transport.ts): pickers, links,
// drops, updates. Desktop asks Tauri; the web build asks the browser and the server.
import { desktop } from "./platform/desktop";
import { web } from "./platform/web";

/** What the updater reports while it downloads (the Tauri updater's events). */
export type DownloadEvent =
  | { event: "Started"; data: { contentLength?: number } }
  | { event: "Progress"; data: { chunkLength: number } }
  | { event: "Finished" };

/** A newer release, ready to install. */
export interface UpdateInfo {
  version: string;
  downloadAndInstall(onEvent?: (e: DownloadEvent) => void): Promise<void>;
}

export interface Platform {
  kind: "desktop" | "web";
  /** `filterName` labels the extension filter in the native dialog. */
  pickFiles(o: { title?: string; filterName?: string; extensions?: string[]; multiple?: boolean }): Promise<string[] | null>;
  pickFolder(o: { title?: string; defaultPath?: string }): Promise<string | null>;
  /** A path the backend writes to. */
  saveFile(o: { defaultName: string; filterName?: string; extensions?: string[] }): Promise<string | null>;
  /** Desktop: nothing. Web: download what the backend wrote. */
  afterSave(path: string): Promise<void>;
  openUrl(url: string): Promise<void>;
  homeDir(): Promise<string>;
  /** `over(true)` while files hover the window, `over(false)` when they leave or land; then `drop`.
   * `failed` says why dropped files couldn't be taken (web: the upload failed). */
  onFileDrop(h: { over(on: boolean): void; drop(paths: string[]): void; failed?(message: string): void }): Promise<() => void>;
  checkUpdate(): Promise<UpdateInfo | null>;
  relaunch(): Promise<void>;
}

// A static test on the build-time constant, so each bundle keeps only its own door.
export const platform: Platform = import.meta.env.VITE_DECKLE_TARGET === "web" ? web : desktop;
