// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";
import { api } from "../api";
import type { WritebackColumns, WritebackPatch, WritebackStatus } from "../types";
import { FIELDS, blocker, changeLine, noColumns, shown } from "../writebackView";
import { Row, Switch } from "./Row";

/**
 * Reading data into calibre (calibre bridge design §5.4): how far you got, finished, when last
 * read and the stars, from your readers into the calibre columns you choose, through calibre's
 * own calibredb. Off until it's turned on; a preview shows the next batch first. Values someone
 * set in calibre are left alone.
 */
export default function CalibreWriteback() {
  const [s, setS] = useState<WritebackStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [password, setPassword] = useState("");

  useEffect(() => {
    api.calibreWritebackStatus().then(setS, (e) => setError(String(e)));
  }, []);

  const act = async (f: () => Promise<WritebackStatus>) => {
    setBusy(true);
    setError(null);
    try {
      setS(await f());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const set = (patch: WritebackPatch) => act(() => api.calibreWritebackSet(patch));

  if (!s) return error ? <p className="warn small">{error}</p> : null;
  const wb = s.writeback;
  const why = blocker(s);
  const plan = s.plan;
  const setColumn = (key: keyof WritebackColumns, label: string) => set({ columns: { ...wb.columns, [key]: label || null } });

  return (
    <>
      <Row
        label="Write reading data to calibre"
        note={why ?? (wb.enabled ? "On: the next batch goes within a minute of a reader being read." : "Off. Check the preview below first.")}
      >
        <Switch label="Write reading data to calibre" checked={wb.enabled} disabled={busy || (!wb.enabled && !!why)} onChange={(on) => set({ enabled: on })} />
      </Row>

      {s.library && (
        <div className="set-sub">
          {FIELDS.map((f) => (
            <Row key={f.key} label={f.label} note={f.hint}>
              <select value={wb.columns[f.key] ?? ""} disabled={busy} onChange={(e) => setColumn(f.key, e.target.value)} aria-label={`${f.label} column`}>
                <option value="">Not written</option>
                {(s.choices[f.key] ?? []).filter((c) => !FIELDS.some((o) => o.key !== f.key && wb.columns[o.key] === c)).map((c) => (
                  <option key={c} value={c}>
                    #{c}
                  </option>
                ))}
              </select>
            </Row>
          ))}
          {noColumns(wb.columns) && (
            <p className="set-note">
              No columns that fit?{" "}
              <button className="link" disabled={busy || !s.calibredb} onClick={() => act(api.calibreCreateColumns)}>
                Create the usual ones
              </button>{" "}
              (#deckle_progress, #deckle_read, #deckle_last_read, #deckle_rating, #deckle_highlights). Close calibre first: adding columns changes
              the library.
            </p>
          )}
        </div>
      )}

      {s.library && wb.columns.highlights !== undefined && (
        <Row
          label="Also into calibre's own annotations"
          note={
            s.annotations_supported
              ? "Listed by chapter in calibre's Browse annotations. calibre's viewer can't jump to them: readers keep their own positions."
              : "Needs calibre 9.18 or later (calibredb annotations import)."
          }
        >
          <Switch
            label="Also into calibre's own annotations"
            checked={wb.annotations}
            disabled={busy || (!wb.annotations && !s.annotations_supported)}
            onChange={(on) => set({ annotations: on })}
          />
        </Row>
      )}

      <details className="set-sub">
        <summary className="muted small">calibre open while Deckle writes? Its content server</summary>
        <p className="set-note">
          calibre won't let another program change a library it has open. Turn on its content server (calibre → Connect/share → Start content server) and give
          its address with the library's id, e.g. <code>http://localhost:8080/#Calibre_Library</code>.
        </p>
        <div className="export-row">
          <label>
            <span className="label">Address</span>
            <input defaultValue={wb.server ?? ""} placeholder="None: write to the folder" spellCheck={false} onBlur={(e) => e.target.value !== (wb.server ?? "") && set({ server: e.target.value })} />
          </label>
          <label>
            <span className="label">User (if it asks)</span>
            <input defaultValue={wb.username ?? ""} spellCheck={false} onBlur={(e) => e.target.value !== (wb.username ?? "") && set({ username: e.target.value })} />
          </label>
          {wb.username && (
            <form
              className="save-row"
              onSubmit={(e) => {
                e.preventDefault();
                if (password) act(() => api.calibreSetPassword(password)).then(() => setPassword(""));
              }}
            >
              <input type="password" autoComplete="new-password" value={password} placeholder="Password" onChange={(e) => setPassword(e.target.value)} aria-label="Content server password" />
              <button className="ghost" type="submit" disabled={!password}>
                Save
              </button>
            </form>
          )}
        </div>
        <Row label="calibredb" note={s.calibredb ?? "Not found"}>
          <input defaultValue={wb.calibredb ?? ""} placeholder="Found by itself" spellCheck={false} onBlur={(e) => e.target.value !== (wb.calibredb ?? "") && set({ calibredb: e.target.value })} aria-label="calibredb path" />
        </Row>
      </details>

      {plan && (
        <div className="set-sub writeback-plan">
          <h4>Next batch</h4>
          {noColumns(wb.columns) ? (
            <p className="muted small">Choose a column above to see what would be written.</p>
          ) : plan.changes.length === 0 && plan.annotations.length === 0 ? (
            <p className="muted small">Nothing to write: calibre already says what your readers say.</p>
          ) : (
            <ul className="wireless-books">
              {plan.changes.slice(0, 30).map((c) => (
                <li key={`${c.uuid}:${c.column}`}>{changeLine(c)}</li>
              ))}
              {plan.changes.length > 30 && <li className="muted">and {plan.changes.length - 30} more</li>}
            </ul>
          )}
          {plan.annotations.length > 0 && (
            <ul className="wireless-books">
              {plan.annotations.map((a) => (
                <li key={a.uuid}>
                  {a.title}: {a.count} {a.count === 1 ? "highlight" : "highlights"} into calibre's annotations
                </li>
              ))}
            </ul>
          )}
          {plan.to_confirm.length > 0 && (
            <>
              <p className="set-note">Matched only by title and author. Same book?</p>
              <ul className="wireless-books">
                {plan.to_confirm.map((t) => (
                  <li key={t.key}>
                    {t.title} ({t.authors.join(", ")}) is calibre's “{t.calibre_title}”{" "}
                    <button className="link" disabled={busy} onClick={() => act(() => api.calibreWritebackConfirm([t.key]))}>
                      Yes
                    </button>
                  </li>
                ))}
              </ul>
            </>
          )}
          {plan.conflicts.length > 0 && (
            <>
              <p className="set-note">Left alone: you set these in calibre, so Deckle won't change them.</p>
              <ul className="wireless-books muted">
                {plan.conflicts.map((c) => (
                  <li key={`${c.title}:${c.column}`}>
                    {c.title} · #{c.column}: {shown(c.field, c.calibre)} in calibre, {shown(c.field, c.reader)} on the reader
                  </li>
                ))}
              </ul>
            </>
          )}
          {plan.unmatched.length > 0 && <p className="muted small">Not in calibre: {plan.unmatched.join(", ")}</p>}
          <button className="ghost" disabled={busy || !!why || (plan.changes.length === 0 && plan.annotations.length === 0)} onClick={() => act(api.calibreWritebackWrite)}>
            {busy ? "Writing…" : "Write now"}
          </button>
        </div>
      )}
      {wb.last_run && (
        <p className="muted small">
          Last written {new Date(wb.last_run).toLocaleString()}: {wb.last_result.join("; ")}
        </p>
      )}
      {(error || s.error) && <p className="warn small">{error ?? s.error}</p>}
    </>
  );
}
