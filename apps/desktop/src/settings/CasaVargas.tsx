// SPDX-License-Identifier: GPL-3.0-or-later
import { useId } from "react";
import { Link } from "./Row";

export const STUDIO = "https://casavargas.app/";

/** The paid products the card suggests: one-time purchases only, as casavargas.app lists them. */
export const PRODUCTS = [
  {
    name: "Beltr",
    price: "$19.99 once",
    what: "Karaoke from the music you already own. Every phone in the room becomes a mic.",
    runs: "Mac · Windows · Linux · five songs free to try",
    url: "https://beltr.app/",
  },
  {
    name: "OneScribe Pro",
    price: "$9.99 once",
    what: "A document scanner that reads what it scans, with the AI on your device.",
    runs: "iPhone · iPad · free to scan",
    url: "https://getonescribe.app/",
  },
];

/** The Casa Vargas house mark (casavargas.app's logo, without the wordmark). */
function HouseMark({ className }: { className: string }) {
  const gradient = useId();
  return (
    <svg className={className} viewBox="-76 -99 152 163" aria-hidden="true">
      <defs>
        <linearGradient id={gradient} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0%" stopColor="#EF9F27" />
          <stop offset="100%" stopColor="#D85A30" />
        </linearGradient>
      </defs>
      <path d="M0,-95 L-72,-30 L-58,-30 L0,-82 L58,-30 L72,-30 Z" fill={`url(#${gradient})`} />
      <rect x="-52" y="-30" width="104" height="90" rx="4" fill={`url(#${gradient})`} opacity="0.88" />
      <path d="M-18,-5 L-32,15 L-18,35" fill="none" stroke="#fff" strokeWidth="5" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M18,-5 L32,15 L18,35" fill="none" stroke="#fff" strokeWidth="5" strokeLinecap="round" strokeLinejoin="round" />
      <line x1="8" y1="-5" x2="-8" y2="35" stroke="#fff" strokeWidth="4" strokeLinecap="round" opacity="0.7" />
    </svg>
  );
}

/** Who makes Deckle, and the paid products that keep it free. Every link opens in the browser. */
export default function CasaVargas() {
  return (
    <section className="cv-card" aria-label="Made by Casa Vargas">
      <div className="cv-head">
        <HouseMark className="cv-mark" />
        <div>
          <div className="cv-word">
            <span className="cv-casa">Casa</span>Vargas
          </div>
          <div className="cv-sub">Indie software studio</div>
        </div>
      </div>
      <p className="cv-pitch">
        <strong>Like my work?</strong> Deckle is free and stays free. It's made by one developer at Casa Vargas. Please consider buying one of my paid products:
        each is a one-time purchase, never a subscription.
      </p>
      <ul className="cv-products">
        {PRODUCTS.map((p) => (
          <li key={p.name}>
            <Link href={p.url}>
              <b>
                {p.name} <em>{p.price}</em>
              </b>
              <span>{p.what}</span>
              <small>{p.runs}</small>
            </Link>
          </li>
        ))}
      </ul>
      <div className="cv-foot">
        <Link href={STUDIO}>See everything at casavargas.app →</Link>
        <span>Links open in your browser.</span>
      </div>
    </section>
  );
}
