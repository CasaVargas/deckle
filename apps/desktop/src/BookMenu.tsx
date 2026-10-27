// SPDX-License-Identifier: GPL-3.0-or-later
// The book menu (cookbook design §8.2): where the Daily has its paper switcher, the book's title
// and recipe count as a menu button. It holds Book settings… for now; more cookbooks will be
// listed here the way papers are.
import { useEffect, useRef, useState } from "react";
import Icon from "./Icon";

export default function BookMenu({ title, count, onSettings }: { title: string; count: string | null; onSettings: () => void }) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement | null>(null);

  // A click outside, or Esc, closes the menu
  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => {
      if (box.current && !box.current.contains(e.target as Node)) setOpen(false);
    };
    const esc = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("mousedown", away);
    window.addEventListener("keydown", esc);
    return () => {
      window.removeEventListener("mousedown", away);
      window.removeEventListener("keydown", esc);
    };
  }, [open]);

  return (
    <div className="paper-switcher row-menu" ref={box}>
      <button className="paper-switch" aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <span className="paper-switch-name">{title}</span>
        {count && <span className="muted">{count}</span>}
        <Icon name="chevron-down" size={14} />
      </button>
      {open && (
        <div className="menu paper-menu" role="menu">
          <button
            role="menuitem"
            onClick={() => {
              setOpen(false);
              onSettings();
            }}
          >
            Book settings…
          </button>
        </div>
      )}
    </div>
  );
}
