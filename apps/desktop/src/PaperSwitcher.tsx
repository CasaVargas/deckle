// SPDX-License-Identifier: GPL-3.0-or-later
// The paper switcher (Daily tab redesign §2): the selected paper as a menu button, listing every
// paper, then Paper settings and New paper. Picking a paper switches every tab to it.
import { useEffect, useRef, useState } from "react";
import Icon from "./Icon";
import { kindleLine, readerLine, slugFromTitle } from "./papersView";
import type { PapersView } from "./types";

export default function PaperSwitcher({
  view,
  selected,
  onSelect,
  onSettings,
  onCreate,
}: {
  view: PapersView;
  selected: string;
  onSelect: (slug: string) => void;
  onSettings: () => void;
  /** Resolves to whether the paper was made (the form closes). */
  onCreate: (title: string, reader: string) => Promise<boolean>;
}) {
  const [open, setOpen] = useState(false);
  const [adding, setAdding] = useState(false);
  const [title, setTitle] = useState("");
  const [reader, setReader] = useState("");
  const box = useRef<HTMLDivElement | null>(null);
  const current = view.papers.find((p) => p.slug === selected) ?? view.papers[0];
  const slug = slugFromTitle(title);

  // A click outside, or Esc, closes the menu and the new-paper form
  useEffect(() => {
    if (!open && !adding) return;
    const away = (e: MouseEvent) => {
      if (box.current && !box.current.contains(e.target as Node)) {
        setOpen(false);
        setAdding(false);
      }
    };
    const esc = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setOpen(false);
        setAdding(false);
      }
    };
    window.addEventListener("mousedown", away);
    window.addEventListener("keydown", esc);
    return () => {
      window.removeEventListener("mousedown", away);
      window.removeEventListener("keydown", esc);
    };
  }, [open, adding]);

  return (
    <div className="paper-switcher row-menu" ref={box}>
      <button className="paper-switch" aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <span className="paper-switch-name">{current?.title ?? "Your paper"}</span>
        {current && readerLine(current) && <span className="muted">{readerLine(current)}</span>}
        <Icon name="chevron-down" size={14} />
      </button>
      {open && (
        <div className="menu paper-menu" role="menu">
          {view.papers.map((p) => (
            <button
              key={p.slug}
              role="menuitemradio"
              aria-checked={p.slug === selected}
              className={p.slug === selected ? "on" : ""}
              onClick={() => {
                setOpen(false);
                onSelect(p.slug);
              }}
            >
              <span className="paper-menu-name">{p.title}</span>
              <span className="muted small">{[readerLine(p), kindleLine(p, view.kindles)].filter(Boolean).join(" · ")}</span>
            </button>
          ))}
          <hr />
          <button
            role="menuitem"
            onClick={() => {
              setOpen(false);
              onSettings();
            }}
          >
            Paper settings…
          </button>
          <button
            role="menuitem"
            onClick={() => {
              setOpen(false);
              setAdding(true);
            }}
          >
            New paper…
          </button>
        </div>
      )}
      {adding && (
        <form
          className="menu paper-new"
          aria-label="New paper"
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
          <div className="paper-actions">
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
  );
}
