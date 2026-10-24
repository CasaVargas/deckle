// SPDX-License-Identifier: GPL-3.0-or-later
// The masthead font setting (masthead design §1): "blackletter" (the default, left out of a save),
// "serif", "sans", or the path to a .ttf.

export type MastheadChoice = "blackletter" | "serif" | "sans" | "file";

/** Which option of the select a saved `masthead` is: anything that isn't a name is a file. */
export function mastheadChoice(value: string | undefined): MastheadChoice {
  const v = (value ?? "").trim().toLowerCase();
  if (v === "" || v === "blackletter") return "blackletter";
  if (v === "serif" || v === "sans") return v;
  return "file";
}

/** The file's name, for the select ("Custom: Fraktur.ttf"). */
export function fontFileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

/** The value to store: the default is stored as nothing, like the backend leaves it out. */
export function mastheadValue(choice: Exclude<MastheadChoice, "file">): string | undefined {
  return choice === "blackletter" ? undefined : choice;
}
