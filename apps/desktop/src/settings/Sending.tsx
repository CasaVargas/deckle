// SPDX-License-Identifier: GPL-3.0-or-later
import { useCapabilities } from "../capabilities";
import Unavailable from "../web/Unavailable";
import { PREVIEWER_REASON } from "../web/gates";
import type { DeviceRow, PreviewerStatus, Settings } from "../types";
import { Link } from "./Row";

interface Props {
  settings: Settings;
  set: <K extends keyof Settings>(k: K, v: Settings[K]) => void;
  previewer: PreviewerStatus | null;
  onPickPreviewer: () => void;
  devices: DeviceRow[];
  onKindles: () => void;
}

const MODES: { value: Settings["cde"]; title: string; body: string; gets: [string, boolean][]; blank?: boolean; pick?: boolean }[] = [
  { value: "auto", title: "Automatic", pick: true, body: "Deckle chooses for each Kindle: a Kindle book where that Kindle keeps the cover, a document where it wouldn't.", gets: [["X-Ray, Word Wise", true], ["Its cover", true]] },
  { value: "ebok", title: "Kindle book", blank: true, body: "X-Ray, Word Wise and real page numbers. Some Kindles show a plain cover for these.", gets: [["X-Ray, Word Wise", true], ["Cover may be plain", false]] },
  { value: "pdoc", title: "Document", body: "Its own cover always shows. No X-Ray or Word Wise.", gets: [["X-Ray, Word Wise", false], ["Its cover", true]] },
];

const WORD: Record<"pdoc" | "ebok", string> = { pdoc: "Document", ebok: "Kindle book" };

/** Sending (Settings redesign §5): how books show up, as a choice you can see, and the tool that builds them. */
export default function Sending({ settings, set, previewer, onPickPreviewer, devices, onKindles }: Props) {
  const caps = useCapabilities();
  const own = devices.filter((d) => d.cde_override);
  return (
    <div className="set-stack">
      <fieldset className="mode-cards">
        <legend>How books show up</legend>
        <div className="mode-grid">
          {MODES.map((m) => (
            <label key={m.value} className={`mode-card ${settings.cde === m.value ? "on" : ""}`}>
              <span className="mode-head">
                <input type="radio" name="mode" checked={settings.cde === m.value} onChange={() => set("cde", m.value)} />
                <strong>{m.title}</strong>
                {m.pick && <span className="mode-pick">Recommended</span>}
              </span>
              <span className="mode-sample" aria-hidden="true">
                <span className={`mode-cover ${m.blank ? "blank" : ""}`}>{m.blank ? "" : "Aa"}</span>
                <span className="mode-gets">
                  {m.gets.map(([what, yes]) => (
                    <span key={what} className={yes ? "yes" : "no"}>
                      {what}
                    </span>
                  ))}
                </span>
              </span>
              <span className="muted">{m.body}</span>
            </label>
          ))}
        </div>
        {own.length > 0 && (
          <p className="muted small">
            {own.map((d) => `${d.name || d.model_hint} is set to ${WORD[d.cde_override!]}`).join("; ")} on its own.{" "}
            <button className="link-action inline" onClick={onKindles}>
              Kindles
            </button>
          </p>
        )}
      </fieldset>

      <section className="set-card set-pad" aria-label="Kindle Previewer">
        <div className="set-card-head">
          <span className={`setup-mark ${previewer?.found ? "ok" : "todo"}`} aria-hidden="true">
            {previewer?.found ? "✓" : "!"}
          </span>
          <strong>{previewer?.found ? "Kindle Previewer is ready" : "Kindle Previewer wasn't found"}</strong>
          {previewer?.version && <span className="muted">version {previewer.version}</span>}
        </div>
        <p className="muted">
          Amazon's free tool that turns a book into the Kindle's own format.{" "}
          {caps.web ? null : previewer?.found ? (
            "Deckle finds it by itself; choose it only if it lives somewhere unusual."
          ) : (
            <>
              {previewer?.download_url ? <Link href={previewer.download_url}>Download it</Link> : "Download it"} and restart Deckle, or choose its command line tool.
            </>
          )}
        </p>
        {caps.web && !previewer?.found && <Unavailable reason={PREVIEWER_REASON} />}
        {/* The server finds its own Previewer: there's nothing to point it at from a browser. */}
        {!caps.web && (
          <div className="set-wide">
            <input type="text" value={settings.previewer_path ?? ""} placeholder={previewer?.path ?? "Found automatically"} onChange={(e) => set("previewer_path", e.target.value || null)} aria-label="Kindle Previewer location" />
            <button className="ghost" onClick={onPickPreviewer}>
              Choose…
            </button>
          </div>
        )}
      </section>
    </div>
  );
}
