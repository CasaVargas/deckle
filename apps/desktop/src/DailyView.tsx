// SPDX-License-Identifier: GPL-3.0-or-later
// The Daily tab (Daily design §5c): today's paper, Your Reading, the schedule, and the paper's
// settings, all backed by digest.toml and the CLI.
import { useCallback, useEffect, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import Icon from "./Icon";
import Readership from "./Readership";
import type { DailyConfig, DailyState, EditionPreview, EmailConfig, EmailStatus, FeedConfig, PlaceRow, ReadingEntry, RunResult, ScheduleStatus, SectionKind } from "./types";
import { ALL_SECTIONS, DEFAULT_EMAIL, SECTION_LABEL, SIGNS } from "./types";

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
  const [schedule, setSchedule] = useState<ScheduleStatus | null>(null);
  const [time, setTime] = useState("05:30");
  const [placeQuery, setPlaceQuery] = useState("");
  const [places, setPlaces] = useState<PlaceRow[] | null>(null);
  const [copied, setCopied] = useState(false);
  const [emailStatus, setEmailStatus] = useState<EmailStatus | null>(null);
  const [password, setPassword] = useState("");
  const [emailMsg, setEmailMsg] = useState<{ text: string; ok: boolean } | null>(null);
  const [testing, setTesting] = useState(false);

  const load = useCallback(async (s?: DailyState) => {
    const st = s ?? (await api.dailyState());
    setState(st);
    setDraft(st.config ? structuredClone(st.config) : null);
    setDirty(false);
    api.emailStatus().then(setEmailStatus).catch(() => {});
  }, []);

  const refreshSide = useCallback(() => {
    api.editionPreview().then(setPreview).catch(() => {});
    api.readingList().then(setReading).catch(() => {});
  }, []);

  useEffect(() => {
    load();
    refreshSide();
    api
      .scheduleGet()
      .then((s) => {
        setSchedule(s);
        if (s.at) setTime(s.at);
      })
      .catch(() => {});
    const un = api.onDailyFinished((r) => {
      setResult({ r, send: runningRef.current === "send" });
      runningRef.current = null;
      setRunning(null);
      refreshSide();
    });
    return () => {
      un.then((f) => f());
    };
  }, [load, refreshSide]);

  const edit = (f: (c: DailyConfig) => void) => {
    setDraft((d) => {
      if (!d) return d;
      const c = structuredClone(d);
      f(c);
      return c;
    });
    setDirty(true);
  };

  const save = async (c = draft): Promise<boolean> => {
    if (!c) return false;
    try {
      await load(await api.dailySave(c));
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
      setEmailMsg({ text: await api.emailTest(), ok: true });
    } catch (e) {
      setEmailMsg({ text: String(e), ok: false });
    }
    setTesting(false);
  };

  const run = async (send: boolean) => {
    if (running) return;
    if (dirty) await save();
    setResult(null);
    runningRef.current = send ? "send" : "preview";
    setRunning(runningRef.current);
    api.dailyRun(send).catch((e) => {
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
      setSchedule(await api.scheduleSet(on ? at : null));
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

  if (!draft) {
    return (
      <div className="daily">
        <section className="card">
          <h2>digest.toml has a problem</h2>
          <p className="fail">{state.error}</p>
          <button className="ghost" onClick={() => api.reveal(state.config_path)}>
            Show the file
          </button>
        </section>
      </div>
    );
  }

  const enabled = draft.sections;
  const disabled = ALL_SECTIONS.filter((s) => !enabled.includes(s));
  const move = (s: SectionKind, by: number) =>
    edit((c) => {
      const i = c.sections.indexOf(s);
      const j = i + by;
      if (i < 0 || j < 0 || j >= c.sections.length) return;
      [c.sections[i], c.sections[j]] = [c.sections[j], c.sections[i]];
    });
  const toggle = (s: SectionKind, on: boolean) =>
    edit((c) => {
      c.sections = on ? [...c.sections, s] : c.sections.filter((x) => x !== s);
    });
  const pending = reading.filter((e) => e.state === "pending").length;

  return (
    <div className="daily">
      <section className="card paper">
        <div className="paper-cover">{preview?.cover ? <img src={preview.cover} alt="" /> : <div className="cover-placeholder">D</div>}</div>
        <div className="paper-body">
          <h2>{draft.title}</h2>
          <p className="muted">
            {preview ? `Last built: ${preview.title.replace(/^.*— /, "")}` : "No edition built yet."}
            {preview && preview.outbox > 0 && ` · ${preview.outbox} waiting for your Kindle`}
            {pending > 0 && ` · ${pending} saved for the next edition`}
          </p>
          <div className="paper-actions">
            <button className="primary" disabled={!!running} onClick={() => run(true)}>
              {running === "send" ? "Building and sending…" : "Send now"}
            </button>
            <button className="ghost" disabled={!!running} onClick={() => run(false)}>
              {running === "preview" ? "Building preview…" : "Preview"}
            </button>
            {preview && (
              <button className="link" onClick={() => api.reveal(preview.file)}>
                Show file
              </button>
            )}
          </div>
          {running && <p className="muted small">Fetching every story and photo takes about a minute.</p>}
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
        </div>
      </section>

      <Readership />

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

      <section className="card">
        <h3>Every morning</h3>
        <div className="row-pair">
          <label className="check">
            <input type="checkbox" checked={!!schedule?.at} onChange={(e) => setScheduleOn(e.target.checked)} />
            Build the paper every day at
          </label>
          <input
            type="time"
            value={time}
            onChange={(e) => {
              setTime(e.target.value);
              if (schedule?.at) setScheduleOn(true, e.target.value);
            }}
          />
        </div>
        <p className="muted small">
          {schedule?.at
            ? `On. Deckle also keeps watching for your Kindle and sends the paper the moment you plug it in. A Mac asleep at ${schedule.at} builds it when it wakes.`
            : "Off. Turn it on and the paper waits for you every morning."}
        </p>
      </section>

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
        <label className="check">
          <input type="checkbox" checked={!!draft.email?.enabled} onChange={(e) => editEmail((m) => (m.enabled = e.target.checked))} />
          Email the paper to my Kindle
        </label>
        <div className="grid2">
          <label>
            <span className="label">Kindle address</span>
            <input value={draft.email?.to ?? ""} placeholder="you_ABC123@kindle.com" spellCheck={false} onChange={(e) => editEmail((m) => (m.to = e.target.value))} />
          </label>
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
        </div>
        <label className="check">
          <input type="checkbox" checked={!!draft.email?.usb_too} onChange={(e) => editEmail((m) => (m.usb_too = e.target.checked))} />
          Also send it over USB when the email goes out (two copies on the Kindle)
        </label>
        <div className="paper-actions">
          <button className="ghost" disabled={testing || !emailStatus?.configured || !emailStatus?.has_password} onClick={sendTest}>
            {testing ? "Sending…" : "Send a test"}
          </button>
          {emailStatus && !emailStatus.configured && <span className="muted small">Fill in the addresses and server first.</span>}
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
            <span className="label">Units</span>
            <select value={draft.units} onChange={(e) => edit((c) => (c.units = e.target.value as DailyConfig["units"]))}>
              <option value="imperial">°F, mph</option>
              <option value="metric">°C, km/h</option>
            </select>
          </label>
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

        <div className="label">Sections, in order</div>
        <ul className="sections">
          {enabled.map((s, i) => (
            <li key={s}>
              <label className="check">
                <input type="checkbox" checked onChange={() => toggle(s, false)} />
                {SECTION_LABEL[s]}
              </label>
              <span>
                <button className="icon small" aria-label="Move up" disabled={i === 0} onClick={() => move(s, -1)}>
                  <Icon name="arrow-up" size={14} />
                </button>
                <button className="icon small" aria-label="Move down" disabled={i === enabled.length - 1} onClick={() => move(s, 1)}>
                  <Icon name="arrow-down" size={14} />
                </button>
              </span>
            </li>
          ))}
          {disabled.map((s) => (
            <li key={s} className="off">
              <label className="check">
                <input type="checkbox" checked={false} onChange={() => toggle(s, true)} />
                {SECTION_LABEL[s]}
              </label>
            </li>
          ))}
        </ul>

        <label>
          <span className="label">Local news search</span>
          <input value={draft.local.google_news} placeholder="e.g. Moore County NC" onChange={(e) => edit((c) => (c.local.google_news = e.target.value))} />
        </label>

        <div className="label">Your feeds</div>
        <div className="feeds">
          {draft.feeds.map((f: FeedConfig, i: number) => (
            <div key={i} className="feed">
              <input value={f.name} placeholder="Section name" onChange={(e) => edit((c) => (c.feeds[i].name = e.target.value))} />
              <input value={f.url} placeholder="https://… (RSS or Atom)" spellCheck={false} onChange={(e) => edit((c) => (c.feeds[i].url = e.target.value))} />
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
    </div>
  );
}
