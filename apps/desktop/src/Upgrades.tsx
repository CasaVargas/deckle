// SPDX-License-Identifier: GPL-3.0-or-later
import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import { byOrigin, defaultTicked, leftAsIs, partwayLine, sourceLine, sourcesLine, upgradeLabel } from "./upgradesView";
import type { RunTarget, UpgradeOf, UpgradeRow, UpgradesView } from "./types";

interface Props {
  target: RunTarget;
  targetKey: string | null;
  busy: boolean;
  /** Puts the originals on the Books tab, each marked as an upgrade of the Kindle's copy. */
  onUpgrade: (items: { source: string; cde: null; upgradeOf: UpgradeOf }[], what: string) => void;
  /** After a Put back changed the Kindle. */
  onChanged: () => void;
  onNotice: (msg: string | null) => void;
  /** How many books can be upgraded, once the room has looked. */
  onCount?: (n: number) => void;
}

/**
 * Books Deckle didn't send whose originals are on this computer (Books and Kindle design §7):
 * ticked by default unless you're partway through, upgraded through the Books tab, with a
 * history to put any of them back.
 */
export default function Upgrades({ target, targetKey, busy, onUpgrade, onChanged, onNotice, onCount }: Props) {
  const [view, setView] = useState<UpgradesView | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [ticked, setTicked] = useState<Set<string>>(new Set());
  const [puttingBack, setPuttingBack] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const v = await api.deviceUpgrades(target);
      setView(v);
      setTicked(defaultTicked(v.rows));
      onCount?.(v.rows.length);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [targetKey]);
  useEffect(() => {
    load();
  }, [load]);

  const toggle = (file: string, on: boolean) =>
    setTicked((t) => {
      const next = new Set(t);
      if (on) next.add(file);
      else next.delete(file);
      return next;
    });

  const [rebuilding, setRebuilding] = useState(false);

  // Books with no original here are rebuilt from the Kindle's own copy first; one that doesn't
  // come back whole stays as it is.
  const upgrade = async (rows: UpgradeRow[]) => {
    if (!view || rows.length === 0) return;
    const sources = new Map(rows.filter((r) => r.source).map((r) => [r.file, r.source as string]));
    const failed: string[] = [];
    const copies = rows.filter((r) => !r.source).map((r) => r.file);
    if (copies.length) {
      setRebuilding(true);
      try {
        for (const r of await api.deviceRebuild(target, copies)) {
          if (r.source) sources.set(r.file, r.source);
          else failed.push(`${rows.find((x) => x.file === r.file)?.title ?? r.file}: ${r.error}`);
        }
      } catch (e) {
        onNotice(String(e));
        return;
      } finally {
        setRebuilding(false);
      }
    }
    const ready = rows.filter((r) => sources.has(r.file));
    if (ready.length === 0) {
      onNotice(failed.join(" "));
      return;
    }
    onUpgrade(
      ready.map((r) => ({ source: sources.get(r.file)!, cde: null, upgradeOf: { device: view.device, file: r.file, title: r.title, origin: r.origin } })),
      `${ready.length === 1 ? `${ready[0].title} is` : `${ready.length} books are`} ready to upgrade. The Kindle's copies are kept, so any of them can be put back.${failed.length ? ` Left as they are: ${failed.join("; ")}.` : ""}`,
    );
  };

  const putBack = async (file: string) => {
    setPuttingBack(file);
    try {
      await api.devicePutBack(target, file);
      onNotice(null);
      onChanged();
      await load();
    } catch (e) {
      onNotice(String(e));
    } finally {
      setPuttingBack(null);
    }
  };

  if (loading && !view) {
    return <p className="muted small upgrades-loading">Looking for originals of this Kindle's books… The first time over MTP, Deckle reads every book once.</p>;
  }
  if (error) return <div className="result fail">Upgrades: {error}</div>;
  if (!view) return null;

  const chosen = view.rows.filter((r) => ticked.has(r.file));
  const left = leftAsIs(view.counts);
  return (
    <section className="upgrades" aria-label="Upgrades">
      {view.rows.length > 0 ? (
        <div className="upgrades-head">
          <div>
            <h3>
              {view.rows.length} book{view.rows.length === 1 ? "" : "s"} can be upgraded
            </h3>
            <p className="muted small">
              Each comes back with what a store book has, built from its original on this computer or, failing that, from the Kindle's own copy. The Kindle's copy is kept with its highlights and
              reading data, so any upgrade can be put back.
            </p>
          </div>
          <button className="primary" disabled={busy || rebuilding || chosen.length === 0} onClick={() => upgrade(chosen)}>
            {rebuilding ? "Rebuilding…" : upgradeLabel(chosen.length)}
          </button>
        </div>
      ) : (
        <p className="muted small">Every book on this Kindle that Deckle can upgrade is already current.</p>
      )}

      {byOrigin(view.rows).map((g) => (
        <div key={g.origin} className="upgrade-group">
          <h4>
            {g.title} <span className="muted small">· {g.rows.length}</span>
          </h4>
          <ul className="books upgrade-list" aria-label={g.title}>
            {g.rows.map((r) => {
              const warn = partwayLine(r);
              return (
                <li key={r.file} className="row">
                  <label className="upgrade-tick">
                    <input type="checkbox" checked={ticked.has(r.file)} onChange={(e) => toggle(r.file, e.target.checked)} aria-label={`Upgrade ${r.title}`} />
                  </label>
                  <div className="cover-thumb" aria-hidden="true">
                    {r.cover ? <img src={r.cover} alt="" /> : <div className="cover-placeholder">{r.title.slice(0, 1)}</div>}
                  </div>
                  <div className="meta">
                    <div className="title-line">
                      <span className="title" title={r.file}>
                        {r.title}
                      </span>
                    </div>
                    <div className="byline">{r.authors.join(", ")}</div>
                    <div className="shipped" title={r.source ?? undefined}>
                      {sourceLine(r)}
                    </div>
                    <div className="gains">
                      {r.gains.map((g) => (
                        <span key={g} className="badge">
                          {g}
                        </span>
                      ))}
                    </div>
                    {warn && <p className="warn small">{warn}</p>}
                  </div>
                  <button className="ghost" disabled={busy || rebuilding} onClick={() => upgrade([r])}>
                    Upgrade
                  </button>
                </li>
              );
            })}
          </ul>
        </div>
      ))}

      {left && <p className="muted small">{left}</p>}
      <p className="muted small">{sourcesLine(view.sources)}</p>
      {view.warnings.map((w) => (
        <p key={w} className="warn small">
          {w}
        </p>
      ))}

      {view.history.length > 0 && (
        <details className="upgrade-history">
          <summary>
            Upgraded on this Kindle ({view.history.length})
          </summary>
          <ul>
            {view.history.map((h) => (
              <li key={`${h.old_file}-${h.at}`}>
                <span>
                  <strong>{h.title}</strong>
                  <span className="muted small"> · {h.at.slice(0, 10)} · was {h.old_file}</span>
                </span>
                <button className="ghost" disabled={busy || puttingBack !== null} onClick={() => putBack(h.old_file)}>
                  {puttingBack === h.old_file ? "Putting back…" : "Put back"}
                </button>
              </li>
            ))}
          </ul>
        </details>
      )}
    </section>
  );
}
