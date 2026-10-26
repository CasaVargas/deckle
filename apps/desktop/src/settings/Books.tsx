// SPDX-License-Identifier: GPL-3.0-or-later
import type { Settings } from "../types";
import { Row, Switch } from "./Row";

interface Props {
  settings: Settings;
  set: <K extends keyof Settings>(k: K, v: Settings[K]) => void;
}

type Flag = "pages_auto" | "generated_cover" | "amazon_match" | "metadata_fill" | "typography" | "popup_notes";

const ROWS: { key: Flag; label: string; note: string; online?: boolean }[] = [
  { key: "pages_auto", label: "Page numbers", note: "Approximate, like the store" },
  { key: "generated_cover", label: "Make a cover when missing", note: "Title and author; never replaces a real cover" },
  { key: "amazon_match", label: "Match the store cover", note: "Features-first books keep Amazon's cover", online: true },
  { key: "metadata_fill", label: "Fill missing details", note: "Only empty fields; the book's own data wins. Also suggests them when you edit a book's details", online: true },
  { key: "typography", label: "Tidy typography", note: "Curly quotes, dashes, ellipses" },
  { key: "popup_notes", label: "Pop-up footnotes", note: "Tap a note number to read it" },
];

export default function Books({ settings, set }: Props) {
  return (
    <>
      {ROWS.map((r) => (
        <Row key={r.key} label={r.label} note={r.note} online={r.online}>
          <Switch label={r.label} checked={settings[r.key]} onChange={(on) => set(r.key, on)} />
        </Row>
      ))}
    </>
  );
}
