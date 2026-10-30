// SPDX-License-Identifier: GPL-3.0-or-later
// Commands and events to the backend. Desktop goes over Tauri IPC; the web build over HTTP and
// SSE (transport/web.ts). A static test on the build-time constant picks the door.
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";
import { webInvoke, webListen } from "./transport/web";

const WEB = import.meta.env.VITE_DECKLE_TARGET === "web";

export function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return WEB ? webInvoke<T>(command, args) : tauriInvoke<T>(command, args);
}

/** Resolves to the function that stops listening. */
export function listen<T>(event: string, cb: (payload: T) => void): Promise<() => void> {
  return WEB ? webListen<T>(event, cb) : tauriListen<T>(event, (ev) => cb(ev.payload));
}
