// SPDX-License-Identifier: GPL-3.0-or-later
import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import { sentMode, titleOf } from "./kindleRooms";
import { byOrigin, defaultTicked, gainsLine, leftAlone, partwayLine, selectedLine, sourceLine, sourcesLine, upgradeLabel, upgradesHeading } from "./upgradesView";
import type { DeviceBookRow, RunTarget, UpgradeOf, UpgradeRow, UpgradesView } from "./types";

interface Props {
  target: RunTarget;
  targetKey: string | null;
  busy: boolean;
  /** Deckle's own books an older Deckle built (the shelf's `updatable`): a group of their own. */
  updates: DeviceBookRow[];
  /** Puts the originals on the Books tab: an upgrade marked as one of the Kindle's copy, an update in the mode it was sent in. */
  onUpgrade: (items: { source: string; cde: "pdoc" | "ebok" | null; upgradeOf?: UpgradeOf }[], what: string) => void;
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
export default function Upgrades({ target, targetKey, busy, updates, onUpgrade, onChanged, onNotice, onCount }: Props) {
  const [view, setView] = useState<UpgradesView | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [ticked, setTicked] = useState<Set<string>>(new Set());
  const [puttingBack, setPuttingBack] = useState<string | null>(null);
  const [historyOpen, setHistoryOpen] = useState(false);
  // Older builds start ticked, as an upgrade you're not partway through does.
  const updateKey = updates.map((b) => b.file).join("|");
  useEffect(() => {
    setTicked((t) => new Set([...t, ...updates.map((b) => b.file)]));
  }, [updateKey]);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const v = await api.deviceUpgrades(target);
      setView(v);
      setTicked(new Set([...defaultTicked(v.rows), ...updates.map((b) => b.file)]));
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
  const upgrade = async (rows: UpgradeRow[], older: DeviceBookRow[]) => {
    if (!view || rows.length + older.length === 0) return;
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
    if (ready.length + older.length === 0) {
      onNotice(failed.join(" "));
      return;
    }
    const n = ready.length + older.length;
    onUpgrade(
      [
        ...ready.map((r) => ({ source: sources.get(r.file)!, cde: null, upgradeOf: { device: view.device, file: r.file, title: r.title, origin: r.origin } })),
        ...older.map((b) => ({ source: b.library_source!, cde: sentMode(b) })),
      ],
      `${n === 1 ? `${ready[0]?.title ?? titleOf(older[0])} is` : `${n} books are`} ready to upgrade.${ready.length ? " The Kindle's copies are kept, so any of them can be put back." : ""}${failed.length ? ` Left as they are: ${failed.join("; ")}.` : ""}`,
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
  const chosenOlder = updates.filter((b) => ticked.has(b.file));
  const total = view.rows.length + updates.length;
  const picked = chosen.length + chosenOlder.length;
  const alone = leftAlone(view.counts);
  const groups: { key: string; title: string; note: string; files: string[]; items: React.ReactNode[] }[] = [
    ...byOrigin(view.rows).map((g) => ({
      key: g.origin,
      title: g.title,
      note: g.origin === "kindle-copy" ? "No original here, so Deckle works from what's on the Kindle" : "Deckle has the original on this computer",
      files: g.rows.map((r) => r.file),
      items: g.rows.map((r) => {
        const warn = partwayLine(r);
        return (
          <li key={r.file} className="urow">
            <input type="checkbox" checked={ticked.has(r.file)} onChange={(e) => toggle(r.file, e.target.checked)} aria-label={`Upgrade ${r.title}`} />
            <span className="urow-cover" aria-hidden="true">
              {r.cover ? <img src={r.cover} alt="" /> : <span className="cover-placeholder">{r.title.slice(0, 1)}</span>}
            </span>
            <span className="urow-meta">
              <span>
                <strong>{r.title}</strong> <span className="muted">{r.authors.join(", ")}</span>
              </span>
              <span className="urow-gains">{gainsLine(r.gains)}</span>
              {warn && <span className="warn small">{warn}</span>}
            </span>
            <span className="urow-why" title={r.source ?? undefined}>
              {sourceLine(r)}
            </span>
          </li>
        );
      }),
    })),
    ...(updates.length
      ? [
          {
            key: "older",
            title: "Built by an older Deckle",
            note: "Deckle can build these better now, in the mode each was sent in",
            files: updates.map((b) => b.file),
            items: updates.map((b) => (
              <li key={b.file} className="urow">
                <input type="checkbox" checked={ticked.has(b.file)} onChange={(e) => toggle(b.file, e.target.checked)} aria-label={`Update ${titleOf(b)}`} />
                <span className="urow-cover" aria-hidden="true">
                  {b.cover ? <img src={b.cover} alt="" /> : <span className="cover-placeholder">{titleOf(b).slice(0, 1)}</span>}
                </span>
                <span className="urow-meta">
                  <span>
                    <strong>{titleOf(b)}</strong> <span className="muted">{b.authors.join(", ")}</span>
                  </span>
                  <span className="urow-gains">{b.updates.map((u) => u.text).join(" ")}</span>
                </span>
                <span className="urow-why">In your library</span>
              </li>
            )),
          },
        ]
      : []),
  ];
  const tickAll = (files: string[], on: boolean) =>
    setTicked((t) => {
      const next = new Set(t);
      for (const f of files) {
        if (on) next.add(f);
        else next.delete(f);
      }
      return next;
    });
  return (
    <section className="upgrades-room" aria-label="Upgrades">
      <div className="upgrades-intro">
        <h2>{upgradesHeading(total)}</h2>
        {total > 0 && (
          <p className="muted">
            Deckle rebuilds each one and puts it back where it was. Your place and your highlights stay. An upgraded book can be put back the way it was from History.
          </p>
        )}
      </div>

      {groups.map((g) => {
        const all = g.files.every((f) => ticked.has(f));
        return (
          <section key={g.key} className="ugroup" aria-label={g.title}>
            <div className="ugroup-head">
              <label>
                <input type="checkbox" checked={all} onChange={(e) => tickAll(g.files, e.target.checked)} aria-label={`Select all: ${g.title}`} />
                <span className="ugroup-title">{g.title}</span>
              </label>
              <span className="muted small">{g.note}</span>
            </div>
            <ul className="urows">{g.items}</ul>
          </section>
        );
      })}

      {alone.total > 0 && (
        <details className="left-alone">
          <summary>
            Deckle leaves {alone.total} book{alone.total === 1 ? "" : "s"} alone
          </summary>
          <p className="muted small">{alone.parts.join(" · ")}.</p>
        </details>
      )}
      <p className="muted small">{sourcesLine(view.sources)}</p>
      {view.warnings.map((w) => (
        <p key={w} className="warn small">
          {w}
        </p>
      ))}

      {view.history.length > 0 && (
        <details className="upgrade-history" open={historyOpen} onToggle={(e) => setHistoryOpen((e.target as HTMLDetailsElement).open)}>
          <summary>History: upgraded on this Kindle ({view.history.length})</summary>
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

      {total > 0 && (
        <div className="foot-bar" role="toolbar" aria-label="Upgrade">
          <span>
            <strong>{selectedLine(picked, total)}</strong> <span className="foot-bar-note">· keep the Kindle plugged in while it runs</span>
          </span>
          {view.history.length > 0 && (
            <button className="foot-bar-quiet" onClick={() => setHistoryOpen((o) => !o)}>
              History
            </button>
          )}
          <button className="primary" disabled={busy || rebuilding || picked === 0} onClick={() => upgrade(chosen, chosenOlder)}>
            {rebuilding ? "Rebuilding…" : upgradeLabel(picked)}
          </button>
        </div>
      )}
    </section>
  );
}
