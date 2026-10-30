// SPDX-License-Identifier: GPL-3.0-or-later
// The web build's door to the backend: commands are POSTs to the server, events one shared
// SSE stream. Answers and rejections look as Tauri's do, so api.ts and the screens can't tell.

/** Every state-changing request carries it; the server refuses a POST without it (CSRF). */
export const CSRF = { "X-Deckle": "1" } as const;

const lostHandlers = new Set<() => void>();

/** `cb` runs when a request answers 401: the session ended, so the login screen comes back. */
export function onSessionLost(cb: () => void): () => void {
  lostHandlers.add(cb);
  return () => lostHandlers.delete(cb);
}

/** The `error` of a `{"error": …}` answer, else the status line. */
export async function errorOf(res: Response): Promise<string> {
  const body = await res.json().catch(() => null);
  return typeof body?.error === "string" ? body.error : `${res.status} ${res.statusText}`.trim();
}

/** Says the session is gone when the answer is a 401; resolves to the answer either way. */
export function checkSession(res: Response): Response {
  if (res.status === 401) for (const cb of [...lostHandlers]) cb();
  return res;
}

export async function webInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const res = checkSession(
    await fetch(`/api/invoke/${command}`, {
      method: "POST",
      headers: { "Content-Type": "application/json", ...CSRF },
      credentials: "same-origin",
      body: JSON.stringify(args ?? {}),
    }),
  );
  // Tauri rejects with the command's error string; so does this.
  if (!res.ok) throw await errorOf(res);
  return (await res.json()) as T;
}

// One EventSource for the page, each event name fanned out to its listeners.
let source: EventSource | null = null;
const listeners = new Map<string, Set<(payload: unknown) => void>>();

function subscribe(es: EventSource, event: string) {
  es.addEventListener(event, (ev) => {
    let payload: unknown;
    try {
      payload = JSON.parse((ev as MessageEvent).data);
    } catch {
      return;
    }
    for (const cb of [...(listeners.get(event) ?? [])]) cb(payload);
  });
}

function open(): EventSource {
  if (!source) {
    source = new EventSource("/api/events");
    for (const event of listeners.keys()) subscribe(source, event);
  }
  return source;
}

/** Opens the stream again, as after logging in: one opened before it was refused and closed. */
export function restartEvents() {
  source?.close();
  source = null;
  if (listeners.size > 0) open();
}

export function webListen<T>(event: string, cb: (payload: T) => void): Promise<() => void> {
  const es = open();
  let set = listeners.get(event);
  if (!set) {
    set = new Set();
    listeners.set(event, set);
    subscribe(es, event);
  }
  const handler = cb as (payload: unknown) => void;
  set.add(handler);
  return Promise.resolve(() => {
    listeners.get(event)?.delete(handler);
  });
}

/** For tests: forget the stream and its listeners. */
export function resetEventsForTest() {
  source?.close();
  source = null;
  listeners.clear();
  lostHandlers.clear();
}
