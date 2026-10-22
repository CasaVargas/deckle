// SPDX-License-Identifier: GPL-3.0-or-later
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { api } from "../api";
import type { DeviceRow, HighlightsStatus } from "../types";
import { Row, Switch } from "./Row";

interface Props {
  devices: DeviceRow[];
  onDeviceOptions: (key: string, cde: "pdoc" | "ebok" | null, verified: boolean) => void;
}

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
    <section className="set-device">
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

export default function Kindles({ devices, onDeviceOptions }: Props) {
  return (
    <>
      <HighlightsSettings />
      {devices.length === 0 && <p className="set-foot">Connect a Kindle to set per-device options.</p>}
      {devices.map((d) => {
        const firmware = d.firmware ?? (d.kind === "mtp" ? "unknown until the first send" : "unknown");
        const covers = d.kind === "mtp" || !d.thumbnails_writable ? "covers via PDOC" : "thumbnails";
        return (
          <section key={d.key} className="set-device">
            <h3>
              {d.model_hint} <span className="tag">{d.kind === "mtp" ? "MTP" : "USB"}</span>
            </h3>
            <p className="set-note">
              Firmware {firmware} · {covers}
            </p>
            {d.compat.length > 0 && (
              <ul className="device-compat" aria-label="Known issues on this Kindle">
                {d.compat.map((m, i) => (
                  <li key={i} className="warn">
                    {m}
                  </li>
                ))}
              </ul>
            )}
            <Row label="Mode on this Kindle">
              <select
                value={d.cde_override ?? "auto"}
                onChange={(e) => onDeviceOptions(d.key, e.target.value === "auto" ? null : (e.target.value as "pdoc" | "ebok"), d.verified_real_asin)}
                aria-label={`Mode on ${d.model_hint}`}
              >
                <option value="auto">Automatic</option>
                <option value="pdoc">Cover first</option>
                <option value="ebok">Features first</option>
              </select>
            </Row>
            <Row label="Keeps Deckle's X-Ray for real ASINs" note="Turn on once you've checked it on this Kindle">
              <Switch label="Keeps Deckle's X-Ray for real ASINs" checked={d.verified_real_asin} onChange={(on) => onDeviceOptions(d.key, d.cde_override, on)} />
            </Row>
          </section>
        );
      })}
    </>
  );
}
