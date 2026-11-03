// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useRef, useState } from "react";
import { authorsText, editFrom, editProblem } from "./editDetails";
import type { BookEdit } from "./types";

interface Props {
  /** The heading: "Edit details", "Rename on the reMarkable". */
  heading: string;
  initial: BookEdit;
  /** What saving will do, in a line or two. */
  note: string;
  /** A reMarkable document has a name, nothing more. */
  titleOnly?: boolean;
  /** The save button's word: "Save", "Rebuild with these". */
  action: string;
  busy?: boolean;
  onSave: (edit: BookEdit) => void | Promise<void>;
  onCancel: () => void;
}

/** The title, authors and series of one book, wherever it is (details editing). */
export default function EditDetails({ heading, initial, note, titleOnly, action, busy, onSave, onCancel }: Props) {
  const [title, setTitle] = useState(initial.title);
  const [authors, setAuthors] = useState(authorsText(initial.authors));
  const [series, setSeries] = useState(initial.series?.name ?? "");
  const [index, setIndex] = useState(initial.series?.index != null ? String(initial.series.index) : "");
  const first = useRef<HTMLInputElement>(null);
  useEffect(() => first.current?.focus(), []);
  const edit = editFrom(title, authors, series, index);
  const problem = editProblem(edit);
  return (
    <div className="sheet-backdrop" onClick={onCancel}>
      <form
        className="sheet edit-details"
        role="dialog"
        aria-label={heading}
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.key === "Escape" && onCancel()}
        onSubmit={(e) => {
          e.preventDefault();
          if (!problem && !busy) void onSave(edit);
        }}
      >
        <div className="sheet-head">
          <h2>{heading}</h2>
          <button type="button" className="ghost" onClick={onCancel}>
            Cancel
          </button>
        </div>
        <label className="edit-field">
          <span className="tk-label">{titleOnly ? "Name" : "Title"}</span>
          <input ref={first} type="text" value={title} onChange={(e) => setTitle(e.target.value)} />
        </label>
        {!titleOnly && (
          <>
            <label className="edit-field">
              <span className="tk-label">Authors</span>
              <input type="text" value={authors} placeholder="One, or several with & between" onChange={(e) => setAuthors(e.target.value)} />
            </label>
            <div className="edit-pair">
              <label className="edit-field">
                <span className="tk-label">Series</span>
                <input type="text" value={series} placeholder="None" onChange={(e) => setSeries(e.target.value)} />
              </label>
              <label className="edit-field edit-index">
                <span className="tk-label">Number</span>
                <input type="text" inputMode="decimal" value={index} disabled={!series.trim()} onChange={(e) => setIndex(e.target.value)} />
              </label>
            </div>
          </>
        )}
        <p className="muted small">{note}</p>
        {problem && <p className="warn small">{problem}</p>}
        <div className="button-row">
          <button className="primary" type="submit" disabled={!!problem || busy}>
            {busy ? "Working…" : action}
          </button>
        </div>
      </form>
    </div>
  );
}
