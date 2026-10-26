// SPDX-License-Identifier: GPL-3.0-or-later
// The paper's own settings (Daily tab redesign §6): a sheet from the right, opened from the paper
// switcher. Name, who it's for, its Kindle and units, the masthead as cards in each face, how it
// reads, the AI editor and learning. Everything saves as it changes.
import { useEffect, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import DailyEditor from "./DailyEditor";
import Icon from "./Icon";
import { loadMastheadFonts } from "./mastheadFonts";
import { FACES, faceFamily, fontFileName, mastheadChoice, mastheadValue } from "./mastheadView";
import type { Face } from "./mastheadView";
import type { DailyConfig, KnownKindle } from "./types";

type Edit = (f: (c: DailyConfig) => void, how?: "now" | "typing") => void;

export default function PaperSheet({
  draft,
  kindles,
  configPath,
  edit,
  flush,
  onClose,
}: {
  draft: DailyConfig;
  kindles: KnownKindle[];
  configPath: string;
  edit: Edit;
  /** Saves typing that's waiting for a pause, as a field is left. */
  flush: () => void;
  onClose: () => void;
}) {
  const [missing, setMissing] = useState<string[]>([]);
  const sheet = useRef<HTMLElement | null>(null);
  const choice = mastheadChoice(draft.masthead);
  const title = draft.title.trim() || "The Deckle Daily";

  useEffect(loadMastheadFonts, []);
  // Which faces can't draw the name, checked once typing pauses
  useEffect(() => {
    const t = setTimeout(() => {
      api
        .mastheadMissing(title)
        .then(setMissing)
        .catch(() => setMissing([]));
    }, 300);
    return () => clearTimeout(t);
  }, [title]);
  useEffect(() => {
    const esc = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", esc);
    sheet.current?.focus();
    return () => window.removeEventListener("keydown", esc);
  }, [onClose]);

  const card = (f: Face) => {
    const on = choice === f.key;
    const cant = missing.includes(f.key);
    return (
      <button
        key={f.key}
        role="radio"
        aria-checked={on}
        className={`face-card ${on ? "on" : ""}`}
        onClick={() => edit((c) => (c.masthead = mastheadValue(f.key)), "now")}
      >
        <span className="face-sample" style={{ fontFamily: faceFamily(f.key) }}>
          {title}
        </span>
        <span className="muted small">
          {f.label} · {cant ? <span className="warn">can't draw every letter</span> : f.note}
        </span>
      </button>
    );
  };

  return (
    <div className="sheet-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <section className="sheet paper-sheet" role="dialog" aria-label="Paper settings" tabIndex={-1} ref={sheet}>
        <div className="sheet-head">
          <h2 className="daily-h2">{draft.title || "Your paper"}</h2>
          <button className="icon" aria-label="Close" onClick={onClose}>
            <Icon name="x" size={16} />
          </button>
        </div>

        <div className="grid2">
          <label>
            <span className="label">Name</span>
            <input value={draft.title} onChange={(e) => edit((c) => (c.title = e.target.value))} onBlur={flush} />
          </label>
          <label>
            <span className="label">For</span>
            <input value={draft.reader ?? ""} placeholder="Who reads it" onChange={(e) => edit((c) => (c.reader = e.target.value))} onBlur={flush} />
          </label>
          <label>
            <span className="label">Goes to</span>
            <select value={draft.kindle ?? ""} onChange={(e) => edit((c) => (c.kindle = e.target.value), "now")}>
              <option value="">Any Kindle</option>
              {kindles.map((k) => (
                <option key={k.sync_id} value={k.name}>
                  {k.name}
                </option>
              ))}
              {draft.kindle && !kindles.some((k) => k.name === draft.kindle) && <option value={draft.kindle}>{draft.kindle} (not named yet)</option>}
            </select>
            {kindles.length === 0 && <span className="muted small">Name a Kindle on the Kindle tab to choose it.</span>}
          </label>
          <label>
            <span className="label">Units</span>
            <select value={draft.units} onChange={(e) => edit((c) => (c.units = e.target.value as DailyConfig["units"]), "now")}>
              <option value="imperial">°F, mph</option>
              <option value="metric">°C, km/h</option>
            </select>
          </label>
        </div>

        <div className="sheet-group" role="radiogroup" aria-label="Masthead">
          <h3>Masthead</h3>
          <span className="label">Blackletter</span>
          <div className="face-cards">{FACES.filter((f) => f.group === "blackletter").map(card)}</div>
          <span className="label">No blackletter</span>
          <div className="face-cards">
            {FACES.filter((f) => f.group === "plain").map(card)}
            {choice === "file" && draft.masthead && (
              <button role="radio" aria-checked className="face-card on">
                <span className="face-sample file">{fontFileName(draft.masthead)}</span>
                <span className="muted small">Your font file</span>
              </button>
            )}
          </div>
          <button
            className="link"
            onClick={async () => {
              const picked = await openDialog({ multiple: false, filters: [{ name: "TrueType font", extensions: ["ttf"] }] });
              if (typeof picked === "string") edit((c) => (c.masthead = picked), "now");
            }}
          >
            Use a font file…
          </button>
        </div>

        <div className="sheet-group">
          <h3>Reading it</h3>
          <label className="check">
            <input type="checkbox" checked={draft.full_text} onChange={(e) => edit((c) => (c.full_text = e.target.checked), "now")} />
            Whole stories, not summaries
          </label>
          <label className="check sub">
            <input type="checkbox" checked={draft.photos} disabled={!draft.full_text} onChange={(e) => edit((c) => (c.photos = e.target.checked), "now")} />
            With their photos
          </label>
          <label className="check">
            <input type="checkbox" checked={!!draft.large_print} onChange={(e) => edit((c) => (c.large_print = e.target.checked), "now")} />
            Large print
          </label>
          <p className="muted small">Nothing smaller than the body text, bigger headlines and one photo a story. Set the text size on the Kindle itself (Aa).</p>
        </div>

        <div className="sheet-group">
          <DailyEditor value={draft.llm} onChange={(value) => edit((c) => void (c.llm = value), "now")} />
        </div>

        <div className="sheet-group">
          <h3>Learning what you like</h3>
          <label className="check">
            <input
              type="checkbox"
              checked={draft.learning?.enabled ?? true}
              onChange={(e) => edit((c) => (c.learning = { window_days: 14, min_stories: 3, nudges: false, ...(c.learning ?? {}), enabled: e.target.checked }), "now")}
            />
            Notice what I tap and highlight in the paper, on this Mac
          </label>
          <label className="check sub">
            <input
              type="checkbox"
              checked={(draft.learning?.enabled ?? true) && (draft.learning?.nudges ?? false)}
              disabled={!(draft.learning?.enabled ?? true)}
              onChange={(e) => edit((c) => (c.learning = { enabled: true, window_days: 14, min_stories: 3, ...(c.learning ?? {}), nudges: e.target.checked }), "now")}
            />
            Suggest new things to follow, as pages in the paper
          </label>
        </div>

        <p className="muted small">
          Saved as you go to <span className="mono">{configPath}</span>; your own comments in it are kept.{" "}
          <button className="link" onClick={() => api.reveal(configPath)}>
            Show the file
          </button>
        </p>
      </section>
    </div>
  );
}
