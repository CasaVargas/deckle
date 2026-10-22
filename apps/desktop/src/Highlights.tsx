// SPDX-License-Identifier: GPL-3.0-or-later
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { api } from "./api";
import { writtenSummary } from "./highlightSummary";
import type { HighlightRow, HighlightsStatus, RunTarget } from "./types";

interface Props {
  target: RunTarget;
  rows: HighlightRow[];
  error: string | null;
  busy: boolean;
  /** Read the Kindle again after a write, so the "new" counts are current. */
  onWritten: () => void;
}

/** The Kindle's highlights, one Markdown file per book (highlights export design §6). */
export default function Highlights({ target, rows, error, busy, onWritten }: Props) {
  const [status, setStatus] = useState<HighlightsStatus | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [writing, setWriting] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    api.highlightsStatus().then(setStatus).catch(() => setStatus({ folder: null, clippings: false }));
  }, []);

  const pending = rows.filter((r) => r.new > 0);
  const current = rows.find((r) => r.key === selected) ?? pending[0] ?? rows[0];
  const files = pending.length;

  const chooseFolder = async (): Promise<boolean> => {
    const dir = await openDialog({ directory: true, multiple: false });
    if (typeof dir !== "string") return false;
    setStatus(await api.highlightsSet(dir, status?.clippings ?? false));
    return true;
  };

  const write = async () => {
    setMessage(null);
    if (!status?.folder && !(await chooseFolder())) return;
    setWriting(true);
    try {
      setMessage(writtenSummary(await api.highlightsWrite(target)));
      onWritten();
    } catch (e) {
      setMessage(String(e));
    } finally {
      setWriting(false);
    }
  };

  return (
    <section className="highlights" aria-label="Highlights">
      <div className="highlights-books">
        <h3>Highlights</h3>
        <p className="muted small">Highlights and notes from this Kindle, including books you sideloaded, which Amazon's own export leaves out.</p>
        <ul>
          {rows.map((r) => (
            <li key={r.key}>
              <button className={`highlight-book${r.key === current?.key ? " on" : ""}`} onClick={() => setSelected(r.key)} aria-pressed={r.key === current?.key}>
                <span className="highlight-title">{r.title}</span>
                <span className="muted small">
                  {r.total} highlight{r.total === 1 ? "" : "s"}
                  {r.author && ` · ${r.author}`}
                </span>
                {r.new > 0 && <span className="highlight-new">+{r.new}</span>}
              </button>
            </li>
          ))}
        </ul>
      </div>
      <div className="highlights-preview">
        <div className="highlights-target">
          <div className="grow">
            <div className="muted small">Writes to</div>
            <div className="mono">{status?.folder ? `${status.folder}/${current?.file ?? ""}` : "No folder chosen yet"}</div>
          </div>
          {status?.folder && (
            <button className="ghost" onClick={chooseFolder} disabled={writing}>
              Change…
            </button>
          )}
          <button className="primary" onClick={write} disabled={busy || writing || (!!status?.folder && files === 0)}>
            {writing ? "Writing…" : !status?.folder ? "Choose a folder…" : files === 0 ? "Up to date" : `Write ${files} file${files === 1 ? "" : "s"}`}
          </button>
        </div>
        {message && (
          <p className="muted small" role="status">
            {message}
          </p>
        )}
        {error && <p className="warn small">My Clippings.txt, {error}. The rest was read.</p>}
        <pre className="highlights-md" aria-label={`Markdown for ${current?.title ?? "this book"}`}>
          {current ? current.markdown || "Nothing new in this book since the last time." : ""}
        </pre>
      </div>
    </section>
  );
}
