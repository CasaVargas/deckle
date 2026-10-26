// SPDX-License-Identifier: GPL-3.0-or-later
// The masthead font setting (masthead design §1, Daily tab redesign §7): one of ten bundled faces
// by key ("consent", the default, is left out of a save), or the path to a .ttf.

export type FaceKey = "consent" | "grenze" | "pirata" | "texturina" | "unifraktur" | "playfair" | "bodoni" | "caslon" | "serif" | "sans";
export type MastheadChoice = FaceKey | "file";

export interface Face {
  key: FaceKey;
  label: string;
  /** A few words on how it looks. */
  note: string;
  group: "blackletter" | "plain";
}

/** The bundled faces in the order the picker shows them; keep in step with `masthead::BUNDLED`. */
export const FACES: Face[] = [
  { key: "consent", label: "Manufacturing Consent", note: "Most like a big daily", group: "blackletter" },
  { key: "grenze", label: "Grenze Gotisch", note: "Lighter, easy to read", group: "blackletter" },
  { key: "pirata", label: "Pirata One", note: "Plain, rounded", group: "blackletter" },
  { key: "texturina", label: "Texturina", note: "Squarer, bookish", group: "blackletter" },
  { key: "unifraktur", label: "Unifraktur", note: "The old default", group: "blackletter" },
  { key: "playfair", label: "Playfair", note: "High contrast", group: "plain" },
  { key: "bodoni", label: "Bodoni", note: "Classic, sharp", group: "plain" },
  { key: "caslon", label: "Caslon", note: "Quiet, literary", group: "plain" },
  { key: "serif", label: "Old Standard", note: "The paper's own type", group: "plain" },
  { key: "sans", label: "Sans", note: "Plain and modern", group: "plain" },
];

/** Which face a saved `masthead` is: nothing is the default, "blackletter" the old one, any
 * other text that isn't a key is a file. */
export function mastheadChoice(value: string | undefined): MastheadChoice {
  const v = (value ?? "").trim().toLowerCase();
  if (v === "") return "consent";
  if (v === "blackletter") return "unifraktur";
  return FACES.find((f) => f.key === v)?.key ?? "file";
}

/** The file's name, for its card ("Fraktur.ttf"). */
export function fontFileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

/** The value to store: the default is stored as nothing, like the backend leaves it out. */
export function mastheadValue(choice: FaceKey): string | undefined {
  return choice === "consent" ? undefined : choice;
}

/** The CSS family a face is loaded under in the app (mastheadFonts.ts). */
export const faceFamily = (key: FaceKey) => `"Deckle Masthead ${key}"`;
