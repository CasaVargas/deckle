// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";
import { api } from "../api";
import { KINDS, diffLine, makeRule, needsYes, ruleLine, targetName } from "../shelvesView";
import type { ShelvesConfig, ShelvesView } from "../types";
import { Row, Switch } from "./Row";

/**
 * Shelves from calibre (calibre bridge design §6.4): rules that make a Kobo's shelves and
 * KOReader's collections follow calibre's tags, series, authors, columns or saved searches, a
 * preview on the plugged-in reader, and Fix shelves. Only shelves Deckle made lose books.
 */
export default function CalibreShelves() {
  const [v, setV] = useState<ShelvesView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [kind, setKind] = useState("tag");
  const [value, setValue] = useState("");
  const [name, setName] = useState("");

  useEffect(() => {
    api.shelvesStatus().then(setV, (e) => setError(String(e)));
  }, []);

  const act = async (f: () => Promise<ShelvesView>) => {
    setBusy(true);
    setError(null);
    try {
      setV(await f());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  if (!v) return error ? <p className="warn small">{error}</p> : null;
  const cfg = v.config;
  const save = (c: ShelvesConfig) => act(() => api.shelvesSet(c));
  const add = () => {
    const r = makeRule(kind, value, name);
    if (typeof r === "string") return setError(r);
    setValue("");
    setName("");
    save({ ...cfg, rules: [...cfg.rules, r] });
  };
  const placeholder = KINDS.find((k) => k.key === kind)?.placeholder ?? "";

  return (
    <>
      {cfg.rules.length > 0 && (
        <ul className="wireless-books">
          {cfg.rules.map((r, i) => (
            <li key={`${i}:${r.from}`}>
              {ruleLine(r)}{" "}
              <button className="link" disabled={busy} onClick={() => save({ ...cfg, rules: cfg.rules.filter((_, j) => j !== i) })}>
                Remove
              </button>
            </li>
          ))}
        </ul>
      )}
      <form
        className="save-row"
        onSubmit={(e) => {
          e.preventDefault();
          add();
        }}
      >
        <select value={kind} onChange={(e) => setKind(e.target.value)} aria-label="Rule kind">
          {KINDS.map((k) => (
            <option key={k.key} value={k.key}>
              {k.label}
            </option>
          ))}
        </select>
        <input value={value} placeholder={placeholder} onChange={(e) => setValue(e.target.value)} aria-label="From" spellCheck={false} />
        {!(kind === "series" && value.trim() === "*") && (
          <input value={name} placeholder="Shelf name" onChange={(e) => setName(e.target.value)} aria-label="Shelf name" />
        )}
        <button className="ghost" type="submit" disabled={busy}>
          Add
        </button>
      </form>
      <Row label="Keep them in step by themselves" note="When a Kobo or PocketBook is plugged in, and when KOReader connects over Wi-Fi.">
        <Switch label="Keep them in step by themselves" checked={cfg.auto} disabled={busy} onChange={(on) => save({ ...cfg, auto: on })} />
      </Row>

      {v.reader ? (
        <div className="set-sub writeback-plan">
          <h4>On {v.reader[1]}</h4>
          {v.targets.length === 0 && <p className="muted small">No Kobo shelves or KOReader on it.</p>}
          {v.targets.map((t) => (
            <div key={t.kind}>
              <p className="small">
                <strong>{targetName(t)}:</strong> {diffLine(t.diff)}
                {t.applied ? " — done" : ""}
              </p>
              {needsYes(t, cfg.kobo_ok) && (
                <p className="set-note">
                  A Kobo signed in to a Kobo account may sync shelves with Kobo's servers and back; Deckle's follow calibre, not the cloud. Deckle backs up the
                  Kobo's database before each change.{" "}
                  <button className="link" disabled={busy} onClick={() => act(() => api.shelvesKoboOk(t.key))}>
                    Write shelves on this Kobo
                  </button>
                </p>
              )}
              {t.note && !needsYes(t, cfg.kobo_ok) && <p className="muted small">{t.note}</p>}
            </div>
          ))}
          <button className="ghost" disabled={busy || cfg.rules.length === 0} onClick={() => act(api.shelvesApply)}>
            {busy ? "Working…" : "Fix shelves"}
          </button>
        </div>
      ) : (
        cfg.rules.length > 0 && <p className="muted small">Plug in a Kobo or PocketBook to see and fix its shelves. KOReader over Wi-Fi gets them when it connects.</p>
      )}
      {v.sets.length > 0 && (
        <p className="muted small">
          In calibre now: {v.sets.slice(0, 8).map(([n, c]) => `${n || "each series"} (${c})`).join(", ")}
          {v.sets.length > 8 ? `, and ${v.sets.length - 8} more` : ""}
        </p>
      )}
      {(error || v.error) && <p className="warn small">{error ?? v.error}</p>}
    </>
  );
}
