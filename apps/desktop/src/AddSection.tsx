// SPDX-License-Identifier: GPL-3.0-or-later
// + Add a section (Daily tab redesign §5): a menu of what can go in the paper, each asking only
// for what it needs. The new row opens in Sections.
import { useEffect, useRef, useState } from "react";
import { addBackPage, addFeed, addFollow, addSubscription, addTopic, addTown, backPagesOff, topicsLeft } from "./sectionAdds";
import type { Added } from "./sectionAdds";
import Icon from "./Icon";
import type { DailyConfig } from "./types";

type Stage = "menu" | "follow" | "topic" | "feed" | "town" | "subscription" | "back";

const CHOICES: { stage: Exclude<Stage, "menu">; title: string; what: string }[] = [
  { stage: "follow", title: "Something you follow", what: "A team, a person, a topic: Deckle searches the news for it" },
  { stage: "topic", title: "A news topic", what: "World, Business, Science…" },
  { stage: "feed", title: "A website or feed", what: "Its address" },
  { stage: "town", title: "Another town", what: "A relative's, or a trip" },
  { stage: "subscription", title: "A paper you subscribe to", what: "Signs in so its stories come whole" },
  { stage: "back", title: "The back pages", what: "Weather, puzzles, lottery, horoscope…" },
];

export default function AddSection({ draft, onAdd }: { draft: DailyConfig; onAdd: (a: Added) => void }) {
  const [stage, setStage] = useState<Stage | null>(null);
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const box = useRef<HTMLDivElement | null>(null);

  const close = () => {
    setStage(null);
    setName("");
    setUrl("");
  };
  const add = (a: Added | null) => {
    if (!a) return;
    onAdd(a);
    close();
  };

  useEffect(() => {
    if (stage === null) return;
    const away = (e: MouseEvent) => {
      if (box.current && !box.current.contains(e.target as Node)) close();
    };
    const esc = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("mousedown", away);
    window.addEventListener("keydown", esc);
    return () => {
      window.removeEventListener("mousedown", away);
      window.removeEventListener("keydown", esc);
    };
  }, [stage]);

  const back = (
    <button type="button" className="quiet add-back" onClick={() => setStage("menu")}>
      <Icon name="arrow-left" size={13} /> All choices
    </button>
  );
  const form = (label: string, onSubmit: () => void, fields: React.ReactNode, ready: boolean) => (
    <form
      className="add-form"
      aria-label={label}
      onSubmit={(e) => {
        e.preventDefault();
        if (ready) onSubmit();
      }}
    >
      {back}
      <strong>{label}</strong>
      {fields}
      <button className="primary" type="submit" disabled={!ready}>
        Add
      </button>
    </form>
  );

  const topics = topicsLeft(draft);
  const backPages = backPagesOff(draft);

  return (
    <div className="add-section row-menu" ref={box}>
      <button className="primary with-icon" aria-haspopup="menu" aria-expanded={stage !== null} onClick={() => (stage === null ? setStage("menu") : close())}>
        <Icon name="plus" size={14} /> Add a section
      </button>
      {stage !== null && (
        <div className="menu add-menu">
          {stage === "menu" &&
            CHOICES.map((c) => (
              <button key={c.stage} role="menuitem" onClick={() => setStage(c.stage)}>
                <span className="add-title">{c.title}</span>
                <span className="muted small">{c.what}</span>
              </button>
            ))}
          {stage === "follow" &&
            form(
              "Something you follow",
              () => add(addFollow(draft, name)),
              <label>
                <span className="label">Name</span>
                <input autoFocus value={name} placeholder="Carolina Panthers" onChange={(e) => setName(e.target.value)} />
              </label>,
              !!name.trim(),
            )}
          {stage === "feed" &&
            form(
              "A website or feed",
              () => add(addFeed(draft, name, url)),
              <>
                <label>
                  <span className="label">Address</span>
                  <input autoFocus value={url} placeholder="https://… (RSS or Atom)" spellCheck={false} onChange={(e) => setUrl(e.target.value)} />
                </label>
                <label>
                  <span className="label">Name</span>
                  <input value={name} placeholder="The site's name" onChange={(e) => setName(e.target.value)} />
                </label>
              </>,
              !!url.trim(),
            )}
          {stage === "town" &&
            form(
              "Another town",
              () => add(addTown(draft, name)),
              <label>
                <span className="label">Town</span>
                <input autoFocus value={name} placeholder="Asheville, NC" onChange={(e) => setName(e.target.value)} />
              </label>,
              !!name.trim(),
            )}
          {stage === "subscription" &&
            form(
              "A paper you subscribe to",
              () => add(addSubscription(draft, name)),
              <label>
                <span className="label">Its website</span>
                <input autoFocus value={name} placeholder="thepilot.com" spellCheck={false} onChange={(e) => setName(e.target.value)} />
              </label>,
              !!name.trim(),
            )}
          {stage === "topic" && (
            <div className="add-form">
              {back}
              <strong>A news topic</strong>
              {topics.length === 0 ? (
                <p className="muted small">Every topic is in the paper.</p>
              ) : (
                <div className="places">
                  {topics.map((t) => (
                    <button key={t.slug} className="chip" onClick={() => add(addTopic(draft, t.slug))}>
                      {t.title}
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}
          {stage === "back" && (
            <div className="add-form">
              {back}
              <strong>The back pages</strong>
              {backPages.length === 0 ? (
                <p className="muted small">They're all in the paper.</p>
              ) : (
                backPages.map((b) => (
                  <button key={b.kind} className="add-choice" onClick={() => add(addBackPage(draft, b.kind))}>
                    <span className="add-title">{b.title}</span>
                    <span className="muted small">{b.what}</span>
                  </button>
                ))
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
