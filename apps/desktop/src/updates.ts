// SPDX-License-Identifier: GPL-3.0-or-later
// In-app updates (auto-update design): one check per launch, and install only on a click.
import { check, type Update } from "@tauri-apps/plugin-updater";

export type UpdateState =
  | { kind: "checking" }
  | { kind: "current" }
  | { kind: "failed" }
  | { kind: "available"; update: Update }
  | { kind: "downloading"; update: Update; done: number; total: number | null }
  | { kind: "ready"; update: Update }
  | { kind: "error"; update: Update; message: string };

/** The footer marker shows for these. */
export const hasUpdate = (s: UpdateState): s is Extract<UpdateState, { update: Update }> => "update" in s;

/** Never rejects: offline, GitHub down or a dev build all end as `failed`, quietly. */
export async function checkForUpdate(): Promise<UpdateState> {
  try {
    const update = await check();
    return update ? { kind: "available", update } : { kind: "current" };
  } catch (e) {
    console.warn("update check failed", e);
    return { kind: "failed" };
  }
}

/**
 * Downloads, verifies and installs; on macOS and Linux the restart is a separate click. On
 * Windows the NSIS installer takes over and closes Deckle itself, so callers don't start this
 * while a job runs. Never rejects.
 */
export async function install(update: Update, onState: (s: UpdateState) => void): Promise<void> {
  let done = 0;
  let total: number | null = null;
  onState({ kind: "downloading", update, done, total });
  try {
    await update.downloadAndInstall((event) => {
      if (event.event === "Started") total = event.data.contentLength ?? null;
      else if (event.event === "Progress") done += event.data.chunkLength;
      else return;
      onState({ kind: "downloading", update, done, total });
    });
    onState({ kind: "ready", update });
  } catch (e) {
    onState({ kind: "error", update, message: String(e) });
  }
}
