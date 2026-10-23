// SPDX-License-Identifier: GPL-3.0-or-later
import { useState } from "react";
import { move } from "./bindList";
import type { BookState } from "./types";

interface Props {
  /** The rows that can be parts, in their list order. */
  books: BookState[];
  busy: boolean;
  onBind: (parts: BookState[], title: string, author: string) => Promise<void>;
  onCancel: () => void;
}

/** Bind several rows into one book: pick and order the parts, name the book (#22). */
export default function BindPanel({ books, busy, onBind, onCancel }: Props) {
  const [order, setOrder] = useState<BookState[]>(books);
  const [left, setLeft] = useState<Set<string>>(new Set());
  const [title, setTitle] = useState("");
  const [author, setAuthor] = useState("");
  const parts = order.filter((b) => !left.has(b.id));
  const toggle = (id: string) =>
    setLeft((s) => {
      const next = new Set(s);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  return (
    <section className="bind-panel" aria-label="Bind into one book">
      <div className="bind-head">
        <h3>Bind into one book</h3>
        <p className="muted small">Each file becomes a part, in this order, with its own entry in the contents.</p>
      </div>
      <ol className="bind-parts">
        {order.map((b, i) => (
          <li key={b.id} className={left.has(b.id) ? "off" : ""}>
            <label>
              <input type="checkbox" checked={!left.has(b.id)} onChange={() => toggle(b.id)} disabled={busy} />
              <span className="bind-title">{b.title}</span>
            </label>
            <span className="bind-move">
              <button className="ghost" aria-label={`Move ${b.title} up`} disabled={busy || i === 0} onClick={() => setOrder((o) => move(o, i, -1))}>
                ↑
              </button>
              <button className="ghost" aria-label={`Move ${b.title} down`} disabled={busy || i === order.length - 1} onClick={() => setOrder((o) => move(o, i, 1))}>
                ↓
              </button>
            </span>
          </li>
        ))}
      </ol>
      <div className="bind-fields">
        <input placeholder="Title (required)" value={title} onChange={(e) => setTitle(e.target.value)} disabled={busy} aria-label="Book title" />
        <input placeholder="Author (optional)" value={author} onChange={(e) => setAuthor(e.target.value)} disabled={busy} aria-label="Book author" />
        <button className="primary" disabled={busy || parts.length < 2 || !title.trim()} onClick={() => onBind(parts, title, author)}>
          {busy ? "Binding…" : `Bind ${parts.length}`}
        </button>
        <button className="ghost" onClick={onCancel} disabled={busy}>
          Cancel
        </button>
      </div>
    </section>
  );
}
