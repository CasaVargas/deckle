# Send-to-Kindle email: the paper over Wi-Fi, every morning

Status: draft, 2026-10-21. Issue #50. Implements "Delivery (b)" in the digest spec (§2, next increment 3).

## 1. Why

The scheduled build (#64) makes the paper every morning, but it only reaches the Kindle when the Kindle is plugged in (#43). The owner wants it to just be there.

On a stock Kindle, Amazon's Send-to-Kindle email address is the only way to deliver wirelessly with no one touching the device:

- the Kindle never fetches on its own, so `deckle serve` (#44) needs a tap each time;
- a pull script on the Kindle needs a jailbreak.

This is an explicit, labelled opt-in. It reverses the v1 non-goal "USB/MTP only" for the paper only, because the edition goes through Amazon.

## 2. Goals

- `digest run` emails the morning's edition to the owner's `@kindle.com` address, through the owner's own SMTP account. It works unattended under the #64 schedule.
- **The SMTP password lives in the OS keychain**, never in `digest.toml`, logs, the app's state or a command-line argument.
- An email that fails never loses the paper: the edition goes to the USB outbox instead, as it does today, and the run's report says why.
- Setup works from both the CLI and the Daily tab, with a "send a test" button.

## 3. Non-goals

- **No Deckle-run mail relay or service.** It's the owner's own SMTP (files-first, no lock-in, no subscription).
- **No emailed long reads or Inbox books** in this increment. They stay on USB, where #72 will give them X-Ray and Word Wise. Adding them later is one more config key.
- **No pruning of emailed editions.** Amazon gives Deckle no way to delete documents it delivered (§7).
- **No OAuth "Sign in with Google" flow.** Accounts that need it use an app password (§6).

## 4. What gets sent

- **The paper's EPUB** (`BuildReport.epub`), attached as `deckle-daily-YYYY-MM-DD.epub`.
  - Send-to-Kindle doesn't accept KFX by email, and Amazon converts what it receives.
  - Nothing is lost: the paper is PDOC with no X-Ray or Word Wise.
  - Amazon's conversion replaces Deckle's typesetting with its own. The cover and the photos come from the EPUB.
- **Subject:** `Deckle Daily — Tuesday, October 21, 2026`. **Body:** one line. Send-to-Kindle ignores both. They're for the owner's Sent folder.
- **Size:** Amazon's documented per-email limit is 50 MB. Deckle checks before sending. An EPUB over 45 MB isn't emailed and goes to USB instead, with a warning. A paper with photos at 1000 px q70 is usually a few MB.

## 5. Config

There's a new table in `digest.toml`. `digest init` writes it commented out:

```toml
# Email the paper to your Kindle each morning (opt-in). The password is not stored here:
# run `deckle digest email password` (or use the Daily tab), which saves it in the system
# keychain. Add the "from" address to Amazon's Approved Personal Document E-mail List.
[email]
enabled = false
to = "you_ABC123@kindle.com"
from = "you@example.com"
smtp_host = "smtp.gmail.com"
smtp_port = 587            # 587 = STARTTLS, 465 = implicit TLS; anything else is refused
username = "you@example.com"
usb_too = false            # also queue the paper for USB when the email succeeds
```

- `to` must end in `@kindle.com` or `@free.kindle.com`, so a typo can't send the paper somewhere else.
- `usb_too = false` is the default, because it avoids two copies of the same paper on the Kindle: the emailed one (Docs) and the sideloaded one.
- Plain-text SMTP isn't supported: no port 25 without TLS. The connection is always TLS, using rustls with webpki roots.
- `deny_unknown_fields` applies, as for every table, and saves go through the existing `toml_edit` merge.

## 6. The password and the keychain

- **Crate:** `keyring` (v3), which covers macOS Keychain, Windows Credential Manager and Linux Secret Service.
- **Entry:** service `com.casavargas.deckle.smtp`, account `<username>@<smtp_host>`. Changing the username or host means a new entry, so a stale password is never sent to the wrong server.
- **Setting it:**
  - CLI: `deckle digest email password` reads the password from the terminal without echo. With `--stdin` it reads one line from stdin, for scripts.
  - App: a password field on the Daily tab. The Tauri command `email_set_password(password)` writes the keychain. It never reads a password back to the UI; the UI only asks `email_status()`, which returns `{configured, has_password}`.
- **Reading it:** only at send time, inside the process doing the send. It's held as a `zeroize::Zeroizing<String>`, never logged, and never part of a report or error string.
- **Headless fallback:** if the keychain is unavailable (Linux with no Secret Service, CI), `DECKLE_SMTP_PASSWORD` is read instead, and the report says it was. A password is never read from the config file.
- **Scheduled runs:**
  - **macOS:** the #64 LaunchAgent runs in the owner's login session and executes the app's own signed binary (`current_exe`, `digest run`). The keychain item, written by the same signed app, is readable with no prompt.
  - **When there is a prompt:** if the CLI was built and signed separately (the dev builds), macOS asks once, "deckle wants to use your confidential information". The owner chooses Always Allow. The README notes this.
  - **Windows:** the Credential Manager entry is per user, and the #64 scheduled task runs as that user.
- **App passwords:** Gmail and iCloud need one (2-step verification). The README links each provider's page. OAuth is out of scope.

## 7. Behaviour in `digest run`

After the edition is built:

1. If `[email].enabled`, send the EPUB.
   - **Success:** the report records `emailed: "<to>"`. If `usb_too` is false, the edition isn't added to the outbox.
   - **Failure** (no password, auth refused, network, too large): the edition goes to the outbox as today, and the report carries `email_error`.
   - The scheduled run's log, and the Daily tab's last-run line, show either result.
2. The run's other output is unchanged: long reads, Inbox books, prune and `watch` all behave as before.

`--to <folder>` and an explicit `digest push` don't send email. Email belongs to the scheduled morning run and to `digest run` with no `--to`.

**Emailed editions pile up in the Kindle's Docs.** Deckle can't remove them, because Amazon has no API for it. The README and the Daily tab say so, and point the owner to *Manage Your Content and Devices*. The prune setting (`keep`) still applies to USB editions only.

## 8. CLI and app surface

- `deckle digest email password [--stdin]`: stores the password in the keychain.
- `deckle digest email test`: sends a one-page test EPUB, "If this reached your Kindle, Deckle Daily email works.", and reports the SMTP result. Use it after adding the address to Amazon's approved list.
- `deckle digest email forget`: deletes the keychain entry.
- **Daily tab, new "Email to Kindle" card:**
  - fields: enabled, to, from, host, port, username, password (write-only);
  - a "Send a test" button;
  - a note on Amazon's approved-sender list and the Docs pile-up.

## 9. Code shape

- `deckle-digest::mail`:
  - `trait Mailer { fn send(&self, msg: &Outgoing) -> Result<(), MailError> }`;
  - `Outgoing { to, from, subject, body, attachment: (name, bytes) }`;
  - `SmtpMailer` (lettre, rustls) implements the trait;
  - tests use a recording fake.
- `deckle-digest::secret`:
  - `fn smtp_password(cfg) -> Result<(Zeroizing<String>, Source), SecretError>`, where `Source` is `Keychain` or `Env`;
  - `set` and `forget`;
  - keyring is behind a small trait, so tests never touch the real keychain.
- The `deckle-cli` run path calls the mailer after the build and before queueing.
- New dependencies: `lettre` (`smtp-transport`, `rustls-tls`, `builder`; no native-tls), `keyring` and `zeroize`.

## 10. Tests the build must add

- Config:
  - the defaults;
  - the starter file's commented block parses once uncommented;
  - a `to` outside `@kindle.com` / `@free.kindle.com` is rejected;
  - port 25 is rejected.
- `digest run` with a fake mailer, success: one message with the EPUB attached under the edition name and the dated subject. Nothing is queued when `usb_too = false`, and the edition is queued when `usb_too = true`.
- Failure paths with a fake mailer, each queueing the edition with `email_error` set:
  - auth error;
  - network error;
  - the 45 MB size guard;
  - no password.
- Secret:
  - the keychain fake wins over the environment;
  - the environment is used when the keychain is unavailable;
  - the password never appears in a `Debug`/`Display` of the report or the errors (a grep over the formatted output).
- Tauri: `email_status` never returns the password.

## 11. Open questions for the owner

- **Long reads:** should they eventually be emailed too, or stay on USB? This draft says USB, so they can get X-Ray and Word Wise through #72.
- **USB as well as email:** should the paper also go to USB when email is on (`usb_too`)? This draft says no, to avoid two copies.

## 12. Sizing

M:

- two new modules behind traits;
- one config table;
- three CLI subcommands;
- one app card;
- one new `digest run` branch.

The only thing that can't be tested in CI is a real SMTP and Kindle round trip. That's the owner's check, using `digest email test`.
