// SPDX-License-Identifier: GPL-3.0-or-later
// Saving as you go (Daily tab redesign §4): one save at a time, always of the newest config.
// Edits that arrive while a save runs wait for it, and only the last of them is written.

export interface SaveQueue<C> {
  /** Saves `c` now, or once the running save ends. A newer push replaces a waiting one. */
  push: (c: C) => Promise<void>;
  /** Whether a save is running or waiting. */
  busy: () => boolean;
  /** Waits for current and queued saves without writing another copy. */
  drain: () => Promise<void>;
}

/**
 * `save` writes one config and resolves when it's on disk; it may throw, which `failed` hears
 * (the queue carries on with anything newer). `settled` runs when the queue empties.
 */
export function saveQueue<C>(save: (c: C) => Promise<void>, failed: (e: unknown) => void, settled: () => void = () => {}): SaveQueue<C> {
  let running = false;
  let waiting: C | null = null;
  let waiters: (() => void)[] = [];

  const run = async (c: C) => {
    running = true;
    try {
      await save(c);
    } catch (e) {
      failed(e);
    }
    if (waiting !== null) {
      const next = waiting;
      waiting = null;
      await run(next);
      return;
    }
    running = false;
    const done = waiters;
    waiters = [];
    done.forEach((f) => f());
    settled();
  };

  return {
    push(c) {
      const done = new Promise<void>((resolve) => waiters.push(resolve));
      if (running) waiting = c;
      else void run(c);
      return done;
    },
    busy: () => running,
    drain: () => running ? new Promise<void>((resolve) => waiters.push(resolve)) : Promise.resolve(),
  };
}

/** Runs `f` once typing pauses for `ms`; `flush` runs a pending call at once (on leaving a field). */
export function debounce(f: () => void, ms: number): { call: () => void; flush: () => void; cancel: () => void } {
  let timer: ReturnType<typeof setTimeout> | null = null;
  const cancel = () => {
    if (timer !== null) clearTimeout(timer);
    timer = null;
  };
  return {
    call() {
      cancel();
      timer = setTimeout(() => {
        timer = null;
        f();
      }, ms);
    },
    flush() {
      if (timer === null) return;
      cancel();
      f();
    },
    cancel,
  };
}
