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
import { DATE_LOCALE, rich, useI18n } from "./i18n";
import type { StepKey } from "./onboarding";
import type { SettingsPage } from "./settingsIndex";
import type { DailyState, ReadingEntry, ScheduleStatus } from "./types";

/** The Mini in miniature: a 5×5 grid, black squares where a 5×5 has them. */
const MINI_BLACK = new Set([0, 4, 12, 20, 24]);

/** The front page in miniature. Grey bars stand for the stories; the real parts are the title,
 * the date, the press time and, once it lands, the first saved story. */
function MiniPaper({ page, title, face, at, save }: { page: StepKey; title: string; face: FaceKey; at: string | null; save: ReadingEntry | null }) {
  const { t, locale } = useI18n();
  useEffect(() => loadMastheadFont(face), [face]);
  const clipping = page === "save";
  const puzzles = page === "extras";
  return (
    <div className={`mini-paper ${clipping ? "clipping" : ""}`} aria-hidden="true">
      <div className="mini-masthead" style={{ fontFamily: `${faceFamily(face)}, ui-serif, Georgia, serif` }}>
        {title}
      </div>
      <div className="mini-dateline">
        <span>{dateline(new Date(), DATE_LOCALE[locale])}</span>
        {at && <span>{t("welcome.mini.printedAt", { time: clock(at) })}</span>}
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
          {save ? <span className="mini-clip-title">{save.title?.trim() || save.url || t("welcome.mini.yourStory")}</span> : <span>{t("welcome.mini.clipEmpty")}</span>}
        </div>
      )}
    </div>
  );
}

export default function Welcome({ onDone, onNotice, onSettings }: { onDone: (goDaily: boolean) => void; onNotice: (m: string) => void; onSettings?: (page: SettingsPage) => void }) {
  const caps = useCapabilities();
  const { t } = useI18n();
  const machine = t(caps.web ? "machine.server" : "machine.mac");
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
  const title = daily?.config?.title?.trim() || t("welcome.untitledPaper");
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
                <h1 id="welcome-title">{t("welcome.welcome.title")}</h1>
                <p className="lede">{t("welcome.welcome.lede")}</p>
                <p>{t("welcome.welcome.body")}</p>
                <p className="muted small">{t("welcome.welcome.noServer")}</p>
              </>
            )}

            {key === "paper" && (
              <>
                <h2 id="welcome-title">{t("welcome.paper.title")}</h2>
                <p>{t("welcome.paper.body")}</p>
                {daily && !daily.exists ? (
                  <div className="status">
                    <p>{t("welcome.paper.none")}</p>
                    <button className="primary" onClick={setUp} disabled={busy}>
                      {t("welcome.paper.setUp")}
                    </button>
                  </div>
                ) : (
                  daily && (
                    <div className="status ok">
                      <p>{t("welcome.paper.ready")}</p>
                    </div>
                  )
                )}
                {!caps.web && (
                  <div className="welcome-schedule">
                    <label className="welcome-time">
                      {t("welcome.paper.printAt")}{" "}
                      <input type="time" value={at} onChange={(e) => setAt(e.target.value)} aria-label={t("welcome.paper.pressTime")} />
                    </label>
                    <div className="row-pair">
                      <p className={scheduled ? "ok small" : "muted small"}>{scheduleLine(schedule, caps.web, t)}</p>
                      <button className="ghost" onClick={turnOn} disabled={busy || !at || !daily?.exists}>
                        {scheduled ? t("welcome.paper.changeTime") : t("welcome.paper.turnOn")}
                      </button>
                    </div>
                  </div>
                )}
                {caps.web && <p className="muted small">{scheduleLine(schedule, true, t)}</p>}
              </>
            )}

            {key === "save" && (
              <>
                <h2 id="welcome-title">{t("welcome.save.title")}</h2>
                <p>{t("welcome.save.body")}</p>
                <BookmarkletInstall which="both" onNotice={onNotice} />
                {!watching ? (
                  <div className="status">
                    <p>
                      {t("welcome.save.needsSchedule")}{" "}
                      <button className="link" onClick={() => go(1)}>
                        {t("welcome.save.turnItOn")}
                      </button>
                    </p>
                  </div>
                ) : (
                  <div className={`status ${save ? "ok" : "waiting"}`} role="status">
                    <p>{saveLine(save, t)}</p>
                  </div>
                )}
              </>
            )}

            {key === "extras" && (
              <>
                <h2 id="welcome-title">{t("welcome.extras.title")}</h2>
                <p>{rich(t("welcome.extras.puzzles"))}</p>
                <p>{rich(t("welcome.extras.puzzleBooks"))}</p>
                <p>{rich(t("welcome.extras.shelf"))}</p>
                <p>
                  {rich(t("welcome.extras.ai", { machine }))}{" "}
                  {onSettings ? (
                    <button
                      className="link"
                      onClick={() => {
                        onDone(false);
                        onSettings("ai");
                      }}
                    >
                      {t("welcome.extras.aiLink")}
                    </button>
                  ) : (
                    t("welcome.extras.aiLink")
                  )}
                </p>
              </>
            )}

            {key === "where" && (
              <>
                <h2 id="welcome-title">{t("welcome.where.title")}</h2>
                <dl className="where">
                  <div>
                    <dt>{t("welcome.where.dailyTab")}</dt>
                    <dd>{t("welcome.where.daily")}</dd>
                  </div>
                  <div>
                    <dt>{t("welcome.where.booksTab")}</dt>
                    <dd>{t("welcome.where.books")}</dd>
                  </div>
                  <div>
                    <dt>{t("welcome.where.cookbookTab")}</dt>
                    <dd>{t("welcome.where.cookbook")}</dd>
                  </div>
                  <div>
                    <dt>{t("welcome.where.puzzlesTab")}</dt>
                    <dd>{t("welcome.where.puzzles")}</dd>
                  </div>
                  <div>
                    <dt>{t("welcome.where.kindleTab")}</dt>
                    <dd>{t("welcome.where.kindle")}</dd>
                  </div>
                  <div>
                    <dt>{t("welcome.where.aiTab")}</dt>
                    <dd>{t("welcome.where.ai", { machine })}</dd>
                  </div>
                </dl>
                <p className="muted small">{t("welcome.where.again")}</p>
              </>
            )}
          </div>

          <div className="welcome-foot">
            <ol className="welcome-marks" aria-label={t("welcome.pages")}>
              {STEPS.map((s, i) => (
                <li key={s.key} className={i === step ? "on" : i < step ? "done" : ""} aria-current={i === step ? "step" : undefined}>
                  <span className="visually-hidden">{t(s.title)}</span>
                </li>
              ))}
            </ol>
            <div className="spacer" />
            {!last && (
              <button className="quiet" onClick={skip}>
                {t("welcome.skip")}
              </button>
            )}
            {step > 0 && (
              <button className="ghost" onClick={() => go(step - 1)}>
                {t("welcome.back")}
              </button>
            )}
            {last ? (
              <button className="primary" onClick={() => onDone(true)}>
                {t("welcome.openDaily")}
              </button>
            ) : (
              <button className="primary" onClick={() => go(step + 1)}>
                {t("welcome.next")}
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
