// SPDX-License-Identifier: GPL-3.0-or-later
// The browser host: pickers browse the server's folders, saves land under the server's
// Exports folder and then download, drops upload. The image updates itself by a new tag.
import type { Platform } from "../platform";
import { fetchRoots, saveTarget, upload } from "../web/serverFiles";
import { openServerPicker } from "../web/ServerPicker";

export const web: Platform = {
  kind: "web",
  pickFiles: ({ title, extensions, multiple = false }) => openServerPicker({ mode: "file", title, extensions, multiple }),
  async pickFolder({ title, defaultPath }) {
    const picked = await openServerPicker({ mode: "folder", title, start: defaultPath });
    return picked?.[0] ?? null;
  },
  async saveFile({ defaultName }) {
    return saveTarget(await fetchRoots(), defaultName);
  },
  async afterSave(path) {
    const a = document.createElement("a");
    a.href = `/api/fs/download?path=${encodeURIComponent(path)}`;
    a.download = path.split("/").pop() ?? "";
    document.body.appendChild(a);
    a.click();
    a.remove();
  },
  async openUrl(url) {
    window.open(url, "_blank", "noopener");
  },
  homeDir: async () => "/data",
  async onFileDrop(h) {
    // Enter and leave fire for every child crossed; a count says when the page itself is left.
    let depth = 0;
    const hasFiles = (e: DragEvent) => !!e.dataTransfer && Array.from(e.dataTransfer.types).includes("Files");
    const enter = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      if (depth++ === 0) h.over(true);
    };
    const over = (e: DragEvent) => {
      if (hasFiles(e)) e.preventDefault();
    };
    const leave = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      if (--depth <= 0) {
        depth = 0;
        h.over(false);
      }
    };
    const drop = async (e: DragEvent) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      depth = 0;
      h.over(false);
      const files = Array.from(e.dataTransfer?.files ?? []);
      if (files.length === 0) return;
      try {
        h.drop(await upload(files));
      } catch (err) {
        h.failed?.(String(err));
      }
    };
    document.addEventListener("dragenter", enter);
    document.addEventListener("dragover", over);
    document.addEventListener("dragleave", leave);
    document.addEventListener("drop", drop);
    return () => {
      document.removeEventListener("dragenter", enter);
      document.removeEventListener("dragover", over);
      document.removeEventListener("dragleave", leave);
      document.removeEventListener("drop", drop);
    };
  },
  checkUpdate: async () => null,
  relaunch: async () => {},
};
