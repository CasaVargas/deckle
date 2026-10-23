// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";
import { api } from "./api";
import { kindlePage, plannedLines } from "./beforeSend";
import Icon from "./Icon";
import type { BookState, CatalogEntry, CoverChoice, CoverOffer, PlanRow, PreflightView, Settings } from "./types";

interface Props {
  book: BookState;
  plan?: PlanRow;
  settings: Settings;
  /** Its entry in the library, when it's there: Deckle's own can have its details fixed. */
  entry: CatalogEntry | null;
  onCover: (id: string, cover: CoverChoice | null) => void;
  /** After Fix details wrote the library copy. */
  onFixed: (book: BookState) => Promise<void>;
  onClose: () => void;
}

type Pane = "plan" | "page" | "cover" | "details";

/** The first page of chapter one in a sandboxed frame, set the way a Kindle sets it. */
function Page({ html, css, label }: { html: string; css: string; label: string }) {
  return <iframe className="kindle-page" title={label} sandbox="" srcDoc={kindlePage(html, css)} />;
}

/**
 * Before you send (Books and Kindle design §8): what Deckle will do to a queued book, the first
 * page of chapter one before and after, its cover, and its details.
 */
export default function BeforeYouSend({ book, plan, settings, entry, onCover, onFixed, onClose }: Props) {
  const [pane, setPane] = useState<Pane>("plan");
  const [view, setView] = useState<PreflightView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [after, setAfter] = useState(true);
  const [offers, setOffers] = useState<CoverOffer[] | null>(null);
  const [looking, setLooking] = useState(false);
  const [coverError, setCoverError] = useState<string | null>(null);
  const [generated, setGenerated] = useState<string | null>(null);
  const [own, setOwn] = useState<string | null | undefined>(undefined);

  useEffect(() => {
    let live = true;
    setView(null);
    setError(null);
    api
      .bookPreflight(book.path, book.coverPick)
      .then((v) => {
        if (!live) return;
        setView(v);
        if (!book.coverPick || book.coverPick.kind === "own") setOwn(v.cover === "own" ? v.cover_url : null);
        if (book.coverPick?.kind === "generated") setGenerated(v.cover_url);
      })
      .catch((e) => live && setError(String(e)));
    return () => {
      live = false;
    };
  }, [book.path, book.coverPick]);

  // The typographic cover is made to be shown as soon as the Cover pane opens.
  useEffect(() => {
    if (pane !== "cover" || generated) return;
    api
      .bookPreflight(book.path, { kind: "generated" })
      .then((v) => setGenerated(v.cover_url))
      .catch(() => {});
  }, [pane, generated, book.path]);

  const findCovers = async () => {
    setLooking(true);
    setCoverError(null);
    try {
      setOffers(await api.coverCandidates(book.title, book.authors.split(",")[0]?.trim() ?? "", null));
    } catch (e) {
      setCoverError(String(e));
    } finally {
      setLooking(false);
    }
  };
  const pickOffer = async (o: CoverOffer) => {
    setCoverError(null);
    try {
      const path = await api.coverFetch(o.large);
      onCover(book.id, { kind: "image", path, thumb: o.thumb });
    } catch (e) {
      setCoverError(String(e));
    }
  };

  const picked = book.coverPick?.kind ?? "own";
  const chapter = after ? view?.after : view?.before;
  return (
    <div className="sheet" role="dialog" aria-label={`Before you send ${book.title}`}>
      <div className="sheet-head">
        <div>
          <h2>{book.title}</h2>
          <div className="muted small">{book.authors}</div>
        </div>
        <button className="icon" aria-label="Close" onClick={onClose}>
          <Icon name="x" size={18} />
        </button>
      </div>
      <div className="seg" role="tablist" aria-label="Before you send">
        {(
          [
            ["plan", "What Deckle will do"],
            ["page", "Page preview"],
            ["cover", "Cover"],
            ["details", "Details"],
          ] as [Pane, string][]
        ).map(([k, t]) => (
          <button key={k} role="tab" aria-selected={pane === k} className={pane === k ? "on" : ""} onClick={() => setPane(k)}>
            {t}
          </button>
        ))}
      </div>
      {error && <div className="result fail">{error}</div>}
      {!view && !error && <p className="muted small">Reading the book…</p>}

      {view && pane === "plan" && (
        <ul className="plan-lines">
          {plannedLines(view, plan, book, settings).map((l) => (
            <li key={l.what} className={l.tone}>
              <strong>{l.what}</strong>
              <span>{l.text}</span>
            </li>
          ))}
        </ul>
      )}

      {view && pane === "page" && (
        <div className="page-pane">
          <div className="seg small" role="group" aria-label="Typography pass">
            <button className={!after ? "on" : ""} aria-pressed={!after} onClick={() => setAfter(false)}>
              Before
            </button>
            <button className={after ? "on" : ""} aria-pressed={after} onClick={() => setAfter(true)}>
              After
            </button>
          </div>
          {chapter ? (
            <>
              <Page html={chapter.html} css={chapter.css} label={after ? "Chapter one, as Deckle sends it" : "Chapter one, as the file has it"} />
              <p className="muted small">
                {chapter.title ?? chapter.href}, the first page as a Kindle sets it.
                {!settings.typography && " Typography fixes are off (Settings → Books), so Before and After match."}
              </p>
            </>
          ) : (
            <p className="muted">Deckle couldn't find chapter one in this book.</p>
          )}
        </div>
      )}

      {view && pane === "cover" && (
        <div className="cover-pane">
          <div className="cover-choices" role="radiogroup" aria-label="Cover">
            <button role="radio" aria-checked={picked === "own"} className={`cover-choice ${picked === "own" ? "on" : ""}`} disabled={own === null && picked !== "own"} onClick={() => onCover(book.id, null)}>
              {own ? <img src={own} alt="" /> : <div className="cover-placeholder">—</div>}
              <span>{own === null ? "It has none" : "Its own"}</span>
            </button>
            <button role="radio" aria-checked={picked === "generated"} className={`cover-choice ${picked === "generated" ? "on" : ""}`} onClick={() => onCover(book.id, { kind: "generated" })}>
              {generated ? <img src={generated} alt="" /> : <div className="cover-placeholder">…</div>}
              <span>Deckle's typographic cover</span>
            </button>
            {book.coverPick?.kind === "image" && (
              <button role="radio" aria-checked="true" className="cover-choice on">
                <img src={book.coverPick.thumb} alt="" />
                <span>From Open Library</span>
              </button>
            )}
          </div>
          <div className="button-row">
            <button className="ghost" onClick={findCovers} disabled={looking}>
              {looking ? "Looking…" : "Find covers on Open Library"}
            </button>
          </div>
          {coverError && <p className="warn small">{coverError}</p>}
          {offers && offers.length === 0 && <p className="muted small">Open Library has no covers for this book.</p>}
          {offers && offers.length > 0 && (
            <div className="cover-offers">
              {offers.map((o) => (
                <button key={o.large} className="cover-choice" onClick={() => pickOffer(o)} title="Use this cover">
                  <img src={o.thumb} alt="" />
                </button>
              ))}
            </div>
          )}
        </div>
      )}

      {pane === "details" && <Details book={book} entry={entry} onFixed={onFixed} />}
    </div>
  );
}

function Details({ book, entry, onFixed }: { book: BookState; entry: CatalogEntry | null; onFixed: (book: BookState) => Promise<void> }) {
  const [title, setTitle] = useState(entry?.title ?? book.title);
  const [authors, setAuthors] = useState((entry?.authors ?? book.authors.split(",")).map((a) => a.trim()).join(", "));
  const [series, setSeries] = useState(entry?.series?.name ?? "");
  const [index, setIndex] = useState(entry?.series?.index != null ? String(entry.series.index) : "");
  const [saving, setSaving] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  if (!entry?.library_dir) {
    return (
      <p className="muted">
        Details can be fixed for books in Deckle's library, where Deckle keeps its own copy. {entry ? `This one is in ${entry.from === "calibre" ? "calibre, which Deckle never changes" : "a folder"}.` : "This one isn't in the library yet."} Send it once
        with the library on and it's added.
      </p>
    );
  }
  const n = Number(index);
  const indexOk = index.trim() === "" || (Number.isFinite(n) && n >= 0);
  const save = async () => {
    setSaving(true);
    setMsg(null);
    try {
      await api.libraryFixDetails(entry.library_dir!, {
        title: title.trim(),
        authors: authors
          .split(",")
          .map((a) => a.trim())
          .filter(Boolean),
        series: series.trim() ? { name: series.trim(), index: index.trim() ? n : null } : null,
      });
      await onFixed(book);
      setMsg("Saved to the library's copy. The original file is unchanged.");
    } catch (e) {
      setMsg(String(e));
    } finally {
      setSaving(false);
    }
  };
  return (
    <form
      className="details-form"
      onSubmit={(e) => {
        e.preventDefault();
        save();
      }}
    >
      <label>
        Title
        <input value={title} onChange={(e) => setTitle(e.target.value)} required />
      </label>
      <label>
        Authors <span className="muted small">separated by commas</span>
        <input value={authors} onChange={(e) => setAuthors(e.target.value)} />
      </label>
      <div className="details-series">
        <label>
          Series
          <input value={series} onChange={(e) => setSeries(e.target.value)} placeholder="None" />
        </label>
        <label>
          Number
          <input value={index} onChange={(e) => setIndex(e.target.value)} inputMode="decimal" aria-invalid={!indexOk} disabled={!series.trim()} />
        </label>
      </div>
      <div className="button-row">
        <button className="primary" type="submit" disabled={saving || !title.trim() || !indexOk}>
          {saving ? "Saving…" : "Save details"}
        </button>
      </div>
      <p className="muted small">{msg ?? "Written to the library's copy of the book, never to the original or to calibre."}</p>
    </form>
  );
}
