// SPDX-License-Identifier: GPL-3.0-or-later
import { useState } from "react";
import { homeDir } from "@tauri-apps/api/path";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { hasFolder, iCloudDrive, newExport } from "./exportsView";
import Icon from "./Icon";
import { Switch } from "./settings/Row";
import type { ExportConfig } from "./types";

/**
 * Export folders (other readers design §7): each edition also goes to these folders, plugged in
 * or not. An iCloud Drive folder reaches an iPad's Files app. Each change saves at once.
 */
export default function ExportFolders({
  exports,
  onSave,
}: {
  exports: ExportConfig[] | undefined;
  onSave: (exports: ExportConfig[]) => Promise<boolean>;
}) {
  const current = exports ?? [];
  const [error, setError] = useState<string | null>(null);

  const change = async (next: ExportConfig[]) => {
    setError(null);
    await onSave(next);
  };
  const patch = (i: number, p: Partial<ExportConfig>) => change(current.map((e, j) => (j === i ? { ...e, ...p } : e)));

  const add = async () => {
    // On a Mac the picker opens on iCloud Drive, where an iPad finds the paper.
    let defaultPath: string | undefined;
    if (navigator.userAgent.includes("Mac")) {
      defaultPath = await homeDir()
        .then(iCloudDrive)
        .catch(() => undefined);
    }
    const picked = await openDialog({ directory: true, multiple: false, defaultPath });
    if (typeof picked !== "string") return;
    if (hasFolder(current, picked)) {
      setError("That folder is already on the list.");
      return;
    }
    await change([...current, newExport(picked, current)]);
  };

  return (
    <section className="card exports" aria-label="Export folders">
      <h3>Export folders</h3>
      <p className="muted small">
        Each morning's paper is also put in these folders as an EPUB, whether or not a Kindle is plugged in. Choose a folder in
        iCloud Drive and open the paper on an iPad from the Files app; choose a Dropbox folder for a Boox. Kindle Previewer isn't
        needed for these.
      </p>

      {current.length > 0 && (
        <ul className="export-list">
          {current.map((e, i) => (
            <li key={e.name}>
              <div className="export-head">
                <strong>{e.name}</strong>
                <span className="mono small muted">{e.dir}</span>
                <button className="icon small" aria-label={`Remove ${e.name}`} onClick={() => change(current.filter((_, j) => j !== i))}>
                  <Icon name="x" size={14} />
                </button>
              </div>
              <div className="export-row">
                <label>
                  <span className="label">Photos</span>
                  <select value={e.photos} onChange={(ev) => patch(i, { photos: ev.target.value as ExportConfig["photos"] })}>
                    <option value="color">Color (iPad, color e-readers)</option>
                    <option value="gray">Gray (e-ink)</option>
                  </select>
                </label>
                <label>
                  <span className="label">Papers kept</span>
                  <select value={e.keep} onChange={(ev) => patch(i, { keep: Number(ev.target.value) })}>
                    {[3, 7, 14, 30, 0].map((n) => (
                      <option key={n} value={n}>
                        {n === 0 ? "All" : n}
                      </option>
                    ))}
                  </select>
                </label>
                <div className="export-books">
                  <span>Long reads too</span>
                  <Switch label={`Long reads to ${e.name}`} checked={e.books} onChange={(on) => patch(i, { books: on })} />
                </div>
              </div>
            </li>
          ))}
        </ul>
      )}

      <div className="yourday-actions">
        <button className="ghost" onClick={add}>
          Add folder…
        </button>
      </div>
      {error && <p className="warn small">{error}</p>}
    </section>
  );
}
