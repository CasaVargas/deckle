// SPDX-License-Identifier: GPL-3.0-or-later
// The Cookbook tab's Add room (cookbook design §8.2): a link box and Fetch that show a recipe as a
// preview card (nothing is written until Add to the book), beside the book card with its cover,
// its delivery line, and Send now and Preview.
import { useEffect, useRef, useState } from "react";
import type { MutableRefObject } from "react";
import { api } from "./api";
import { parseAddresses } from "./articles";
import { addResultLine, bookLine, chipPicture, deliveryLine, factsLine, lastAddedLine, sourceLine, wantsDrawing, webUrl } from "./cookbookRooms";
import type { CookbookBook, CookbookDelivery, CookbookPicture, CookbookSaved, RecipePreview } from "./types";

/** Addresses and files handed to the tab from outside it (a paste or a drop). `seq` tells new from seen. */
export interface Incoming {
  seq: number;
  targets: string[];
}

const PICTURES: { key: Exclude<CookbookPicture, "default">; title: string }[] = [
  { key: "site", title: "Site photo" },
  { key: "illustration", title: "Illustration" },
  { key: "none", title: "None" },
];

export default function CookbookAdd({
  book,
  delivery,
  focusAdd,
  incoming,
  taken,
  onChanged,
  onNotice,
  onRecipe,
  onDelivery,
}: {
  book: CookbookBook | null;
  delivery: CookbookDelivery | null;
  /** Bumped to focus the link box (the Books tab's Recipe… entry). */
  focusAdd: number;
  incoming: Incoming | null;
  /** The last `incoming.seq` handled, kept by the tab so a room switch doesn't replay it. */
  taken: MutableRefObject<number>;
  /** The book changed (a recipe was filed, a build ran): load it again. */
  onChanged: () => void;
  onNotice: (m: string) => void;
  /** Open this recipe's row on Recipes. */
  onRecipe: (id: string) => void;
  onDelivery: () => void;
}) {
  const [text, setText] = useState("");
  const [fetching, setFetching] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [preview, setPreview] = useState<{ target: string; p: RecipePreview } | null>(null);
  const [pick, setPick] = useState<CookbookPicture>("default");
  const [chapter, setChapter] = useState<string | null>(null);
  const [note, setNote] = useState("");
  const [adding, setAdding] = useState(false);
  const [saved, setSaved] = useState<CookbookSaved | null>(null);
  const [drawing, setDrawing] = useState(false);
  /** More addresses waiting behind the one on the card (a paste or drop of several). */
  const [queue, setQueue] = useState<string[]>([]);
  const input = useRef<HTMLInputElement | null>(null);
  const asked = useRef(0);

  const bookPicture = book?.config.picture ?? "site";
  const illustrationsOn = !!book?.config.illustrations.enabled;

  const fetchTarget = async (target: string, keepSaved = false) => {
    const t = target.trim();
    if (!t) return;
    const mine = ++asked.current;
    setFetching(true);
    setProblem(null);
    setPreview(null);
    if (!keepSaved) setSaved(null);
    try {
      const p = await api.cookbookPreview(t);
      if (mine !== asked.current) return;
      setPreview({ target: t, p });
      setPick("default");
      setChapter(null);
      setNote("");
    } catch (e) {
      if (mine === asked.current) setProblem(String(e));
    } finally {
      if (mine === asked.current) setFetching(false);
    }
  };
  const fetchRef = useRef(fetchTarget);
  fetchRef.current = fetchTarget;

  // The next waiting address, once the card in front of it is done
  const advance = (rest = queue) => {
    setPreview(null);
    if (rest.length === 0) return;
    setQueue(rest.slice(1));
    setText(rest[0]);
    fetchRef.current(rest[0], true);
  };

  // The Books tab's Recipe… entry: the link box takes focus
  useEffect(() => {
    if (focusAdd > 0) {
      input.current?.focus();
      input.current?.select();
    }
  }, [focusAdd]);

  // A paste or drop: the first address goes on the card now, the rest wait their turn
  useEffect(() => {
    if (!incoming || incoming.seq === taken.current) return;
    taken.current = incoming.seq;
    const [first, ...rest] = incoming.targets;
    if (!first) return;
    setQueue(rest);
    setText(first);
    fetchRef.current(first);
  }, [incoming, taken]);

  const add = async () => {
    if (!preview || adding) return;
    setAdding(true);
    try {
      const s = await api.cookbookAdd(preview.target, chapter, note.trim() || null, pick === "default" ? null : pick);
      setSaved(s);
      setText("");
      onChanged();
      advance();
      if (wantsDrawing(s.picture, bookPicture, illustrationsOn, s.has_illustration)) {
        setDrawing(true);
        try {
          await api.cookbookIllustrate(s.id, false, false);
        } catch (e) {
          onNotice(String(e));
        }
        setDrawing(false);
        onChanged();
      }
    } catch (e) {
      onNotice(String(e));
    } finally {
      setAdding(false);
    }
  };

  const p = preview?.p;
  const chosen = chipPicture(pick, bookPicture);
  const chapters = book?.config.chapters ?? [];
  const chapterId = chapter ?? p?.chapter ?? "";
  const source = p ? webUrl(p.source_url) : null;

  return (
    <div className="daily-columns">
      <div className="daily-side">
        <section className="card" aria-label="Add a recipe">
          <h3>Add a recipe</h3>
          <form
            className="article-row"
            onSubmit={(e) => {
              e.preventDefault();
              setQueue([]);
              const found = parseAddresses(text);
              fetchTarget(found[0] ?? text);
            }}
          >
            <input
              id="cookbook-link"
              ref={input}
              type="text"
              value={text}
              onChange={(e) => {
                setText(e.target.value);
                setProblem(null);
              }}
              onPaste={(e) => {
                const found = parseAddresses(e.clipboardData.getData("text"));
                if (found.length === 0) return;
                e.preventDefault();
                setText(found[0]);
                setQueue(found.slice(1));
                fetchTarget(found[0]);
              }}
              placeholder="Paste a recipe's web address"
              aria-label="Recipe web address"
              spellCheck={false}
            />
            <button type="submit" className="ghost" disabled={fetching || !text.trim()}>
              {fetching ? "Fetching…" : "Fetch"}
            </button>
          </form>
          <p className="muted small">Paste anywhere on this tab, or drop .webloc, .url and .html files.</p>
          {problem && <p className="fail small">{problem}</p>}
          {saved && (
            <p className="run-result ok" role="status">
              {addResultLine(saved.stored, saved.home_name)}: {saved.title}.{" "}
              <button className="link" onClick={() => onRecipe(saved.id)}>
                See it in Recipes
              </button>
              {saved.picture_note && <span className="muted small"> {saved.picture_note}</span>}
              {drawing && <span className="muted small"> Drawing the illustration…</span>}
            </p>
          )}
        </section>

        {p && (
          <section className="card cookbook-preview" aria-label="Recipe preview">
            <div className="cookbook-preview-body">
              {p.picture ? (
                <img className={`cookbook-photo ${chosen === "none" ? "off" : ""}`} src={p.picture} alt="" />
              ) : (
                <div className="cookbook-photo none muted small">No photo</div>
              )}
              <div className="cookbook-facts">
                <h3>{p.title}</h3>
                <p className="muted small">{p.site_name}</p>
                <p className="small">{factsLine(p)}</p>
                <p className="muted small">{sourceLine(p)}</p>
                {source && (
                  <button className="link" onClick={() => api.openLink(source).catch((e) => onNotice(String(e)))}>
                    Open the page
                  </button>
                )}
              </div>
            </div>

            <span className="label">Picture</span>
            <div className="seg small" role="group" aria-label="Picture">
              {PICTURES.map((c) => (
                <button
                  key={c.key}
                  className={chosen === c.key ? "on" : ""}
                  aria-pressed={chosen === c.key}
                  disabled={c.key === "illustration" && !illustrationsOn && chosen !== "illustration"}
                  title={c.key === "illustration" && !illustrationsOn ? "Turn on illustrations in Book settings first" : undefined}
                  onClick={() => setPick(c.key)}
                >
                  {c.title}
                </button>
              ))}
            </div>
            {wantsDrawing(pick, bookPicture, illustrationsOn, false) && <p className="muted small">It is drawn after you add it.</p>}

            <label>
              <span className="label">Chapter</span>
              <select value={chapterId} onChange={(e) => setChapter(e.target.value)}>
                {!chapters.some((c) => c.id === chapterId) && <option value={chapterId}>{p.chapter_name}</option>}
                {chapters.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span className="label">Note</span>
              <input type="text" value={note} onChange={(e) => setNote(e.target.value)} placeholder="Printed under the title" />
            </label>

            <div className="paper-actions cookbook-add-actions">
              <button className="primary" disabled={adding} onClick={add}>
                {adding ? "Adding…" : "Add to the book"}
              </button>
              <button className="ghost" disabled={adding} onClick={() => advance()}>
                Not this one
              </button>
              {p.exists && <span className="muted small">Already in the book: adding updates it.</span>}
              {queue.length > 0 && <span className="muted small">{queue.length} more waiting</span>}
            </div>
          </section>
        )}
      </div>

      <div className="daily-side">
        <BookCard book={book} delivery={delivery} onChanged={onChanged} onNotice={onNotice} onDelivery={onDelivery} />
      </div>
    </div>
  );
}

/** The book as it stands: its cover, its counts, where it goes, and Send now and Preview. */
function BookCard({ book, delivery, onChanged, onNotice, onDelivery }: { book: CookbookBook | null; delivery: CookbookDelivery | null; onChanged: () => void; onNotice: (m: string) => void; onDelivery: () => void }) {
  const [running, setRunning] = useState<null | "send" | "preview">(null);
  const [result, setResult] = useState<{ text: string; file?: string } | null>(null);

  const run = async (what: "send" | "preview") => {
    if (running) return;
    setRunning(what);
    setResult(null);
    try {
      if (what === "send") setResult({ text: (await api.cookbookSendNow()).message });
      else setResult({ text: "Preview built.", file: await api.cookbookPreviewBook() });
    } catch (e) {
      onNotice(String(e));
    } finally {
      setRunning(null);
      onChanged();
    }
  };

  if (!book) return <section className="card" aria-label="The book" />;
  const last = lastAddedLine(book.newest);
  return (
    <section className="card cookbook-book" aria-label="The book">
      <div className="cookbook-book-head">
        {book.cover ? <img className="cookbook-cover" src={book.cover} alt="" /> : <div className="cookbook-cover" aria-hidden="true" />}
        <div>
          <h3>{book.config.title}</h3>
          <p>{bookLine(book.recipes, book.printed_chapters)}</p>
          {last && <p className="muted small">{last}</p>}
          <p className="muted small">
            {delivery?.error ? "Where it goes can't be told right now" : deliveryLine(delivery?.kindles ?? [])}
            {" · "}
            <button className="link" onClick={onDelivery}>
              Delivery
            </button>
          </p>
        </div>
      </div>
      {book.build_error && <p className="warn small">{book.build_error}</p>}
      <div className="paper-actions">
        <button className="primary" disabled={!!running || book.recipes === 0} onClick={() => run("send")}>
          {running === "send" ? "Building and sending…" : "Send now"}
        </button>
        <button className="ghost" disabled={!!running || book.recipes === 0} onClick={() => run("preview")}>
          {running === "preview" ? "Building preview…" : "Preview"}
        </button>
      </div>
      {result && (
        <div className="run-result ok" role="status">
          {result.text}
          {result.file && (
            <>
              {" "}
              <button className="link" onClick={() => api.reveal(result.file ?? "")}>
                Show file
              </button>
            </>
          )}
        </div>
      )}
    </section>
  );
}
