// SPDX-License-Identifier: GPL-3.0-or-later
import { providerName } from "../aiView";
import type { SettingsPage } from "../settingsIndex";
import type { Settings } from "../types";
import { Switch } from "./Row";

interface Props {
  settings: Settings;
  set: <K extends keyof Settings>(k: K, v: Settings[K]) => void;
  onPage: (page: SettingsPage) => void;
}

/** Word Wise's example: two hard words, hinted when the level reaches them (Deckle's own sentence). */
function Example({ level, on }: { level: number; on: boolean }) {
  const hint = (word: string, gloss: string, from: number) => (on && level >= from ? <ruby>{word}<rt>{gloss}</rt></ruby> : word);
  return (
    <figure className="ww-example" aria-label="How Word Wise looks at this level">
      <p>
        The keeper's {hint("tenebrous", "dark, gloomy", 1)} halls ran on for miles, and every {hint("stalactite", "hanging stone point", 3)} dripped in time with the tide.
      </p>
      <figcaption>Example at this level</figcaption>
    </figure>
  );
}

/** The chapter recaps toggle's help (AI extras design §8). */
const RECAPS_HELP =
  "Adds a short AI-written recap at the end of each chapter. Uses the AI setting: with a service online the book's text is sent to it and billed to your key; on this Mac nothing leaves it.";

/** Reading aids (Settings redesign §6): Word Wise, X-Ray and chapter recaps, help that appears while you read. */
export default function ReadingAids({ settings, set, onPage }: Props) {
  const ai = settings.llm_provider !== "off";
  const where = providerName(settings);
  const recaps = settings.chapter_recaps || settings.book_summary || settings.series_recap;
  return (
    <div className="set-stack">
      <p className="overview-lead">Help that appears while you read. Word Wise and X-Ray need a book sent as a Kindle book; chapter recaps go to every e-reader.</p>

      <section className="set-card set-pad" aria-label="Word Wise">
        <div className="set-card-head">
          <h3 className="set-card-title">Word Wise</h3>
          <Switch label="Word Wise" checked={settings.word_wise} onChange={(on) => set("word_wise", on)} />
        </div>
        <div className="ww-body">
          <div className="ww-controls">
            <p className="muted">Short meanings above harder words, so you don't stop to look them up.</p>
            <label htmlFor="ww-level" className="tk-label">
              How many hints
            </label>
            <input id="ww-level" type="range" min={1} max={5} value={settings.word_wise_level} disabled={!settings.word_wise} onChange={(e) => set("word_wise_level", Number(e.target.value))} />
            <div className="range-ends">
              <span>Rarest words only</span>
              <span>Every hint</span>
            </div>
            <p className="muted small">At every hint, the Kindle's own slider decides how many show. The first book downloads a 12 MB dictionary.</p>
          </div>
          <Example level={settings.word_wise_level} on={settings.word_wise} />
        </div>
      </section>

      <section className="set-card set-pad" aria-label="X-Ray">
        <div className="set-card-head">
          <h3 className="set-card-title">X-Ray</h3>
          <Switch label="X-Ray" checked={settings.xray} onChange={(on) => set("xray", on)} />
        </div>
        <p className="muted">Who's who and what's what: tap a name in the book to see where it comes up.</p>
        <p className="muted">
          <strong>Who finds the people and places:</strong>{" "}
          {ai ? (
            <>
              the AI model, {where}. Better character lists.
            </>
          ) : (
            <>Deckle's own lists of names; nothing goes online. An AI model writes better ones.</>
          )}{" "}
          <button className="link-action inline" onClick={() => onPage("ai")}>
            {ai ? "Change it under AI →" : "Choose one under AI →"}
          </button>
        </p>
        <label className="tick-row">
          <input type="checkbox" checked={settings.wikipedia} disabled={!settings.xray} onChange={(e) => set("wikipedia", e.target.checked)} />
          <span>
            <strong>Add Wikipedia summaries</strong> <span className="muted">for real people and places</span> <span className="tag">Online</span>
          </span>
        </label>
      </section>

      <section className="set-card set-pad" aria-label="Chapter recaps">
        <div className="set-card-head">
          <h3 className="set-card-title">
            Chapter recaps <span className="tag">AI</span>
          </h3>
          <Switch label="Chapter recaps" checked={settings.chapter_recaps} onChange={(on) => set("chapter_recaps", on)} />
        </div>
        <p className="muted">{RECAPS_HELP}</p>
        <label className="tick-row">
          <input type="checkbox" checked={settings.book_summary} onChange={(e) => set("book_summary", e.target.checked)} />
          <span>
            <strong>Add a book summary</strong> <span className="muted">on a page after the last chapter, written from the recaps. It gives away the ending.</span>
          </span>
        </label>
        <label className="tick-row">
          <input type="checkbox" checked={settings.series_recap} onChange={(e) => set("series_recap", e.target.checked)} />
          <span>
            <strong>Recap the series so far</strong> <span className="muted">on a page before chapter 1 of a later book, from the earlier books Deckle recapped.</span>
          </span>
        </label>
        {recaps && !ai && (
          <p className="muted small">
            Needs an AI model: none is chosen yet.{" "}
            <button className="link-action inline" onClick={() => onPage("ai")}>
              Choose one under AI →
            </button>
          </p>
        )}
      </section>
    </div>
  );
}
