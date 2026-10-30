// SPDX-License-Identifier: GPL-3.0-or-later
// Commands and events to the backend. Desktop goes over Tauri IPC; the web build swaps this door.
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";

export function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return tauriInvoke<T>(command, args);
}

/** Resolves to the function that stops listening. */
export function listen<T>(event: string, cb: (payload: T) => void): Promise<() => void> {
  return tauriListen<T>(event, (ev) => cb(ev.payload));
}
