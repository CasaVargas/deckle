// SPDX-License-Identifier: GPL-3.0-or-later
// Your papers (many papers design §7): one card per paper, main first. The selected paper is the
// one every room below works on.
import { useEffect, useRef, useState } from "react";
import { anyKindleHint, deliveryLine, kindleLine, readerLine, slugFromTitle, typeLine } from "./papersView";
import type { PapersView } from "./types";

export default function Papers({
  view,
  selected,
  busy,
  onSelect,
  onPreview,
  onCreate,
}: {
  view: PapersView;
  selected: string;
  /** A build is running: Preview waits. */
  busy: boolean;
  onSelect: (slug: string) => void;
  onPreview: (slug: string) => void;
  /** Resolves to whether the paper was made (the form closes). */
  onCreate: (title: string, reader: string) => Promise<boolean>;
}) {
  const [adding, setAdding] = useState(false);
  const [title, setTitle] = useState("");
  const [reader, setReader] = useState("");
  const hint = anyKindleHint(view.papers);
  // A new or picked paper's card is scrolled into the row's view
  const selectedRef = useRef<HTMLElement | null>(null);
  useEffect(() => {
    selectedRef.current?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }, [selected, view]);
  const slug = slugFromTitle(title);

  return (
    <section className="papers" aria-label="Your papers">
      <div className="papers-head">
        <h3>Your papers</h3>
        {!adding && (
          <button className="ghost" onClick={() => setAdding(true)}>
            New paper
          </button>
        )}
      </div>
      <div className="paper-cards">
        {view.papers.map((p) => {
          const on = p.slug === selected;
          return (
            <article key={p.slug} ref={on ? selectedRef : undefined} className={`paper-card ${on ? "on" : ""}`} aria-current={on ? "true" : undefined}>
              <div className="paper-card-title">
                <strong>{p.title}</strong>
                {readerLine(p) && <span className="muted small"> {readerLine(p)}</span>}
              </div>
              <div className="paper-card-sections muted small" title={p.sections.join(" · ")}>
                {p.sections.join(" · ") || "No sections"}
              </div>
              <dl className="paper-card-lines small">
                <dt>Kindle</dt>
                <dd>{kindleLine(p, view.kindles)}</dd>
                <dt>Delivered</dt>
                <dd>{deliveryLine(p)}</dd>
                <dt>Type</dt>
                <dd>{typeLine(p)}</dd>
              </dl>
              <div className="paper-card-actions">
                <button className={on ? "primary" : "ghost"} disabled={on} onClick={() => onSelect(p.slug)}>
                  {on ? "Editing" : "Edit"}
                </button>
                <button className="ghost" disabled={busy} onClick={() => onPreview(p.slug)}>
                  Preview
                </button>
              </div>
            </article>
          );
        })}
        {adding && (
          <form
            className="paper-card new"
            onSubmit={async (e) => {
              e.preventDefault();
              if (await onCreate(title, reader)) {
                setAdding(false);
                setTitle("");
                setReader("");
              }
            }}
          >
            <label>
              <span className="label">Name</span>
              <input autoFocus value={title} placeholder="The Kid Courier" onChange={(e) => setTitle(e.target.value)} />
            </label>
            <label>
              <span className="label">For</span>
              <input value={reader} placeholder="Maya" onChange={(e) => setReader(e.target.value)} />
            </label>
            {slug && <p className="muted small">Saved as papers/{slug}.toml</p>}
            <div className="paper-card-actions">
              <button className="primary" type="submit" disabled={!slug}>
                Make it
              </button>
              <button className="ghost" type="button" onClick={() => setAdding(false)}>
                Cancel
              </button>
            </div>
          </form>
        )}
      </div>
      {hint && <p className="muted small">{hint}</p>}
      {view.notes.map((n) => (
        <p key={n} className="warn small">
          {n}
        </p>
      ))}
    </section>
  );
}
