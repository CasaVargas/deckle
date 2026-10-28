// SPDX-License-Identifier: GPL-3.0-or-later
import type { ExportConfig, PressSetupStatus } from "./types";

export function pressView(folder: ExportConfig, status?: PressSetupStatus | null) {
  const ready = !!status?.enabled && status.watcher_ready && !status.error;
  const error = status?.error;
  const message = error === "protocol_conflict"
    ? "This folder belongs to another paper or other computer. To disconnect, turn requests off there and wait for its producer.json to sync with enabled: false. Remove only .deckle-press/producer.json from this folder, wait for that removal to sync, then retry. Keep requests, editions, and returns."
    : error === "storage_unavailable"
      ? "Setup could not reach the folder or start the watcher. Check that the folder is available, then retry setup. An old shared notice may still need disconnecting."
      : error
        ? "Setup needs attention. Check the folder and its request controls, then retry."
        : ready
          ? "Ready for requests while this computer is awake and its watcher is running. Cloud sync can delay both requests and delivery."
          : folder.press_enabled
            ? "Enabled on this computer. Use Check setup to verify the folder and start the watcher."
            : "Let compatible clients request a fresh full paper through this synced folder.";
  return {
    visible: folder.format === "bundle",
    checked: status ? status.enabled : !!folder.press_enabled,
    label: error ? "Needs attention" : ready ? "Ready" : folder.press_enabled ? "Enabled · not checked" : "Off",
    message,
  };
}

/** Wait for all pending saves and reject a moved row before sending its index. */
export async function setupSavedExport(
  index: number, expected: ExportConfig, enabled: boolean,
  save: () => Promise<boolean>, current: () => ExportConfig[],
  setup: (index: number, enabled: boolean, expected: Pick<ExportConfig, "dir" | "format">) => Promise<PressSetupStatus>,
): Promise<PressSetupStatus> {
  if (!await save()) throw new Error("Save folder settings before setting up requests.");
  const folder = current()[index];
  if (!folder || folder.dir !== expected.dir || folder.format !== expected.format) {
    throw new Error("Folder changed. Reopen Delivery and try again.");
  }
  return setup(index, enabled, { dir: expected.dir, format: expected.format });
}

/** Ordinary folder editing cannot activate requests or move an existing activation. */
export function patchExport(folder: ExportConfig, patch: Partial<ExportConfig>): ExportConfig {
  const next = { ...folder, ...patch, press_enabled: folder.press_enabled };
  if (next.dir !== folder.dir || next.format !== folder.format) next.press_enabled = false;
  return next;
}
