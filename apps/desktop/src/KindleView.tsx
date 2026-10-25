// SPDX-License-Identifier: GPL-3.0-or-later
// The Kindle tab (Books and Kindle design §7), in rooms like the Daily tab: the Shelf (every book,
// how far in, filtered and sorted), Upgrades, Highlights, Words, and This Kindle (space, make
// room, back up and restore).
import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import Highlights from "./Highlights";
import Icon from "./Icon";
import { KINDLE_ROOMS, SHELF_FILTERS, SHELF_SORTS, filterCounts, kindleRoomNotes, progressLine, shelfBooks, size, updatable } from "./kindleRooms";
import type { KindleRoom, ShelfFilter, ShelfSort } from "./kindleRooms";
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
}

/** The class badge a book on the Shelf wears, when its class says something. */
const CLASS_BADGE: Partial<Record<UpgradeClass, { text: string; title: string }>> = {
  "has-source": { text: "Can be upgraded", title: "Its original is on this computer: see Upgrades" },
  "kindle-copy": { text: "Can be upgraded", title: "Deckle can rebuild it from the Kindle's own copy: see Upgrades" },
  "amazon-own": { text: "Amazon", title: "From the Kindle Store or Send to Kindle: Amazon keeps a copy" },
  locked: { text: "Locked", title: "Locked by DRM: Deckle never opens it" },
  "fixed-layout": { text: "Fixed layout", title: "Comics and picture books are left as they are" },
};

/** The mode an update keeps: the one it was sent in. */
function sentMode(book: DeviceBookRow): "pdoc" | "ebok" | null {
  return book.cde === "EBOK" ? "ebok" : book.cde === "PDOC" ? "pdoc" : null;
}

function updateBadges(book: DeviceBookRow) {
  const older = book.updates.filter((u) => u.older);
  const missing = book.updates.filter((u) => !u.older);
  return (
    <>
      {older.length > 0 && (
        <span className="badge stale" title={older.map((u) => u.text).join("\n")}>
          Older build
        </span>
      )}
      {missing.map((u) => (
        <span key={u.feature} className="badge stale" title={u.text}>
          Missing {u.feature}
        </span>
      ))}
      {book.duplicate && (
        <span className="badge dup" title="Another book on this Kindle is the same one (same ASIN, or same title and author)">
          Duplicate
        </span>
      )}
    </>
  );
}

function BookLine({ book, busy, now, onResend, onRemove }: { book: DeviceBookRow; busy: boolean; now: Date; onResend: Props["onResend"]; onRemove: (file: string) => Promise<void> }) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [removing, setRemoving] = useState(false);
  const title = book.title ?? book.file;
  const canUpgrade = book.library_source && book.cde === "PDOC";
  return (
    <li className="row">
      <div className="cover-thumb" aria-hidden="true">
        {book.cover ? <img src={book.cover} alt="" /> : <div className="cover-placeholder">{title.slice(0, 1)}</div>}
      </div>
      <div className="meta">
        <div className="title-line">
          <span className="title" title={book.file}>
            {title}
          </span>
          {book.cde ? (
            <span className={`badge ${book.cde === "EBOK" ? "features" : "cover"}`} title={book.cde === "EBOK" ? "A Kindle book (EBOK): X-Ray and Word Wise capable" : "A personal document (PDOC): the cover always shows"}>
              {book.cde}
            </span>
          ) : (
            <span className="badge">{book.format.toUpperCase()}</span>
          )}
          {updateBadges(book)}
          {CLASS_BADGE[book.class] && (
            <span className="badge" title={CLASS_BADGE[book.class]!.title}>
              {CLASS_BADGE[book.class]!.text}
            </span>
          )}
        </div>
        <div className="byline">
          {book.authors.length > 0 ? book.authors.join(", ") : book.title ? <em>Unknown author</em> : <span className="mono">{book.format} file</span>}
        </div>
        <div className="progress-line">
          <div className={`progress-track ${book.reading}`} aria-hidden="true">
            <div className="progress-fill" style={{ width: `${Math.round((book.fraction ?? 0) * 100)}%` }} />
          </div>
          <span className="muted small">{progressLine(book, now)}</span>
        </div>
        <div className="shipped">
          {[book.xray && "X-Ray", book.word_wise && "Word Wise", size(book.size), book.from_deckle ? (book.library_source ? "in your library" : "from Deckle") : "not from Deckle"]
            .filter(Boolean)
            .join(" · ")}
        </div>
        {confirming && (
          <div className="confirm" role="alert">
            Remove <strong>{title}</strong> and its X-Ray and Word Wise files from this Kindle?
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
          </div>
        )}
      </div>
      <div className="row-menu">
        <button className="icon" aria-label={`Actions for ${title}`} onClick={() => setMenuOpen((o) => !o)} disabled={busy || removing}>
          <Icon name="dots" size={18} />
        </button>
        {menuOpen && (
          <div className="menu" onMouseLeave={() => setMenuOpen(false)}>
            {book.updates.length > 0 && book.library_source && (
              <button
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
              disabled={!book.library_source}
              title={book.library_source ? undefined : "Not in your library, so Deckle has no copy to send"}
              onClick={() => {
                setMenuOpen(false);
                onResend([{ source: book.library_source!, cde: null }], `${title} is ready to send again.`);
              }}
            >
              Re-send
            </button>
            {canUpgrade && (
              <button
                onClick={() => {
                  setMenuOpen(false);
                  onResend([{ source: book.library_source!, cde: "ebok" }], `${title} is ready to send again as EBOK.`);
                }}
              >
                Upgrade to EBOK (X-Ray, Word Wise)
              </button>
            )}
            <button
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
      </div>
    </li>
  );
}

export default function KindleView({ target, targetKey, device, busy, onResend, onNotice, onRenamed }: Props) {
  const [shelf, setShelf] = useState<DeviceShelf | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [room, setRoom] = useState<KindleRoom>("shelf");
  const [filter, setFilter] = useState<ShelfFilter>("all");
  const [sort, setSort] = useState<ShelfSort>("recent");
  const [query, setQuery] = useState("");
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
  const notes = kindleRoomNotes(shelf, upgradesCount);
  const counts = shelf ? filterCounts(shelf.books) : null;
  const shown = shelf ? shelfBooks(shelf.books, filter, query, sort) : [];
  return (
    <div className="daily-rooms kindle-rooms">
      <nav className="rooms" aria-label="Kindle">
        {KINDLE_ROOMS.map((r) => (
          <button key={r.key} className={room === r.key ? "on" : ""} aria-current={room === r.key ? "page" : undefined} onClick={() => open(r.key)}>
            <span className="room-title">{r.title}</span>
            {notes[r.key] && <span className="room-note">{notes[r.key]}</span>}
          </button>
        ))}
      </nav>

      <main className="list-area">
        <div className="shelf-head">
          <div>
            <h2>{shelf?.device ?? "Kindle"}</h2>
            <div className="muted small">{shelf ? notes.shelf : loading ? "Reading the Kindle…" : ""}</div>
          </div>
          <button className="ghost" onClick={load} disabled={loading}>
            {loading ? "Reading…" : "Refresh"}
          </button>
        </div>
        {error && <div className="result fail">{error}</div>}

        {room === "shelf" && shelf && (
          <>
            <div className="shelf-tools">
              <div className="filter-chips" role="group" aria-label="Show">
                {SHELF_FILTERS.map((f) => (
                  <button key={f.key} className={`filter-chip ${filter === f.key ? "on" : ""}`} aria-pressed={filter === f.key} onClick={() => setFilter(f.key)} disabled={!!counts && counts[f.key] === 0 && f.key !== "all"}>
                    {f.title} <span className="muted">{counts?.[f.key] ?? 0}</span>
                  </button>
                ))}
              </div>
              <div className="shelf-search">
                <input type="search" placeholder="Search titles and authors" value={query} onChange={(e) => setQuery(e.target.value)} aria-label="Search this Kindle" />
                <select value={sort} onChange={(e) => setSort(e.target.value as ShelfSort)} aria-label="Sort by">
                  {SHELF_SORTS.map((o) => (
                    <option key={o.key} value={o.key}>
                      {o.title}
                    </option>
                  ))}
                </select>
              </div>
            </div>
            {shelf.unread_positions > 0 && <p className="muted small">{shelf.unread_positions} books' places weren't read this time; they show as not started.</p>}
            {shown.length === 0 ? (
              <p className="muted">{shelf.books.length === 0 ? "No books in documents/ yet." : "No books match."}</p>
            ) : (
              <ul className="books" aria-label="Books on this Kindle">
                {shown.map((b) => (
                  <BookLine key={b.file} book={b} busy={busy || loading} now={now} onResend={onResend} onRemove={remove} />
                ))}
              </ul>
            )}
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
