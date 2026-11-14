// SPDX-License-Identifier: GPL-3.0-or-later
import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "./api";
import { platform } from "./platform";
import { size } from "./kindleRooms";
import { backupLine, lastReadLabel, progressLabel, readerBooks, readerLine, removeConfirm, type ReaderSort } from "./readerRooms";
import EditDetails from "./EditDetails";
import { editLabel } from "./bookEdit";
import type { BookEdit, ReaderBook, ReaderLibrary, ReaderRow } from "./types";

interface Props {
  reader: ReaderRow;
  onNotice: (msg: string | null) => void;
}

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/**
 * A Kobo, PocketBook or reMarkable on the Devices tab: every book with how far it's read,
 * removing the ones you choose (where the reader allows it), and a backup of everything.
 */
export default function ReaderView({ reader, onNotice }: Props) {
  const [lib, setLib] = useState<ReaderLibrary | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<ReaderSort>("recent");
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const [working, setWorking] = useState<"removing" | "backing-up" | null>(null);
  const [last, setLast] = useState<string | null>(null);
  const [editing, setEditing] = useState<ReaderBook | null>(null);
  const [saving, setSaving] = useState(false);
  // What can be changed here: an EPUB's own details on a drive, a document's name over SSH.
  const editable = (b: ReaderBook) =>
    reader.kind === "remarkable" ? reader.key === "remarkable/ssh" : b.removable && b.id.toLowerCase().endsWith(".epub");
  const saveEdit = async (b: ReaderBook, edit: BookEdit) => {
    setSaving(true);
    try {
      const r = await api.readersEdit(reader.key, b.id, edit);
      onNotice(`${reader.kind === "remarkable" ? `Renamed to ${edit.title}` : `${editLabel(edit)} saved on the ${reader.name}`}.${r.note ? ` ${r.note}` : ""}`);
      setEditing(null);
      await load();
    } catch (e) {
      onNotice(String(e));
    } finally {
      setSaving(false);
    }
  };

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setLib(await api.readersLibrary(reader.key));
    } catch (e) {
      setError(String(e));
      setLib(null);
    } finally {
      setLoading(false);
    }
    api.readersLastBackup(reader.key).then(setLast).catch(() => setLast(null));
  }, [reader.key]);

  useEffect(() => {
    setChosen(new Set());
    setQuery("");
    load();
  }, [load]);

  const shown = useMemo(() => (lib ? readerBooks(lib.books, query, sort) : []), [lib, query, sort]);
  const idle = working === null && !loading;
  const toggle = (id: string) =>
    setChosen((c) => {
      const next = new Set(c);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const remove = async () => {
    const ids = [...chosen];
    if (!ids.length || !(await platform.confirm(removeConfirm(reader, ids.length)))) return;
    setWorking("removing");
    try {
      const r = await api.readersRemove(reader.key, ids);
      const parts = [`${plural(r.removed.length, "book")} removed`];
      if (r.failed.length) parts.push(`couldn't remove ${r.failed.map(([id, why]) => `${id.split("/").pop() || "it"} (${why})`).join(", ")}`);
      if (r.database_backup) parts.push(`the Kobo's database was copied to ${r.database_backup} first`);
      onNotice(`${parts.join("; ")}.`);
      setChosen(new Set());
      await load();
    } catch (e) {
      onNotice(String(e));
    } finally {
      setWorking(null);
    }
  };

  const backUp = async (into: string | null) => {
    setWorking("backing-up");
    try {
      const b = await api.readersBackup(reader.key, into);
      const parts = [`Backed up ${plural(b.copied.length, "file")} to ${b.dir}`];
      if (b.failed.length) parts.push(`couldn't copy ${b.failed.map(([f]) => f || "the folder").join(", ")}`);
      onNotice(`${parts.join("; ")}.`);
      setLast(b.dir);
    } catch (e) {
      onNotice(String(e));
    } finally {
      setWorking(null);
    }
  };
  const chooseAndBackUp = async () => {
    const dir = await platform.pickFolder({ title: `Back up the ${reader.name} into…` });
    if (typeof dir === "string") await backUp(dir);
  };

  const used = lib?.total != null && lib.free != null ? lib.total - lib.free : null;
  const removable = shown.filter((b) => b.removable && reader.can_remove);
  const allChosen = removable.length > 0 && removable.every((b) => chosen.has(b.id));

  return (
    <div className="this-kindle-grid reader-view">
      <div className="tk-main reader-main">
        <section className="tk-card" aria-label="Library">
          <h2>
            {reader.name} <span className="muted">{lib ? plural(lib.books.length, "book") : ""}</span>
          </h2>
          <p className="muted small">{readerLine(reader)}</p>
          {lib?.total != null && lib.free != null && used != null && (
            <div className="reader-space" aria-label="Space">
              <div className="progress-track">
                <div className="progress-fill" style={{ width: `${Math.min(100, (used * 100) / Math.max(1, lib.total))}%` }} />
              </div>
              <span className="muted small">
                {size(used)} used of {size(lib.total)} · {size(lib.free)} free
              </span>
            </div>
          )}
          <div className="reader-tools">
            <input type="search" placeholder="Search titles, authors, folders" value={query} onChange={(e) => setQuery(e.target.value)} aria-label="Search this reader" />
            <select value={sort} onChange={(e) => setSort(e.target.value as ReaderSort)} aria-label="Sort">
              <option value="recent">Recently read</option>
              <option value="title">Title</option>
              <option value="size">Size</option>
            </select>
            <button className="ghost" disabled={!idle} onClick={load}>
              {loading ? "Reading…" : "Refresh"}
            </button>
          </div>
          {error && <p className="warn small">{error}</p>}
          {lib?.warnings.map((w) => (
            <p key={w} className="muted small warn">
              {w}
            </p>
          ))}
          {loading && !lib && <p className="muted">Reading the {reader.name}…</p>}
          {lib && shown.length === 0 && <p className="muted">{query ? "Nothing matches." : "No books on it yet."}</p>}
          {shown.length > 0 && (
            <div className="reader-list" role="list">
              {reader.can_remove && removable.length > 0 && (
                <label className="reader-all muted small">
                  <input type="checkbox" checked={allChosen} onChange={() => setChosen(allChosen ? new Set() : new Set(removable.map((b) => b.id)))} />
                  Choose all {removable.length} shown
                </label>
              )}
              {shown.map((b) => (
                <div key={b.id} className="reader-row" role="listitem">
                  {reader.can_remove && (
                    <input type="checkbox" aria-label={`Choose ${b.title}`} disabled={!b.removable} checked={chosen.has(b.id)} onChange={() => toggle(b.id)} title={b.removable ? undefined : (b.note ?? "Can't be removed here")} />
                  )}
                  <div className="reader-book">
                    <strong>{b.title}</strong>
                    <span className="muted small">
                      {[b.authors.join(", "), b.folder, b.format.toUpperCase(), b.bytes != null ? size(b.bytes) : ""].filter(Boolean).join(" · ")}
                    </span>
                    {b.note && <span className="muted small">{b.note}</span>}
                    {editable(b) && (
                      <button className="link-action inline small" disabled={!idle} onClick={() => setEditing(b)}>
                        {reader.kind === "remarkable" ? "Rename…" : "Edit details…"}
                      </button>
                    )}
                  </div>
                  <div className="reader-progress">
                    {b.percent != null && (
                      <div className="progress-track" aria-hidden="true">
                        <div className="progress-fill" style={{ width: `${b.finished ? 100 : Math.min(100, b.percent)}%` }} />
                      </div>
                    )}
                    <span className="small">{progressLabel(b)}</span>
                    <span className="muted small">{lastReadLabel(b.last_read)}</span>
                  </div>
                </div>
              ))}
            </div>
          )}
        </section>
      </div>

      <div className="tk-side">
        <section className="tk-card" aria-label="Make room">
          <h2>Make room</h2>
          {reader.can_remove ? (
            <>
              <p className="muted small">Choose books in the list, then remove them from the {reader.name}.</p>
              <div className="button-row">
                <button className="primary danger-fill" disabled={!idle || chosen.size === 0} onClick={remove}>
                  {working === "removing" ? "Removing…" : chosen.size ? `Remove ${plural(chosen.size, "book")}` : "Remove"}
                </button>
              </div>
            </>
          ) : (
            <p className="muted small">{reader.remove_note}</p>
          )}
        </section>

        <section className="tk-card" aria-label="Back up">
          <h2>Back up</h2>
          <p className="muted small">{backupLine(reader, last)}</p>
          <div className="button-row">
            <button className="primary" disabled={!idle} onClick={() => backUp(null)}>
              {working === "backing-up" ? "Backing up…" : "Back up now"}
            </button>
            <button className="ghost" disabled={!idle} onClick={chooseAndBackUp}>
              To a folder…
            </button>
            {last && (
              <button className="ghost" onClick={() => api.reveal(last).catch((e) => onNotice(String(e)))}>
                Show
              </button>
            )}
          </div>
        </section>
      </div>
      {editing && (
        <EditDetails
          heading={reader.kind === "remarkable" ? "Rename on the reMarkable" : "Edit details"}
          titleOnly={reader.kind === "remarkable"}
          initial={{ title: editing.title, authors: editing.authors, series: null }}
          note={
            reader.kind === "remarkable"
              ? "The tablet restarts its reader for a few seconds to show the new name."
              : reader.kind === "kobo"
                ? "Written into the book's file on the Kobo and into its library (its database is copied aside first). The Kobo shows it once you eject it."
                : "Written into the book's file on the PocketBook; it shows the new details the next time it reads its library."
          }
          action="Save"
          busy={saving}
          onCancel={() => setEditing(null)}
          onSave={(edit) => saveEdit(editing, edit)}
        />
      )}
    </div>
  );
}
