// SPDX-License-Identifier: GPL-3.0-or-later
// The masthead faces as web fonts, for the picker's cards: the same files the paper is drawn
// with, from crates/deckle-digest/fonts (Vite bundles them).
import { faceFamily } from "./mastheadView";
import type { FaceKey } from "./mastheadView";

const FILES: Record<FaceKey, string> = {
  consent: new URL("../../../crates/deckle-digest/fonts/ManufacturingConsent-Regular.ttf", import.meta.url).href,
  grenze: new URL("../../../crates/deckle-digest/fonts/GrenzeGotisch-SemiBold.ttf", import.meta.url).href,
  pirata: new URL("../../../crates/deckle-digest/fonts/PirataOne-Regular.ttf", import.meta.url).href,
  texturina: new URL("../../../crates/deckle-digest/fonts/Texturina-Bold.ttf", import.meta.url).href,
  unifraktur: new URL("../../../crates/deckle-digest/fonts/UnifrakturMaguntia-Book.ttf", import.meta.url).href,
  playfair: new URL("../../../crates/deckle-digest/fonts/PlayfairDisplay-Bold.ttf", import.meta.url).href,
  bodoni: new URL("../../../crates/deckle-digest/fonts/BodoniModa-Bold.ttf", import.meta.url).href,
  caslon: new URL("../../../crates/deckle-digest/fonts/LibreCaslonDisplay-Regular.ttf", import.meta.url).href,
  serif: new URL("../../../crates/deckle-digest/fonts/OldStandard-Bold.ttf", import.meta.url).href,
  sans: new URL("../../../crates/deckle-digest/fonts/Inter-Bold.ttf", import.meta.url).href,
};

const loaded = new Set<FaceKey>();

/** Adds one face to the document once; the text shows its fallback until it loads. */
export function loadMastheadFont(key: FaceKey) {
  if (loaded.has(key) || typeof FontFace === "undefined") return;
  loaded.add(key);
  const face = new FontFace(faceFamily(key).replace(/"/g, ""), `url(${FILES[key]})`);
  document.fonts.add(face);
  face.load().catch(() => {});
}

/** Adds the ten faces to the document once; the cards show their fallback until each loads. */
export function loadMastheadFonts() {
  for (const key of Object.keys(FILES) as FaceKey[]) loadMastheadFont(key);
}
