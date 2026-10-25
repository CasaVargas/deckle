// SPDX-License-Identifier: GPL-3.0-or-later
import type { DeviceRow } from "./types";

/** What the footer calls a Kindle: its owner-given name and model, else the model alone. */
export const deviceLabel = (d: Pick<DeviceRow, "name" | "model_hint">): string => {
  const name = d.name?.trim();
  return name ? `${name} — ${d.model_hint}` : d.model_hint;
};
