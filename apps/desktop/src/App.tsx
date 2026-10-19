// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export default function App() {
  const [version, setVersion] = useState<string>("");

  useEffect(() => {
    invoke<string>("app_version").then(setVersion).catch(() => setVersion("dev"));
  }, []);

  return (
    <div className="app">
      <main className="dropzone" role="region" aria-label="Book drop zone">
        <div className="dropzone-inner">
          <div className="glyph" aria-hidden="true">
            ⬇
          </div>
          <h1>Drop EPUBs here</h1>
          <p>Each book becomes a Kindle file with Enhanced Typesetting, a cover that shows, and real page numbers.</p>
        </div>
      </main>
      <footer className="bar">
        <div className="device">
          <span className="dot" aria-hidden="true" />
          No Kindle connected
        </div>
        <button className="primary" disabled>
          Send to Kindle
        </button>
        <div className="version">Deckle {version}</div>
      </footer>
    </div>
  );
}
