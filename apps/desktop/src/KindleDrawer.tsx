// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";
import Icon from "./Icon";
import { better, coverLine, highlightsOf, onKindle, progressLine, sentMode, size, titleOf } from "./kindleRooms";
import type { KindleRoom } from "./kindleRooms";
import type { DeviceBookRow, HighlightRow } from "./types";

type Tab = "reading" | "highlights" | "details";

interface Props {
  book: DeviceBookRow;
  highlights: HighlightRow[];
  busy: boolean;
  /** Update, Send again and Send as a Kindle book: the library copy goes on the Books tab. */
  onResend: (cde: "pdoc" | "ebok" | null, what: string) => void;
  onRemove: () => Promise<void>;
  /** Upgrade… and Open Highlights go to their tab. */
  onRoom: (room: KindleRoom) => void;
  onClose: () => void;
}

/**
 * A book on the Kindle (Kindle tab redesign §4): how far in and what Deckle can do better,
 * its highlights, and its file; Send again and Remove at the foot.
 */
export default function KindleDrawer({ book, highlights, busy, onResend, onRemove, onRoom, onClose }: Props) {
  const [tab, setTab] = useState<Tab>("reading");
  const [confirming, setConfirming] = useState(false);
  const [removing, setRemoving] = useState(false);
  const title = titleOf(book);
  const offer = better(book);
  const mine = highlightsOf(book, highlights);
  const now = new Date();
  useEffect(() => {
    const onKey = (ev: KeyboardEvent) => ev.key === "Escape" && !ev.defaultPrevented && onClose();
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  const act = () => {
    if (!offer) return;
    if (offer.kind === "upgrade") onRoom("upgrades");
    else if (offer.kind === "update") onResend(sentMode(book), `${title} is ready to update.`);
    else onResend("ebok", `${title} is ready to send again as a Kindle book.`);
  };
  const pct = book.reading === "finished" ? 100 : Math.round((book.fraction ?? 0) * 100);

  return (
    <div className="sheet drawer" role="dialog" aria-label={title}>
      <div className="drawer-tabs">
        <div className="tab-strip" role="tablist" aria-label="Book">
          {(
            [
              ["reading", "Reading"],
              ["highlights", mine ? `Highlights ${mine.total}` : "Highlights"],
              ["details", "Details"],
            ] as [Tab, string][]
          ).map(([k, t]) => (
            <button key={k} role="tab" aria-selected={tab === k} className={tab === k ? "on" : ""} onClick={() => setTab(k)}>
              {t}
            </button>
          ))}
        </div>
        <button className="icon" aria-label="Close" onClick={onClose}>
          <Icon name="x" size={18} />
        </button>
      </div>

      <div className="drawer-head">
        <div className="drawer-cover" aria-hidden="true">
          {book.cover ? <img src={book.cover} alt="" /> : <div className="cover-placeholder">{title.slice(0, 1)}</div>}
        </div>
        <div className="drawer-title">
          <h2>{title}</h2>
          <div className="muted">{book.authors.join(", ") || "Unknown author"}</div>
          {tab === "reading" && (
            <>
              <span className="drawer-bar" aria-hidden="true">
                <span className={book.reading === "finished" ? "done" : ""} style={{ width: `${pct}%` }} />
              </span>
              <span className="small">{progressLine(book, now)}</span>
            </>
          )}
        </div>
      </div>

      {tab === "reading" && (
        <div className="kindle-pane">
          {offer && (
            <div className="better">
              <div>
                <strong>{offer.title}</strong>
                <p>{offer.body}</p>
              </div>
              <button className="primary" onClick={act} disabled={busy}>
                {offer.action}
              </button>
            </div>
          )}
          <section>
            <h3 className="pane-h3">On the Kindle</h3>
            <dl className="facts">
              {onKindle(book).map(([k, v]) => (
                <div key={k}>
                  <dt>{k}</dt>
                  <dd>{v}</dd>
                </div>
              ))}
            </dl>
          </section>
        </div>
      )}

      {tab === "highlights" && (
        <div className="kindle-pane">
          {mine ? (
            <>
              <p>
                <strong>
                  {mine.total} highlight{mine.total === 1 ? "" : "s"}
                </strong>
                {mine.new > 0 ? `, ${mine.new} new since they were last saved.` : ", all saved."}
              </p>
              {mine.markdown && <pre className="highlights-preview">{mine.markdown}</pre>}
              <button className="ghost" onClick={() => onRoom("highlights")}>
                Open Highlights to save them
              </button>
            </>
          ) : (
            <p className="muted">No highlights in this book yet. Highlight a passage on the Kindle and it shows here.</p>
          )}
        </div>
      )}

      {tab === "details" && (
        <div className="kindle-pane">
          <dl className="facts">
            {(
              [
                ["File", book.file],
                ["Format", book.format.toUpperCase()],
                ["Mode", book.cde ?? "—"],
                ["Size", size(book.size)],
                ["Status", coverLine(book)],
              ] as [string, string][]
            ).map(([k, v]) => (
              <div key={k}>
                <dt>{k}</dt>
                <dd className={k === "File" ? "mono" : undefined}>{v}</dd>
              </div>
            ))}
          </dl>
        </div>
      )}

      <div className="drawer-foot">
        {confirming ? (
          <>
            <span className="foot-note">Remove {title} from this Kindle? Its reading position goes too.</span>
            <button className="ghost" disabled={removing} onClick={() => setConfirming(false)}>
              Keep
            </button>
            <button
              className="primary danger-fill"
              disabled={removing}
              onClick={async () => {
                setRemoving(true);
                await onRemove();
                setRemoving(false);
              }}
            >
              {removing ? "Removing…" : "Remove"}
            </button>
          </>
        ) : (
          <>
            <button className="ghost" disabled={busy || !book.library_source} title={book.library_source ? undefined : "Not in your library, so Deckle has no copy to send"} onClick={() => onResend(null, `${title} is ready to send again.`)}>
              Send again
            </button>
            <span className="spacer" />
            <button className="quiet danger" disabled={busy} onClick={() => setConfirming(true)}>
              Remove from Kindle…
            </button>
          </>
        )}
      </div>
    </div>
  );
}
