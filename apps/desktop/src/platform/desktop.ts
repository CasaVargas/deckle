// SPDX-License-Identifier: GPL-3.0-or-later
// The desktop host: Tauri's dialogs, opener, webview drops, updater and process plugins.
import { homeDir } from "@tauri-apps/api/path";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import type { Platform } from "../platform";

const filters = (name: string | undefined, extensions: string[] | undefined) => (extensions ? [{ name: name ?? "", extensions }] : undefined);

export const desktop: Platform = {
  kind: "desktop",
  async pickFiles({ title, filterName, extensions, multiple = false }) {
    const picked = await open({ multiple, directory: false, title, filters: filters(filterName, extensions) });
    if (picked === null) return null;
    return Array.isArray(picked) ? picked : [picked];
  },
  async pickFolder({ title, defaultPath }) {
    const picked = await open({ directory: true, multiple: false, title, defaultPath });
    return typeof picked === "string" ? picked : null;
  },
  async saveFile({ defaultName, filterName, extensions }) {
    return (await save({ defaultPath: defaultName, filters: filters(filterName, extensions) })) ?? null;
  },
  afterSave: async () => {},
  openUrl: (url) => openUrl(url),
  homeDir,
  onFileDrop: (h) =>
    getCurrentWebview().onDragDropEvent((ev) => {
      if (ev.payload.type === "enter" || ev.payload.type === "over") h.over(true);
      else if (ev.payload.type === "leave") h.over(false);
      else if (ev.payload.type === "drop") {
        h.over(false);
        h.drop(ev.payload.paths);
      }
    }),
  checkUpdate: () => check(),
  relaunch,
};
