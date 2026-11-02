// SPDX-License-Identifier: GPL-3.0-or-later
// The welcome (welcome design §4): a card over the app on first launch, five pages. It sets up
// the paper and its morning schedule, gets the bookmarklets onto the bookmarks bar and watches
// for the first save, shows the back pages and the extras (the puzzles, the almanac, the AI
// model if wanted), then says where everything else is. Skip is always one click away.
//
// Beside the pages sits a miniature of tomorrow's front page, set in the paper's own masthead
// face. It carries the day's date and the press time as you set it, and page 3's first save
// lands on it as a clipping, so what the welcome talks about is in view the whole way.
import { useEffect, useRef, useState } from "react";
import { useCapabilities } from "./capabilities";
import { api } from "./api";
import BookmarkletInstall from "./BookmarkletInstall";
import { loadMastheadFont } from "./mastheadFonts";
import { faceFamily, mastheadChoice } from "./mastheadView";
import type { FaceKey } from "./mastheadView";
import { STEPS, clock, dateline, firstSave, mainAt, saveLine, scheduleLine } from "./onboarding";
import type { StepKey } from "./onboarding";
import type { SettingsPage } from "./settingsIndex";
import type { DailyState, ReadingEntry, ScheduleStatus } from "./types";

/** The Mini in miniature: a 5×5 grid, black squares where a 5×5 has them. */
const MINI_BLACK = new Set([0, 4, 12, 20, 24]);

/** The front page in miniature. Grey bars stand for the stories; the real parts are the title,
 * the date, the press time and, once it lands, the first saved story. */
function MiniPaper({ page, title, face, at, save }: { page: StepKey; title: string; face: FaceKey; at: string | null; save: ReadingEntry | null }) {
  useEffect(() => loadMastheadFont(face), [face]);
  const clipping = page === "save";
  const puzzles = page === "extras";
  return (
    <div className={`mini-paper ${clipping ? "clipping" : ""}`} aria-hidden="true">
      <div className="mini-masthead" style={{ fontFamily: `${faceFamily(face)}, ui-serif, Georgia, serif` }}>
        {title}
      </div>
      <div className="mini-dateline">
        <span>{dateline(new Date())}</span>
        {at && <span>Printed at {clock(at)}</span>}
      </div>
      <div className="mini-cols">
        <div className="mini-col lead">
          <i className="mini-head" />
          <i className="mini-head short" />
          <i className="mini-photo" />
          <i className="mini-line" />
          <i className="mini-line" />
          <i className="mini-line short" />
        </div>
        <div className="mini-col">
          {puzzles ? (
            <>
              <i className="mini-head small" style={{ marginTop: 0 }} />
              <div className="mini-grid">
                {Array.from({ length: 25 }, (_, i) => (
                  <i key={i} className={MINI_BLACK.has(i) ? "black" : ""} />
                ))}
              </div>
              <i className="mini-line" />
              <i className="mini-line short" />
            </>
          ) : (
            <>
              <i className="mini-weather" />
              <i className="mini-line" />
              <i className="mini-line" />
              <i className="mini-line short" />
            </>
          )}
          <i className="mini-head small" />
          <i className="mini-line" />
          <i className="mini-line" />
          <i className="mini-line" />
          <i className="mini-line short" />
        </div>
      </div>
      {clipping && (
        <div className={`mini-clip ${save ? "landed" : ""}`}>
          {save ? <span className="mini-clip-title">{save.title?.trim() || save.url || "Your story"}</span> : <span>The story you save goes here</span>}
        </div>
      )}
    </div>
  );
}

export default function Welcome({ onDone, onNotice, onSettings }: { onDone: (goDaily: boolean) => void; onNotice: (m: string) => void; onSettings?: (page: SettingsPage) => void }) {
  const caps = useCapabilities();
  const [step, setStep] = useState(0);
  const [daily, setDaily] = useState<DailyState | null>(null);
  const [schedule, setSchedule] = useState<ScheduleStatus | null>(null);
  const [at, setAt] = useState("05:30");
  const [busy, setBusy] = useState(false);
  /** When page 3 opened: a bookmarklet save after this is the one we're waiting for. */
  const [since, setSince] = useState<string | null>(null);
  const [save, setSave] = useState<ReadingEntry | null>(null);
  const card = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    card.current?.focus();
    api.dailyState().then(setDaily).catch(() => {});
    if (!caps.web) {
      api
        .scheduleGet()
        .then((s) => {
          setSchedule(s);
          const was = mainAt(s);
          if (was) setAt(was);
        })
        .catch(() => {});
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Page 3: look for the first save every 3 s while it's open.
  useEffect(() => {
    if (!since) return;
    const look = () => {
      api
        .readingList()
        .then((rows) => setSave(firstSave(rows, since)))
        .catch(() => {});
    };
    look();
    const t = setInterval(look, 3000);
    return () => clearInterval(t);
  }, [since]);

  const skip = () => onDone(false);
  const go = (n: number) => {
    const next = Math.max(0, Math.min(STEPS.length - 1, n));
    if (STEPS[next].key === "save" && !since) setSince(new Date().toISOString());
    setStep(next);
  };
  const setUp = async () => {
    setBusy(true);
    try {
      setDaily(await api.dailySetup());
    } catch (e) {
      onNotice(String(e));
    } finally {
      setBusy(false);
    }
  };
  const turnOn = async () => {
    setBusy(true);
    try {
      setSchedule(await api.scheduleSet(at));
    } catch (e) {
      onNotice(String(e));
    } finally {
      setBusy(false);
    }
  };

  const key = STEPS[step].key;
  const last = step === STEPS.length - 1;
  const watching = caps.web || !!schedule?.watch;
  const scheduled = watching && !!mainAt(schedule);
  const title = daily?.config?.title?.trim() || "Your paper";
  const face = mastheadChoice(daily?.config?.masthead);
  const printedAt = caps.web ? null : scheduled ? mainAt(schedule) : key === "paper" ? at : null;

  return (
    <div className="welcome-backdrop" role="dialog" aria-modal="true" aria-labelledby="welcome-title">
      <div
        className="welcome"
        ref={card}
        tabIndex={-1}
        onKeyDown={(e) => {
          if (e.key === "Escape") skip();
        }}
      >
        <div className="welcome-desk">
          <MiniPaper page={key} title={title} face={face === "file" ? "consent" : face} at={printedAt} save={save} />
        </div>

        <div className="welcome-pane">
          <div className="welcome-body" key={key}>
            {key === "welcome" && (
              <>
                <h1 id="welcome-title">Welcome to Deckle</h1>
                <p className="lede">A morning paper for your e-reader, printed on your own computer. And the books you own, finished like the store's.</p>
                <p>Four short pages: your paper and when it prints, the bookmarklet that saves what you're reading into it, the back pages and the extras, and where everything else is.</p>
                <p className="muted small">Nothing goes through a server. There's no account to make.</p>
              </>
            )}

            {key === "paper" && (
              <>
                <h2 id="welcome-title">Your paper</h2>
                <p>Every morning Deckle builds a newspaper: the weather, local news, your feeds and the articles you saved, whole and with their photos.</p>
                {daily && !daily.exists ? (
                  <div className="status">
                    <p>There's no paper yet. Set one up to start with the weather and local news for your town; you'll name the town on the Daily tab.</p>
                    <button className="primary" onClick={setUp} disabled={busy}>
                      Set up your paper
                    </button>
                  </div>
                ) : (
                  daily && (
                    <div className="status ok">
                      <p>Your paper is set up. The Daily tab has its town, feeds and sections.</p>
                    </div>
                  )
                )}
                {!caps.web && (
                  <div className="welcome-schedule">
                    <label className="welcome-time">
                      Print it every morning at{" "}
                      <input type="time" value={at} onChange={(e) => setAt(e.target.value)} aria-label="Press time" />
                    </label>
                    <div className="row-pair">
                      <p className={scheduled ? "ok small" : "muted small"}>{scheduleLine(schedule, caps.web)}</p>
                      <button className="ghost" onClick={turnOn} disabled={busy || !at || !daily?.exists}>
                        {scheduled ? "Change the time" : "Turn on the morning schedule"}
                      </button>
                    </div>
                  </div>
                )}
                {caps.web && <p className="muted small">{scheduleLine(schedule, true)}</p>}
              </>
            )}

            {key === "save" && (
              <>
                <h2 id="welcome-title">Save to paper</h2>
                <p>Reading something you'd rather finish on the Kindle? A bookmarklet is a bookmark that does something: click it and the page you're on is in tomorrow's paper.</p>
                <BookmarkletInstall which="both" onNotice={onNotice} />
                {!watching ? (
                  <div className="status">
                    <p>
                      The bookmarklets work while the morning schedule is on.{" "}
                      <button className="link" onClick={() => go(1)}>
                        Turn it on
                      </button>
                    </p>
                  </div>
                ) : (
                  <div className={`status ${save ? "ok" : "waiting"}`} role="status">
                    <p>{saveLine(save)}</p>
                  </div>
                )}
              </>
            )}

            {key === "extras" && (
              <>
                <h2 id="welcome-title">The back pages</h2>
                <p>
                  Every paper ends the way a paper should. <strong>The Mini</strong>, a 5×5 crossword whose theme answers are yesterday's headlines; a <strong>word search</strong> hiding
                  more of them; a <strong>word ladder</strong>; an <strong>almanac</strong> with sunrise, the moon, the usual first frost and tonight's meteor shower; a <strong>horoscope</strong>;
                  and <strong>On this day</strong>. Yesterday's answers print the next morning, and a Kindle that keeps notes marks yours: write the answer as a note on the clue and the
                  paper says how many you solved.
                </p>
                <p>
                  <strong>Puzzle books to write on.</strong> Crosswords with your own words and clues, word searches, sudoku, word ladders, cryptograms, nonograms and mazes, as a book that
                  takes pen ink on a Kindle Scribe or a reMarkable. The Puzzles tab makes one in a minute.
                </p>
                <p>
                  <strong>From Your Shelf.</strong> The books you're in the middle of, a past highlight and the words you looked up this week, read from your own Kindle, on this computer only.
                </p>
                <p>
                  <strong>An AI model, if you want one.</strong> Off by default. Turned on, it folds the same story from several feeds into one, writes short section briefs, and gives your
                  books X-Ray descriptions and chapter recaps. It can run on this {caps.web ? "server" : "Mac"} for free through Ollama, with nothing leaving it, or at a service you pay.{" "}
                  {onSettings ? (
                    <button
                      className="link"
                      onClick={() => {
                        onDone(false);
                        onSettings("ai");
                      }}
                    >
                      Settings → AI sets it up.
                    </button>
                  ) : (
                    "Settings → AI sets it up."
                  )}
                </p>
              </>
            )}

            {key === "where" && (
              <>
                <h2 id="welcome-title">Where things are</h2>
                <dl className="where">
                  <div>
                    <dt>Daily</dt>
                    <dd>Your town, feeds and interests; the sections in print order; and Delivery: a Kindle by USB or email, a Kobo, a PocketBook, a reMarkable, or any folder.</dd>
                  </div>
                  <div>
                    <dt>Books</dt>
                    <dd>Drop in EPUBs, Word documents and PDFs; plug in the Kindle; press Send. They come out like store books.</dd>
                  </div>
                  <div>
                    <dt>Cookbook</dt>
                    <dd>The recipes you save, as one book that keeps up with itself.</dd>
                  </div>
                  <div>
                    <dt>Puzzles</dt>
                    <dd>A puzzle book to write on, seven kinds, sent to the Scribe or saved for a reMarkable.</dd>
                  </div>
                  <div>
                    <dt>Kindle</dt>
                    <dd>What's on it, how far you are, and what Deckle can make better.</dd>
                  </div>
                  <div>
                    <dt>Settings → AI</dt>
                    <dd>Off, on this {caps.web ? "server" : "Mac"}, or a service online, for X-Ray, chapter recaps and the paper's editor.</dd>
                  </div>
                </dl>
                <p className="muted small">This welcome is under Settings → About if you want it again.</p>
              </>
            )}
          </div>

          <div className="welcome-foot">
            <ol className="welcome-marks" aria-label="Pages">
              {STEPS.map((s, i) => (
                <li key={s.key} className={i === step ? "on" : i < step ? "done" : ""} aria-current={i === step ? "step" : undefined}>
                  <span className="visually-hidden">{s.title}</span>
                </li>
              ))}
            </ol>
            <div className="spacer" />
            {!last && (
              <button className="quiet" onClick={skip}>
                Skip
              </button>
            )}
            {step > 0 && (
              <button className="ghost" onClick={() => go(step - 1)}>
                Back
              </button>
            )}
            {last ? (
              <button className="primary" onClick={() => onDone(true)}>
                Open the Daily tab
              </button>
            ) : (
              <button className="primary" onClick={() => go(step + 1)}>
                Next
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
