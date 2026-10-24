// SPDX-License-Identifier: GPL-3.0-or-later
import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "./api";
import { fromLabel, libraryItems, seriesLine } from "./libraryView";
import type { LibraryFilter, LibraryItem } from "./libraryView";
import type { CatalogEntry, CatalogView } from "./types";

interface Props {
  catalog: CatalogView;
  /** A Kindle is picked in the footer: "Not on this Kindle" means something. */
  hasKindle: boolean;
  /** Paths already in To send. */
  queued: Set<string>;
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
export default function Library({ catalog, hasKindle, queued, busy, onAdd }: Props) {
  const [query, setQuery] = useState("");
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
        <h2>Your library</h2>
        <span className="muted small">{catalog.entries.length} books</span>
        <div className="spacer" />
        {picked.size > 0 && (
          <button className="primary" onClick={add} disabled={busy}>
            Add {picked.size} to To send
          </button>
        )}
      </div>
      <div className="shelf-tools">
        <div className="filter-chips" role="group" aria-label="Show">
          <button className={`filter-chip ${filter === "all" ? "on" : ""}`} aria-pressed={filter === "all"} onClick={() => setFilter("all")}>
            All
          </button>
          <button
            className={`filter-chip ${filter === "not-on-kindle" ? "on" : ""}`}
            aria-pressed={filter === "not-on-kindle"}
            disabled={!hasKindle}
            title={hasKindle ? undefined : "Connect a Kindle to see what isn't on it"}
            onClick={() => setFilter("not-on-kindle")}
          >
            Not on this Kindle
          </button>
          <button className={`filter-chip ${bySeries ? "on" : ""}`} aria-pressed={bySeries} onClick={() => setBySeries((s) => !s)}>
            Series
          </button>
        </div>
        <div className="shelf-search">
          <input type="search" placeholder="Search titles, authors, series" value={query} onChange={(e) => setQuery(e.target.value)} aria-label="Search your library" />
        </div>
      </div>
      {catalog.warnings.map((w) => (
        <p key={w} className="warn small">
          {w}
        </p>
      ))}
      {items.length === 0 ? (
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
    </section>
  );
}
