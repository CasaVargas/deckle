// SPDX-License-Identifier: GPL-3.0-or-later
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { onSessionLost, webInvoke, webListen, resetEventsForTest } from "../transport/web";

const json = (status: number, body: unknown) => new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });

/** Enough of EventSource to count how many are opened and to send them events. */
class FakeEventSource {
  static made: FakeEventSource[] = [];
  handlers = new Map<string, ((ev: MessageEvent) => void)[]>();
  closed = false;
  constructor(public url: string) {
    FakeEventSource.made.push(this);
  }
  addEventListener(name: string, h: (ev: MessageEvent) => void) {
    this.handlers.set(name, [...(this.handlers.get(name) ?? []), h]);
  }
  close() {
    this.closed = true;
  }
  send(name: string, data: unknown) {
    for (const h of this.handlers.get(name) ?? []) h({ data: JSON.stringify(data) } as MessageEvent);
  }
}

beforeEach(() => {
  FakeEventSource.made = [];
  resetEventsForTest();
  vi.stubGlobal("EventSource", FakeEventSource);
});
afterEach(() => vi.unstubAllGlobals());

test("web_invoke_posts_with_csrf_header_and_returns_json", async () => {
  const fetch = vi.fn().mockResolvedValue(json(200, { title: "Today" }));
  vi.stubGlobal("fetch", fetch);
  await expect(webInvoke("daily_load", { paper: "main" })).resolves.toEqual({ title: "Today" });
  const [url, init] = fetch.mock.calls[0];
  expect(url).toBe("/api/invoke/daily_load");
  expect(init.method).toBe("POST");
  expect(init.credentials).toBe("same-origin");
  expect(init.headers["X-Deckle"]).toBe("1");
  expect(init.headers["Content-Type"]).toBe("application/json");
  expect(JSON.parse(init.body)).toEqual({ paper: "main" });
});

test("web_invoke_400_rejects_with_message", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(json(400, { error: "no such paper" })));
  // The bare string, as Tauri rejects: callers show String(e).
  await expect(webInvoke("daily_load", {})).rejects.toBe("no such paper");
});

test("web_transport_401_shows_login", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(json(401, { error: "not logged in" })));
  const lost = vi.fn();
  const stop = onSessionLost(lost);
  await expect(webInvoke("app_version")).rejects.toBe("not logged in");
  expect(lost).toHaveBeenCalledTimes(1);
  stop();
});

test("web_listen_fans_out_one_eventsource", async () => {
  const a = vi.fn();
  const b = vi.fn();
  const c = vi.fn();
  const stopA = await webListen("book-progress", a);
  await webListen("book-progress", b);
  await webListen("run-finished", c);
  expect(FakeEventSource.made).toHaveLength(1);
  const es = FakeEventSource.made[0];
  expect(es.url).toBe("/api/events");
  es.send("book-progress", { id: "1" });
  expect(a).toHaveBeenCalledWith({ id: "1" });
  expect(b).toHaveBeenCalledWith({ id: "1" });
  expect(c).not.toHaveBeenCalled();
  stopA();
  es.send("book-progress", { id: "2" });
  expect(a).toHaveBeenCalledTimes(1);
  expect(b).toHaveBeenCalledTimes(2);
});
