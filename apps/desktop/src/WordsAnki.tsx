// SPDX-License-Identifier: GPL-3.0-or-later
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { api } from "./api";
import type { RunTarget } from "./types";

/** The words looked up on this Kindle, as Anki flashcards (#135). */
export default function WordsAnki({ target, busy }: { target: RunTarget; busy: boolean }) {
  const [exporting, setExporting] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  const exportWords = async () => {
    setMessage(null);
    const to = await saveDialog({ defaultPath: "Kindle words.csv", filters: [{ name: "CSV for Anki", extensions: ["csv"] }] });
    if (typeof to !== "string") return;
    setExporting(true);
    try {
      const n = await api.wordsAnki(target, to);
      setMessage(`${n} word${n === 1 ? "" : "s"} written. In Anki: File → Import, then pick the file.`);
    } catch (e) {
      setMessage(String(e));
    } finally {
      setExporting(false);
    }
  };

  return (
    <section className="card words-anki" aria-label="Looked-up words">
      <div className="card-head">
        <h3>Looked-up words</h3>
        <button className="ghost" onClick={exportWords} disabled={busy || exporting}>
          {exporting ? "Exporting…" : "Export to Anki…"}
        </button>
      </div>
      <p className="muted small">Every English word you looked up on this Kindle, once each, as flashcards: the word, a short definition, the sentence you met it in and the book.</p>
      {message && (
        <p className="muted small" role="status">
          {message}
        </p>
      )}
    </section>
  );
}
