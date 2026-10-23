// SPDX-License-Identifier: GPL-3.0-or-later
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { api } from "./api";
import { defaultSpare, size, spaceSlices, spareLine } from "./kindleRooms";
import type { BackupReport, DeviceShelf, RunTarget } from "./types";

interface Props {
  target: RunTarget;
  shelf: DeviceShelf;
  busy: boolean;
  /** After something changed the Kindle or the backups. */
  onChanged: () => Promise<void>;
  onNotice: (msg: string | null) => void;
}

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/**
 * This Kindle (Books and Kindle design §7): where the space goes, what could make room, and a
 * backup of every DRM-free book with its reading position and highlights.
 */
export default function ThisKindle({ target, shelf, busy, onChanged, onNotice }: Props) {
  const [ticked, setTicked] = useState<Set<string>>(() => defaultSpare(shelf.spare));
  const [confirming, setConfirming] = useState(false);
  const [working, setWorking] = useState<"removing" | "backing-up" | "restoring" | null>(null);
  const [report, setReport] = useState<BackupReport | null>(null);
  useEffect(() => setTicked(defaultSpare(shelf.spare)), [shelf.spare]);

  const chosen = shelf.spare.filter((s) => ticked.has(s.file));
  const freed = chosen.reduce((n, s) => n + s.size, 0);
  const toggle = (file: string, on: boolean) =>
    setTicked((t) => {
      const next = new Set(t);
      if (on) next.add(file);
      else next.delete(file);
      return next;
    });

  const remove = async () => {
    setWorking("removing");
    try {
      const failed = await api.deviceRemoveMany(
        target,
        chosen.map((s) => s.file),
      );
      onNotice(failed.length ? `Couldn't remove: ${failed.map(([f, e]) => `${f} (${e})`).join("; ")}` : `${plural(chosen.length - failed.length, "book")} removed, ${size(freed)} free again.`);
      setConfirming(false);
      await onChanged();
    } catch (e) {
      onNotice(String(e));
    } finally {
      setWorking(null);
    }
  };

  const backUp = async (into: string | null) => {
    setWorking("backing-up");
    try {
      const r = await api.deviceBackup(target, into);
      setReport(r);
      onNotice(r.failed.length ? `Backed up ${plural(r.copied.length, "book")}; ${r.failed.length} couldn't be copied.` : null);
      await onChanged();
    } catch (e) {
      onNotice(String(e));
    } finally {
      setWorking(null);
    }
  };

  const chooseAndBackUp = async () => {
    const dir = await openDialog({ directory: true, title: "Back up this Kindle's books into…" });
    if (typeof dir === "string") await backUp(dir);
  };

  const restore = async (dir: string) => {
    setWorking("restoring");
    try {
      const r = await api.deviceRestore(target, dir);
      const parts = [`${plural(r.restored.length, "book")} put on this Kindle`];
      if (r.already.length) parts.push(`${r.already.length} already here`);
      if (r.failed.length) parts.push(`couldn't copy ${r.failed.map(([f]) => f).join(", ")}`);
      onNotice(`${parts.join(", ")}.`);
      await onChanged();
    } catch (e) {
      onNotice(String(e));
    } finally {
      setWorking(null);
    }
  };

  const chooseAndRestore = async () => {
    const dir = await openDialog({ directory: true, title: "Restore a Deckle backup from…" });
    if (typeof dir === "string") await restore(dir);
  };

  const idle = !busy && working === null;
  return (
    <div className="this-kindle">
      {shelf.split ? (
        <section className="card" aria-label="Space">
          <h3>
            {size(shelf.split.free)} free of {size(shelf.split.total)}
          </h3>
          <div className="space-bar" role="img" aria-label={spaceSlices(shelf.split).map((s) => `${s.label}: ${size(s.bytes)}`).join(", ")}>
            {spaceSlices(shelf.split).map((s) => (
              <div key={s.key} className={`space-slice slice-${s.key}`} style={{ width: `${s.pct}%` }} title={`${s.label}: ${size(s.bytes)}`} />
            ))}
          </div>
          <ul className="space-legend">
            {spaceSlices(shelf.split).map((s) => (
              <li key={s.key}>
                <span className={`swatch slice-${s.key}`} aria-hidden="true" />
                {s.label} <span className="muted">{size(s.bytes)}</span>
              </li>
            ))}
          </ul>
        </section>
      ) : (
        <p className="muted small">This Kindle doesn't say how much room it has.</p>
      )}

      <section className="card" aria-label="Make room">
        <h3>Make room</h3>
        {shelf.spare.length === 0 ? (
          <p className="muted small">Nothing to clear: no finished books over 10 MB, no second copies, and no old papers.</p>
        ) : (
          <>
            <p className="muted small">Finished books over 10 MB, second copies of the same book, and papers past what the Daily keeps. Finished books aren't ticked: tick the ones you won't reread.</p>
            <ul className="books spare-list">
              {shelf.spare.map((s) => (
                <li key={s.file} className="row">
                  <label className="upgrade-tick">
                    <input type="checkbox" checked={ticked.has(s.file)} onChange={(e) => toggle(s.file, e.target.checked)} aria-label={`Remove ${s.title}`} />
                  </label>
                  <div className="meta">
                    <div className="title-line">
                      <span className="title" title={s.file}>
                        {s.title}
                      </span>
                    </div>
                    <div className="shipped">{spareLine(s)}</div>
                  </div>
                  <span className="muted small">{size(s.size)}</span>
                </li>
              ))}
            </ul>
            {confirming ? (
              <div className="confirm" role="alert">
                Remove {plural(chosen.length, "book")} and their reading data from this Kindle?
                <button className="ghost danger" disabled={!idle} onClick={remove}>
                  {working === "removing" ? "Removing…" : "Remove"}
                </button>
                <button className="ghost" disabled={!idle} onClick={() => setConfirming(false)}>
                  Keep
                </button>
              </div>
            ) : (
              <button className="primary" disabled={!idle || chosen.length === 0} onClick={() => setConfirming(true)}>
                {chosen.length ? `Remove ${plural(chosen.length, "book")} · ${size(freed)}` : "Remove"}
              </button>
            )}
          </>
        )}
      </section>

      <section className="card" aria-label="Back up">
        <h3>Back up</h3>
        <p className="muted small">
          Every DRM-free book on this Kindle is copied to this computer with its reading position and highlights. Amazon's own books are left out, since Amazon downloads them again, and so is the
          paper. Restore puts a backup on any Kindle, leaving the books already there alone.
        </p>
        <div className="button-row">
          <button className="primary" disabled={!idle} onClick={() => backUp(null)}>
            {working === "backing-up" ? "Backing up…" : "Back up now"}
          </button>
          <button className="ghost" disabled={!idle} onClick={chooseAndBackUp}>
            Back up to a folder…
          </button>
          <button className="ghost" disabled={!idle} onClick={chooseAndRestore}>
            Restore from a folder…
          </button>
        </div>
        {working === "backing-up" && <p className="muted small">The first backup over MTP reads every book once, so it takes a while on a full Kindle.</p>}
        {report && (
          <p className="muted small" role="status">
            {plural(report.copied.length, "book")} copied to {report.dir}
            {report.skipped.length ? `; ${report.skipped.length} left out` : ""}.
          </p>
        )}
        {shelf.backups.length > 0 && (
          <ul className="backup-list">
            {shelf.backups.map((b) => (
              <li key={b.dir}>
                <span>
                  <strong>{b.device}</strong>
                  <span className="muted small">
                    {" "}
                    · {b.at.slice(0, 10)} · {plural(b.books, "book")} · {size(b.size)}
                  </span>
                </span>
                <button className="ghost" disabled={!idle} onClick={() => restore(b.dir)} title={b.dir}>
                  {working === "restoring" ? "Restoring…" : "Restore here"}
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
