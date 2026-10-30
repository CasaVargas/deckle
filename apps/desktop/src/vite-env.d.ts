// SPDX-License-Identifier: GPL-3.0-or-later
/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** "web" builds the browser front end; anything else is the desktop app. */
  readonly VITE_DECKLE_TARGET?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
