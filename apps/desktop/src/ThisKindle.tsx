// SPDX-License-Identifier: GPL-3.0-or-later
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { api } from "./api";
import { connectedBy, defaultSpare, freesLine, size, spaceSlices, spareGroups, spareLine } from "./kindleRooms";
import type { BackupReport, DeviceRow, DeviceShelf, RunTarget } from "./types";

interface Props {
  target: RunTarget;
  /** The Kindle's row, for its name; null when the footer has none. */
  device: DeviceRow | null;
  shelf: DeviceShelf;
  busy: boolean;
  /** After something changed the Kindle or the backups. */
  onChanged: () => Promise<void>;
  onNotice: (msg: string | null) => void;
  /** After the Kindle was named. */
  onRenamed: () => Promise<void>;
}

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/**
 * This Kindle (Books and Kindle design §7): where the space goes, what could make room, and a
 * backup of every DRM-free book with its reading position and highlights.
 */
export default function ThisKindle({ target, device, shelf, busy, onChanged, onNotice, onRenamed }: Props) {
  const [name, setName] = useState(device?.name ?? "");
  const [naming, setNaming] = useState(false);
  useEffect(() => setName(device?.name ?? ""), [device?.sync_id, device?.name]);
  const saveName = async () => {
    if (!device) return;
    setNaming(true);
    try {
      await api.nameKindle(device.sync_id, name);
      await onRenamed();
      onNotice(name.trim() ? `This Kindle is now "${name.trim()}". Send to it by name, even when it's not plugged in.` : "Name cleared.");
    } catch (e) {
      onNotice(String(e));
    } finally {
      setNaming(false);
    }
  };
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
  const tickAll = (files: string[], on: boolean) =>
    setTicked((t) => {
      const next = new Set(t);
      for (const f of files) {
        if (on) next.add(f);
        else next.delete(f);
      }
      return next;
    });
  const last = [...shelf.backups].sort((a, b) => b.at.localeCompare(a.at))[0];
  return (
    <div className="this-kindle-grid">
      <div className="tk-main">
        {shelf.split ? (
          <section className="tk-card" aria-label="Space">
            <h2>
              {size(shelf.split.free)} free <span className="muted">of {size(shelf.split.total)}</span>
            </h2>
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

        <section className="tk-card" aria-label="Make room">
          <h2>Make room</h2>
          {shelf.spare.length === 0 ? (
            <p className="muted small">Nothing to clear: no finished books over 10 MB, no second copies, and no old papers.</p>
          ) : (
            <>
              <p className="muted small">Finished books start unticked, in case you want to read them again.</p>
              {spareGroups(shelf.spare).map((g) => {
                const files = g.items.map((s) => s.file);
                return (
                  <div key={g.why} className="spare-group">
                    <label className="spare-group-head">
                      <input type="checkbox" checked={files.every((f) => ticked.has(f))} onChange={(e) => tickAll(files, e.target.checked)} aria-label={`Select all: ${g.title}`} />
                      <strong>{g.title}</strong> <span className="muted">· {size(g.total)}</span>
                    </label>
                    <ul className="spare-rows">
                      {g.items.map((s) => (
                        <li key={s.file}>
                          <input type="checkbox" checked={ticked.has(s.file)} onChange={(e) => toggle(s.file, e.target.checked)} aria-label={`Remove ${s.title}`} />
                          <span className="spare-what" title={s.file}>
                            {s.title} <span className="muted small">{spareLine(s)}</span>
                          </span>
                          <span className="muted small">{size(s.size)}</span>
                        </li>
                      ))}
                    </ul>
                  </div>
                );
              })}
              <div className="spare-foot">
                {confirming ? (
                  <>
                    <span>Remove {plural(chosen.length, "book")} and their reading data from this Kindle?</span>
                    <button className="ghost" disabled={!idle} onClick={() => setConfirming(false)}>
                      Keep
                    </button>
                    <button className="primary danger-fill" disabled={!idle} onClick={remove}>
                      {working === "removing" ? "Removing…" : "Remove"}
                    </button>
                  </>
                ) : (
                  <>
                    <span className="muted">{freesLine(shelf.spare, ticked)}</span>
                    <button className="primary" disabled={!idle || chosen.length === 0} onClick={() => setConfirming(true)}>
                      {chosen.length ? `Remove ${chosen.length} from the Kindle` : "Remove"}
                    </button>
                  </>
                )}
              </div>
            </>
          )}
        </section>
      </div>

      <div className="tk-side">
        {device && (
          <section className="tk-card" aria-label="Name">
            <label className="tk-label" htmlFor="kindle-name">
              Name
            </label>
            <div className="button-row">
              <input id="kindle-name" type="text" value={name} placeholder={device.model_hint} onChange={(e) => setName(e.target.value)} />
              <button className="ghost" disabled={busy || naming || name.trim() === (device.name ?? "")} onClick={saveName}>
                {naming ? "Saving…" : "Save"}
              </button>
            </div>
            <p className="muted small">
              {device.model_hint} · {connectedBy(device.kind)}. A name tells it apart from others in the house; books sent to it by name wait until it's plugged in.
            </p>
          </section>
        )}

        <section className="tk-card" aria-label="Back up">
          <h2>Back up</h2>
          <p className="muted small">Copies every book Amazon doesn't keep for you, with your places and highlights, to this computer. Restore puts a backup on any Kindle.</p>
          <p className="small">{last ? `Last backup: ${last.at.slice(0, 10)}, ${plural(last.books, "book")}` : "Not backed up yet"}</p>
          <div className="button-row">
            <button className="primary" disabled={!idle} onClick={() => backUp(null)}>
              {working === "backing-up" ? "Backing up…" : "Back up now"}
            </button>
            <button className="ghost" disabled={!idle} onClick={chooseAndBackUp}>
              To a folder…
            </button>
            <button className="quiet" disabled={!idle} onClick={chooseAndRestore}>
              Restore…
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
            <details className="backup-more">
              <summary>Earlier backups ({shelf.backups.length})</summary>
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
            </details>
          )}
        </section>
      </div>
    </div>
  );
}
