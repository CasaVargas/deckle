// SPDX-License-Identifier: GPL-3.0-or-later
import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "./api";
import { fromLabel, libraryItems, seriesLine } from "./libraryView";
import type { LibraryFilter, LibraryItem } from "./libraryView";
import type { BookSearchReport, CatalogEntry, CatalogView } from "./types";

interface Props {
  catalog: CatalogView;
  /** A Kindle is picked in the footer: "Not on this Kindle" means something. */
  hasKindle: boolean;
  /** Paths already in To send. */
  queued: Set<string>;
  /** The top bar's search text: it filters the covers as you type. */
  query: string;
  busy: boolean;
  onAdd: (paths: string[]) => void;
}

/** Fetches covers for the cards that scroll into view, a few at a time. */
function useCovers() {
  const [covers, setCovers] = useState<Record<string, string | null>>({});
  const wanted = useRef(new Set<string>());
  const asked = useRef(new Set<string>());
  const timer = useRef<number | null>(null);
  const flush = useCallback(() => {
    timer.current = null;
    const batch = [...wanted.current].filter((p) => !asked.current.has(p)).slice(0, 24);
    wanted.current.clear();
    if (batch.length === 0) return;
    batch.forEach((p) => asked.current.add(p));
    api
      .libraryCovers(batch)
      .then((rows) => setCovers((c) => ({ ...c, ...Object.fromEntries(rows) })))
      .catch(() => {});
  }, []);
  const want = useCallback(
    (path: string) => {
      if (asked.current.has(path)) return;
      wanted.current.add(path);
      if (timer.current == null) timer.current = window.setTimeout(flush, 120);
    },
    [flush],
  );
  return { covers, want };
}

function Card({ book, cover, selected, queued, onToggle, onSeen }: { book: CatalogEntry; cover: string | null | undefined; selected: boolean; queued: boolean; onToggle: () => void; onSeen: () => void }) {
  const ref = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el || cover !== undefined) return;
    if (typeof IntersectionObserver === "undefined") {
      onSeen();
      return;
    }
    const io = new IntersectionObserver((seen) => seen.some((s) => s.isIntersecting) && onSeen(), { rootMargin: "300px" });
    io.observe(el);
    return () => io.disconnect();
  }, [cover, onSeen]);
  const index = book.series?.index;
  return (
    <button ref={ref} className={`lib-card ${selected ? "on" : ""}`} aria-pressed={selected} disabled={queued} onClick={onToggle} title={`${book.title} · ${fromLabel(book)}`}>
      <div className="lib-cover">
        {cover ? <img src={cover} alt="" /> : <div className="cover-placeholder">{book.title.slice(0, 1)}</div>}
        {index != null && <span className="lib-index">{index}</span>}
        {selected && <span className="lib-tick" aria-hidden="true">✓</span>}
      </div>
      <span className="lib-title">{book.title}</span>
      <span className="lib-author">{book.authors.join(", ")}</span>
      <span className="lib-foot">{queued ? "In To send" : book.on_kindle ? "On this Kindle" : fromLabel(book)}</span>
    </button>
  );
}

/**
 * The Books tab's library (Books and Kindle design §8): Deckle's library, calibre and your
 * folders as covers, searched, grouped by series with their gaps, and filtered to what isn't on
 * the Kindle yet. Picked books go to To send.
 */
export default function Library({ catalog, hasKindle, queued, query, busy, onAdd }: Props) {
  const [search, setSearch] = useState<BookSearchReport | null>(null);
  const [searching, setSearching] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const searchVersion = useRef(0);
  useEffect(() => () => { searchVersion.current++; }, []);
  // New words drop the last Kindle search: its results answered the old ones.
  useEffect(() => {
    searchVersion.current++;
    setSearch(null);
    setSearchError(null);
    setSearching(false);
  }, [query]);
  const searchAll = async () => {
    const version = ++searchVersion.current;
    setSearching(true);
    setSearchError(null);
    setSearch(null);
    try {
      const result = await api.librarySearch(query);
      if (version === searchVersion.current) setSearch(result);
    } catch (e) {
      if (version === searchVersion.current) setSearchError(String(e));
    } finally {
      if (version === searchVersion.current) setSearching(false);
    }
  };
  const [filter, setFilter] = useState<LibraryFilter>("all");
  const [bySeries, setBySeries] = useState(true);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const { covers, want } = useCovers();
  const items: LibraryItem[] = libraryItems(catalog.entries, query, hasKindle ? filter : "all", bySeries);
  const toggle = (path: string) =>
    setPicked((p) => {
      const next = new Set(p);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  const add = () => {
    onAdd([...picked]);
    setPicked(new Set());
  };
  const card = (b: CatalogEntry) => (
    <Card key={b.path} book={b} cover={covers[b.path]} selected={picked.has(b.path)} queued={queued.has(b.path)} onToggle={() => toggle(b.path)} onSeen={() => want(b.path)} />
  );
  return (
    <section className="library" aria-label="Your library">
      <div className="library-head">
        <h2>{search ? "Found on your Kindles" : "Your library"}</h2>
        <span className="muted small">{search ? `${search.results.length} results` : `${catalog.entries.length} books`}</span>
        <div className="spacer" />
        {query.trim() && !search && (
          <button className="quiet" onClick={() => void searchAll()} disabled={busy || searching}>
            {searching ? "Searching…" : "Also search Kindles"}
          </button>
        )}
        {search && (
          <button className="quiet" onClick={() => setSearch(null)}>
            Back to library
          </button>
        )}
      </div>
      {!search && (
        <div className="shelf-tools">
          <div className="segmented" role="group" aria-label="Show">
            <button className={filter === "all" ? "on" : ""} aria-pressed={filter === "all"} onClick={() => setFilter("all")}>
              All
            </button>
            <button
              className={filter === "not-on-kindle" ? "on" : ""}
              aria-pressed={filter === "not-on-kindle"}
              disabled={!hasKindle}
              title={hasKindle ? undefined : "Connect a Kindle to see what isn't on it"}
              onClick={() => setFilter("not-on-kindle")}
            >
              Not on Kindle
            </button>
          </div>
          <button className={`toggle ${bySeries ? "on" : ""}`} aria-pressed={bySeries} onClick={() => setBySeries((s) => !s)}>
            Group by series
          </button>
        </div>
      )}
      {catalog.warnings.map((w) => (
        <p key={w} className="warn small">
          {w}
        </p>
      ))}
      {searchError && <p className="warn" role="alert">{searchError}</p>}
      {searching && <p role="status">Reading connected Kindles…</p>}
      {search ? (
        <section className="book-search-results" aria-label="Search results" aria-live="polite">
          <p className="muted small">Unplugged Kindles show previous sends, not a current inventory.</p>
          {search.warnings.map((warning) => <p className="warn" key={warning}>{warning}</p>)}
          {search.results.length === 0 && <p>No books match.</p>}
          {search.results.map((hit, i) => (
            <article key={i} className="hit">
              <h3>{hit.title}</h3>
              <p className="muted small">{hit.authors.join(", ")}{hit.series ? ` · ${hit.series.name}` : ""}</p>
              <ul>{hit.locations.map((loc, j) => (
                <li key={j}>
                  <span className={`dot ${loc.kind === "connected" ? "on" : ""}`} aria-hidden="true" />
                  <span>{loc.name} · {loc.kind === "library" ? "in your library" : loc.kind === "connected" ? "on a connected Kindle" : "sent before; may have been removed"}<br /><span className="muted small">{loc.device ? `${loc.device} · ` : ""}{loc.path}</span></span>
                </li>
              ))}</ul>
            </article>
          ))}
        </section>
      ) : items.length === 0 ? (
        <p className="muted">{catalog.entries.length === 0 ? "Nothing here yet." : "No books match."}</p>
      ) : (
        <div className="lib-items">
          {items.map((it) =>
            it.kind === "series" ? (
              <div key={`series:${it.key}`} className="lib-series">
                <h3 className="lib-series-line">{seriesLine(it)}</h3>
                <div className="lib-grid">
                  {it.books.map(card)}
                  {it.missing.map((n) => (
                    <div key={`gap-${n}`} className="lib-gap" title={`No. ${n} isn't in your library`}>
                      <span>{n}</span>
                      <span className="muted small">Not in your library</span>
                    </div>
                  ))}
                </div>
              </div>
            ) : null,
          )}
          <div className="lib-grid">{items.filter((i): i is Extract<LibraryItem, { kind: "book" }> => i.kind === "book").map((i) => card(i.book))}</div>
        </div>
      )}
      {picked.size > 0 && (
        <div className="selection-bar" role="toolbar" aria-label="Selected books">
          <span className="selection-count">{picked.size} selected</span>
          <button className="quiet" onClick={() => setPicked(new Set())}>
            Clear
          </button>
          <button className="primary" onClick={add} disabled={busy}>
            Add to To send
          </button>
        </div>
      )}
    </section>
  );
}
