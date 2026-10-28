// SPDX-License-Identifier: GPL-3.0-or-later
// Delivery (Daily tab redesign §8): where the paper goes, in two groups: the Kindle (over USB,
// by email), then other readers (a reMarkable, each export folder, Kobo and PocketBook). A row
// says how it gets there and where it stands, and opens to its settings.
import { patchExport } from "./pressView";
import { useState } from "react";
import type { ReactNode } from "react";
import { deliveryGroups } from "./dailyRooms";
import type { DeliveryRow } from "./dailyRooms";
import { DEFAULT_EMAIL } from "./types";
import { DEFAULT_REMARKABLE } from "./exportsView";
import { FolderSettings, addFolder } from "./ExportFolders";
import Icon from "./Icon";
import RemarkableCard from "./RemarkableCard";
import type { Change } from "./SectionsTab";
import type { DailyConfig, ExportConfig, PressSetupStatus } from "./types";

export default function DeliveryTab({
  draft,
  change,
  emailBody,
  setupPress,
  pressBusy = false,
}: {
  draft: DailyConfig;
  setupPress: (index: number, folder: ExportConfig, enabled: boolean) => Promise<PressSetupStatus>;
  pressBusy?: boolean;
  change: Change;
  /** The email settings, which keep their own password and test state. */
  emailBody: ReactNode;
}) {
  const [open, setOpen] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const groups = deliveryGroups(draft);
  const folders = draft.export ?? [];

  const toggle = (key: string, on: boolean) => {
    if (key === "email") change((c) => ({ ...c, email: { ...(c.email ?? DEFAULT_EMAIL), enabled: on } }));
    if (key === "remarkable") change((c) => ({ ...c, remarkable: { ...(c.remarkable ?? DEFAULT_REMARKABLE), enabled: on } }));
    if (on) setOpen(key);
  };

  const body = (key: string): ReactNode => {
    if (key === "usb")
      return (
        <label className="inline">
          Keep the last
          <select value={draft.keep} onChange={(e) => change((c) => ({ ...c, keep: Number(e.target.value) }))}>
            {Array.from({ length: 14 }, (_, n) => n + 1).map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </select>
          papers on the Kindle
        </label>
      );
    if (key === "email") return emailBody;
    if (key === "remarkable")
      return draft.remarkable?.enabled ? (
        <RemarkableCard remarkable={draft.remarkable} onSave={async (r) => (change((c) => ({ ...c, remarkable: r })), true)} />
      ) : (
        <p className="muted small">Turn it on to send the paper to a reMarkable.</p>
      );
    if (key.startsWith("folder:")) {
      const i = Number(key.slice("folder:".length));
      const e = folders[i];
      if (!e) return null;
      return (
        <FolderSettings
          key={`${i}:${e.dir}:${e.format}`}
          e={e}
          onSetup={(enabled) => setupPress(i, e, enabled)}
          onPatch={(p) => change((c) => ({ ...c, export: (c.export ?? []).map((x, j) => (j === i ? patchExport(x, p) : x)) }))}
          onRemove={() => {
            setOpen(null);
            change((c) => ({ ...c, export: (c.export ?? []).filter((_, j) => j !== i) }));
          }}
        />
      );
    }
    return null;
  };

  const row = (r: DeliveryRow) => {
    const isOpen = open === r.key;
    return (
      <li key={r.key} className={`dl-row ${isOpen ? "open" : ""}`}>
        <div className="dl-head">
          <span className={`press-dot ${r.on === false ? "" : "on"}`} aria-hidden="true" />
          {r.opens ? (
            <button className="sec-open" aria-expanded={isOpen} onClick={() => setOpen(isOpen ? null : r.key)}>
              <strong>{r.name}</strong>
              <span className="muted small">{r.how}</span>
            </button>
          ) : (
            <span className="sec-open">
              <strong>{r.name}</strong>
              <span className="muted small">{r.how}</span>
            </span>
          )}
          <span className="dl-status muted small">{r.status}</span>
          {r.on === null ? <span /> : <input className="switch" type="checkbox" aria-label={`${r.name}: on`} checked={r.on} onChange={() => toggle(r.key, !r.on)} />}
          {r.opens ? (
            <button className="icon small" aria-label={isOpen ? `Close ${r.name}` : `Open ${r.name}`} onClick={() => setOpen(isOpen ? null : r.key)}>
              <Icon name={isOpen ? "chevron-down" : "chevron-right"} size={14} />
            </button>
          ) : (
            <span />
          )}
        </div>
        {isOpen && <div className="sec-body">{body(r.key)}</div>}
      </li>
    );
  };

  return (
    <fieldset disabled={pressBusy} className="sections-tab delivery-tab" style={{ border: 0, padding: 0, margin: 0, minWidth: 0 }} aria-busy={pressBusy}>
      <div>
        <h2 className="daily-h2">Where it goes</h2>
        <p className="muted small">Each morning's paper goes to every place that's on. One that isn't plugged in gets it the next time.</p>
      </div>
      <section aria-label="Kindle">
        <h3 className="dl-group">Kindle</h3>
        <ul className="sec-list">{groups.kindle.map(row)}</ul>
      </section>
      <section aria-label="Other readers">
        <h3 className="dl-group">Other readers</h3>
        <ul className="sec-list">
          {groups.others.map(row)}
          <li className="dl-add">
            <button
              className="link"
              onClick={async () => {
                setError(null);
                const next = await addFolder(folders);
                if (typeof next === "string") setError(next);
                else if (next) {
                  change((c) => ({ ...c, export: next }));
                  setOpen(`folder:${next.length - 1}`);
                }
              }}
            >
              + Add a folder
            </button>
            <span className="muted small">iCloud Drive for an iPad, Dropbox for a Boox</span>
            {error && <span className="warn small">{error}</span>}
          </li>
        </ul>
      </section>
    </fieldset>
  );
}
