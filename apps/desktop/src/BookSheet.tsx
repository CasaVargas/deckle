// SPDX-License-Identifier: GPL-3.0-or-later
// Book settings (cookbook design §8.2): a sheet from the right, like the Daily's paper settings
// (Esc or × closes it). The book's title, subtitle and default picture, its chapters, illustrations,
// and the cookbook.toml it is saved to. Everything saves as it changes, one save at a time.
import { useEffect, useRef, useState } from "react";
import { api } from "./api";
import { OTHER_CHAPTER, addChapter, canRemoveChapter, illustrateProgress, illustrationSends, moveChapter, removeChapterWarning, reorderChapters } from "./cookbookRooms";
import { debounce, saveQueue } from "./saveQueue";
import Icon from "./Icon";
import type { CookbookBook, CookbookConfig, CookbookIllustrateEvent } from "./types";

/** Illustrate all while it runs, as the tab keeps it (so closing the sheet doesn't lose it). */
export interface IllustrateRun {
  event: CookbookIllustrateEvent | null;
  /** Stop was pressed; the run ends after the recipe being drawn. */
  stopping: boolean;
}

const PICTURES: { key: CookbookConfig["picture"]; title: string }[] = [
  { key: "site", title: "Site photo" },
  { key: "illustration", title: "Illustration" },
  { key: "none", title: "None" },
];

export default function BookSheet({
  book,
  apiKey,
  onApiKey,
  run,
  runResult,
  onIllustrateAll,
  onStop,
  onChanged,
  onClose,
}: {
  book: CookbookBook;
  /** The desktop's `llm_api_key` setting, which Illustrate all draws with. */
  apiKey: string;
  onApiKey: (key: string) => Promise<void>;
  run: IllustrateRun | null;
  /** How the last Illustrate all ended (or why it couldn't start). */
  runResult: string | null;
  onIllustrateAll: () => void;
  onStop: () => void;
  /** The book changed: load it again. */
  onChanged: () => void;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState<CookbookConfig>(() => structuredClone(book.config));
  const draftRef = useRef(draft);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [key, setKey] = useState(apiKey);
  const [confirm, setConfirm] = useState<string | null>(null);
  const [dragging, setDragging] = useState<number | null>(null);
  const [over, setOver] = useState<number | null>(null);
  const [fresh, setFresh] = useState<string | null>(null);
  const sheet = useRef<HTMLElement | null>(null);
  // The last non-blank name of each chapter: a blank one is never saved
  const lastName = useRef<Record<string, string>>(Object.fromEntries(book.config.chapters.map((c) => [c.id, c.name])));

  const live = useRef({ onChanged, onApiKey });
  live.current = { onChanged, onApiKey };

  const queue = useRef(
    saveQueue<CookbookConfig>(
      async (c) => {
        await api.cookbookSaveConfig(c);
        setError(null);
      },
      (e) => setError(String(e)),
      () => {
        setSaving(false);
        live.current.onChanged();
      },
    ),
  ).current;

  const push = () => {
    const c = structuredClone(draftRef.current);
    c.chapters = c.chapters.map((ch) => {
      if (ch.name.trim()) lastName.current[ch.id] = ch.name.trim();
      return { ...ch, name: ch.name.trim() || lastName.current[ch.id] || "Chapter" };
    });
    setSaving(true);
    return queue.push(c);
  };
  const typing = useRef(debounce(() => void push(), 500)).current;
  useEffect(() => () => typing.flush(), [typing]);

  /** An edit: switches and menus save at once, typing once it pauses (or the field is left). */
  const change = (f: (c: CookbookConfig) => CookbookConfig, how: "now" | "typing" = "now") => {
    const c = f(structuredClone(draftRef.current));
    draftRef.current = c;
    setDraft(c);
    if (how === "typing") typing.call();
    else {
      typing.cancel();
      void push();
    }
  };

  // The key is the desktop's own setting: it goes to Settings after a pause, not on every letter
  const keyRef = useRef(apiKey);
  const keyDone = useRef<Promise<void>>(Promise.resolve());
  const keyPush = useRef(
    debounce(() => {
      keyDone.current = live.current.onApiKey(keyRef.current);
    }, 500),
  );
  useEffect(() => {
    const p = keyPush.current;
    return () => p.flush();
  }, []);

  useEffect(() => {
    const esc = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", esc);
    sheet.current?.focus();
    return () => window.removeEventListener("keydown", esc);
  }, [onClose]);

  const counts = Object.fromEntries(book.chapters.map((c) => [c.id, c.recipes.length]));
  const other = draft.chapters.find((c) => c.id === OTHER_CHAPTER)?.name ?? "Everything else";
  const running = run !== null;

  /** Illustrate all draws from what is on disk and in Settings, so anything still waiting is saved first. */
  const illustrateAll = async () => {
    keyPush.current.flush();
    typing.cancel();
    await Promise.all([keyDone.current, push()]);
    onIllustrateAll();
  };

  const moveBy = (id: string, by: -1 | 1, focus: string) => {
    const next = moveChapter(draftRef.current.chapters, id, by);
    if (!next) return;
    change((c) => ({ ...c, chapters: next }));
    // Moving a focused row in the DOM drops its focus: put it back
    requestAnimationFrame(() => document.querySelector<HTMLElement>(`[data-focus="${focus}-${id}"]`)?.focus());
  };

  const removeChapter = (id: string) => {
    setConfirm(null);
    change((c) => ({ ...c, chapters: c.chapters.filter((x) => x.id !== id) }));
  };

  return (
    <div className="sheet-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <section className="sheet book-sheet" role="dialog" aria-label="Book settings" tabIndex={-1} ref={sheet}>
        <div className="sheet-head">
          <h2 className="daily-h2">{draft.title.trim() || "Your cookbook"}</h2>
          <button
            className="icon"
            aria-label="Close"
            onClick={() => {
              typing.flush();
              onClose();
            }}
          >
            <Icon name="x" size={16} />
          </button>
        </div>

        <div className="grid2">
          <label>
            <span className="label">Title</span>
            <input value={draft.title} onChange={(e) => change((c) => ({ ...c, title: e.target.value }), "typing")} onBlur={typing.flush} />
          </label>
          <label>
            <span className="label">Subtitle</span>
            <input
              value={draft.subtitle}
              placeholder="The count is added on the cover"
              onChange={(e) => change((c) => ({ ...c, subtitle: e.target.value }), "typing")}
              onBlur={typing.flush}
            />
          </label>
        </div>

        <div className="sheet-group">
          <span className="label">Default picture</span>
          <div className="seg small" role="group" aria-label="Default picture">
            {PICTURES.map((p) => (
              <button
                key={p.key}
                className={draft.picture === p.key ? "on" : ""}
                aria-pressed={draft.picture === p.key}
                disabled={p.key === "illustration" && !draft.illustrations.enabled && draft.picture !== "illustration"}
                title={p.key === "illustration" && !draft.illustrations.enabled ? "Turn on illustrations below first" : undefined}
                onClick={() => change((c) => ({ ...c, picture: p.key }))}
              >
                {p.title}
              </button>
            ))}
          </div>
          <p className="muted small">What a recipe prints when it hasn't chosen its own.</p>
        </div>

        <div className="sheet-group">
          <h3>Chapters</h3>
          <ul className="sec-list chapter-list">
            {draft.chapters.map((c, i) => {
              const n = counts[c.id] ?? 0;
              const warning = confirm === c.id ? removeChapterWarning(c, n, other) : null;
              const keyMove = (e: React.KeyboardEvent, focus: string) => {
                if (!e.altKey || (e.key !== "ArrowUp" && e.key !== "ArrowDown")) return;
                e.preventDefault();
                moveBy(c.id, e.key === "ArrowUp" ? -1 : 1, focus);
              };
              return (
                <li
                  key={c.id}
                  className={`sec-row chapter-row ${over === i && dragging !== i ? "over" : ""} ${dragging === i ? "dragging" : ""}`}
                  onDragOver={(e) => {
                    if (dragging === null) return;
                    e.preventDefault();
                    setOver(i);
                  }}
                  onDragLeave={() => setOver((o) => (o === i ? null : o))}
                  onDrop={(e) => {
                    e.preventDefault();
                    const from = dragging;
                    setDragging(null);
                    setOver(null);
                    const next = from === null ? null : reorderChapters(draftRef.current.chapters, from, i);
                    if (next) change((d) => ({ ...d, chapters: next }));
                  }}
                  onDragEnd={() => {
                    setDragging(null);
                    setOver(null);
                  }}
                >
                  <div className="chapter-head">
                    <button
                      className="handle"
                      draggable
                      data-focus={`handle-${c.id}`}
                      aria-label={`Move ${c.name}`}
                      title="Drag, or press Alt+↑ or Alt+↓"
                      onKeyDown={(e) => keyMove(e, "handle")}
                      onDragStart={(e) => {
                        setDragging(i);
                        e.dataTransfer.effectAllowed = "move";
                        e.dataTransfer.setData("text/plain", String(i));
                      }}
                    >
                      ⋮⋮
                    </button>
                    <input
                      type="text"
                      value={c.name}
                      autoFocus={fresh === c.id}
                      data-focus={`name-${c.id}`}
                      aria-label={`Chapter name`}
                      onFocus={(e) => fresh === c.id && e.currentTarget.select()}
                      onChange={(e) => change((d) => ({ ...d, chapters: d.chapters.map((x) => (x.id === c.id ? { ...x, name: e.target.value } : x)) }), "typing")}
                      onKeyDown={(e) => keyMove(e, "name")}
                      onBlur={() => {
                        setFresh(null);
                        // A blank name goes back to the last one
                        if (!c.name.trim()) change((d) => ({ ...d, chapters: d.chapters.map((x) => (x.id === c.id ? { ...x, name: lastName.current[c.id] ?? "Chapter" } : x)) }));
                        else typing.flush();
                      }}
                    />
                    <span className="muted small chapter-count">{n === 1 ? "1 recipe" : `${n} recipes`}</span>
                    <button
                      className="ghost small"
                      disabled={!canRemoveChapter(c.id)}
                      title={canRemoveChapter(c.id) ? undefined : "Everything else holds what has no chapter, so it stays"}
                      onClick={() => (n === 0 ? removeChapter(c.id) : setConfirm(confirm === c.id ? null : c.id))}
                    >
                      Remove
                    </button>
                  </div>
                  {warning && (
                    <div className="chapter-warning" role="alert">
                      <span className="warn small">{warning}</span>
                      <button className="ghost danger small" onClick={() => removeChapter(c.id)}>
                        Remove it
                      </button>
                      <button className="ghost small" onClick={() => setConfirm(null)}>
                        Keep it
                      </button>
                    </div>
                  )}
                </li>
              );
            })}
            <li className="dl-add">
              <button
                className="link"
                onClick={() => {
                  const next = addChapter(draftRef.current.chapters, "New chapter");
                  const added = next.find((c) => !draftRef.current.chapters.some((x) => x.id === c.id));
                  setFresh(added?.id ?? null);
                  change((c) => ({ ...c, chapters: next }));
                }}
              >
                + Add a chapter
              </button>
              <span className="muted small">Which chapter a new recipe lands in is worked out when it is added; match words are in cookbook.toml.</span>
            </li>
          </ul>
        </div>

        <div className="sheet-group">
          <h3>Illustrations</h3>
          <label className="check">
            <input
              className="switch"
              type="checkbox"
              checked={draft.illustrations.enabled}
              disabled={running}
              onChange={(e) => change((c) => ({ ...c, illustrations: { ...c.illustrations, enabled: e.target.checked } }))}
            />
            Draw an engraving of each dish
          </label>
          <p className="muted small">{illustrationSends(draft.illustrations.endpoint)}</p>
          {draft.illustrations.enabled && (
            <>
              <div className="grid2">
                <label>
                  <span className="label">Endpoint</span>
                  <input
                    value={draft.illustrations.endpoint}
                    placeholder="https://api.openai.com/v1"
                    spellCheck={false}
                    onChange={(e) => change((c) => ({ ...c, illustrations: { ...c.illustrations, endpoint: e.target.value } }), "typing")}
                    onBlur={typing.flush}
                  />
                </label>
                <label>
                  <span className="label">Model</span>
                  <input
                    value={draft.illustrations.model}
                    placeholder="gpt-image-1"
                    spellCheck={false}
                    onChange={(e) => change((c) => ({ ...c, illustrations: { ...c.illustrations, model: e.target.value } }), "typing")}
                    onBlur={typing.flush}
                  />
                </label>
              </div>
              <label>
                <span className="label">API key</span>
                <input
                  type="password"
                  value={key}
                  placeholder="Optional"
                  autoComplete="off"
                  onChange={(e) => {
                    setKey(e.target.value);
                    keyRef.current = e.target.value;
                    keyPush.current.call();
                  }}
                  onBlur={() => keyPush.current.flush()}
                />
              </label>
              <p className="muted small">
                Uses the API key from Settings → Reading aids, stored on this Mac; empty uses OPENAI_API_KEY. X-Ray uses the same key, so it has to be one this endpoint accepts. It is never written to cookbook.toml.
              </p>
              <div className="paper-actions">
                <button className="ghost" disabled={running} onClick={illustrateAll}>
                  {running ? "Illustrating…" : "Illustrate all"}
                </button>
                {running && (
                  <button className="ghost" disabled={run.stopping} onClick={onStop}>
                    {run.stopping ? "Stopping after this one…" : "Stop"}
                  </button>
                )}
              </div>
              {running && (
                <p className="small" role="status">
                  {run.event ? illustrateProgress(run.event) : "Starting…"}
                </p>
              )}
              {!running && runResult && (
                <p className="small" role="status">
                  {runResult}
                </p>
              )}
              <p className="muted small">Draws the recipes that have no illustration yet, one at a time. Turning this on doesn't draw anything by itself.</p>
            </>
          )}
        </div>

        {error && (
          <p className="fail small" role="alert">
            Couldn't save: {error}
          </p>
        )}
        <p className="muted small">
          {saving ? "Saving… " : "Saved as you go to "}
          <span className="mono">{book.config_path}</span>; your own comments in it are kept.{" "}
          <button className="link" onClick={() => api.reveal(book.config_path)}>
            Show the file
          </button>
        </p>
      </section>
    </div>
  );
}
