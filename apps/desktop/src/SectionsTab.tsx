// SPDX-License-Identifier: GPL-3.0-or-later
// Sections (Daily tab redesign §4): the page plan and where each section comes from, as one
// list. A row opens beneath itself to its sources; every change saves as it's made.
import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import AddSection from "./AddSection";
import Icon from "./Icon";
import { listText, removeInterest, setList, setQuery } from "./interests";
import { step } from "./pagePlan";
import Shelf from "./Shelf";
import SubscriptionsCard from "./SubscriptionsCard";
import { topicTitle } from "./topics";
import YourDay from "./YourDay";
import type { DailyConfig, NextPaper, PagePlanRow, PlaceNews, PlaceRow } from "./types";
import { SIGNS } from "./types";

/** How an edit saves: switches and menus at once, typing once it pauses. */
export type Change = (f: (c: DailyConfig) => DailyConfig, how?: "now" | "typing") => void;

const GROUP_TIP: Partial<Record<PagePlanRow["kind"], string>> = {
  feeds: "Switches all your feeds",
  interests: "Switches everything you follow",
  subscribed: "Switches all your subscribed papers",
};

/** A row's identity that survives renaming it: its kind and where it is in the file, so an open
 * row stays open when its name saves. */
export function rowId(r: PagePlanRow): string {
  if (r.feed !== null) return `feeds:${r.feed}`;
  if (r.interest !== null) return `interests:${r.interest}`;
  if (r.place !== null) return `town:${r.place}`;
  if (r.subscribed) return `subscribed:${r.subscribed.site}:${r.subscribed.slug}`;
  return r.kind;
}

const LOTTERY: [string, string][] = [
  ["powerball", "Powerball"],
  ["megamillions", "Mega Millions"],
];

export default function SectionsTab({
  next,
  draft,
  locked,
  saving,
  change,
  flush,
  saveNow,
  onMove,
  onBudget,
  onToggle,
  onReading,
  onNotice,
  footer,
}: {
  next: NextPaper | null;
  draft: DailyConfig;
  /** A move or budget waits for the plan to catch up with the last save. */
  locked: boolean;
  saving: boolean;
  change: Change;
  /** Saves typing that's waiting for a pause, as a field is left. */
  flush: () => void;
  /** Saves a whole config at once (the cards that keep their own drafts). */
  saveNow: (c: DailyConfig) => Promise<boolean>;
  onMove: (from: number, to: number) => void;
  onBudget: (row: PagePlanRow, n: number) => void;
  onToggle: (row: PagePlanRow, on: boolean) => void;
  onReading: () => void;
  onNotice: (m: string) => void;
  /** What's left of the paper's own settings, until they move to their sheet. */
  footer?: ReactNode;
}) {
  const [open, setOpen] = useState<string | null>(null);
  // A row just added: its first field takes focus when it appears
  const [fresh, setFresh] = useState<string | null>(null);
  const [dragging, setDragging] = useState<number | null>(null);
  const [over, setOver] = useState<number | null>(null);
  if (!next) return null;
  const rows = next.plan;

  const body = (r: PagePlanRow): ReactNode => {
    switch (r.kind) {
      case "reading":
        return <ReadingBody draft={draft} change={change} flush={flush} onReading={onReading} onNotice={onNotice} />;
      case "weather":
        return (
          <>
            <PlaceField draft={draft} change={change} onNotice={onNotice} note="Local news searches it too." />
            <label className="sec-field">
              <span className="label">Units</span>
              <select value={draft.units} onChange={(e) => change((c) => ({ ...c, units: e.target.value as DailyConfig["units"] }))}>
                <option value="imperial">°F, mph</option>
                <option value="metric">°C, km/h</option>
              </select>
            </label>
            <YourDay
              day={draft.day}
              emailOn={draft.email?.enabled ?? false}
              onSave={(day) => saveNow({ ...draft, day })}
            />
          </>
        );
      case "local":
        return r.place === null ? (
          <LocalBody draft={draft} change={change} flush={flush} onNotice={onNotice} />
        ) : (
          <TownBody index={r.place} draft={draft} change={change} flush={flush} fresh={fresh === rowId(r)} onRemove={() => setOpen(null)} />
        );
      case "interests":
        return r.interest === null ? (
          <p className="muted small">Turn it on, then add something to follow.</p>
        ) : (
          <InterestBody index={r.interest} draft={draft} change={change} flush={flush} fresh={fresh === rowId(r)} onRemove={() => setOpen(null)} />
        );
      case "feeds":
        return r.feed === null ? (
          <p className="muted small">Turn it on, then add a website or a news topic.</p>
        ) : (
          <FeedBody index={r.feed} draft={draft} change={change} flush={flush} fresh={fresh === rowId(r)} onRemove={() => setOpen(null)} />
        );
      case "subscribed":
        return (
          <SubscriptionsCard
            subscriptions={draft.subscriptions ?? []}
            config={draft}
            dirty={false}
            onChange={(subs) => change((c) => ({ ...c, subscriptions: subs }), "typing")}
            onConfig={(f) => change(f)}
            onSave={() => saveNow(draft)}
          />
        );
      case "shelf":
        return <Shelf inPaper={draft.sections.includes("shelf")} />;
      case "lottery":
        return (
          <div className="sec-checks">
            {LOTTERY.map(([k, label]) => (
              <label key={k} className="check">
                <input
                  type="checkbox"
                  checked={draft.lottery.games.includes(k)}
                  onChange={(e) =>
                    change((c) => ({ ...c, lottery: { games: e.target.checked ? [...c.lottery.games, k] : c.lottery.games.filter((g) => g !== k) } }))
                  }
                />
                {label}
              </label>
            ))}
          </div>
        );
      case "horoscope":
        return (
          <>
            <div className="places">
              {SIGNS.map((s) => (
                <button
                  key={s}
                  className={`chip ${draft.horoscope.signs.includes(s) ? "on" : ""}`}
                  aria-pressed={draft.horoscope.signs.includes(s)}
                  onClick={() =>
                    change((c) => ({
                      ...c,
                      horoscope: { signs: c.horoscope.signs.includes(s) ? c.horoscope.signs.filter((x) => x !== s) : [...c.horoscope.signs, s] },
                    }))
                  }
                >
                  {s[0].toUpperCase() + s.slice(1)}
                </button>
              ))}
            </div>
            <p className="muted small">{draft.horoscope.signs.length === 0 ? "None chosen: all twelve." : "Only these signs."}</p>
          </>
        );
      default:
        return <p className="muted small">Nothing to set: it's either in the paper or not.</p>;
    }
  };

  const row = (r: PagePlanRow, i: number, on: boolean) => {
    const k = on ? rowId(r) : `off:${rowId(r)}`;
    const isOpen = open === k;
    const movable = on && !locked && r.place === null;
    const keyMove = (e: React.KeyboardEvent) => {
      if (!movable || !e.altKey || (e.key !== "ArrowUp" && e.key !== "ArrowDown")) return;
      e.preventDefault();
      const to = step(rows, i, e.key === "ArrowUp" ? -1 : 1);
      if (to !== null) onMove(i, to);
    };
    return (
      <li
        key={k}
        className={`sec-row ${on ? "" : "off"} ${isOpen ? "open" : ""} ${r.place !== null ? "town" : ""} ${over === i && dragging !== i ? "over" : ""} ${dragging === i ? "dragging" : ""}`}
        onDragOver={(e) => {
          if (!on) return;
          e.preventDefault();
          setOver(i);
        }}
        onDragLeave={() => setOver((o) => (o === i ? null : o))}
        onDrop={(e) => {
          e.preventDefault();
          const from = Number(e.dataTransfer.getData("text/plain"));
          setDragging(null);
          setOver(null);
          if (on && !locked && !Number.isNaN(from) && from !== i) onMove(from, i);
        }}
        onDragEnd={() => {
          setDragging(null);
          setOver(null);
        }}
      >
        {/* Only the head drags: WebKit won't focus a field inside a draggable element. */}
        <div
          className="sec-head"
          draggable={movable}
          onDragStart={(e) => {
            setDragging(i);
            e.dataTransfer.effectAllowed = "move";
            e.dataTransfer.setData("text/plain", String(i));
          }}
        >
          <span className="handle" aria-hidden="true">
            {movable ? "⋮⋮" : r.place !== null ? "↳" : ""}
          </span>
          <button
            className="sec-open"
            aria-expanded={isOpen}
            title={movable ? "Alt+↑ or Alt+↓ moves it" : undefined}
            onClick={() => setOpen(isOpen ? null : k)}
            onKeyDown={keyMove}
          >
            <strong>{r.name}</strong>
            <span className="muted small">{r.detail}</span>
          </button>
          {on && r.items !== null ? (
            <select aria-label={`${r.name}: stories`} disabled={locked} value={r.items} onChange={(e) => onBudget(r, Number(e.target.value))}>
              {Array.from({ length: 20 }, (_, n) => n + 1).map((n) => (
                <option key={n} value={n}>
                  {r.kind === "reading" ? `up to ${n}` : `${n} ${n === 1 ? "story" : "stories"}`}
                </option>
              ))}
            </select>
          ) : (
            <span className="budget muted small">{on ? r.budget : ""}</span>
          )}
          {r.place === null ? (
            <input
              className="switch"
              type="checkbox"
              aria-label={`${r.name} in the paper`}
              title={GROUP_TIP[r.kind]}
              checked={on}
              disabled={locked}
              onChange={() => onToggle(r, !on)}
            />
          ) : (
            <span />
          )}
          <button className="icon small" aria-label={isOpen ? `Close ${r.name}` : `Open ${r.name}`} onClick={() => setOpen(isOpen ? null : k)}>
            <Icon name={isOpen ? "chevron-down" : "chevron-right"} size={14} />
          </button>
        </div>
        {isOpen && (
          <div className="sec-body">
            {body(r)}
            {movable && (
              <div className="sec-move">
                <button className="quiet" disabled={step(rows, i, -1) === null} onClick={() => onMove(i, step(rows, i, -1)!)}>
                  <Icon name="arrow-up" size={13} /> Move up
                </button>
                <button className="quiet" disabled={step(rows, i, 1) === null} onClick={() => onMove(i, step(rows, i, 1)!)}>
                  <Icon name="arrow-down" size={13} /> Move down
                </button>
              </div>
            )}
          </div>
        )}
      </li>
    );
  };

  return (
    <div className="sections-tab">
      <div className="sections-head">
        <div className="grow">
          <h2 className="daily-h2">What's in the paper</h2>
          <p className="muted small">
            In the order you'll read it, about {next.pages} Kindle pages. Changes save as you make them
            {saving ? " · Saving…" : ""}.
          </p>
        </div>
        <AddSection
          draft={draft}
          onAdd={(a) => {
            change(() => a.cfg);
            setOpen(a.row);
            setFresh(a.row);
          }}
        />
      </div>
      <ol className="sec-list" aria-label="Sections">
        {rows.map((r, i) => row(r, i, true))}
        {next.off.length > 0 && (
          <li className="sec-divider" aria-hidden="true">
            Not in the paper
          </li>
        )}
        {next.off.map((r, i) => row(r, i, false))}
      </ol>
      {footer}
    </div>
  );
}

/** A text field that keeps its own text and commits on leaving it or Enter: for fields the
 * config normalises (comma lists), where saving mid-typing would eat what's being typed. */
function CommitField({ value, onCommit, ...rest }: { value: string; onCommit: (v: string) => void } & Omit<React.InputHTMLAttributes<HTMLInputElement>, "value" | "onChange">) {
  const [text, setText] = useState(value);
  useEffect(() => setText(value), [value]);
  return (
    <input
      {...rest}
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={() => text !== value && onCommit(text)}
      onKeyDown={(e) => {
        if (e.key === "Enter") (e.target as HTMLInputElement).blur();
      }}
    />
  );
}

/** The paper's town: the weather's place and Local's, found by name. */
function PlaceField({ draft, change, onNotice, note }: { draft: DailyConfig; change: Change; onNotice: (m: string) => void; note: string }) {
  const [query, setQuery] = useState("");
  const [found, setFound] = useState<PlaceRow[] | null>(null);
  const find = async () => {
    if (!query.trim()) return;
    setFound(null);
    try {
      setFound(await api.geocode(query));
    } catch (e) {
      onNotice(String(e));
      setFound([]);
    }
  };
  return (
    <div className="sec-field">
      <span className="label">Your town</span>
      <div className="place">
        <span>
          {draft.place || <em>Not set</em>}
          {draft.latitude != null && <span className="muted small"> · {draft.latitude.toFixed(2)}, {draft.longitude?.toFixed(2)}</span>}
        </span>
        <form
          className="save-row"
          onSubmit={(e) => {
            e.preventDefault();
            find();
          }}
        >
          <input placeholder="Town, state (e.g. Carthage, NC)" aria-label="Find a town" value={query} onChange={(e) => setQuery(e.target.value)} />
          <button className="ghost" type="submit">
            Find
          </button>
        </form>
        {found && (
          <div className="places">
            {found.length === 0 && <span className="muted small">No matches.</span>}
            {found.map((p) => (
              <button
                key={`${p.latitude},${p.longitude}`}
                className="chip"
                onClick={() => {
                  change((c) => ({ ...c, place: p.label, latitude: Math.round(p.latitude * 100) / 100, longitude: Math.round(p.longitude * 100) / 100 }));
                  setFound(null);
                  setQuery("");
                }}
              >
                {p.label}
              </button>
            ))}
          </div>
        )}
      </div>
      <span className="muted small">{note}</span>
    </div>
  );
}

function LocalBody({ draft, change, flush, onNotice }: { draft: DailyConfig; change: Change; flush: () => void; onNotice: (m: string) => void }) {
  const search = draft.local.google_news;
  const [own, setOwn] = useState(!!search.trim() && search.trim().toLowerCase() !== draft.place.trim().toLowerCase());
  return (
    <>
      <PlaceField draft={draft} change={change} onNotice={onNotice} note="The weather uses it too." />
      {own ? (
        <label className="sec-field">
          <span className="label">Google News search</span>
          <input
            value={search}
            placeholder="e.g. Moore County NC"
            onChange={(e) => change((c) => ({ ...c, local: { ...c.local, google_news: e.target.value } }), "typing")}
            onBlur={flush}
          />
        </label>
      ) : (
        <button className="link" onClick={() => setOwn(true)}>
          Search Google News for something else
        </button>
      )}
    </>
  );
}

function TownBody({ index, draft, change, flush, fresh, onRemove }: { index: number; draft: DailyConfig; change: Change; flush: () => void; fresh: boolean; onRemove: () => void }) {
  const p = draft.local.places?.[index];
  if (!p) return null;
  const set = (f: (x: PlaceNews) => PlaceNews) =>
    change((c) => ({ ...c, local: { ...c.local, places: (c.local.places ?? []).map((x, i) => (i === index ? f(x) : x)) } }), "typing");
  return (
    <>
      <div className="grid2">
        <label>
          <span className="label">Name</span>
          <input autoFocus={fresh} value={p.name} placeholder="Asheville, NC" onChange={(e) => set((x) => ({ ...x, name: e.target.value }))} onBlur={flush} />
        </label>
        <label>
          <span className="label">Google News search</span>
          <input value={p.google_news} placeholder={p.name || "Same as the name"} onChange={(e) => set((x) => ({ ...x, google_news: e.target.value }))} onBlur={flush} />
        </label>
      </div>
      <p className="muted small">Its own section after Local. The weather stays on your town.</p>
      <button
        className="quiet"
        onClick={() => {
          onRemove();
          change((c) => ({ ...c, local: { ...c.local, places: (c.local.places ?? []).filter((_, i) => i !== index) } }));
        }}
      >
        Remove this town
      </button>
    </>
  );
}

function InterestBody({ index, draft, change, flush, fresh, onRemove }: { index: number; draft: DailyConfig; change: Change; flush: () => void; fresh: boolean; onRemove: () => void }) {
  const it = draft.interests?.[index];
  if (!it) return null;
  return (
    <>
      <div className="grid2">
        <label>
          <span className="label">Name</span>
          <input
            autoFocus={fresh}
            value={it.name}
            placeholder="Carolina Panthers"
            onChange={(e) => change((c) => ({ ...c, interests: (c.interests ?? []).map((x, i) => (i === index ? { ...x, name: e.target.value } : x)) }), "typing")}
            onBlur={flush}
          />
        </label>
        <label>
          <span className="label">Its own search</span>
          <CommitField
            value={it.query ?? ""}
            placeholder={`Instead of "${it.name || "its name"}"`}
            title="Your own Google News search instead of the quoted name; OR and -word work"
            onCommit={(v) => change((c) => setQuery(c, index, v))}
          />
        </label>
        <label>
          <span className="label">Also</span>
          <CommitField value={listText(it.also)} placeholder="Bryce Young, Dave Canales" onCommit={(v) => change((c) => setList(c, index, "also", v))} />
        </label>
        <label>
          <span className="label">Leave out</span>
          <CommitField value={listText(it.not)} placeholder="Florida Panthers, hockey" onCommit={(v) => change((c) => setList(c, index, "not", v))} />
        </label>
      </div>
      <button
        className="quiet"
        onClick={() => {
          onRemove();
          change((c) => removeInterest(c, index));
        }}
      >
        Stop following {it.name || "this"}
      </button>
    </>
  );
}

function FeedBody({ index, draft, change, flush, fresh, onRemove }: { index: number; draft: DailyConfig; change: Change; flush: () => void; fresh: boolean; onRemove: () => void }) {
  const f = draft.feeds[index];
  if (!f) return null;
  const set = (p: Partial<typeof f>) => change((c) => ({ ...c, feeds: c.feeds.map((x, i) => (i === index ? { ...x, ...p } : x)) }), "typing");
  return (
    <>
      {f.topic ? (
        <p className="muted small">A Google News topic: {topicTitle(f.topic)}.</p>
      ) : (
        <div className="grid2">
          <label>
            <span className="label">Name</span>
            <input autoFocus={fresh} value={f.name} placeholder="Section name" onChange={(e) => set({ name: e.target.value })} onBlur={flush} />
          </label>
          <label>
            <span className="label">Address</span>
            <input value={f.url} placeholder="https://… (RSS or Atom)" spellCheck={false} onChange={(e) => set({ url: e.target.value })} onBlur={flush} />
          </label>
        </div>
      )}
      <button
        className="quiet"
        onClick={() => {
          onRemove();
          change((c) => ({ ...c, feeds: c.feeds.filter((_, i) => i !== index) }));
        }}
      >
        Remove {f.name || "this feed"}
      </button>
    </>
  );
}

function ReadingBody({
  draft,
  change,
  flush,
  onReading,
  onNotice,
}: {
  draft: DailyConfig;
  change: Change;
  flush: () => void;
  onReading: () => void;
  onNotice: (m: string) => void;
}) {
  const [copied, setCopied] = useState(false);
  return (
    <>
      <label className="sec-field inline">
        Articles over
        <input
          type="number"
          min={500}
          step={500}
          value={draft.reading.long_read_words}
          onChange={(e) => change((c) => ({ ...c, reading: { ...c.reading, long_read_words: Math.max(500, Number(e.target.value) || 500) } }), "typing")}
          onBlur={flush}
        />
        words come as their own book.
      </label>
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
          <p className="muted small">Add it as a bookmark; clicking it saves the page you're on (while the morning schedule is on).</p>
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
      <button className="link" onClick={onReading}>
        What's saved, on Reading
      </button>
    </>
  );
}
