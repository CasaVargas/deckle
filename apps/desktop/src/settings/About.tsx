// SPDX-License-Identifier: GPL-3.0-or-later
import { useCapabilities } from "../capabilities";
import Unavailable from "../web/Unavailable";
import { UPDATER_REASON } from "../web/gates";
import { platform } from "../platform";
import { install, type UpdateState } from "../updates";
import CasaVargas from "./CasaVargas";
import { Link, REPO, Row } from "./Row";
import { LOCALES, useI18n, type Locale } from "../i18n";

interface Props {
  version: string;
  update: UpdateState;
  onUpdateState: (s: UpdateState) => void;
  /** A conversion or send is running: nothing that closes Deckle may start. */
  busy: boolean;
  /** Shows the welcome again (welcome design §4). */
  onWelcome: () => void;
}

const WINDOWS = navigator.userAgent.includes("Windows");
const mb = (bytes: number) => `${(bytes / 1048576).toFixed(1)} MB`;

/** The update line: one state at a time, and nothing happens without a click. */
function UpdateStatus({ update, onUpdateState, busy }: Omit<Props, "version" | "onWelcome">) {
  const releases = <Link href={`${REPO}/releases`}>Releases</Link>;
  switch (update.kind) {
    case "checking":
      return <>Checking for updates…</>;
    case "current":
      return <>Up to date</>;
    case "failed":
      return <>Couldn't check for updates ({releases})</>;
    case "available":
    case "error":
      return (
        <span className="update">
          {update.kind === "error" ? (
            <span className="update-error">The update didn't install: {update.message}</span>
          ) : (
            <>
              Deckle {update.update.version} is available
              <Link href={`${REPO}/releases/tag/v${update.update.version}`}>What's new</Link>
            </>
          )}
          <button className="primary" disabled={busy} onClick={() => install(update.update, onUpdateState)}>
            {update.kind === "error" ? "Try again" : "Update"}
          </button>
          {busy ? <span>Finish the current job first.</span> : WINDOWS && <span>Deckle closes to finish installing.</span>}
          {update.kind === "error" && <span>Or download it from {releases}.</span>}
        </span>
      );
    case "downloading": {
      const pct = update.total ? Math.min(100, (update.done / update.total) * 100) : 0;
      return (
        <div className="progress">
          <div className="progress-track">
            <div className="progress-fill" style={{ width: `${pct}%` }} />
          </div>
          <span className="progress-label">Downloading… {update.total ? `${mb(update.done)} of ${mb(update.total)}` : mb(update.done)}</span>
        </div>
      );
    }
    case "ready":
      return (
        <span className="update">
          Deckle {update.update.version} is installed.
          <button className="primary" disabled={busy} onClick={() => platform.relaunch()}>
            Restart to finish
          </button>
          {busy && <span>Finish the current job first.</span>}
        </span>
      );
  }
}

/** The Language row (UI translations design §4): each language named in itself, live on change. */
function LanguageRow() {
  const { t, locale, setLocale } = useI18n();
  return (
    <Row label={t("settings.language.label")} note={t("settings.language.note")}>
      <select value={locale} aria-label={t("settings.language.label")} onChange={(e) => setLocale(e.target.value as Locale)}>
        {LOCALES.map((l) => (
          <option key={l.id} value={l.id} lang={l.id}>
            {l.name}
          </option>
        ))}
      </select>
    </Row>
  );
}

export default function About({ version, update, onUpdateState, busy, onWelcome }: Props) {
  const caps = useCapabilities();
  return (
    <>
      <p className="set-lead">
        Deckle {version} · GPL-3.0-or-later · <Link href={REPO}>Source</Link>
      </p>
      <div className="set-block">
        {caps.updater ? <UpdateStatus update={update} onUpdateState={onUpdateState} busy={busy} /> : <Unavailable reason={UPDATER_REASON} />}
      </div>
      <LanguageRow />
      <Row label="The welcome" note="The pages from first launch: your paper, the bookmarklets, where things are.">
        <button className="ghost" onClick={onWelcome}>
          Show it again
        </button>
      </Row>
      <CasaVargas />
      <p className="set-prose">
        What Deckle sends online, and when, is under <strong>Online &amp; privacy</strong>.
      </p>
      <p className="set-prose">
        Word Wise: Kindles that already have Amazon's English Word Wise dictionary use it; others get Deckle's, shown when the Kindle's Word Wise language is set to
        中文 (Chinese). The hints are English either way.
      </p>
      <h3>Credits</h3>
      <p className="set-prose">
        Word Wise definitions come from <Link href="https://en.wiktionary.org/">Wiktionary</Link> via <Link href="https://kaikki.org/">kaikki.org</Link> (
        <Link href="https://creativecommons.org/licenses/by-sa/4.0/">CC BY-SA 4.0</Link>), word frequencies from <Link href="https://github.com/rspeer/wordfreq">wordfreq</Link>{" "}
        (CC BY-SA 4.0). Icons from <Link href="https://tabler.io/icons">Tabler Icons</Link> (MIT). Built on the research and code of jhowell (KFX Input, kfxlib), xxyzz
        (WordDumb, Proficiency), jloutsch (kfxgen) and seapy (kindle-cli). Kindle Previewer is Amazon's and is never bundled. Kindle is a trademark of Amazon; Deckle
        is not affiliated with Amazon.
      </p>
    </>
  );
}
