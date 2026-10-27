// SPDX-License-Identifier: GPL-3.0-or-later
// The Cookbook tab's Recipes room (cookbook design §8.2): "What's in the book", one list in book
// order with a heading per chapter. A row opens beneath itself, one at a time, and every change in
// it saves as it is made: menus and chips at once, typing after a pause and on leaving the field,
// one save at a time per recipe (the Daily's save rules). A failed save shows its reason on the row
// and keeps what was typed.
import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "./api";
import { debounce, saveQueue } from "./saveQueue";
import type { SaveQueue } from "./saveQueue";
import Icon from "./Icon";
import { chipPicture, edited, failedSave, groupByChapter, recipeDetail, recipeDraft, recipePatch, rowSaveLine, savedRow, wantsDrawing, webUrl } from "./cookbookRooms";
import type { RowState } from "./cookbookRooms";
import type { CookbookBook, CookbookPicture, CookbookRecipe } from "./types";

const PICTURES: { key: CookbookPicture; title: string }[] = [
  { key: "site", title: "Site photo" },
  { key: "illustration", title: "Illustration" },
  { key: "none", title: "None" },
  { key: "default", title: "Book default" },
];

export default function CookbookRecipes({
  book,
  initialOpen,
  onOpened,
  drawingAll,
  onChanged,
  onNotice,
}: {
  book: CookbookBook | null;
  /** The recipe whose row opens first (Add's "See it in Recipes"). */
  initialOpen: string | null;
  /** Called once `initialOpen` has been taken, so a later visit doesn't open it again. */
  onOpened: () => void;
  /** Illustrate all is running: the book draws one picture at a time, so Redraw waits. */
  drawingAll: boolean;
  /** The book changed: load it again. */
  onChanged: () => void;
  onNotice: (m: string) => void;
}) {
  const [open, setOpen] = useState<string | null>(initialOpen);
  const [query, setQuery] = useState("");
  const [confirm, setConfirm] = useState<string | null>(null);
  const [drawing, setDrawing] = useState<string | null>(null);
  // The open rows' state: the newest edit is in rowsRef (a save reads it), state mirrors it for display
  const rowsRef = useRef<Record<string, RowState>>({});
  const [rows, setRowsState] = useState<Record<string, RowState>>({});
  const mounted = useRef(true);

  const live = useRef({ book, onChanged, onNotice });
  live.current = { book, onChanged, onNotice };

  const setRow = useCallback((id: string, f: (r: RowState) => RowState) => {
    const cur = rowsRef.current[id];
    if (!cur) return;
    rowsRef.current = { ...rowsRef.current, [id]: f(cur) };
    setRowsState(rowsRef.current);
  }, []);

  // One save queue and one typing pause per recipe, made when first needed
  const savers = useRef(new Map<string, { queue: SaveQueue<void>; typing: ReturnType<typeof debounce> }>());
  const saver = (id: string) => {
    let s = savers.current.get(id);
    if (!s) {
      const queue = saveQueue<void>(
        async () => {
          const row = rowsRef.current[id];
          if (!row) return;
          const patch = recipePatch(row.draft, row.base);
          if (!patch) {
            setRow(id, (r) => ({ ...r, saving: false }));
            return;
          }
          const saved = await api.cookbookUpdate(id, patch);
          setRow(id, (r) => savedRow(r, recipeDraft(saved)));
          // Choosing Illustration, with illustrations on and nothing drawn yet, draws it
          const b = live.current.book;
          if (patch.picture !== undefined && b && wantsDrawing(saved.picture, b.config.picture, b.config.illustrations.enabled, saved.has_illustration)) {
            setDrawing(id);
            try {
              await api.cookbookIllustrate(id, false, false);
            } catch (e) {
              setRow(id, (r) => failedSave(r, String(e)));
            }
            setDrawing(null);
          }
        },
        (e) => {
          // Reasons show on the row; a row that is gone has nowhere to show it
          if (mounted.current && rowsRef.current[id]) setRow(id, (r) => failedSave(r, String(e)));
          else live.current.onNotice(String(e));
        },
        () => live.current.onChanged(),
      );
      s = { queue, typing: debounce(() => void push(id), 500) };
      savers.current.set(id, s);
    }
    return s;
  };
  const push = (id: string) => {
    setRow(id, (r) => ({ ...r, saving: true }));
    return saver(id).queue.push();
  };

  /** An edit: menus and chips save at once, typing once it pauses (or the field is left). */
  const change = (id: string, c: Partial<RowState["draft"]>, how: "now" | "typing") => {
    setRow(id, (r) => edited(r, c));
    const s = saver(id);
    if (how === "typing") s.typing.call();
    else {
      s.typing.cancel();
      void push(id);
    }
  };

  useEffect(() => {
    mounted.current = true;
    const all = savers.current;
    return () => {
      mounted.current = false;
      all.forEach((s) => s.typing.flush());
    };
  }, []);

  const startRow = (r: CookbookRecipe) => {
    const cur = rowsRef.current[r.id];
    // A row with unsaved typing (a failed save) keeps it; any other starts from the book
    if (!cur || (!cur.saving && recipePatch(cur.draft, cur.base) === null)) {
      rowsRef.current = { ...rowsRef.current, [r.id]: { draft: recipeDraft(r), base: recipeDraft(r), error: null, saving: false } };
      setRowsState(rowsRef.current);
    }
  };

  const toggle = (r: CookbookRecipe) => {
    if (open) savers.current.get(open)?.typing.flush();
    setConfirm(null);
    if (open === r.id) {
      setOpen(null);
      return;
    }
    startRow(r);
    setOpen(r.id);
  };

  // Landing from Add: the row opens and scrolls into view, once the book is there to find it in
  const first = useRef(initialOpen);
  useEffect(() => {
    const id = first.current;
    if (!id || !book) return;
    const r = book.chapters.flatMap((c) => c.recipes).find((x) => x.id === id);
    if (!r) return;
    first.current = null;
    startRow(r);
    document.getElementById(`recipe-${id}`)?.scrollIntoView({ block: "nearest" });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [book]);
  // A later visit starts with nothing open
  useEffect(() => onOpened, [onOpened]);

  const remove = async (r: CookbookRecipe) => {
    const s = savers.current.get(r.id);
    s?.typing.cancel();
    try {
      await api.cookbookRemove(r.id);
    } catch (e) {
      onNotice(String(e));
      return;
    }
    const rest = { ...rowsRef.current };
    delete rest[r.id];
    rowsRef.current = rest;
    setRowsState(rest);
    savers.current.delete(r.id);
    setConfirm(null);
    setOpen(null);
    onChanged();
  };

  const redraw = async (r: CookbookRecipe) => {
    setDrawing(r.id);
    try {
      await api.cookbookIllustrate(r.id, false, true);
      setRow(r.id, (x) => ({ ...x, error: null }));
    } catch (e) {
      setRow(r.id, (x) => failedSave(x, String(e)));
    }
    setDrawing(null);
    onChanged();
  };

  if (!book) return <p className="muted">Loading the book…</p>;
  const groups = groupByChapter(book.chapters, query);
  const illustrationsOn = book.config.illustrations.enabled;
  const chapters = book.config.chapters;

  const body = (r: CookbookRecipe, row: RowState) => {
    const chosen = chipPicture(row.draft.picture, book.config.picture);
    const source = webUrl(r.url);
    const line = rowSaveLine(row);
    return (
      <div className="sec-body recipe-body">
        <label className="sec-field">
          <span className="label">Title</span>
          <input
            type="text"
            value={row.draft.title}
            placeholder={r.site_title}
            onChange={(e) => change(r.id, { title: e.target.value }, "typing")}
            onBlur={() => savers.current.get(r.id)?.typing.flush()}
          />
        </label>
        <label className="sec-field">
          <span className="label">Chapter</span>
          <select value={row.draft.chapter} onChange={(e) => change(r.id, { chapter: e.target.value }, "now")}>
            {!chapters.some((c) => c.id === row.draft.chapter) && <option value={row.draft.chapter}>{r.home_name} (its chapter was removed)</option>}
            {chapters.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
        </label>
        <div className="sec-field">
          <span className="label">Picture</span>
          <div className="recipe-pictures">
            <div className="seg small" role="group" aria-label="Picture">
              {PICTURES.map((c) => {
                const on = c.key === "default" ? row.draft.picture === "default" : row.draft.picture === c.key;
                return (
                  <button
                    key={c.key}
                    className={on ? "on" : ""}
                    aria-pressed={on}
                    disabled={c.key === "illustration" && !illustrationsOn && row.draft.picture !== "illustration"}
                    title={c.key === "illustration" && !illustrationsOn ? "Turn on illustrations in Book settings first" : undefined}
                    onClick={() => change(r.id, { picture: c.key }, "now")}
                  >
                    {c.title}
                  </button>
                );
              })}
            </div>
            {illustrationsOn && chosen === "illustration" && (
              <button className="ghost small" disabled={drawingAll || drawing !== null} onClick={() => redraw(r)}>
                {drawing === r.id ? "Drawing…" : r.has_illustration ? "Redraw" : "Draw it"}
              </button>
            )}
          </div>
          {row.draft.picture === "default" && <span className="muted small">Follows the book: {PICTURE_NAME[book.config.picture]}.</span>}
        </div>
        <label className="sec-field">
          <span className="label">Note</span>
          <input
            type="text"
            value={row.draft.note}
            placeholder="Printed under the title"
            onChange={(e) => change(r.id, { note: e.target.value }, "typing")}
            onBlur={() => savers.current.get(r.id)?.typing.flush()}
          />
        </label>
        <div className="sec-field">
          <span className="label">Source</span>
          <span className="recipe-source">
            <span className="mono">{r.url}</span>
            {source && (
              <button className="link" onClick={() => api.openLink(source).catch((e) => onNotice(String(e)))}>
                Open
              </button>
            )}
          </span>
        </div>
        <div className="recipe-actions">
          {confirm === r.id ? (
            <>
              <span className="small">Take it out of the book?</span>
              <button className="ghost danger" onClick={() => remove(r)}>
                Remove it
              </button>
              <button className="ghost" onClick={() => setConfirm(null)}>
                Keep it
              </button>
            </>
          ) : (
            <button className="ghost danger" disabled={row.saving} onClick={() => setConfirm(r.id)}>
              Remove
            </button>
          )}
          {line && (
            <span className={`small ${row.error ? "fail" : "muted"}`} role={row.error ? "alert" : "status"}>
              {line}
            </span>
          )}
        </div>
      </div>
    );
  };

  return (
    <div className="sections-tab recipes-tab">
      <div>
        <h2 className="daily-h2">What's in the book</h2>
        <p className="muted small">Chapters print in this order, recipes alphabetically. Open one to rename it, move it or change its picture; it saves as you go.</p>
      </div>
      {book.recipes > 0 && (
        <input type="search" className="recipe-search" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search titles and ingredients" aria-label="Search recipes" spellCheck={false} />
      )}
      {book.recipes === 0 ? (
        <p className="muted">Nothing here yet. Add a recipe on the Add tab.</p>
      ) : groups.length === 0 ? (
        <p className="muted">No recipe has "{query.trim()}" in its title or ingredients.</p>
      ) : (
        <ul className="sec-list recipe-list">
          {groups.map((g) => (
            <li key={g.id} className="recipe-group">
              <div className="recipe-chapter">
                <strong>{g.name}</strong>
                <span className="muted small">{g.empty ?? g.count}</span>
              </div>
              {g.recipes.length > 0 && (
                <ul>
                  {g.recipes.map((r) => {
                    const isOpen = open === r.id;
                    const row = rows[r.id];
                    return (
                      <li key={r.id} id={`recipe-${r.id}`} className={`sec-row recipe-row ${isOpen ? "open" : ""}`}>
                        <div className="recipe-head">
                          {r.thumb ? <img className="recipe-thumb" src={r.thumb} alt="" /> : <span className="recipe-thumb none" aria-hidden="true" />}
                          <button className="sec-open" aria-expanded={isOpen} onClick={() => toggle(r)}>
                            <strong>
                              {r.is_new && (
                                <span className="recipe-new" title="Added in the last week">
                                  ★{" "}
                                </span>
                              )}
                              {r.title}
                            </strong>
                            <span className="muted small">{recipeDetail(r)}</span>
                          </button>
                          <button className="icon small" aria-label={isOpen ? `Close ${r.title}` : `Open ${r.title}`} onClick={() => toggle(r)}>
                            <Icon name={isOpen ? "chevron-down" : "chevron-right"} size={14} />
                          </button>
                        </div>
                        {isOpen && row && body(r, row)}
                      </li>
                    );
                  })}
                </ul>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

const PICTURE_NAME: Record<Exclude<CookbookPicture, "default">, string> = { site: "site photo", illustration: "illustration", none: "no picture" };
