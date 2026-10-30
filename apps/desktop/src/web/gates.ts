// SPDX-License-Identifier: GPL-3.0-or-later
// The one-line reasons a feature the web app can't offer gives in its place.

export const PREVIEWER_REASON = "Kindle book conversion needs Kindle Previewer, which doesn't run on Linux.";
export const NO_DEVICE_REASON = "No e-reader is connected to this server.";
export const REVEAL_REASON = "Show in folder isn't available in the web app.";
export const UPDATER_REASON = "Updates come with a new image tag.";
export const SMTP_ENV = "DECKLE_SMTP_PASSWORD";

export const passwordReason = (env: string) => `Passwords come from the server's environment: set ${env}.`;

/** The server's variable for a site's password (deckle-digest `site_env`): ASCII letters
 * uppercased, every other character `_`. */
export function siteEnv(site: string): string {
  return `DECKLE_SITE_PASSWORD_${Array.from(site, (c) => (/[A-Za-z0-9]/.test(c) ? c.toUpperCase() : "_")).join("")}`;
}
