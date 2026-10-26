// SPDX-License-Identifier: GPL-3.0-or-later
// The Daily tab (Daily design §5c, tomorrow's paper design §8), in rooms: Today (the press and
// tomorrow's paper so far, with its page plan and week), Editions, Reading, Delivery (where the
// paper goes) and The paper's settings, all backed by digest.toml and the CLI.
import { useCallback, useEffect, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import { ROOMS, destinations, roomNotes } from "./dailyRooms";
import { fontFileName, mastheadChoice, mastheadValue } from "./mastheadView";
import type { MastheadChoice } from "./mastheadView";
import type { Place, Room } from "./dailyRooms";
import Icon from "./Icon";
import PagePlanCard from "./PagePlanCard";
import { reorder, setBudget } from "./pagePlan";
import PressBar from "./PressBar";
import Readership from "./Readership";
import EveryEdition from "./EveryEdition";
import Shelf from "./Shelf";
import ThisWeek from "./ThisWeek";
import TomorrowsPaper from "./TomorrowsPaper";
import { TOPICS, toggleTopic, topicOn, topicTitle } from "./topics";
import { addInterest, listText, removeInterest, setList, setQuery } from "./interests";
import ExportFolders from "./ExportFolders";
import { exportParts, remarkablePart } from "./exportsView";
import RemarkableCard from "./RemarkableCard";
import SubscriptionsCard from "./SubscriptionsCard";
import YourDay from "./YourDay";
import DailyEditor from "./DailyEditor";
import Papers from "./Papers";
import { DAYS } from "./papersView";
import type { DailyConfig, DailyState, EditionPreview, EmailConfig, EmailStatus, FeedConfig, NextPaper, PagePlanRow, PapersView, PlaceRow, ReadingEntry, RunResult } from "./types";
import { DEFAULT_EMAIL, SIGNS } from "./types";

/** Amazon's Manage Your Content and Devices: the approved-sender list and the emailed Docs. */
const MYCD = "https://www.amazon.com/mycd";

function summary(r: RunResult, send: boolean): string {
  if (!r.ok && !r.report) return r.log || "Something went wrong.";
  const build = send ? r.report?.build : r.report;
  const parts: string[] = [];
  if (build?.title) parts.push(build.title);
  if (send) {
    const run = r.report?.run;
    const outcome = r.report?.outcome;
    if (r.report?.emailed) parts.push(`Emailed to ${r.report.emailed}.`);
    if (r.report?.email_error) parts.push(`Email failed: ${r.report.email_error}. Sent over USB instead.`);
    parts.push(...exportParts(r.report?.exports));
    const rm = remarkablePart(r.report);
    if (rm) parts.push(rm);
    if (run?.error) parts.push(`Not converted: ${run.error}`);
    else if (outcome?.flush?.delivered?.length) parts.push("Sent to your Kindle.");
    else if (outcome?.queued) parts.push("Waiting for your Kindle: it goes over when you plug it in.");
  } else {
    parts.push("Preview built. Saved articles stay saved until you send.");
  }
  return parts.join(" — ");
}

function counts(r: RunResult, send: boolean): { title: string; items: number; unavailable: string | null }[] {
  const build = send ? r.report?.build : r.report;
  return build?.sections ?? [];
}

function host(url: string | null): string {
  if (!url) return "";
  try {
    return new URL(url).host.replace(/^www\./, "");
  } catch {
    return url;
  }
}

export default function DailyView({ onNotice }: { onNotice: (m: string) => void }) {
  const [state, setState] = useState<DailyState | null>(null);
  const [draft, setDraft] = useState<DailyConfig | null>(null);
  const [dirty, setDirty] = useState(false);
  const [preview, setPreview] = useState<EditionPreview | null>(null);
  const [running, setRunning] = useState<null | "preview" | "send">(null);
  const runningRef = useRef<null | "preview" | "send">(null);
  const [result, setResult] = useState<{ r: RunResult; send: boolean } | null>(null);
  const [reading, setReading] = useState<ReadingEntry[]>([]);
  const [saveUrl, setSaveUrl] = useState("");
  const [saveMsg, setSaveMsg] = useState<string | null>(null);
  const [next, setNext] = useState<NextPaper | null>(null);
  const [newInterest, setNewInterest] = useState("");
  const [planSaving, setPlanSaving] = useState(false);
  const [time, setTime] = useState("05:30");
  const [placeQuery, setPlaceQuery] = useState("");
  const [places, setPlaces] = useState<PlaceRow[] | null>(null);
  const [copied, setCopied] = useState(false);
  const [emailStatus, setEmailStatus] = useState<EmailStatus | null>(null);
  const [password, setPassword] = useState("");
  const [emailMsg, setEmailMsg] = useState<{ text: string; ok: boolean } | null>(null);
  const [testing, setTesting] = useState(false);
  const [room, setRoom] = useState<Room>("today");
  const [openPlace, setOpenPlace] = useState<Place["key"] | null>(null);
  // Your papers (many papers §7): the selected paper is the one every room works on
  const [papers, setPapers] = useState<PapersView | null>(null);
  const [paper, setPaper] = useState("main");
  const arg = paper === "main" ? null : paper;
  const isMain = paper === "main";

  const loadPapers = useCallback(() => {
    api.papersList().then(setPapers).catch(() => setPapers(null));
  }, []);

  const load = useCallback(
    async (s?: DailyState) => {
      const st = s ?? (await api.dailyState(arg));
      setState(st);
      setDraft(st.config ? structuredClone(st.config) : null);
      setDirty(false);
      api.emailStatus(arg).then(setEmailStatus).catch(() => {});
    },
    [arg],
  );

  // Tomorrow's paper: recomputed from local files often; headlines fetched on open and focus only
  const loadNext = useCallback(() => {
    api.dailyNext(arg).then(setNext).catch(() => {});
  }, [arg]);
  const refreshNext = useCallback(() => {
    api.dailyNextRefresh(arg).then(setNext).catch(() => {});
  }, [arg]);

  const refreshSide = useCallback(() => {
    api.editionPreview(arg).then(setPreview).catch(() => {});
    api.readingList().then(setReading).catch(() => {});
    loadNext();
  }, [arg, loadNext]);
  const refreshSideRef = useRef(refreshSide);
  refreshSideRef.current = refreshSide;

  useEffect(() => {
    const un = api.onDailyFinished((r) => {
      setResult({ r, send: runningRef.current === "send" });
      runningRef.current = null;
      setRunning(null);
      refreshSideRef.current();
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  // Everything below follows the selected paper.
  useEffect(() => {
    load();
    loadPapers();
    refreshSide();
    refreshNext();
    setResult(null);
    const onFocus = () => refreshNext();
    window.addEventListener("focus", onFocus);
    const tick = setInterval(() => {
      if (document.visibilityState === "visible") loadNext();
    }, 60_000);
    return () => {
      window.removeEventListener("focus", onFocus);
      clearInterval(tick);
    };
  }, [load, loadPapers, refreshSide, refreshNext, loadNext]);

  // The press time shown is the selected paper's: its installed job, else its file's
  useEffect(() => {
    const card = papers?.papers.find((p) => p.slug === paper);
    api
      .scheduleGet()
      .then((s) => {
        const job = s.papers?.find((p) => p.slug === paper);
        setTime(job?.at ?? card?.at ?? (isMain && s.at ? s.at : "05:30"));
      })
      .catch(() => {});
  }, [paper, papers, isMain]);

  const choosePaper = (slug: string) => {
    if (slug === paper) return;
    if (dirty) {
      onNotice("Save your changes to this paper first.");
      return;
    }
    setPaper(slug);
  };

  const edit = (f: (c: DailyConfig) => void) => {
    setDraft((d) => {
      if (!d) return d;
      const c = structuredClone(d);
      f(c);
      return c;
    });
    setDirty(true);
  };

  /** Like `edit`, for the pure helpers that return a new config. */
  const editWith = (f: (c: DailyConfig) => DailyConfig) => {
    setDraft((d) => (d ? f(d) : d));
    setDirty(true);
  };

  const save = async (c = draft): Promise<boolean> => {
    if (!c) return false;
    const daysBefore = (state?.config?.schedule?.days ?? []).join();
    try {
      await load(await api.dailySave(c, arg));
      // New days move an installed job: it's reinstalled with them.
      const card = papers?.papers.find((p) => p.slug === paper);
      if (card?.installed && next?.press.at && (c.schedule?.days ?? []).join() !== daysBefore) {
        await api.scheduleSet(next.press.at, arg);
      }
      loadNext();
      loadPapers();
      return true;
    } catch (e) {
      onNotice(String(e));
      return false;
    }
  };

  const editEmail = (f: (e: EmailConfig) => void) =>
    edit((c) => {
      c.email = c.email ?? { ...DEFAULT_EMAIL };
      f(c.email);
    });

  // The keychain entry is named after the saved username and host, so unsaved edits go first.
  const savePassword = async () => {
    if (!password) return;
    if (dirty && !(await save())) return;
    try {
      setEmailStatus(await api.emailSetPassword(password));
      setEmailMsg({ text: "Password saved in the system keychain.", ok: true });
    } catch (e) {
      setEmailMsg({ text: String(e), ok: false });
    }
    setPassword("");
  };

  const sendTest = async () => {
    if (testing) return;
    if (dirty && !(await save())) return;
    setTesting(true);
    setEmailMsg(null);
    try {
      setEmailMsg({ text: await api.emailTest(arg), ok: true });
    } catch (e) {
      setEmailMsg({ text: String(e), ok: false });
    }
    setTesting(false);
  };

  const run = async (send: boolean, slug = paper) => {
    if (running) return;
    if (dirty) await save();
    setResult(null);
    runningRef.current = send ? "send" : "preview";
    setRunning(runningRef.current);
    api.dailyRun(send, slug === "main" ? null : slug).catch((e) => {
      runningRef.current = null;
      setRunning(null);
      onNotice(String(e));
    });
  };

  const addReading = async () => {
    if (!saveUrl.trim()) return;
    try {
      setSaveMsg(await api.readingSave(saveUrl));
      setSaveUrl("");
      refreshSide();
    } catch (e) {
      setSaveMsg(String(e));
    }
  };

  const findPlace = async () => {
    if (!placeQuery.trim()) return;
    setPlaces(null);
    try {
      setPlaces(await api.geocode(placeQuery));
    } catch (e) {
      onNotice(String(e));
      setPlaces([]);
    }
  };

  const setScheduleOn = async (on: boolean, at = time) => {
    try {
      await api.scheduleSet(on ? at : null, arg);
      loadNext();
      loadPapers();
      load();
    } catch (e) {
      onNotice(String(e));
    }
  };

  const createPaper = async (title: string, reader: string): Promise<boolean> => {
    if (dirty) {
      onNotice("Save your changes to this paper first.");
      return false;
    }
    try {
      const slug = await api.paperCreate(title, reader);
      setPaper(slug);
      setRoom("paper");
      return true;
    } catch (e) {
      onNotice(String(e));
      return false;
    }
  };

  const stopPresses = async (on: boolean) => {
    try {
      setNext(await api.pressStop(on, arg));
    } catch (e) {
      onNotice(String(e));
    }
  };

  if (!state) return <div className="daily" />;

  if (!state.exists) {
    return (
      <div className="daily">
        <section className="card setup">
          <h2>Your morning paper</h2>
          <p>
            Every morning Deckle builds a newspaper for your Kindle: the weather, local news, your feeds and the articles you
            saved, whole and with their photos. Nothing goes through a server.
          </p>
          <button className="primary" onClick={() => api.dailySetup().then((s) => load(s)).catch((e) => onNotice(String(e)))}>
            Set up your paper
          </button>
        </section>
      </div>
    );
  }

  const papersStrip = papers && (
    <Papers
      view={papers}
      selected={paper}
      busy={running !== null}
      onSelect={choosePaper}
      onPreview={(slug) => {
        if (slug !== paper && dirty) return onNotice("Save your changes to this paper first.");
        setPaper(slug);
        setRoom("today");
        run(false, slug);
      }}
      onCreate={createPaper}
    />
  );
  const mainTitle = papers?.papers[0]?.title ?? "the main paper";

  if (!draft) {
    return (
      <div className="daily">
        {papersStrip}
        <section className="card">
          <h2>{isMain ? "digest.toml" : `papers/${paper}.toml`} has a problem</h2>
          <p className="fail">{state.error}</p>
          <button className="ghost" onClick={() => api.reveal(state.config_path)}>
            Show the file
          </button>
        </section>
      </div>
    );
  }

  // Page plan edits write digest.toml at once. They apply to the saved config the plan was
  // made from, one at a time: with unsaved edits in The paper (a feed added or removed) the
  // plan waits for them to be saved.
  const planLocked = dirty || planSaving || !next;
  const planSave = async (f: (c: DailyConfig) => DailyConfig) => {
    if (planLocked || !state.config) return;
    setPlanSaving(true);
    try {
      const c = f(structuredClone(state.config));
      if (c !== state.config) await save(c);
    } finally {
      setPlanSaving(false);
    }
  };
  const planMove = (from: number, to: number) => planSave((c) => (next ? reorder(c, next.plan, from, to) : c));
  const planBudget = (row: PagePlanRow, n: number) => planSave((c) => setBudget(c, row, n));
  const planToggle = (row: PagePlanRow, on: boolean) =>
    planSave((c) => ({ ...c, sections: on ? [...c.sections, row.kind] : c.sections.filter((x) => x !== row.kind) }));

  const notes = roomNotes(draft, next, reading, dirty);

  const emailCard = (
    <section className="card email-card">
      <div className="card-head">
        <h3>Email to Kindle</h3>
        {dirty && (
          <button className="primary" onClick={() => save()}>
            Save
          </button>
        )}
      </div>
      <p className="muted small">
        The morning paper arrives over Wi-Fi with no cable: Deckle emails it to your Kindle's Send-to-Kindle address from your
        own mail account. If an email fails, the paper waits for USB as before.
      </p>
      {!isMain && <p className="muted small">It goes from the mail account set on {mainTitle}.</p>}
      <label className="check">
        <input type="checkbox" checked={!!draft.email?.enabled} onChange={(e) => editEmail((m) => (m.enabled = e.target.checked))} />
        Email the paper to my Kindle
      </label>
      <div className="grid2">
        <label>
          <span className="label">{isMain ? "Kindle address" : "Send to"}</span>
          <input value={draft.email?.to ?? ""} placeholder="you_ABC123@kindle.com" spellCheck={false} onChange={(e) => editEmail((m) => (m.to = e.target.value))} />
        </label>
        {isMain && (<>
        <label>
          <span className="label">From</span>
          <input value={draft.email?.from ?? ""} placeholder="you@example.com" spellCheck={false} onChange={(e) => editEmail((m) => (m.from = e.target.value))} />
        </label>
        <label>
          <span className="label">SMTP server</span>
          <input value={draft.email?.smtp_host ?? ""} placeholder="smtp.gmail.com" spellCheck={false} onChange={(e) => editEmail((m) => (m.smtp_host = e.target.value))} />
        </label>
        <label>
          <span className="label">Port</span>
          <select value={draft.email?.smtp_port ?? 587} onChange={(e) => editEmail((m) => (m.smtp_port = Number(e.target.value)))}>
            <option value={587}>587 (STARTTLS)</option>
            <option value={465}>465 (TLS)</option>
          </select>
        </label>
        <label>
          <span className="label">Username</span>
          <input value={draft.email?.username ?? ""} placeholder="you@example.com" spellCheck={false} onChange={(e) => editEmail((m) => (m.username = e.target.value))} />
        </label>
        <div>
          <span className="label">Password</span>
          <form
            className="save-row"
            onSubmit={(e) => {
              e.preventDefault();
              savePassword();
            }}
          >
            <input
              type="password"
              autoComplete="new-password"
              value={password}
              placeholder={emailStatus?.has_password ? "Saved in the keychain" : "App password"}
              onChange={(e) => setPassword(e.target.value)}
            />
            <button className="ghost" type="submit" disabled={!password}>
              {emailStatus?.has_password ? "Replace" : "Save"}
            </button>
          </form>
        </div>
        </>)}
      </div>
      <label className="check">
        <input type="checkbox" checked={!!draft.email?.usb_too} onChange={(e) => editEmail((m) => (m.usb_too = e.target.checked))} />
        Also send it over USB when the email goes out (two copies on the Kindle)
      </label>
      <div className="paper-actions">
        <button className="ghost" disabled={testing || !emailStatus?.configured || !emailStatus?.has_password} onClick={sendTest}>
          {testing ? "Sending…" : "Send a test"}
        </button>
        {emailStatus && !emailStatus.configured && (
          <span className="muted small">{isMain ? "Fill in the addresses and server first." : `Fill in Send to, and the account on ${mainTitle}.`}</span>
        )}
      </div>
      {emailMsg && <p className={`small ${emailMsg.ok ? "" : "fail"}`}>{emailMsg.text}</p>}
      <p className="muted small">
        Add the From address to Amazon's Approved Personal Document E-mail List (Manage Your Content and Devices → Preferences →
        Personal Document Settings), or Amazon drops the email. Gmail and iCloud need an app password. The password is kept in the
        system keychain, never in digest.toml.
      </p>
      <p className="muted small">
        Emailed papers collect in your Kindle's Docs: Deckle can't remove them, so the "keep" setting only applies over USB. Clear
        old ones in{" "}
        <button className="link" onClick={() => api.openLink(MYCD).catch(() => {})}>
          Manage Your Content and Devices
        </button>
      </p>
    </section>
  );
  const exportFolders = (
    <ExportFolders
      exports={draft.export}
      onSave={(exports) => {
        const c = structuredClone(draft);
        c.export = exports;
        setDraft(c);
        return save(c);
      }}
    />
  );
  const remarkableCard = (
    <RemarkableCard
      remarkable={draft.remarkable}
      onSave={(remarkable) => {
        const c = structuredClone(draft);
        c.remarkable = remarkable;
        setDraft(c);
        return save(c);
      }}
    />
  );

  // A place's settings, opened beneath its row in Delivery
  const placeSettings = (key: Place["key"]) => {
    if (key === "email") return emailCard;
    if (key === "exports") return exportFolders;
    if (key === "remarkable") return remarkableCard;
    return null;
  };

  return (
    <div className="daily-rooms">
      <nav className="rooms" aria-label="Daily">
        {ROOMS.map((r) => (
          <button key={r.key} className={room === r.key ? "on" : ""} aria-current={room === r.key ? "page" : undefined} onClick={() => setRoom(r.key)}>
            <span className="room-title">{r.title}</span>
            {notes[r.key] && <span className="room-note">{notes[r.key]}</span>}
          </button>
        ))}
      </nav>

      <div className="daily">
        {papersStrip}
        {room === "today" && (
          <>
            <PressBar press={next?.press ?? null} time={time} onTime={setTime} onSchedule={setScheduleOn} onRestart={() => stopPresses(false)} />

            <div className="daily-columns">
              <TomorrowsPaper
                next={next}
                preview={preview}
                running={running}
                onRun={run}
                onStop={() => stopPresses(true)}
                onShowFile={() => preview && api.reveal(preview.file)}
              >
                {result && (
                  <div className={`run-result ${result.r.ok ? "ok" : "fail"}`}>
                    <div>{summary(result.r, result.send)}</div>
                    <div className="section-counts">
                      {counts(result.r, result.send).map((s) => (
                        <span key={s.title} className={s.unavailable && s.items === 0 ? "warn" : ""} title={s.unavailable ?? ""}>
                          {s.title} {s.items}
                        </span>
                      ))}
                    </div>
                    {result.r.log && <pre className="log">{result.r.log}</pre>}
                  </div>
                )}
              </TomorrowsPaper>
              <div className="daily-side">
                <PagePlanCard
                  next={next}
                  locked={planLocked}
                  lockedWhy={dirty ? "Save your changes first, in The paper or Email to Kindle." : undefined}
                  onMove={planMove}
                  onBudget={planBudget}
                  onToggle={planToggle}
                />
                {next && <ThisWeek week={next.week} onAllEditions={() => setRoom("editions")} />}
              </div>
            </div>
          </>
        )}

        {room === "editions" && (
          <>
            <Readership key={paper} paper={arg} onApplied={() => load().then(loadNext)} />

            {isMain ? (
              <EveryEdition
                disabled={planLocked}
                building={running !== null}
                onFolder={(folder) => planSave((c) => ({ ...c, morgue: { keep_days: c.morgue?.keep_days ?? 365, folder } }))}
              />
            ) : (
              <section className="card">
                <h3>Every edition</h3>
                <p className="muted small">The app shows {mainTitle}'s editions for now. This paper's are kept in its own folder in the Morgue.</p>
              </section>
            )}
          </>
        )}

        {room === "reading" && !isMain && (
          <section className="card">
            <h3>Your Reading</h3>
            <p className="muted small">Saved articles go to {mainTitle}.</p>
          </section>
        )}

        {room === "reading" && isMain && (
          <>
            <section className="card">
              <h3>Your Reading</h3>
              <p className="muted small">Save an article now; it's in tomorrow's paper, whole. Over {draft.reading.long_read_words.toLocaleString()} words, it comes as its own book.</p>
              <form
                className="save-row"
                onSubmit={(e) => {
                  e.preventDefault();
                  addReading();
                }}
              >
                <input type="url" placeholder="Paste a link (https://…)" value={saveUrl} onChange={(e) => setSaveUrl(e.target.value)} spellCheck={false} />
                <button className="ghost" type="submit" disabled={!saveUrl.trim()}>
                  Save
                </button>
              </form>
              {saveMsg && <p className="muted small">{saveMsg}</p>}
              {reading.length > 0 && (
                <ul className="reading">
                  {reading.map((e) => (
                    <li key={e.id}>
                      <span className={`state ${e.state}`}>{e.state === "book" ? "own book" : e.state}</span>
                      <span className="what" title={e.url ?? e.file ?? ""}>
                        {e.title ?? host(e.url) ?? e.file}
                        {e.title && <span className="muted"> · {host(e.url)}</span>}
                        {e.note && e.state !== "printed" && <span className="muted"> · {e.note}</span>}
                      </span>
                      <button className="icon small" aria-label="Remove" onClick={() => api.readingRemove(e.id).then(refreshSide)}>
                        <Icon name="x" size={14} />
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              <div className="row-pair">
                <div>
                  <div className="label">Inbox folder</div>
                  <div className="mono">{draft.reading.inbox}</div>
                  <p className="muted small">Drop links or saved pages here. Point it at iCloud Drive to save from your phone.</p>
                </div>
                <button
                  className="ghost"
                  onClick={async () => {
                    const dir = await openDialog({ directory: true, multiple: false });
                    if (typeof dir === "string") {
                      const c = structuredClone(draft);
                      c.reading.inbox = dir;
                      save(c);
                    }
                  }}
                >
                  Change…
                </button>
              </div>
              <div className="row-pair">
                <div>
                  <div className="label">Bookmarklet</div>
                  <p className="muted small">Copy it, then add a bookmark in your browser and paste it as the address. Clicking it saves the page you're on (while Deckle's morning schedule is on).</p>
                </div>
                <button
                  className="ghost"
                  onClick={async () => {
                    try {
                      await navigator.clipboard.writeText(await api.bookmarklet());
                      setCopied(true);
                      setTimeout(() => setCopied(false), 2000);
                    } catch (e) {
                      onNotice(String(e));
                    }
                  }}
                >
                  {copied ? "Copied" : "Copy bookmarklet"}
                </button>
              </div>
            </section>

            <Shelf inPaper={draft.sections.includes("shelf")} />
          </>
        )}

        {room === "delivery" && (
          <section className="card delivery" aria-label="Where the paper goes">
            <h3>Where the paper goes</h3>
            <p className="muted small">
              Each morning's paper goes to every place that's on. A reader that isn't plugged in keeps its copy waiting and gets it
              the next time.
            </p>
            <ul className="place-list">
              {destinations(draft).map((p) => {
                const head = (
                  <>
                    <span className={`press-dot ${p.on ? "on" : ""}`} aria-hidden="true" />
                    <span className="place-name">
                      <strong>{p.name}</strong>
                      <span className="muted small">{p.how}</span>
                    </span>
                    <span className="place-status">{p.status}</span>
                  </>
                );
                return (
                  <li key={p.key}>
                    {p.open ? (
                      <button className="place-head" aria-expanded={openPlace === p.key} onClick={() => setOpenPlace(openPlace === p.key ? null : p.key)}>
                        {head}
                        <Icon name="chevron-down" />
                      </button>
                    ) : (
                      <div className="place-head">{head}</div>
                    )}
                    {openPlace === p.key && <div className="place-body">{placeSettings(p.key)}</div>}
                  </li>
                );
              })}
            </ul>
          </section>
        )}

        {room === "paper" && (
          <>
            <section className="card settings-card">
              <div className="card-head">
                <h3>The paper</h3>
                <button className="primary" disabled={!dirty} onClick={() => save()}>
                  {dirty ? "Save" : "Saved"}
                </button>
              </div>

              <div className="grid2">
                <label>
                  <span className="label">Name</span>
                  <input value={draft.title} onChange={(e) => edit((c) => (c.title = e.target.value))} />
                </label>
                <label>
                  <span className="label">For</span>
                  <input value={draft.reader ?? ""} placeholder="Who reads it" onChange={(e) => edit((c) => (c.reader = e.target.value))} />
                </label>
                <label>
                  <span className="label">Kindle</span>
                  <select value={draft.kindle ?? ""} onChange={(e) => edit((c) => (c.kindle = e.target.value))}>
                    <option value="">Any Kindle</option>
                    {(papers?.kindles ?? []).map((k) => (
                      <option key={k.sync_id} value={k.name}>
                        {k.name}
                      </option>
                    ))}
                    {draft.kindle && !(papers?.kindles ?? []).some((k) => k.name === draft.kindle) && (
                      <option value={draft.kindle}>{draft.kindle} (not named yet)</option>
                    )}
                  </select>
                </label>
                <label>
                  <span className="label">Units</span>
                  <select value={draft.units} onChange={(e) => edit((c) => (c.units = e.target.value as DailyConfig["units"]))}>
                    <option value="imperial">°F, mph</option>
                    <option value="metric">°C, km/h</option>
                  </select>
                </label>
                <label>
                  <span className="label">Title font</span>
                  <select
                    value={mastheadChoice(draft.masthead)}
                    onChange={async (e) => {
                      const choice = e.target.value as MastheadChoice;
                      if (choice !== "file") {
                        edit((c) => (c.masthead = mastheadValue(choice)));
                        return;
                      }
                      const picked = await openDialog({
                        multiple: false,
                        filters: [{ name: "TrueType font", extensions: ["ttf"] }],
                      });
                      if (typeof picked === "string") edit((c) => (c.masthead = picked));
                    }}
                  >
                    <option value="blackletter">Blackletter</option>
                    <option value="serif">Serif</option>
                    <option value="sans">Sans</option>
                    <option value="file">
                      {mastheadChoice(draft.masthead) === "file" && draft.masthead
                        ? `Custom: ${fontFileName(draft.masthead)}`
                        : "Choose a .ttf file…"}
                    </option>
                  </select>
                </label>
              </div>

              {(papers?.kindles.length ?? 0) === 0 && (
                <p className="muted small">To send this paper to one Kindle only, name your Kindles in the Kindle tab.</p>
              )}

              <div className="grid2">
                <div>
                  <div className="label">Type</div>
                  <label className="check">
                    <input type="checkbox" checked={!!draft.large_print} onChange={(e) => edit((c) => (c.large_print = e.target.checked))} />
                    Large print
                  </label>
                  <p className="muted small">
                    Nothing smaller than the body text, bigger headlines and one photo a story. Set the text size on the Kindle itself
                    (Aa). It applies to every book on that Kindle.
                  </p>
                </div>
                <div>
                  <div className="label">Days</div>
                  <div className="places">
                    {DAYS.map((d) => {
                      const days = draft.schedule?.days ?? [];
                      return (
                        <button
                          key={d}
                          className={`chip ${days.includes(d) ? "on" : ""}`}
                          onClick={() =>
                            edit((c) => {
                              const cur = c.schedule?.days ?? [];
                              const nextDays = cur.includes(d) ? cur.filter((x) => x !== d) : [...cur, d];
                              c.schedule = { ...c.schedule, days: DAYS.filter((x) => nextDays.includes(x)) };
                            })
                          }
                        >
                          {d[0].toUpperCase() + d.slice(1)}
                        </button>
                      );
                    })}
                  </div>
                  <p className="muted small">{(draft.schedule?.days ?? []).length === 0 ? "None chosen: every day." : "Built on these days only."} The time is set on Today.</p>
                </div>
              </div>

              <div className="label">Place</div>
              <div className="place">
                <span>
                  {draft.place || <em>Not set</em>}
                  {draft.latitude != null && <span className="muted small"> · {draft.latitude.toFixed(2)}, {draft.longitude?.toFixed(2)}</span>}
                </span>
                <form
                  className="save-row"
                  onSubmit={(e) => {
                    e.preventDefault();
                    findPlace();
                  }}
                >
                  <input placeholder="Town, state (e.g. Carthage, NC)" value={placeQuery} onChange={(e) => setPlaceQuery(e.target.value)} />
                  <button className="ghost" type="submit">
                    Find
                  </button>
                </form>
                {places && (
                  <div className="places">
                    {places.length === 0 && <span className="muted small">No matches.</span>}
                    {places.map((p) => (
                      <button
                        key={`${p.latitude},${p.longitude}`}
                        className="chip"
                        onClick={() => {
                          edit((c) => {
                            c.place = p.label;
                            c.latitude = Math.round(p.latitude * 100) / 100;
                            c.longitude = Math.round(p.longitude * 100) / 100;
                          });
                          setPlaces(null);
                          setPlaceQuery("");
                        }}
                      >
                        {p.label}
                      </button>
                    ))}
                  </div>
                )}
              </div>

              <label>
                <span className="label">Local news search</span>
                <input value={draft.local.google_news} placeholder="e.g. Moore County NC" onChange={(e) => edit((c) => (c.local.google_news = e.target.value))} />
              </label>

              <div className="label">Other places</div>
              <p className="muted small">Local news for a relative's town or a trip, each its own section after Local. Weather stays on your place.</p>
              <div className="feeds">
                {(draft.local.places ?? []).map((p, i) => (
                  <div key={i} className="feed">
                    <input value={p.name} placeholder="Name (e.g. Asheville, NC)" onChange={(e) => edit((c) => (c.local.places![i].name = e.target.value))} />
                    <input value={p.google_news} placeholder="Google News search" onChange={(e) => edit((c) => (c.local.places![i].google_news = e.target.value))} />
                    <button className="icon small" aria-label="Remove place" onClick={() => edit((c) => c.local.places!.splice(i, 1))}>
                      <Icon name="x" size={14} />
                    </button>
                  </div>
                ))}
                <button className="ghost" onClick={() => edit((c) => (c.local.places = [...(c.local.places ?? []), { name: "", google_news: "", feeds: [] }]))}>
                  Add a place
                </button>
              </div>

              <div className="label">Topics</div>
              <p className="muted small">Each ticked topic becomes its own section, from Google News.</p>
              <div className="topics">
                {TOPICS.map((t) => (
                  <label key={t.slug} className="check">
                    <input type="checkbox" checked={topicOn(draft.feeds, t.slug)} onChange={() => edit((c) => (c.feeds = toggleTopic(c.feeds, t.slug)))} />
                    {t.title}
                  </label>
                ))}
              </div>

              <div className="label">Your interests</div>
              <p className="muted small">Things you care about, each its own section: Deckle searches the news for them every morning.</p>
              <label className="check">
                <input
                  type="checkbox"
                  checked={draft.learning?.enabled ?? true}
                  onChange={(e) => edit((c) => (c.learning = { window_days: 14, min_stories: 3, nudges: false, ...(c.learning ?? {}), enabled: e.target.checked }))}
                />
                Deckle keeps track of what you tap and highlight in the paper, on this Mac, to suggest new interests.
              </label>
              <div className="feeds">
                {(draft.interests ?? []).map((it, i) => (
                  <div key={i} className="feed interest">
                    <input value={it.name} placeholder="e.g. Carolina Panthers" onChange={(e) => edit((c) => (c.interests![i].name = e.target.value))} />
                    <input
                      value={it.query ?? ""}
                      placeholder={`Search: "${it.name || "Rust"}" programming`}
                      title="Your own Google News search instead of the quoted name; OR and -word work"
                      onChange={(e) => editWith((c) => setQuery(c, i, e.target.value))}
                    />
                    <input
                      value={listText(it.also)}
                      placeholder="Also: Bryce Young, Dave Canales"
                      onChange={(e) => editWith((c) => setList(c, i, "also", e.target.value))}
                    />
                    <input
                      value={listText(it.not)}
                      placeholder="Leave out: Florida Panthers, hockey"
                      onChange={(e) => editWith((c) => setList(c, i, "not", e.target.value))}
                    />
                    <button className="icon small" aria-label="Remove interest" onClick={() => editWith((c) => removeInterest(c, i))}>
                      <Icon name="x" size={14} />
                    </button>
                  </div>
                ))}
                <form
                  className="add-interest"
                  onSubmit={(e) => {
                    e.preventDefault();
                    if (!newInterest.trim()) return;
                    editWith((c) => addInterest(c, newInterest));
                    setNewInterest("");
                  }}
                >
                  <input value={newInterest} placeholder="Add an interest… (Enter)" onChange={(e) => setNewInterest(e.target.value)} />
                </form>
              </div>

              <div className="label">Your feeds</div>
              <div className="feeds">
                {draft.feeds.map((f: FeedConfig, i: number) => (
                  <div key={i} className="feed">
                    <input value={f.name} placeholder="Section name" onChange={(e) => edit((c) => (c.feeds[i].name = e.target.value))} />
                    {f.topic ? (
                      <span className="muted small">topic: {topicTitle(f.topic)}</span>
                    ) : (
                      <input value={f.url} placeholder="https://… (RSS or Atom)" spellCheck={false} onChange={(e) => edit((c) => (c.feeds[i].url = e.target.value))} />
                    )}
                    <button className="icon small" aria-label="Remove feed" onClick={() => edit((c) => c.feeds.splice(i, 1))}>
                      <Icon name="x" size={14} />
                    </button>
                  </div>
                ))}
                <button className="ghost" onClick={() => edit((c) => c.feeds.push({ name: "", url: "", max_items: 8 }))}>
                  Add a feed
                </button>
              </div>

              <div className="grid2">
                <div>
                  <div className="label">Lottery</div>
                  {[
                    ["powerball", "Powerball"],
                    ["megamillions", "Mega Millions"],
                  ].map(([k, label]) => (
                    <label key={k} className="check">
                      <input
                        type="checkbox"
                        checked={draft.lottery.games.includes(k)}
                        onChange={(e) => edit((c) => (c.lottery.games = e.target.checked ? [...c.lottery.games, k] : c.lottery.games.filter((g) => g !== k)))}
                      />
                      {label}
                    </label>
                  ))}
                </div>
                <div>
                  <div className="label">Stories</div>
                  <label className="check">
                    <input type="checkbox" checked={draft.full_text} onChange={(e) => edit((c) => (c.full_text = e.target.checked))} />
                    Whole stories, not summaries
                  </label>
                  <label className="check">
                    <input type="checkbox" checked={draft.photos} disabled={!draft.full_text} onChange={(e) => edit((c) => (c.photos = e.target.checked))} />
                    With their photos
                  </label>
                  <label className="inline">
                    Keep the last
                    <input type="number" min={1} max={14} value={draft.keep} onChange={(e) => edit((c) => (c.keep = Math.max(1, Number(e.target.value) || 1)))} />
                    editions on the Kindle
                  </label>
                </div>
              </div>

              <DailyEditor value={draft.llm} onChange={(value) => edit((c) => { c.llm = value; })} />

              <div className="label">Horoscope {draft.horoscope.signs.length === 0 && <span className="muted small">(none chosen: all twelve)</span>}</div>
              <div className="places">
                {SIGNS.map((s) => (
                  <button
                    key={s}
                    className={`chip ${draft.horoscope.signs.includes(s) ? "on" : ""}`}
                    onClick={() => edit((c) => (c.horoscope.signs = c.horoscope.signs.includes(s) ? c.horoscope.signs.filter((x) => x !== s) : [...c.horoscope.signs, s]))}
                  >
                    {s[0].toUpperCase() + s.slice(1)}
                  </button>
                ))}
              </div>

              <p className="muted small">
                Saved to <span className="mono">{state.config_path}</span>; your own comments in that file are kept.
              </p>
            </section>

            <YourDay
              day={draft.day}
              emailOn={draft.email?.enabled ?? false}
              onSave={(day) => {
                const c = structuredClone(draft);
                c.day = day;
                setDraft(c);
                return save(c);
              }}
            />
            <SubscriptionsCard
              subscriptions={draft.subscriptions ?? []}
              dirty={dirty}
              onChange={(subs) => edit((c) => (c.subscriptions = subs))}
              onSave={() => save()}
            />
          </>
        )}
      </div>
    </div>
  );
}
