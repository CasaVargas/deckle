// SPDX-License-Identifier: GPL-3.0-or-later
import type { Settings } from "../types";
import { Row, Switch } from "./Row";

interface Props {
  settings: Settings;
  set: <K extends keyof Settings>(k: K, v: Settings[K]) => void;
}

function levelLabel(level: number) {
  if (level === 5) return "All hints; the Kindle's slider decides";
  if (level === 1) return "Rarest words only";
  return "Fewer hints";
}

export default function WordWise({ settings, set }: Props) {
  return (
    <>
      <Row label="Word Wise" note="For books sent with features">
        <Switch label="Word Wise" checked={settings.word_wise} onChange={(on) => set("word_wise", on)} />
      </Row>
      <Row label={`Hint level ${settings.word_wise_level}`} note={levelLabel(settings.word_wise_level)}>
        <input
          type="range"
          min={1}
          max={5}
          value={settings.word_wise_level}
          disabled={!settings.word_wise}
          onChange={(e) => set("word_wise_level", Number(e.target.value))}
          aria-label="Hint level"
        />
      </Row>
      <p className="set-foot">The first book downloads a 12 MB dictionary from Wiktionary. More in About.</p>
    </>
  );
}
