// SPDX-License-Identifier: GPL-3.0-or-later
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { api } from "../api";
import type { HighlightsStatus, ReadershipView, ShelfStatus } from "../types";
import { Row, Switch } from "./Row";

/** Where highlights go, and whether the morning schedule writes them (highlights export design §6). */
function HighlightsSettings() {
  const [status, setStatus] = useState<HighlightsStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    api.highlightsStatus().then(setStatus).catch((e) => setError(String(e)));
  }, []);
  const set = (folder: string | null, clippings: boolean) =>
    api
      .highlightsSet(folder, clippings)
      .then((s) => {
        setStatus(s);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  const choose = async () => {
    const dir = await openDialog({ directory: true, multiple: false });
    if (typeof dir === "string") await set(dir, status?.clippings ?? false);
  };
  return (
    <section className="set-card set-pad set-device">
      <h3>Highlights</h3>
      <Row label="Folder" note={status?.folder ?? "Not chosen yet. One Markdown file per book goes here."}>
        <button className="ghost" onClick={choose}>
          {status?.folder ? "Change…" : "Choose…"}
        </button>
      </Row>
      <Row label="Update these files every time this Kindle connects" note="Runs with the morning schedule (deckle digest watch).">
        <Switch label="Update highlights on connect" checked={status?.clippings ?? false} disabled={!status?.folder} onChange={(on) => set(null, on)} />
      </Row>
      {error && <p className="set-note warn">{error}</p>}
    </section>
  );
}

/** `[sync] readership`: note how far the paper was read (readership design §7). */
function ReadershipSettings() {
  const [view, setView] = useState<ReadershipView | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    api.readershipReport().then(setView).catch((e) => setError(String(e)));
  }, []);
  const set = (on: boolean) =>
    api
      .readershipSet(on)
      .then((v) => {
        setView(v);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  return (
    <section className="set-card set-pad set-device">
      <h3>Reading</h3>
      <Row label="Note how far I read the paper" note="Read from the Kindle's own reading position when it connects. Shown on the Daily tab; stays on this Mac.">
        <Switch label="Note how far I read the paper" checked={view?.enabled ?? true} disabled={!view} onChange={set} />
      </Row>
      {error && <p className="set-note warn">{error}</p>}
    </section>
  );
}

/** `[sync] shelf`: read the books in progress (and what the Kindle shares) for From Your Shelf. */
function ShelfSettings() {
  const [status, setStatus] = useState<ShelfStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    api.shelfStatus().then(setStatus).catch((e) => setError(String(e)));
  }, []);
  const set = (on: boolean) =>
    api
      .shelfSet(on)
      .then((s) => {
        setStatus(s);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  return (
    <section className="set-card set-pad set-device">
      <h3>From Your Shelf</h3>
      <Row
        label="Use my Kindle for From Your Shelf"
        note="Reads the books you're in the middle of when it connects, and a past highlight and words you looked up on Kindles that share them. Stays on this Mac."
      >
        <Switch label="Use my Kindle for From Your Shelf" checked={status?.enabled ?? true} disabled={!status} onChange={set} />
      </Row>
      {error && <p className="set-note warn">{error}</p>}
    </section>
  );
}

/** Highlights & reading (Settings redesign §2): where highlights go, and what Deckle notes about your reading. */
export default function HighlightsReading() {
  return (
    <div className="set-stack">
      <HighlightsSettings />
      <ReadershipSettings />
      <ShelfSettings />
    </div>
  );
}
