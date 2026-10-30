// SPDX-License-Identifier: GPL-3.0-or-later
// The browser host. A placeholder until the web build lands; nothing selects it on desktop.
import type { Platform } from "../platform";

const notYet = (what: string) => () => Promise.reject(new Error(`${what}: not implemented in the web build yet`));

export const web: Platform = {
  kind: "web",
  pickFiles: notYet("pickFiles"),
  pickFolder: notYet("pickFolder"),
  saveFile: notYet("saveFile"),
  afterSave: notYet("afterSave"),
  openUrl: notYet("openUrl"),
  homeDir: notYet("homeDir"),
  onFileDrop: notYet("onFileDrop"),
  checkUpdate: notYet("checkUpdate"),
  relaunch: notYet("relaunch"),
};
