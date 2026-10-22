// SPDX-License-Identifier: GPL-3.0-or-later
import type { ReactNode } from "react";
import { api } from "../api";

export const REPO = "https://github.com/CasaVargas/deckle";

/** An external link: opens in the default browser (a plain anchor would do nothing in the app). */
export function Link({ href, children }: { href: string; children: ReactNode }) {
  return (
    <a
      href={href}
      onClick={(e) => {
        e.preventDefault();
        api.openLink(href).catch(() => {});
      }}
    >
      {children}
    </a>
  );
}

/** One setting: a short label (and an optional one-line note) on the left, its control on the right. */
export function Row({ label, note, online, children }: { label: ReactNode; note?: ReactNode; online?: boolean; children?: ReactNode }) {
  return (
    <div className="set-row">
      <div className="set-label">
        <div>
          {label}
          {online && <span className="tag">Online</span>}
        </div>
        {note && <div className="set-note">{note}</div>}
      </div>
      {children && <div className="set-control">{children}</div>}
    </div>
  );
}

/** A checkbox drawn as a switch. */
export function Switch({ checked, onChange, disabled, label }: { checked: boolean; onChange: (on: boolean) => void; disabled?: boolean; label: string }) {
  return <input type="checkbox" role="switch" className="switch" aria-label={label} checked={checked} disabled={disabled} onChange={(e) => onChange(e.target.checked)} />;
}
