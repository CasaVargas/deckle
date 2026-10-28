// SPDX-License-Identifier: GPL-3.0-or-later
import { useState } from "react";
import { pressView } from "./pressView";
import { homeDir } from "@tauri-apps/api/path";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { hasFolder, iCloudDrive, newExport } from "./exportsView";
import { Switch } from "./settings/Row";
import { SCREEN_LABEL } from "./types";
import type { ExportConfig, PressSetupStatus, Screen } from "./types";

/**
 * Export folders (other readers design §7, Daily tab redesign §8): each edition also goes to
 * these folders, plugged in or not. An iCloud Drive folder reaches an iPad's Files app. In
 * Delivery each folder is a row; these are its settings, and the picker that adds one.
 */

/** Picks a folder to add (on a Mac the picker opens on iCloud Drive, where an iPad finds the
 * paper). Resolves to the new list, or a reason it wasn't added, or null when cancelled. */
export async function addFolder(current: ExportConfig[]): Promise<ExportConfig[] | string | null> {
  let defaultPath: string | undefined;
  if (navigator.userAgent.includes("Mac")) {
    defaultPath = await homeDir()
      .then(iCloudDrive)
      .catch(() => undefined);
  }
  const picked = await openDialog({ directory: true, multiple: false, defaultPath });
  if (typeof picked !== "string") return null;
  if (hasFolder(current, picked)) return "That folder is already on the list.";
  return [...current, newExport(picked, current)];
}

/** One folder's settings: what goes there, how, and how many stay. */
export function FolderSettings({ e, onPatch, onRemove, onSetup }: { e: ExportConfig; onPatch: (p: Partial<ExportConfig>) => void; onRemove: () => void; onSetup: (enabled: boolean) => Promise<PressSetupStatus> }) {
  const [status, setStatus] = useState<PressSetupStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const view = pressView(e, status);
  const setup = async (enabled: boolean) => {
    setBusy(true);
    setStatus(null);
    setFailure(null);
    try { setStatus(await onSetup(enabled)); }
    catch { setFailure("Could not complete setup. Save your settings, check the selected folder, and retry."); }
    finally { setBusy(false); }
  };
  return (
    <>
      <div className="export-row">
        <label>
          <span className="label">As</span>
          <select value={e.format} onChange={(ev) => onPatch({ format: ev.target.value as ExportConfig["format"] })}>
            <option value="epub">EPUB (any reader)</option>
            <option value="pdf">PDF, laid out for a screen</option>
            <option value="bundle">Bundle, for a reader app</option>
          </select>
        </label>
        {e.format === "pdf" && (
          <label>
            <span className="label">Screen</span>
            <select value={e.screen} onChange={(ev) => onPatch({ screen: ev.target.value as Screen })}>
              {(Object.keys(SCREEN_LABEL) as Screen[]).map((s) => (
                <option key={s} value={s}>
                  {SCREEN_LABEL[s]}
                </option>
              ))}
            </select>
          </label>
        )}
        <label>
          <span className="label">Photos</span>
          <select value={e.photos} onChange={(ev) => onPatch({ photos: ev.target.value as ExportConfig["photos"] })}>
            <option value="color">Color (iPad, color e-readers)</option>
            <option value="gray">Gray (e-ink)</option>
          </select>
        </label>
        <label>
          <span className="label">Papers kept</span>
          <select value={e.keep} onChange={(ev) => onPatch({ keep: Number(ev.target.value) })}>
            {[3, 7, 14, 30, 0].map((n) => (
              <option key={n} value={n}>
                {n === 0 ? "All" : n}
              </option>
            ))}
          </select>
        </label>
        <div className="export-books">
          <span>Long reads too</span>
          <Switch label={`Long reads to ${e.name}`} checked={e.books} onChange={(on) => onPatch({ books: on })} />
        </div>
      </div>
      {view.visible && <div className="set-row" style={{ alignSelf: "stretch" }}>
        <div className="set-label">
          <div>Fresh-paper requests <span className="muted small" role="status">{busy ? "Setting up…" : failure ? "Needs attention" : view.label}</span></div>
          <p className="muted small">{failure ?? view.message}</p>
          <p className="muted small">Only this folder receives the fresh paper. Existing optional model features may incur their usual provider costs.</p>
          {e.press_enabled && !busy && <button className="quiet" onClick={() => void setup(true)}>Check setup</button>}
          {(failure || status?.error) && !busy && <button className="quiet" onClick={() => void setup(true)}>Retry setup</button>}
        </div>
        <Switch label={`Fresh-paper requests for ${e.name}`} checked={view.checked} disabled={busy} onChange={(on) => void setup(on)} />
      </div>}
      <p className="muted small">An EPUB opens in any reader; Kindle Previewer isn't needed for these.</p>
      <button className="quiet" onClick={onRemove}>
        Stop putting the paper in {e.name}
      </button>
    </>
  );
}
