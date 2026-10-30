// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { passwordReason, siteEnv } from "./gates";

test("site_env_matches_the_servers_rule", () => {
  expect(siteEnv("gazette.example")).toBe("DECKLE_SITE_PASSWORD_GAZETTE_EXAMPLE");
  expect(siteEnv("a-b.co.uk")).toBe("DECKLE_SITE_PASSWORD_A_B_CO_UK");
  // One `_` per character, as Rust's char replace; only ASCII is uppercased.
  expect(siteEnv("café.fr")).toBe("DECKLE_SITE_PASSWORD_CAF__FR");
});

test("password_reason_names_the_variable", () => {
  expect(passwordReason("DECKLE_SMTP_PASSWORD")).toBe("Passwords come from the server's environment: set DECKLE_SMTP_PASSWORD.");
});
