// SPDX-License-Identifier: GPL-3.0-or-later
// The Kindle tab (Books and Kindle design §7, Kindle tab redesign §2–3), in tabs: the Shelf (what's
// worth a look, what you're reading, then every book as covers), Upgrades, Highlights, Words, and
// This Kindle (space, make room, back up and restore).
import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import Highlights from "./Highlights";
import Icon from "./Icon";
import { KINDLE_ROOMS, SHELF_FROMS, SHELF_SHOWS, SHELF_SORTS, canBeBetter, continueReading, coverLine, progressLine, shelfBooks, showCounts, tabCounts, titleOf, updatable, worthALook } from "./kindleRooms";
import type { KindleRoom, ShelfFrom, ShelfShow, ShelfSort } from "./kindleRooms";
import ThisKindle from "./ThisKindle";
import Upgrades from "./Upgrades";
import WordsAnki from "./WordsAnki";
import type { DeviceBookRow, DeviceRow, DeviceShelf, RunTarget, UpgradeClass, UpgradeOf } from "./types";

interface Props {
  /** The Kindle picked in the footer; null when none is connected. */
  target: RunTarget | null;
  targetKey: string | null;
  /** That Kindle's row: its name and sync id, for This Kindle's Name field. */
  device: DeviceRow | null;
  busy: boolean;
  /** Put library copies on the Books tab for this Kindle, each with its mode; `what` opens
   * the notice. */
  onResend: (items: { source: string; cde: "pdoc" | "ebok" | null; upgradeOf?: UpgradeOf }[], what: string) => void;
  onNotice: (msg: string | null) => void;
  /** After the Kindle was named. */
  onRenamed: () => Promise<void>;
  /** The top bar's search text. */
  query: string;
  /** The Kindle's free space once read, for the top bar's pill. */
  onFree: (bytes: number | null) => void;
}

/** The mode an update keeps: the one it was sent in. */
function sentMode(book: DeviceBookRow): "pdoc" | "ebok" | null {
  return book.cde === "EBOK" ? "ebok" : book.cde === "PDOC" ? "pdoc" : null;
}

/** A thin bar for how far in: green when finished. */
function Bar({ book }: { book: DeviceBookRow }) {
  const pct = book.reading === "finished" ? 100 : Math.round((book.fraction ?? 0) * 100);
  return (
    <span className="cover-bar" aria-hidden="true">
      <span className={book.reading === "finished" ? "done" : ""} style={{ width: `${pct}%` }} />
    </span>
  );
}

function Cover({ book, className }: { book: DeviceBookRow; className: string }) {
  const title = titleOf(book);
  return (
    <span className={className} aria-hidden="true">
      {book.cover ? <img src={book.cover} alt="" /> : <span className="cover-placeholder">{title.slice(0, 1)}</span>}
    </span>
  );
}

/** A book on the Shelf: its cover, how far in and what it is, and what can be done with it. */
function BookCard({ book, busy, onResend, onRemove }: { book: DeviceBookRow; busy: boolean; onResend: Props["onResend"]; onRemove: (file: string) => Promise<void> }) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [removing, setRemoving] = useState(false);
  const title = titleOf(book);
  const canUpgrade = book.library_source && book.cde === "PDOC";
  return (
    <li className="kcard">
      <button className="kcard-open" title={book.file} onClick={() => setMenuOpen((o) => !o)} disabled={busy || removing} aria-haspopup="menu" aria-expanded={menuOpen}>
        <span className="kcard-cover">
          <Cover book={book} className="kcard-img" />
          {canBeBetter(book) && <span className="upgrade-mark">Upgrade</span>}
          <Bar book={book} />
        </span>
        <span className="kcard-title">{title}</span>
        <span className="kcard-line">{coverLine(book)}</span>
      </button>
      {menuOpen && (
        <div className="menu" role="menu" onMouseLeave={() => setMenuOpen(false)}>
          {book.updates.length > 0 && book.library_source && (
            <button
              role="menuitem"
              title={book.updates.map((u) => u.text).join("\n")}
              onClick={() => {
                setMenuOpen(false);
                onResend([{ source: book.library_source!, cde: sentMode(book) }], `${title} is ready to update.`);
              }}
            >
              Update
            </button>
          )}
          <button
            role="menuitem"
            disabled={!book.library_source}
            title={book.library_source ? undefined : "Not in your library, so Deckle has no copy to send"}
            onClick={() => {
              setMenuOpen(false);
              onResend([{ source: book.library_source!, cde: null }], `${title} is ready to send again.`);
            }}
          >
            Send again
          </button>
          {canUpgrade && (
            <button
              role="menuitem"
              onClick={() => {
                setMenuOpen(false);
                onResend([{ source: book.library_source!, cde: "ebok" }], `${title} is ready to send again as a Kindle book.`);
              }}
            >
              Make it a Kindle book (X-Ray, Word Wise)
            </button>
          )}
          <button
            role="menuitem"
            className="danger"
            onClick={() => {
              setMenuOpen(false);
              setConfirming(true);
            }}
          >
            Remove from Kindle…
          </button>
        </div>
      )}
      {confirming && (
        <div className="confirm kcard-confirm" role="alert">
          Remove <strong>{title}</strong>? Its reading position goes too.
          <span className="button-row">
            <button
              className="ghost danger"
              disabled={removing}
              onClick={async () => {
                setRemoving(true);
                await onRemove(book.file);
                setRemoving(false);
                setConfirming(false);
              }}
            >
              {removing ? "Removing…" : "Remove"}
            </button>
            <button className="ghost" disabled={removing} onClick={() => setConfirming(false)}>
              Keep
            </button>
          </span>
        </div>
      )}
    </li>
  );
}

/** "Read just now", "Read 5 min ago". */
function readAgo(at: Date | null, now: Date): string {
  if (!at) return "";
  const mins = Math.floor((now.getTime() - at.getTime()) / 60_000);
  return mins < 1 ? "Read just now" : mins < 60 ? `Read ${mins} min ago` : `Read at ${at.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}`;
}

export default function KindleView({ target, targetKey, device, busy, onResend, onNotice, onRenamed, query, onFree }: Props) {
  const [shelf, setShelf] = useState<DeviceShelf | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [room, setRoom] = useState<KindleRoom>("shelf");
  const [show, setShow] = useState<ShelfShow>("all");
  const [from, setFrom] = useState<ShelfFrom>("anywhere");
  const [sort, setSort] = useState<ShelfSort>("recent");
  const [readAt, setReadAt] = useState<Date | null>(null);
  // The Upgrades room reads every book once over MTP: it waits until it's opened, then stays.
  const [upgradesOpened, setUpgradesOpened] = useState(false);
  const [upgradesCount, setUpgradesCount] = useState<number | null>(null);

  const load = useCallback(async () => {
    if (!target) {
      setShelf(null);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      setShelf(await api.deviceBooks(target));
      setReadAt(new Date());
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
  useEffect(() => onFree(shelf?.space?.free ?? null), [shelf, onFree]);
  useEffect(() => {
    setUpgradesOpened(false);
    setUpgradesCount(null);
  }, [targetKey]);

  const remove = async (file: string) => {
    if (!target) return;
    try {
      await api.deviceRemove(target, file);
      onNotice(null);
      await load();
    } catch (e) {
      onNotice(String(e));
    }
  };

  if (!target) {
    return (
      <main className="list-area empty">
        <div className="dropzone-inner">
          <h1>No Kindle connected</h1>
          <p>Connect a Kindle by USB to see the books on it, how far into each you are, how much room is left, and which ones Deckle can upgrade.</p>
        </div>
      </main>
    );
  }

  const open = (r: KindleRoom) => {
    setRoom(r);
    if (r === "upgrades") setUpgradesOpened(true);
  };
  const now = new Date();
  const toUpdate = shelf ? updatable(shelf.books) : [];
  const updateAll = () =>
    onResend(
      toUpdate.map((b) => ({ source: b.library_source!, cde: sentMode(b) })),
      `${toUpdate.length} book${toUpdate.length === 1 ? " is" : "s are"} ready to update, each in the mode it was sent in.`,
    );
  const counts = tabCounts(shelf, upgradesCount);
  const nudges = shelf ? worthALook(shelf, upgradesCount) : [];
  const reading = shelf ? continueReading(shelf.books) : [];
  const shown = shelf ? shelfBooks(shelf.books, show, from, query, sort) : [];
  const showing = shelf ? showCounts(shelf.books, from) : null;
  return (
    <div className="kindle-view">
      <nav className="kindle-tabs" aria-label="Kindle">
        {KINDLE_ROOMS.map((r) => (
          <button key={r.key} className={room === r.key ? "on" : ""} aria-current={room === r.key ? "page" : undefined} onClick={() => open(r.key)}>
            {r.title}
            {r.key === "upgrades" && counts.upgrades > 0 && <span className="tab-badge">{counts.upgrades}</span>}
            {r.key === "highlights" && counts.highlights && <span className="tab-note">{counts.highlights}</span>}
          </button>
        ))}
        <span className="spacer" />
        <span className="tab-note">{loading ? "Reading the Kindle…" : readAgo(readAt, now)}</span>
        <button className="icon" aria-label="Read the Kindle again" onClick={load} disabled={loading}>
          <Icon name="refresh" size={16} />
        </button>
      </nav>
      <main className="kindle-main">
        {error && <div className="result fail">{error}</div>}

        {room === "shelf" && shelf && (
          <>
            {nudges.length > 0 && (
              <section className="nudges" aria-label="Worth a look">
                {nudges.map((n) => (
                  <div key={n.key} className="nudge">
                    <strong>{n.title}</strong>
                    <span>{n.body}</span>
                    <button className="link-action" onClick={() => open(n.room)}>
                      {n.action} →
                    </button>
                  </div>
                ))}
              </section>
            )}
            {reading.length > 0 && !query.trim() && (
              <section aria-label="Continue reading">
                <h2 className="kindle-h2">Continue reading</h2>
                <ul className="reading-cards">
                  {reading.map((b) => (
                    <li key={b.file} className="reading-card">
                      <Cover book={b} className="reading-cover" />
                      <span className="reading-meta">
                        <span className="reading-title">{titleOf(b)}</span>
                        <span className="muted small">{b.authors.join(", ")}</span>
                        <span className="spacer" />
                        <Bar book={b} />
                        <span className="small">{progressLine(b, now)}</span>
                      </span>
                    </li>
                  ))}
                </ul>
              </section>
            )}
            <section aria-label="On this Kindle">
              <div className="shelf-bar">
                <h2 className="kindle-h2">
                  On this Kindle <span className="muted">{showing?.all ?? 0}</span>
                </h2>
                <div className="segmented" role="group" aria-label="Show">
                  {SHELF_SHOWS.map((o) => (
                    <button key={o.key} className={show === o.key ? "on" : ""} aria-pressed={show === o.key} onClick={() => setShow(o.key)} disabled={o.key !== "all" && showing?.[o.key] === 0}>
                      {o.title}
                    </button>
                  ))}
                </div>
                <select className="quiet-select" value={from} onChange={(e) => setFrom(e.target.value as ShelfFrom)} aria-label="From">
                  {SHELF_FROMS.map((o) => (
                    <option key={o.key} value={o.key}>
                      {o.title}
                    </option>
                  ))}
                </select>
                <span className="spacer" />
                <select className="quiet-select" value={sort} onChange={(e) => setSort(e.target.value as ShelfSort)} aria-label="Sort by">
                  {SHELF_SORTS.map((o) => (
                    <option key={o.key} value={o.key}>
                      {o.title}
                    </option>
                  ))}
                </select>
              </div>
              {shelf.unread_positions > 0 && <p className="muted small">{shelf.unread_positions} books' places weren't read this time; they show as unread.</p>}
              {shown.length === 0 ? (
                <p className="muted">{shelf.books.length === 0 ? "No books in documents/ yet." : "No books match."}</p>
              ) : (
                <ul className="kcards" aria-label="Books on this Kindle">
                  {shown.map((b) => (
                    <BookCard key={b.file} book={b} busy={busy || loading} onResend={onResend} onRemove={remove} />
                  ))}
                </ul>
              )}
            </section>
          </>
        )}

        {upgradesOpened && shelf && (
          <div hidden={room !== "upgrades"}>
            {toUpdate.length > 0 && (
              <div className="update-banner" role="status">
                <span>
                  <strong>{toUpdate.length === 1 ? "One of Deckle's books" : `${toUpdate.length} of Deckle's books`} can be updated</strong>{" "}
                  — Deckle can build {toUpdate.length === 1 ? "it" : "them"} better now.
                </span>
                <button className="primary" onClick={updateAll} disabled={busy}>
                  Update all
                </button>
              </div>
            )}
            <Upgrades target={target} targetKey={targetKey} busy={busy || loading} onUpgrade={onResend} onChanged={load} onNotice={onNotice} onCount={setUpgradesCount} />
          </div>
        )}

        {room === "highlights" &&
          shelf &&
          (shelf.highlights.length > 0 || shelf.highlights_error ? (
            <Highlights target={target} rows={shelf.highlights} error={shelf.highlights_error} busy={busy || loading} onWritten={load} />
          ) : (
            <p className="muted">No highlights on this Kindle yet. Highlight a passage and it shows here, ready to write out as Markdown.</p>
          ))}

        {room === "words" && shelf && <WordsAnki target={target} busy={busy || loading} />}

        {room === "device" && shelf && <ThisKindle target={target} device={device} shelf={shelf} busy={busy || loading} onChanged={load} onNotice={onNotice} onRenamed={onRenamed} />}
      </main>
    </div>
  );
}
