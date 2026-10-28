// SPDX-License-Identifier: GPL-3.0-or-later
// The Daily tab (Daily design §5c, Daily tab redesign §2–3), in tabs: Today (tomorrow's paper so
// far, when it goes to press, a link box and the week), Sections, Reading, Editions and Delivery,
// all backed by digest.toml and the CLI. The paper switcher picks which paper they work on.
import { useCallback, useEffect, useRef, useState } from "react";
import { setupSavedExport, setupWithRefresh } from "./pressView";
import { debounce, saveQueue } from "./saveQueue";
import SectionsTab from "./SectionsTab";
import type { Change } from "./SectionsTab";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import { ROOMS, goesLine, stopLine, toggleDay } from "./dailyRooms";
import PaperSheet from "./PaperSheet";
import type { Room } from "./dailyRooms";
import Icon from "./Icon";
import { reorder, setBudget } from "./pagePlan";
import PressCard from "./PressCard";
import PaperSwitcher from "./PaperSwitcher";
import Readership from "./Readership";
import EveryEdition from "./EveryEdition";
import Shelf from "./Shelf";
import ThisWeek from "./ThisWeek";
import TomorrowsPaper from "./TomorrowsPaper";
import DeliveryTab from "./DeliveryTab";
import { exportParts, remarkablePart } from "./exportsView";
import { anyKindleHint } from "./papersView";
import { keepUnfinished, savable } from "./subscriptions";
import type { ExportConfig, PressSetupStatus, DailyConfig, DailyState, Day, EditionPreview, EmailConfig, EmailStatus, NextPaper, PagePlanRow, PapersView, ReadershipView, ReadingEntry, RunResult } from "./types";
import { DEFAULT_EMAIL } from "./types";

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

export default function DailyView({
  onNotice,
  initialRoom,
  onRoomTaken,
}: {
  onNotice: (m: string) => void;
  /** A room to open on (the Cookbook's Delivery note links to Delivery); `onRoomTaken` says it was used. */
  initialRoom?: Room | null;
  onRoomTaken?: () => void;
}) {
  const [state, setState] = useState<DailyState | null>(null);
  const [draft, setDraft] = useState<DailyConfig | null>(null);
  // Saving as you go (Daily tab redesign §4): the newest edit is in draftRef; `seq` counts edits,
  // so a save that lands after newer typing leaves the typing alone.
  const draftRef = useRef<DailyConfig | null>(null);
  const seq = useRef(0);
  const [saving, setSaving] = useState(false);
  const [planStale, setPlanStale] = useState(false);
  const failed = useRef<string | null>(null);
  const [preview, setPreview] = useState<EditionPreview | null>(null);
  const [running, setRunning] = useState<null | "preview" | "send">(null);
  const runningRef = useRef<null | "preview" | "send">(null);
  const [result, setResult] = useState<{ r: RunResult; send: boolean } | null>(null);
  const [reading, setReading] = useState<ReadingEntry[]>([]);
  const [saveUrl, setSaveUrl] = useState("");
  const [saveMsg, setSaveMsg] = useState<string | null>(null);
  const [next, setNext] = useState<NextPaper | null>(null);
  const [time, setTime] = useState("05:30");
  const [copied, setCopied] = useState(false);
  const [emailStatus, setEmailStatus] = useState<EmailStatus | null>(null);
  const [password, setPassword] = useState("");
  const [emailMsg, setEmailMsg] = useState<{ text: string; ok: boolean } | null>(null);
  const [testing, setTesting] = useState(false);
  const [room, setRoom] = useState<Room>(initialRoom ?? "today");
  useEffect(() => {
    if (initialRoom) onRoomTaken?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const [readership, setReadership] = useState<ReadershipView | null>(null);
  const [sheetOpen, setSheetOpen] = useState(false);
  const [pressBusy, setPressBusy] = useState(false);
  const pressBusyRef = useRef(false);
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
      draftRef.current = st.config ? structuredClone(st.config) : null;
      setDraft(draftRef.current);
      api.emailStatus(arg).then(setEmailStatus).catch(() => {});
    },
    [arg],
  );

  // Tomorrow's paper: recomputed from local files often; headlines fetched on open and focus only
  const loadNext = useCallback(() => {
    api
      .dailyNext(arg)
      .then((n) => {
        setNext(n);
        setPlanStale(false);
      })
      .catch(() => {});
  }, [arg]);
  const refreshNext = useCallback(() => {
    api.dailyNextRefresh(arg).then(setNext).catch(() => {});
  }, [arg]);

  const refreshSide = useCallback(() => {
    api.editionPreview(arg).then(setPreview).catch(() => {});
    api.readingList().then(setReading).catch(() => {});
    api.readershipReport(arg).then(setReadership).catch(() => setReadership(null));
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

  // What a save needs that changes between renders
  const live = useRef({ paper, papers, next, state, onNotice, loadNext, loadPapers });
  live.current = { paper, papers, next, state, onNotice, loadNext, loadPapers };

  // One save at a time, of the newest config, to the paper it was made on
  const queue = useRef(
    saveQueue<{ c: DailyConfig; slug: string }>(
      async ({ c, slug }) => {
        const at = seq.current;
        const { papers: ps, next: n, state: before } = live.current;
        const daysBefore = (before?.config?.schedule?.days ?? []).join();
        const st = await api.dailySave(savable(c), slug === "main" ? null : slug);
        if (slug !== live.current.paper) return;
        setState(st);
        if (seq.current === at && st.config) {
          draftRef.current = structuredClone(keepUnfinished(st.config, c));
          setDraft(draftRef.current);
        }
        // New days move an installed job: it's reinstalled with them.
        const card = ps?.papers.find((p) => p.slug === slug);
        if (card?.installed && n?.press.at && (c.schedule?.days ?? []).join() !== daysBefore) {
          await api.scheduleSet(n.press.at, slug === "main" ? null : slug);
        }
      },
      (e) => {
        failed.current = String(e);
        live.current.onNotice(String(e));
      },
      () => {
        setSaving(false);
        live.current.loadNext();
        live.current.loadPapers();
      },
    ),
  ).current;

  const push = (c: DailyConfig) => {
    failed.current = null;
    setSaving(true);
    setPlanStale(true);
    return queue.push({ c, slug: live.current.paper });
  };
  const typing = useRef(debounce(() => draftRef.current && push(draftRef.current), 500)).current;
  const closeSheet = useCallback(() => {
    typing.flush();
    setSheetOpen(false);
  }, [typing]);
  useEffect(() => () => typing.flush(), [typing]);

  /** An edit: switches and menus save at once, typing once it pauses (or the field is left). */
  const change: Change = (f, how = "now") => {
    const d = draftRef.current;
    if (!d || pressBusyRef.current) return;
    const c = f(structuredClone(d));
    draftRef.current = c;
    setDraft(c);
    seq.current++;
    if (how === "typing") typing.call();
    else {
      typing.cancel();
      push(c);
    }
  };
  /** The old in-place style: `f` edits a copy. */
  const edit = (f: (c: DailyConfig) => void, how: "now" | "typing" = "typing") =>
    change((c) => {
      f(c);
      return c;
    }, how);
  /** Saves now (a config, or the newest edits) and says whether it worked. */
  const saveNow = async (c?: DailyConfig): Promise<boolean> => {
    typing.cancel();
    if (c) {
      draftRef.current = c;
      setDraft(c);
      seq.current++;
    }
    if (!draftRef.current) return false;
    await push(draftRef.current);
    return failed.current === null;
  };

  const setupPress = async (index: number, folder: ExportConfig, enabled: boolean): Promise<PressSetupStatus> => {
    if (pressBusyRef.current) throw new Error("Setup is already running.");
    pressBusyRef.current = true;
    setPressBusy(true);
    const slug = live.current.paper;
    const paperArg = slug === "main" ? null : slug;
    try {
      return await setupWithRefresh(
        () => setupSavedExport(index, folder, enabled, async () => {
          typing.flush();
          await queue.drain();
          return failed.current === null;
        },
          () => live.current.paper === slug ? draftRef.current?.export ?? [] : [],
          (i, on, expected) => api.pressSetup(paperArg, i, on, expected)),
        async () => {
          const st = await api.dailyState(paperArg);
          if (live.current.paper !== slug) return false;
          setState(st);
          draftRef.current = st.config ? structuredClone(st.config) : null;
          setDraft(draftRef.current);
          return !!st.config && !st.error;
        },
        () => { failed.current = null; },
      );
    } finally {
      pressBusyRef.current = false;
      setPressBusy(false);
    }
  };

  const choosePaper = (slug: string) => {
    if (slug === paper || pressBusyRef.current) return;
    typing.flush();
    setPaper(slug);
  };

  const editEmail = (f: (e: EmailConfig) => void, how: "now" | "typing" = "typing") =>
    edit((c) => {
      c.email = c.email ?? { ...DEFAULT_EMAIL };
      f(c.email);
    }, how);

  // The keychain entry is named after the saved username and host, so unsaved edits go first.
  const savePassword = async () => {
    if (!password) return;
    if (!(await saveNow())) return;
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
    if (!(await saveNow())) return;
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
    await saveNow();
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
    typing.flush();
    try {
      const slug = await api.paperCreate(title, reader);
      setPaper(slug);
      setRoom("sections");
      return true;
    } catch (e) {
      onNotice(String(e));
      return false;
    }
  };

  const toggleDayOn = (d: Day) => change((c) => ({ ...c, schedule: { ...c.schedule, days: toggleDay(c.schedule?.days ?? [], d) } }));

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

  const switcher = papers && (
    <PaperSwitcher view={papers} selected={paper} onSelect={choosePaper} onSettings={() => setSheetOpen(true)} onCreate={createPaper} />
  );
  const paperNotes = papers ? [anyKindleHint(papers.papers), ...papers.notes].filter((n): n is string => !!n) : [];
  const mainTitle = papers?.papers[0]?.title ?? "the main paper";

  if (!draft) {
    return (
      <div className="daily">
        {switcher}
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

  // Page plan edits save at once, on the newest config. A move or budget waits while a save
  // runs and until the plan is rebuilt from it, so its rows match the file.
  const planLocked = saving || planStale || !next;
  const planSave = (f: (c: DailyConfig) => DailyConfig) => {
    if (planLocked) return;
    change(f);
  };
  const planMove = (from: number, to: number) => planSave((c) => (next ? reorder(c, next.plan, from, to) : c));
  const planBudget = (row: PagePlanRow, n: number) => planSave((c) => setBudget(c, row, n));
  const planToggle = (row: PagePlanRow, on: boolean) =>
    planSave((c) => ({ ...c, sections: on ? [...c.sections, row.kind] : c.sections.filter((x) => x !== row.kind) }));

  const emailBody = (
    <section className="email-card" aria-label="Email to Kindle">
      <p className="muted small">
        The morning paper arrives over Wi-Fi with no cable: Deckle emails it to your Kindle's Send-to-Kindle address from your
        own mail account. If an email fails, the paper waits for USB as before.
      </p>
      {!isMain && <p className="muted small">It goes from the mail account set on {mainTitle}.</p>}
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
          <select value={draft.email?.smtp_port ?? 587} onChange={(e) => editEmail((m) => (m.smtp_port = Number(e.target.value)), "now")}>
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
        <input type="checkbox" checked={!!draft.email?.usb_too} onChange={(e) => editEmail((m) => (m.usb_too = e.target.checked), "now")} />
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
  const saveBox = (
    <form
      className="save-row"
      onSubmit={(e) => {
        e.preventDefault();
        addReading();
      }}
    >
      <input type="url" placeholder="Paste a link (https://…)" value={saveUrl} onChange={(e) => setSaveUrl(e.target.value)} spellCheck={false} aria-label="A link to save" />
      <button className="ghost" type="submit" disabled={!saveUrl.trim()}>
        Save
      </button>
    </form>
  );
  const waiting = reading.filter((e) => e.state === "pending").length;

  return (
    <div className="daily-view">
      <nav className="kindle-tabs daily-tabs" aria-label="Daily">
        {switcher}
        {ROOMS.map((r) => (
          <button key={r.key} disabled={pressBusy} className={room === r.key ? "on" : ""} aria-current={room === r.key ? "page" : undefined} onClick={() => setRoom(r.key)}>
            {r.title}
          </button>
        ))}
      </nav>

      <div className="daily">
        {paperNotes.map((n) => (
          <p key={n} className="warn small">
            {n}
          </p>
        ))}
        {room === "today" && (
          <div className="daily-columns">
            <TomorrowsPaper next={next} masthead={draft.masthead} onSections={() => setRoom("sections")} />
            <div className="daily-side">
              <PressCard
                press={next?.press ?? null}
                time={time}
                days={draft.schedule?.days ?? []}
                goes={goesLine(draft)}
                running={running}
                onTime={setTime}
                onSchedule={setScheduleOn}
                onDay={toggleDayOn}
                onRestart={() => stopPresses(false)}
                onStop={() => stopPresses(true)}
                onRun={run}
                onDelivery={() => setRoom("delivery")}
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
                <p className="muted small">
                  {preview ? `Last built ${preview.title.replace(/^.*— /, "")}` : "No edition built yet."}
                  {preview && preview.outbox > 0 && ` · ${preview.outbox} waiting for your Kindle`}
                  {preview && (
                    <>
                      {" · "}
                      <button className="link" onClick={() => api.reveal(preview.file)}>
                        Show file
                      </button>
                    </>
                  )}
                </p>
              </PressCard>

              <section className="card" aria-label="Save something for tomorrow">
                <h3>Save something for tomorrow</h3>
                {isMain ? (
                  <>
                    {saveBox}
                    {saveMsg && <p className="muted small">{saveMsg}</p>}
                    <p className="muted small">
                      {waiting === 0 ? "Nothing saved yet." : `${waiting} saved for tomorrow.`}{" "}
                      <button className="link" onClick={() => setRoom("reading")}>
                        Your Reading
                      </button>
                    </p>
                  </>
                ) : (
                  <p className="muted small">Saved links go to {mainTitle}.</p>
                )}
              </section>

              {next && <ThisWeek week={next.week} stop={stopLine(readership)} onAllEditions={() => setRoom("editions")} />}
            </div>
          </div>
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
              {saveBox}
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
                    if (typeof dir === "string") change((c) => ({ ...c, reading: { ...c.reading, inbox: dir } }));
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

        {room === "delivery" && <DeliveryTab key={paper} draft={draft} change={change} emailBody={emailBody} setupPress={setupPress} pressBusy={pressBusy} />}

        {room === "sections" && (
          <SectionsTab
            next={next}
            draft={draft}
            locked={planLocked}
            saving={saving}
            change={change}
            flush={typing.flush}
            saveNow={saveNow}
            onMove={planMove}
            onBudget={planBudget}
            onToggle={planToggle}
            onReading={() => setRoom("reading")}
            onNotice={onNotice}
          />
        )}
      </div>
      {sheetOpen && (
        <PaperSheet
          draft={draft}
          kindles={papers?.kindles ?? []}
          configPath={state.config_path}
          edit={edit}
          flush={typing.flush}
          onClose={closeSheet}
        />
      )}
    </div>
  );
}
