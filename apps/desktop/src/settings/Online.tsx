// SPDX-License-Identifier: GPL-3.0-or-later
import type { SettingsPage } from "../settingsIndex";
import type { Settings } from "../types";
import { Switch } from "./Row";

interface Props {
  settings: Settings;
  set: <K extends keyof Settings>(k: K, v: Settings[K]) => void;
  onPage: (page: SettingsPage) => void;
}

/**
 * Online & privacy (Settings redesign §7): every time Deckle goes online, what it sends and to
 * whom. The switches are the same settings as on their own pages.
 */
export default function Online({ settings, set, onPage }: Props) {
  const ai = settings.llm_provider !== "off";
  const provider = settings.llm_provider === "anthropic" ? "Anthropic" : "the provider you set";
  const rows: { title: string; sends: string; to: string; why: string; on: boolean; toggle: (on: boolean) => void; page: SettingsPage; where: string }[] = [
    { title: "Fill missing details", sends: "a book's title, author or ISBN", to: "Open Library", why: "Fills in and suggests what a book leaves empty, and finds covers.", on: settings.metadata_fill, toggle: (on) => set("metadata_fill", on), page: "books", where: "Books" },
    { title: "Match the store cover", sends: "a book's title, author or ISBN", to: "Open Library", why: "Finds the Amazon edition, so a Kindle book keeps its store cover.", on: settings.amazon_match, toggle: (on) => set("amazon_match", on), page: "books", where: "Books" },
    { title: "Wikipedia summaries", sends: "the names X-Ray finds", to: "Wikipedia", why: "Short descriptions of real people and places.", on: settings.wikipedia, toggle: (on) => set("wikipedia", on), page: "reading", where: "Reading aids" },
    {
      title: "AI model for X-Ray",
      sends: "text from the book",
      to: ai ? provider : "the provider you pick",
      why: "Better character lists. Turning it on picks Anthropic; change it under Reading aids.",
      on: ai,
      toggle: (on) => set("llm_provider", on ? "anthropic" : "off"),
      page: "reading",
      where: "Reading aids",
    },
  ];
  return (
    <div className="set-stack">
      <p className="overview-lead">Deckle works on this Mac. These are the only times it goes online, what it sends, and where. Everything optional is off until you turn it on.</p>
      <section className="set-card online-rows" aria-label="You choose">
        <div className="online-head">You choose</div>
        {rows.map((r) => (
          <div key={r.title} className="online-row">
            <div>
              <strong>{r.title}</strong>
              <p className="muted">
                Sends {r.sends} to <strong className="to">{r.to}</strong>. {r.why}
              </p>
              <button className="link-action" onClick={() => onPage(r.page)}>
                {r.where} →
              </button>
            </div>
            <Switch label={r.title} checked={r.on} onChange={r.toggle} />
          </div>
        ))}
      </section>
      <section className="set-card online-rows" aria-label="Always">
        <div className="online-head">Always, and what they carry</div>
        <div className="online-row">
          <div>
            <strong>Checking for updates</strong>
            <p className="muted">When Deckle starts it asks GitHub whether there's a newer version. The request carries nothing about you or your books.</p>
          </div>
        </div>
        <div className="online-row">
          <div>
            <strong>The Word Wise dictionary</strong>
            <p className="muted">Downloaded once from GitHub, the first time a book needs Deckle's.</p>
          </div>
        </div>
      </section>
      <section className="set-card online-rows" aria-label="Only when you ask">
        <div className="online-head">Only when you ask</div>
        <div className="online-row">
          <div>
            <strong>The KOReader plugin store</strong>
            <p className="muted">
              Opening the store asks GitHub for Deckle's recommended list, and searching, details and installs ask GitHub too. Nothing about your
              reader or books is sent. Answers are kept for an hour. A GitHub token in DECKLE_GITHUB_TOKEN, if you set one, goes only to GitHub's
              API.
            </p>
            <button className="link-action" onClick={() => onPage("koreader")}>
              KOReader →
            </button>
          </div>
        </div>
      </section>
      <p className="muted small">A Kindle itself may ask Amazon about books sent as Kindle books. The Daily's news comes from the sites you choose on the Daily tab.</p>
    </div>
  );
}
