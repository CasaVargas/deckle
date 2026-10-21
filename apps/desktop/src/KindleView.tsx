// SPDX-License-Identifier: GPL-3.0-or-later
import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import type { DeviceBookRow, DeviceShelf, RunTarget } from "./types";

interface Props {
  /** The Kindle picked in the footer; null when none is connected. */
  target: RunTarget | null;
  targetKey: string | null;
  busy: boolean;
  /** Put library copies on the Books tab for this Kindle, each with its mode; `what` opens
   * the notice. */
  onResend: (items: { source: string; cde: "pdoc" | "ebok" | null }[], what: string) => void;
  onNotice: (msg: string | null) => void;
}

function size(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1e3))} KB`;
}

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

function BookLine({ book, busy, onResend, onRemove }: { book: DeviceBookRow; busy: boolean; onResend: Props["onResend"]; onRemove: (file: string) => Promise<void> }) {
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
        </div>
        <div className="byline">
          {book.authors.length > 0 ? book.authors.join(", ") : book.title ? <em>Unknown author</em> : <span className="mono">{book.format} file</span>}
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
          ⋯
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

export default function KindleView({ target, targetKey, busy, onResend, onNotice }: Props) {
  const [shelf, setShelf] = useState<DeviceShelf | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

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
          <p>Connect a Kindle by USB to see the books on it, how much room is left, and which ones have X-Ray and Word Wise.</p>
        </div>
      </main>
    );
  }

  const used = shelf?.space ? shelf.space.total - shelf.space.free : 0;
  const updatable = shelf?.books.filter((b) => b.updates.length > 0 && b.library_source) ?? [];
  const updateAll = () =>
    onResend(
      updatable.map((b) => ({ source: b.library_source!, cde: sentMode(b) })),
      `${updatable.length} book${updatable.length === 1 ? " is" : "s are"} ready to update, each in the mode it was sent in.`,
    );
  const deckle = shelf?.books.filter((b) => b.from_deckle).length ?? 0;
  return (
    <main className="list-area">
      <div className="shelf-head">
        <div>
          <h2>{shelf?.device ?? "Kindle"}</h2>
          <div className="muted small">
            {shelf
              ? `${shelf.books.length} book${shelf.books.length === 1 ? "" : "s"}${deckle ? `, ${deckle} from Deckle` : ""}`
              : loading
                ? "Reading the Kindle…"
                : ""}
          </div>
        </div>
        {shelf?.space && (
          <div className="space" title={`${size(shelf.space.free)} free of ${size(shelf.space.total)}`}>
            <div className="space-track">
              <div className="space-fill" style={{ width: `${Math.min(100, (used / shelf.space.total) * 100)}%` }} />
            </div>
            <span className="muted small">{size(shelf.space.free)} free</span>
          </div>
        )}
        <button className="ghost" onClick={load} disabled={loading}>
          {loading ? "Reading…" : "Refresh"}
        </button>
      </div>
      {error && <div className="result fail">{error}</div>}
      {updatable.length > 0 && (
        <div className="update-banner" role="status">
          <span>
            <strong>
              {updatable.length} book{updatable.length === 1 ? "" : "s"} can be updated
            </strong>{" "}
            — Deckle can build {updatable.length === 1 ? "it" : "them"} better now.
          </span>
          <button className="primary solo" onClick={updateAll} disabled={busy}>
            Update all
          </button>
        </div>
      )}
      {shelf && shelf.books.length === 0 && <p className="muted">No books in documents/ yet.</p>}
      {shelf && (
        <ul className="books" aria-label="Books on this Kindle">
          {shelf.books.map((b) => (
            <BookLine key={b.file} book={b} busy={busy || loading} onResend={onResend} onRemove={remove} />
          ))}
        </ul>
      )}
    </main>
  );
}
