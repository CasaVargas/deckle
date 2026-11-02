// SPDX-License-Identifier: GPL-3.0-or-later
// The Puzzles tab (Puzzles tab design §1): make a write-on puzzle book, see its first page as
// it will print, send it to the Scribe or save it, and keep every book made with the options
// that made it, so Make again gives the same book.
import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "./api";
import { platform } from "./platform";
import PuzzlePreview from "./PuzzlePreview";
import { DIFFICULTIES, SCREENS, bookLine, makingLine, mb, ownListPrompt, problem, requestFor, sendLine, sizeChoices } from "./puzzleBooks";
import type { DeviceRow, MadePuzzleBook, PuzzleBookRequest, PuzzleKind, PuzzlePreview as Preview, PuzzleProgress, RunTarget } from "./types";

interface Props {
  onNotice: (m: string) => void;
  /** The connected Kindle, when one is, and the target that sends to it. */
  device: DeviceRow | null;
  deviceTarget: RunTarget | null;
}

/** How long typing in the options waits before the preview is remade. */
const PREVIEW_DELAY = 500;

export default function PuzzlesView({ onNotice, device, deviceTarget }: Props) {
  const [kinds, setKinds] = useState<PuzzleKind[]>([]);
  const [request, setRequest] = useState<PuzzleBookRequest | null>(null);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [previewing, setPreviewing] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [making, setMaking] = useState<PuzzleProgress | null>(null);
  const [books, setBooks] = useState<MadePuzzleBook[]>([]);
  const [sending, setSending] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const latest = useRef<PuzzleBookRequest | null>(null);
  latest.current = request;

  const loadBooks = useCallback(() => {
    api.puzzlesList().then(setBooks).catch(() => setBooks([]));
  }, []);

  useEffect(() => {
    api
      .puzzlesKinds()
      .then((ks) => {
        setKinds(ks);
        setRequest((r) => r ?? requestFor(ks[0]));
      })
      .catch((e) => onNotice(String(e)));
    loadBooks();
  }, [loadBooks, onNotice]);

  useEffect(() => {
    const un = api.onPuzzlesProgress((e) => setMaking((m) => (m ? e : m)));
    return () => {
      un.then((f) => f());
    };
  }, []);

  const kind = kinds.find((k) => k.key === request?.kind) ?? null;
  const wrong = request && kind ? problem(request, kind) : null;

  // The preview follows the options, a moment after typing stops.
  useEffect(() => {
    if (!request || !kind || wrong) return;
    setPreviewing(true);
    const t = setTimeout(() => {
      const asked = request;
      api
        .puzzlesPreview(asked)
        .then((p) => {
          if (latest.current === asked) {
            setPreview(p);
            setPreviewError(null);
          }
        })
        .catch((e) => {
          if (latest.current === asked) setPreviewError(String(e));
        })
        .finally(() => {
          if (latest.current === asked) setPreviewing(false);
        });
    }, PREVIEW_DELAY);
    return () => clearTimeout(t);
  }, [request, kind, wrong]);

  const patch = (p: Partial<PuzzleBookRequest>) => setRequest((r) => (r ? { ...r, ...p } : r));
  const pickKind = (k: PuzzleKind) => {
    setPreview(null);
    setRequest(requestFor(k, request));
  };
  const pickPicture = async () => {
    const picked = (await platform.pickFiles({ filterName: "Pictures", extensions: ["png", "jpg", "jpeg", "gif", "webp"] }))?.[0];
    if (picked) patch({ picture: picked });
  };

  const make = async () => {
    if (!request) return;
    setNote(null);
    setMaking({ made: 0, of: request.count });
    try {
      const made = await api.puzzlesMake(request);
      setNote(`${made.name}: ${made.pages} pages${made.warnings.length ? `. ${made.warnings.join(". ")}` : ""}`);
      loadBooks();
    } catch (e) {
      onNotice(String(e));
    } finally {
      setMaking(null);
    }
  };
  const send = async (book: MadePuzzleBook, target: RunTarget, replace = false) => {
    setSending(book.path);
    try {
      const where = await api.puzzlesSend(book.path, target, replace);
      setNote(`Sent ${book.title} to ${where}`);
    } catch (e) {
      const msg = String(e);
      if (!replace && /already there/.test(msg) && window.confirm(`${book.name} is already there. Replace it? Ink on the old one may stay.`)) {
        await send(book, target, true);
        return;
      }
      onNotice(msg);
    } finally {
      setSending(null);
    }
  };
  const saveCopy = async (book: MadePuzzleBook) => {
    const dir = await platform.pickFolder({ title: "Save the book into" });
    if (typeof dir === "string") await send(book, { kind: "folder", path: dir });
  };
  const makeAgain = (book: MadePuzzleBook) => {
    const k = kinds.find((x) => x.key === book.request.kind);
    if (!k) return;
    setPreview(null);
    setRequest({ ...book.request });
    window.scrollTo({ top: 0 });
  };

  const own = kind ? ownListPrompt(kind) : null;
  const deviceName = device ? device.name || device.model_hint : null;

  return (
    <div className="daily-view">
      <div className="daily puzzles">
        <div className="puzzles-head">
          <h2 className="daily-h2">Make a puzzle book</h2>
          <p className="muted">A PDF you solve in ink on a Kindle Scribe or a reMarkable, with the answers at the back. The same options and date make the same book again.</p>
        </div>

        <div className="daily-columns puzzles-columns">
          <div className="puzzles-form">
            <fieldset className="mode-cards">
              <legend className="visually-hidden">Kind</legend>
              <div className="mode-grid puzzles-kinds">
                {kinds.map((k) => (
                  <label key={k.key} className={`mode-card ${request?.kind === k.key ? "on" : ""}`}>
                    <span className="mode-head">
                      <input type="radio" name="puzzle-kind" checked={request?.kind === k.key} onChange={() => pickKind(k)} />
                      <strong>{k.title}</strong>
                    </span>
                    <span className="muted">{k.blurb}</span>
                  </label>
                ))}
              </div>
            </fieldset>

            {request && kind && (
              <>
                {(own || kind.takes === "picture") && (
                  <section className="set-card set-pad" aria-label="Theme">
                    {own && (
                      <label className="puzzles-own">
                        <span className="tk-label">{own.label}</span>
                        <textarea rows={4} value={kind.takes === "quotes" ? request.quotes : request.words} placeholder={own.placeholder} spellCheck={false} onChange={(e) => patch(kind.takes === "quotes" ? { quotes: e.target.value } : { words: e.target.value })} />
                        <span className="muted small">{own.help}</span>
                      </label>
                    )}
                    {kind.takes === "picture" && (
                      <div className="puzzles-own">
                        <span className="tk-label">Your own picture, as puzzle 1</span>
                        <div className="row-pair">
                          <button className="ghost" onClick={pickPicture}>
                            {request.picture ? "Change picture…" : "Choose a picture…"}
                          </button>
                          {request.picture && (
                            <>
                              <span className="muted small">{request.picture.split(/[\\/]/).pop()}</span>
                              <button className="link-action" onClick={() => patch({ picture: null })}>
                                Remove
                              </button>
                            </>
                          )}
                        </div>
                        <span className="muted small">Any photo: the darker half of its squares are shaded. The rest of the book is Deckle's own pictures and shapes.</span>
                      </div>
                    )}
                  </section>
                )}

                <section className="set-card set-pad puzzles-options" aria-label="Options">
                  <div className="ai-fields">
                    {kind.sizes && (
                      <label>
                        <span className="tk-label">Size</span>
                        <select value={request.size} onChange={(e) => patch({ size: Number(e.target.value) })}>
                          {sizeChoices(kind).map((s) => (
                            <option key={s.size} value={s.size}>
                              {s.label}
                            </option>
                          ))}
                        </select>
                      </label>
                    )}
                    {kind.graded && (
                      <label>
                        <span className="tk-label">Difficulty</span>
                        <select value={request.difficulty} onChange={(e) => patch({ difficulty: e.target.value as PuzzleBookRequest["difficulty"] })}>
                          {DIFFICULTIES.map((d, i) => (
                            <option key={d} value={d}>
                              {d[0].toUpperCase() + d.slice(1)}: {kind.difficulties[i]}
                            </option>
                          ))}
                        </select>
                      </label>
                    )}
                    <label>
                      <span className="tk-label">{kind.key === "ladder" ? "Pages" : "Puzzles"}</span>
                      <input type="number" min={1} max={200} value={request.count} onChange={(e) => patch({ count: Number(e.target.value) })} />
                    </label>
                    <label>
                      <span className="tk-label">For</span>
                      <select value={request.screen} onChange={(e) => patch({ screen: e.target.value })}>
                        {SCREENS.map((s) => (
                          <option key={s.key} value={s.key}>
                            {s.name}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label className="puzzles-title">
                      <span className="tk-label">Title</span>
                      <input type="text" value={request.title} placeholder={kind.title} onChange={(e) => patch({ title: e.target.value })} />
                    </label>
                  </div>
                  {wrong && <p className="muted small warn">{wrong}</p>}
                </section>

                <div className="puzzles-actions">
                  <button className="primary" onClick={make} disabled={!!wrong || !!making}>
                    {making ? makingLine(request.kind, making.made, making.of) : "Make the book"}
                  </button>
                  {making && (
                    <div className="progress" aria-label="Making the book">
                      <div className="progress-track">
                        <div className="progress-fill" style={{ width: `${making.of ? (making.made * 100) / making.of : 0}%` }} />
                      </div>
                    </div>
                  )}
                  <span className="muted small">Kept in ~/Deckle/Puzzles. Then send it, or save a copy, from Your books below.</span>
                </div>
                {note && (
                  <p className="muted small" role="status">
                    {note}
                  </p>
                )}
              </>
            )}
          </div>

          <aside className="daily-side puzzles-side" aria-label="Preview">
            <div className="set-card set-pad">
              <div className="set-card-head">
                <h3 className="set-card-title">{preview ? "No. 1" : "Preview"}</h3>
                {preview && (
                  <span className="muted small">
                    {preview.pages} pages{previewing ? " · updating…" : ""}
                  </span>
                )}
              </div>
              {preview?.first ? <PuzzlePreview puzzle={preview.first} /> : <p className="muted">{previewError ?? (previewing ? "Making the first puzzle…" : "Pick a kind to see its first page.")}</p>}
              {preview && (
                <p className="muted small">
                  {preview.title} · {preview.subtitle}
                </p>
              )}
              {preview?.warnings.map((w) => (
                <p key={w} className="muted small warn">
                  {w}
                </p>
              ))}
              {preview && kind && (kind.takes === "clued" || kind.takes === "words") && <p className="muted small">Your words are tinted here; in ink they're plain.</p>}
            </div>
          </aside>
        </div>

        <section className="set-card puzzles-books" aria-label="Your books">
          <div className="online-head">Your books</div>
          {books.length === 0 ? (
            <p className="muted" style={{ padding: "12px 16px", margin: 0 }}>
              None yet. The first one lands here, with Make again and Send.
            </p>
          ) : (
            books.map((b) => (
              <div key={b.path} className="online-row puzzles-book">
                <div style={{ minWidth: 0 }}>
                  <strong>{b.title}</strong> <span className="muted small">{b.date}</span>
                  <p className="muted">
                    {bookLine(b)} · {mb(b.bytes)}
                  </p>
                  <span className="muted small">{sendLine(deviceName)}</span>
                </div>
                <div className="puzzles-book-actions">
                  <button className="ghost" onClick={() => makeAgain(b)}>
                    Make again
                  </button>
                  <button className="ghost" onClick={() => saveCopy(b)} disabled={sending === b.path}>
                    Save a copy…
                  </button>
                  {deviceTarget && (
                    <button className="primary" onClick={() => send(b, deviceTarget)} disabled={sending === b.path}>
                      {sending === b.path ? "Sending…" : `Send to ${deviceName}`}
                    </button>
                  )}
                </div>
              </div>
            ))
          )}
        </section>
      </div>
    </div>
  );
}
