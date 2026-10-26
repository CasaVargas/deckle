// SPDX-License-Identifier: GPL-3.0-or-later
import type { DeviceRow } from "../types";
import { Row, Switch } from "./Row";

interface Props {
  devices: DeviceRow[];
  onDeviceOptions: (key: string, cde: "pdoc" | "ebok" | null, verified: boolean) => void;
}

export default function Kindles({ devices, onDeviceOptions }: Props) {
  return (
    <div className="set-stack">
      {devices.length === 0 && <p className="set-foot">Connect a Kindle to set per-device options.</p>}
      {devices.map((d) => {
        const firmware = d.firmware ?? (d.kind === "mtp" ? "unknown until the first send" : "unknown");
        const covers = d.kind === "mtp" || !d.thumbnails_writable ? "covers via PDOC" : "thumbnails";
        return (
          <section key={d.key} className="set-card set-pad set-device">
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
    </div>
  );
}
