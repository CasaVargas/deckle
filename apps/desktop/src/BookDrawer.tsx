// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";
import { api } from "./api";
import { kindlePage, plannedLines } from "./beforeSend";
import { changedFields, draftFrom, problems, toDetails, type Draft, type Field } from "./detailsDraft";
import Icon from "./Icon";
import { applyAll, suggestionsFor, type Suggestion } from "./suggest";
import { presetSettings, PRESETS } from "./presets";
import type { BookState, CatalogEntry, CoverChoice, CoverOffer, PlanRow, PreflightView, Settings, Suggestions } from "./types";

export type DrawerTab = "details" | "cover" | "kindle";

interface Props {
  /** The book in To send, when it's there: Cover and On the Kindle preview it. */
  book: BookState | null;
  /** Its entry in the library, when it's there: Deckle's own can have its details fixed. */
  entry: CatalogEntry | null;
  plan?: PlanRow;
  settings: Settings;
  cover: string | null;
  initialTab: DrawerTab;
  onCover: (id: string, cover: CoverChoice | null) => void;
  onAsin: (id: string, asin: string | null) => void;
  /** After Save wrote the library copy. */
  onFixed: (book: BookState | null) => Promise<void>;
  onClose: () => void;
}

/** The first page of chapter one in a sandboxed frame, set the way a Kindle sets it. */
function Page({ html, css, label }: { html: string; css: string; label: string }) {
  return <iframe className="kindle-page" title={label} sandbox="" srcDoc={kindlePage(html, css)} />;
}

/** Where the book lives, and so whether its details can be edited here. */
function home(entry: CatalogEntry | null): string {
  if (!entry) return "Not in your library yet. Send it once with the library on, then its details can be edited here.";
  if (entry.library_dir) return "In Deckle's library. Edits go to Deckle's copy; the original file stays as it is.";
  if (entry.from === "calibre") return "From calibre, which Deckle never changes. Its details can't be edited here.";
  return "From a folder Deckle reads but doesn't change. Its details can't be edited here.";
}

/**
 * Book details (Books tab redesign §5): one drawer for one book, from its queue row or the
 * library's selection bar. Details edits the library copy; Cover and On the Kindle are what
 * Before you send was (Books and Kindle design §8).
 */
export default function BookDrawer({ book, entry, plan, settings, cover, initialTab, onCover, onAsin, onFixed, onClose }: Props) {
  const path = book?.path ?? entry?.path ?? "";
  const preset = settings.book_presets[path];
  settings = presetSettings(settings, path);
  const previewable = !!book && book.mode !== "error" && book.mode !== "ready";
  const [tab, setTab] = useState<DrawerTab>(previewable || initialTab === "details" ? initialTab : "details");

  // Details: what's saved, what's typed, and the prompt when closing with changes.
  const [saved, setSaved] = useState<Draft>(() => draftFrom(entry, book));
  const [draft, setDraft] = useState<Draft>(saved);
  const [saving, setSaving] = useState(false);
  const [msg, setMsg] = useState<{ text: string; fail: boolean } | null>(null);
  const [confirming, setConfirming] = useState(false);
  // A library book's form is filled from its copy's OPF; until that's read, it waits. If the
  // read fails, only title, authors and series are offered, so nothing unread is overwritten.
  const [read, setRead] = useState<"reading" | "read" | "failed">(entry?.library_dir ? "reading" : "failed");
  useEffect(() => {
    if (!entry?.library_dir) return;
    let live = true;
    api
      .libraryReadDetails(entry.library_dir)
      .then((d) => {
        if (!live) return;
        const fresh = draftFrom(entry, book, d);
        setSaved(fresh);
        setDraft(fresh);
        setRead("read");
      })
      .catch((e) => {
        if (!live) return;
        setRead("failed");
        setMsg({ text: `Couldn't read all of this book's details: ${e}`, fail: true });
      });
    return () => {
      live = false;
    };
  }, [entry?.library_dir]);
  const changed = changedFields(saved, draft);
  const issues = problems(draft);
  const dirty = changed.length > 0;
  const editable = !!entry?.library_dir && read !== "reading";
  const full = editable && read === "read";
  const asinEditable = previewable;

  // Open Library's suggestions for empty fields (§6): looked up on opening when Fill missing
  // details is on, else only when asked. Nothing is used until the person says so.
  type Lookup = { kind: "idle" } | { kind: "looking" } | { kind: "found"; found: Suggestions } | { kind: "none" } | { kind: "failed"; error: string };
  const [lookup, setLookup] = useState<Lookup>({ kind: "idle" });
  const look = async (asked: boolean) => {
    if (!entry?.library_dir) return;
    setLookup({ kind: "looking" });
    try {
      const found = await api.librarySuggestDetails(entry.library_dir, asked);
      setLookup(found ? { kind: "found", found } : { kind: "none" });
    } catch (e) {
      setLookup({ kind: "failed", error: String(e) });
    }
  };
  useEffect(() => {
    if (read === "read" && settings.metadata_fill && lookup.kind === "idle") void look(false);
  }, [read]);
  const suggestions = lookup.kind === "found" ? suggestionsFor(draft, lookup.found.suggested) : [];
  const patch = (p: Partial<Draft>) => {
    setMsg(null);
    setDraft((d) => ({ ...d, ...p }));
  };
  const hint = (f: Field) => {
    const s = suggestions.find((x) => x.field === f);
    return s && <SuggestLine s={s} onUse={() => patch(s.patch)} />;
  };

  const save = async (): Promise<boolean> => {
    setSaving(true);
    setMsg(null);
    try {
      const library = changed.some((f) => f !== "asin");
      if (library && entry?.library_dir) {
        const all = toDetails(draft);
        await api.libraryFixDetails(entry.library_dir, full ? all : { title: all.title, authors: all.authors, series: all.series });
      }
      if (changed.includes("asin") && book) onAsin(book.id, draft.asin.trim() ? draft.asin.trim().toUpperCase() : null);
      if (library) await onFixed(book);
      setSaved(draft);
      setMsg({ text: library ? "Saved to Deckle's copy. The original file is unchanged." : "Saved.", fail: false });
      return true;
    } catch (e) {
      setMsg({ text: String(e), fail: true });
      return false;
    } finally {
      setSaving(false);
    }
  };
  const requestClose = () => (dirty ? setConfirming(true) : onClose());
  useEffect(() => {
    const onKey = (ev: KeyboardEvent) => ev.key === "Escape" && !ev.defaultPrevented && requestClose();
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  });

  // Cover and On the Kindle: the preflight of the queued book.
  const [view, setView] = useState<PreflightView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [after, setAfter] = useState(true);
  const [offers, setOffers] = useState<CoverOffer[] | null>(null);
  const [looking, setLooking] = useState(false);
  const [coverError, setCoverError] = useState<string | null>(null);
  const [generated, setGenerated] = useState<string | null>(null);
  const [own, setOwn] = useState<string | null | undefined>(undefined);

  useEffect(() => {
    if (!book || !previewable) return;
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
  }, [book?.path, book?.coverPick, preset, previewable]);

  // The typographic cover is made to be shown as soon as the Cover tab opens.
  useEffect(() => {
    if (!book || tab !== "cover" || generated) return;
    api
      .bookPreflight(book.path, { kind: "generated" })
      .then((v) => setGenerated(v.cover_url))
      .catch(() => {});
  }, [tab, generated, book?.path]);

  const findCovers = async () => {
    if (!book) return;
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
    if (!book) return;
    setCoverError(null);
    try {
      const fetched = await api.coverFetch(o.large);
      onCover(book.id, { kind: "image", path: fetched, thumb: o.thumb });
    } catch (e) {
      setCoverError(String(e));
    }
  };

  const title = saved.title || book?.title || "Untitled";
  const picked = book?.coverPick?.kind ?? "own";
  const chapter = after ? view?.after : view?.before;
  const set = <K extends Field>(k: K, v: Draft[K]) => {
    setMsg(null);
    setDraft((d) => ({ ...d, [k]: v }));
  };
  const mark = (f: Field) => changed.includes(f) && <span className="edited">● Edited</span>;

  return (
    <div className="sheet drawer" role="dialog" aria-label={`${title}: details`}>
      <div className="drawer-tabs">
        <div className="tab-strip" role="tablist" aria-label="Book">
          {(
            [
              ["details", "Details"],
              ["cover", "Cover"],
              ["kindle", "On the Kindle"],
            ] as [DrawerTab, string][]
          ).map(([k, t]) => (
            <button
              key={k}
              role="tab"
              aria-selected={tab === k}
              className={tab === k ? "on" : ""}
              disabled={k !== "details" && !previewable}
              title={k !== "details" && !previewable ? "Add it to To send to preview it" : undefined}
              onClick={() => setTab(k)}
            >
              {t}
            </button>
          ))}
        </div>
        <button className="icon" aria-label="Close" onClick={requestClose}>
          <Icon name="x" size={18} />
        </button>
      </div>

      <div className="drawer-head">
        <div className="drawer-cover" aria-hidden="true">
          {cover ? <img src={cover} alt="" /> : <div className="cover-placeholder">{title.slice(0, 1)}</div>}
        </div>
        <div className="drawer-title">
          <h2>{title}</h2>
          <div className="muted">{saved.authors.join(", ") || "Unknown author"}</div>
          {preset && <div className="muted small">{PRESETS[preset]} style · saved for this book</div>}
        </div>
      </div>

      {tab === "details" && (
        <form
          className="details-form"
          onSubmit={(e) => {
            e.preventDefault();
            if (dirty && Object.keys(issues).length === 0) void save();
          }}
        >
          <p className="drawer-home muted small">{read === "reading" ? "Reading its details…" : home(entry)}</p>
          {full && (
            <div className={`suggest-banner ${suggestions.length ? "has" : ""}`} role="status">
              {lookup.kind === "idle" && (
                <>
                  <span>Missing details? Open Library may have them.</span>
                  <button type="button" className="quiet" onClick={() => void look(true)}>
                    Look up on Open Library
                  </button>
                </>
              )}
              {lookup.kind === "looking" && <span>Looking on Open Library…</span>}
              {lookup.kind === "found" && suggestions.length > 0 && (
                <>
                  <span>
                    <strong>
                      Open Library has {suggestions.length} detail{suggestions.length > 1 ? "s" : ""} this book is missing
                    </strong>{" "}
                    {lookup.found.source === "isbn" ? "for this edition" : "for this title"}. Nothing changes until you use them.
                  </span>
                  <button type="button" className="ghost" onClick={() => patch(applyAll(draft, suggestions))}>
                    Use all {suggestions.length}
                  </button>
                </>
              )}
              {lookup.kind === "found" && suggestions.length === 0 && <span>Open Library has nothing to add.</span>}
              {lookup.kind === "none" && <span>Open Library has no record of this book.</span>}
              {lookup.kind === "failed" && (
                <>
                  <span className="warn">Couldn't reach Open Library: {lookup.error}</span>
                  <button type="button" className="quiet" onClick={() => void look(true)}>
                    Try again
                  </button>
                </>
              )}
            </div>
          )}
          <label>
            <span className="field-label">Title {mark("title")}</span>
            <input value={draft.title} onChange={(e) => set("title", e.target.value)} disabled={!editable} aria-invalid={!!issues.title} className={changed.includes("title") ? "changed" : ""} />
            {issues.title && <span className="field-problem">{issues.title}</span>}
          </label>
          <div className="field">
            <span className="field-label" id="authors-label">Authors {mark("authors")}</span>
            <TokenField labelId="authors-label" value={draft.authors} onChange={(v) => set("authors", v)} disabled={!editable} changed={changed.includes("authors")} add="Add an author" another="Add another" />
            {hint("authors")}
          </div>
          <div className="details-series">
            <label>
              <span className="field-label">Series {mark("series")}</span>
              <input value={draft.series} onChange={(e) => set("series", e.target.value)} placeholder="None" disabled={!editable} className={changed.includes("series") ? "changed" : ""} />
            </label>
            <label>
              <span className="field-label">Number {mark("index")}</span>
              <input value={draft.index} onChange={(e) => set("index", e.target.value)} inputMode="decimal" aria-invalid={!!issues.index} disabled={!editable || !draft.series.trim()} className={changed.includes("index") ? "changed" : ""} />
            </label>
          </div>
          {issues.index && <span className="field-problem">{issues.index}</span>}
          {hint("series")}
          {full && (
            <>
              <label>
                <span className="field-label">Description {mark("description")}</span>
                <textarea rows={4} value={draft.description} onChange={(e) => set("description", e.target.value)} className={changed.includes("description") ? "changed" : ""} />
                {hint("description") ?? <span className="muted small">Shown on the Kindle's page for the book.</span>}
              </label>
              <div className="details-pair">
                <label>
                  <span className="field-label">Publisher {mark("publisher")}</span>
                  <input value={draft.publisher} onChange={(e) => set("publisher", e.target.value)} className={changed.includes("publisher") ? "changed" : ""} />
                  {hint("publisher")}
                </label>
                <label>
                  <span className="field-label">Published {mark("published")}</span>
                  <input value={draft.published} onChange={(e) => set("published", e.target.value)} placeholder="2020 or 2020-09-15" aria-invalid={!!issues.published} className={changed.includes("published") ? "changed" : ""} />
                  {issues.published && <span className="field-problem">{issues.published}</span>}
                  {hint("published")}
                </label>
                <label>
                  <span className="field-label">Language {mark("language")}</span>
                  <input value={draft.language} onChange={(e) => set("language", e.target.value)} placeholder="en" spellCheck={false} aria-invalid={!!issues.language} className={changed.includes("language") ? "changed" : ""} />
                  {issues.language && <span className="field-problem">{issues.language}</span>}
                  {hint("language")}
                </label>
                <label>
                  <span className="field-label">ISBN {mark("isbn")}</span>
                  <input value={draft.isbn} onChange={(e) => set("isbn", e.target.value)} inputMode="numeric" spellCheck={false} aria-invalid={!!issues.isbn} className={changed.includes("isbn") ? "changed" : ""} />
                  {issues.isbn && <span className="field-problem">{issues.isbn}</span>}
                </label>
              </div>
              <div className="field">
                <span className="field-label" id="tags-label">Tags {mark("tags")}</span>
                <TokenField labelId="tags-label" value={draft.tags} onChange={(v) => set("tags", v)} disabled={false} changed={changed.includes("tags")} add="Add a tag" another="Add another" />
                {hint("tags")}
              </div>
            </>
          )}
          {(full || asinEditable) && (
            <details className="more" open={changed.some((f) => f === "titleSort" || f === "authorSort" || f === "asin") || !!saved.asin}>
              <summary>More</summary>
              <div className="more-fields">
                {full && (
                  <>
                    <label>
                      <span className="field-label">Sort title {mark("titleSort")}</span>
                      <input value={draft.titleSort} onChange={(e) => set("titleSort", e.target.value)} placeholder={draft.title} className={changed.includes("titleSort") ? "changed" : ""} />
                    </label>
                    <label>
                      <span className="field-label">Sort author {mark("authorSort")}</span>
                      <input value={draft.authorSort} onChange={(e) => set("authorSort", e.target.value)} placeholder="Last name, First name" className={changed.includes("authorSort") ? "changed" : ""} />
                      <span className="muted small">How the first author is filed, like "Corey, James S. A."</span>
                    </label>
                  </>
                )}
                {asinEditable && (
                  <label>
                    <span className="field-label">Amazon ASIN {mark("asin")}</span>
                    <input value={draft.asin} onChange={(e) => set("asin", e.target.value)} placeholder="B0… from the book's Amazon page" spellCheck={false} maxLength={12} aria-invalid={!!issues.asin} className={`asin ${changed.includes("asin") ? "changed" : ""}`} />
                    <span className="muted small">{issues.asin ?? "With Prefer features (EBOK), the Kindle fetches Amazon's cover for this edition. Kept with this send, not the library."}</span>
                  </label>
                )}
              </div>
            </details>
          )}
        </form>
      )}

      {tab === "cover" && book && (
        <div className="cover-pane">
          {!view && !error && <p className="muted small">Reading the book…</p>}
          {error && <div className="result fail">{error}</div>}
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

      {tab === "kindle" && book && (
        <div className="kindle-pane">
          {!view && !error && <p className="muted small">Reading the book…</p>}
          {error && <div className="result fail">{error}</div>}
          {view && (
            <>
              <ul className="plan-lines">
                {plannedLines(view, plan, book, settings, !!preset).map((l) => (
                  <li key={l.what} className={l.tone}>
                    <strong>{l.what}</strong>
                    <span>{l.text}</span>
                  </li>
                ))}
              </ul>
              <div className="page-pane">
                <div className="segmented" role="group" aria-label="Typography pass">
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
                      {!settings.typography && " Typography cleanup is off for this book."}
                    </p>
                  </>
                ) : (
                  <p className="muted">Deckle couldn't find chapter one in this book.</p>
                )}
              </div>
            </>
          )}
        </div>
      )}

      {tab === "details" && (editable || asinEditable) && (
        <div className="drawer-foot">
          {confirming ? (
            <>
              <span className="foot-note">You have changes that aren't saved.</span>
              <button className="quiet" onClick={onClose}>
                Discard
              </button>
              <button className="ghost" onClick={() => setConfirming(false)}>
                Keep editing
              </button>
              <button className="primary" disabled={saving || Object.keys(issues).length > 0} onClick={async () => (await save()) && onClose()}>
                Save
              </button>
            </>
          ) : (
            <>
              <span className={`foot-note ${msg?.fail ? "fail" : ""}`} role="status">
                {msg?.text ?? (dirty ? `${changed.length} unsaved change${changed.length > 1 ? "s" : ""}` : "Your original file stays exactly as it is.")}
              </span>
              <button className="ghost" disabled={!dirty || saving} onClick={() => { setDraft(saved); setMsg(null); }}>
                Undo changes
              </button>
              <button className="primary" disabled={!dirty || saving || Object.keys(issues).length > 0} onClick={() => void save()}>
                {saving ? "Saving…" : "Save"}
              </button>
            </>
          )}
        </div>
      )}
    </div>
  );
}

/** "Suggested: … · Use" under an empty field. */
function SuggestLine({ s, onUse }: { s: Suggestion; onUse: () => void }) {
  return (
    <span className="suggest-line">
      <span className="suggest-text" title={s.text}>
        Suggested: {s.text}
      </span>
      <button type="button" className="use" onClick={onUse}>
        Use
      </button>
    </span>
  );
}

/** Removable tokens (authors, tags); Return or a comma adds what's typed, Backspace on empty removes the last. */
function TokenField({ labelId, value, onChange, disabled, changed, add, another }: { labelId: string; value: string[]; onChange: (v: string[]) => void; disabled: boolean; changed: boolean; add: string; another: string }) {
  const [text, setText] = useState("");
  const commit = () => {
    const t = text.trim();
    if (t) onChange([...value, t]);
    setText("");
  };
  return (
    <div className={`token-field ${changed ? "changed" : ""} ${disabled ? "disabled" : ""}`} role="group" aria-labelledby={labelId}>
      {value.map((a, i) => (
        <span key={`${a}-${i}`} className="token">
          {a}
          {!disabled && (
            <button type="button" className="token-x" aria-label={`Remove ${a}`} onClick={() => onChange(value.filter((_, j) => j !== i))}>
              <Icon name="x" size={12} />
            </button>
          )}
        </span>
      ))}
      {!disabled && (
        <input
          aria-label={add}
          value={text}
          placeholder={value.length ? another : add}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === ",") {
              e.preventDefault();
              commit();
            } else if (e.key === "Backspace" && !text && value.length) onChange(value.slice(0, -1));
          }}
          onBlur={commit}
        />
      )}
    </div>
  );
}
