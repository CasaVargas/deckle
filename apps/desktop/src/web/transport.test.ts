// SPDX-License-Identifier: GPL-3.0-or-later
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { onSessionLost, webInvoke, webListen, resetEventsForTest } from "../transport/web";

const json = (status: number, body: unknown) => new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });

/** Enough of EventSource to count how many are opened and to send them events. */
class FakeEventSource {
  static made: FakeEventSource[] = [];
  handlers = new Map<string, ((ev: MessageEvent) => void)[]>();
  closed = false;
  readyState = 0;
  constructor(public url: string) {
    FakeEventSource.made.push(this);
  }
  addEventListener(name: string, h: (ev: MessageEvent) => void) {
    this.handlers.set(name, [...(this.handlers.get(name) ?? []), h]);
  }
  close() {
    this.closed = true;
  }
  fire(name: string) {
    for (const h of this.handlers.get(name) ?? []) h({} as MessageEvent);
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

test("web_events_reconnect_after_close", async () => {
  vi.useFakeTimers();
  try {
    const got = vi.fn();
    await webListen("run-finished", got);
    const first = FakeEventSource.made[0];
    // A proxy 502 while the container restarts: the browser gives up on the stream for good.
    first.readyState = 2;
    first.fire("error");
    expect(FakeEventSource.made).toHaveLength(1);
    vi.advanceTimersByTime(999);
    expect(FakeEventSource.made).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(FakeEventSource.made).toHaveLength(2);
    expect(first.closed).toBe(true);
    const second = FakeEventSource.made[1];
    second.send("run-finished", { ok: true });
    expect(got).toHaveBeenCalledWith({ ok: true });
    // Failing again waits twice as long; opening resets the wait.
    second.readyState = 2;
    second.fire("error");
    vi.advanceTimersByTime(1999);
    expect(FakeEventSource.made).toHaveLength(2);
    vi.advanceTimersByTime(1);
    expect(FakeEventSource.made).toHaveLength(3);
    const third = FakeEventSource.made[2];
    third.fire("open");
    third.readyState = 2;
    third.fire("error");
    vi.advanceTimersByTime(1000);
    expect(FakeEventSource.made).toHaveLength(4);
    // An error the browser retries by itself (still CONNECTING) opens nothing new.
    const fourth = FakeEventSource.made[3];
    fourth.readyState = 0;
    fourth.fire("error");
    vi.advanceTimersByTime(60_000);
    expect(FakeEventSource.made).toHaveLength(4);
  } finally {
    vi.useRealTimers();
  }
});
