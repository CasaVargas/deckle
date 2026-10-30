// SPDX-License-Identifier: GPL-3.0-or-later
// What this install can do (api.capabilities), shared by every screen that gates a feature.
// Asked again when the page comes back into view: a Kindle may have been attached meanwhile.
import { useSyncExternalStore } from "react";
import { api } from "./api";
import { platform } from "./platform";
import type { Capabilities } from "./types";

// Until the answer comes: the desktop can do everything; the web app assumes the least.
let current: Capabilities =
  platform.kind === "web"
    ? { web: true, previewer: false, usb: false, keychain: false, updater: false, reveal: false, schedule: "config" }
    : { web: false, previewer: true, usb: true, keychain: true, updater: true, reveal: true, schedule: "install" };
const subscribers = new Set<() => void>();
let asked = false;

export function refreshCapabilities(): Promise<void> {
  asked = true;
  return api
    .capabilities()
    .then((c) => {
      current = c;
      for (const s of subscribers) s();
    })
    .catch(() => {});
}

function subscribe(cb: () => void) {
  if (!asked) refreshCapabilities();
  subscribers.add(cb);
  return () => subscribers.delete(cb);
}

if (typeof document !== "undefined") {
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible" && subscribers.size > 0) refreshCapabilities();
  });
}

export function useCapabilities(): Capabilities {
  return useSyncExternalStore(subscribe, () => current);
}
