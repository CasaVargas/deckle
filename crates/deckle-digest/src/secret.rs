// SPDX-License-Identifier: GPL-3.0-or-later
//! The SMTP password (email design §6): in the system keychain, never in `digest.toml`, a
//! log, a report or an error. `DECKLE_SMTP_PASSWORD` stands in only where there is no
//! keychain (Linux without Secret Service, CI).
//!
//! The keychain sits behind [`Keyring`], so tests use [`MemoryKeyring`] and never touch the
//! real one. Nothing here derives `Debug` over a password, and no error carries one.

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde::Serialize;
use zeroize::Zeroizing;

use crate::config::EmailConfig;

/// The keychain service every entry is filed under.
pub const SERVICE: &str = "com.casavargas.deckle.smtp";

/// The headless fallback, read only when there is no keychain.
pub const ENV: &str = "DECKLE_SMTP_PASSWORD";

/// `<username>@<smtp_host>`: a new username or host is a new entry, so a stale password never
/// goes to the wrong server.
pub fn account(cfg: &EmailConfig) -> String {
    format!("{}@{}", cfg.username.trim(), cfg.smtp_host.trim())
}

/// Where the password came from, for the run's report.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Keychain,
    Env,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyringError {
    /// No keychain to ask: no Secret Service, a locked or refused keychain.
    Unavailable(String),
    Failed(String),
}

impl std::fmt::Display for KeyringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyringError::Unavailable(e) => write!(f, "no system keychain: {e}"),
            KeyringError::Failed(e) => write!(f, "keychain: {e}"),
        }
    }
}

/// The system keychain, or a stand-in for it.
pub trait Keyring {
    fn get(&self, account: &str) -> Result<Option<Zeroizing<String>>, KeyringError>;
    fn set(&self, account: &str, password: &str) -> Result<(), KeyringError>;
    /// Whether there was an entry to delete.
    fn delete(&self, account: &str) -> Result<bool, KeyringError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SecretError {
    /// `[email]` lacks the username or host the entry is named after.
    NoAccount,
    /// The keychain works but has no password for this account.
    NotSet {
        account: String,
    },
    /// No keychain, and no `DECKLE_SMTP_PASSWORD` either.
    NoKeychain(String),
    Keychain(String),
    Empty,
}

impl std::fmt::Display for SecretError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecretError::NoAccount => {
                write!(f, "[email] needs username and smtp_host before a password")
            }
            SecretError::NotSet { account } => write!(
                f,
                "no SMTP password in the keychain for {account}: run `deckle digest email \
                 password` (or set it on the Daily tab)"
            ),
            SecretError::NoKeychain(why) => write!(
                f,
                "no system keychain ({why}) and {ENV} isn't set; set it for headless runs"
            ),
            SecretError::Keychain(why) => write!(f, "keychain: {why}"),
            SecretError::Empty => write!(f, "the password is empty"),
        }
    }
}

impl std::error::Error for SecretError {}

fn account_for(cfg: &EmailConfig) -> Result<String, SecretError> {
    if cfg.username.trim().is_empty() || cfg.smtp_host.trim().is_empty() {
        return Err(SecretError::NoAccount);
    }
    Ok(account(cfg))
}

/// The password for `cfg`'s account: the keychain's, or `DECKLE_SMTP_PASSWORD` (through
/// `env`) only when there is no keychain at all.
pub fn smtp_password(
    cfg: &EmailConfig,
    keys: &dyn Keyring,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<(Zeroizing<String>, Source), SecretError> {
    let account = account_for(cfg)?;
    match keys.get(&account) {
        Ok(Some(pw)) => Ok((pw, Source::Keychain)),
        Ok(None) => Err(SecretError::NotSet { account }),
        Err(KeyringError::Unavailable(why)) => match env(ENV).map(Zeroizing::new) {
            Some(pw) if !pw.is_empty() => Ok((pw, Source::Env)),
            _ => Err(SecretError::NoKeychain(why)),
        },
        Err(KeyringError::Failed(why)) => Err(SecretError::Keychain(why)),
    }
}

/// Whether a send would find a password (the Daily tab shows it; the password stays put).
pub fn has_password(
    cfg: &EmailConfig,
    keys: &dyn Keyring,
    env: &dyn Fn(&str) -> Option<String>,
) -> bool {
    smtp_password(cfg, keys, env).is_ok()
}

/// Saves `password` in the keychain for `cfg`'s account.
pub fn set(cfg: &EmailConfig, keys: &dyn Keyring, password: &str) -> Result<(), SecretError> {
    let account = account_for(cfg)?;
    if password.is_empty() {
        return Err(SecretError::Empty);
    }
    keys.set(&account, password).map_err(|e| match e {
        KeyringError::Unavailable(why) => SecretError::NoKeychain(why),
        KeyringError::Failed(why) => SecretError::Keychain(why),
    })
}

/// Deletes `cfg`'s keychain entry; `false` when there was none.
pub fn forget(cfg: &EmailConfig, keys: &dyn Keyring) -> Result<bool, SecretError> {
    let account = account_for(cfg)?;
    keys.delete(&account).map_err(|e| match e {
        KeyringError::Unavailable(why) => SecretError::NoKeychain(why),
        KeyringError::Failed(why) => SecretError::Keychain(why),
    })
}

/// The OS keychain: macOS Keychain, Windows Credential Manager, Linux Secret Service.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemKeyring;

impl SystemKeyring {
    fn entry(account: &str) -> Result<keyring::Entry, KeyringError> {
        keyring::Entry::new(SERVICE, account).map_err(keyring_error)
    }
}

/// keyring's errors, without their payloads: `BadEncoding` holds the stored bytes.
fn keyring_error(e: keyring::Error) -> KeyringError {
    match e {
        keyring::Error::NoStorageAccess(e) | keyring::Error::PlatformFailure(e) => {
            KeyringError::Unavailable(e.to_string())
        }
        keyring::Error::BadEncoding(_) => {
            KeyringError::Failed("the stored password isn't UTF-8".into())
        }
        keyring::Error::TooLong(what, max) => {
            KeyringError::Failed(format!("{what} is longer than {max}"))
        }
        keyring::Error::Invalid(what, why) => KeyringError::Failed(format!("{what}: {why}")),
        keyring::Error::Ambiguous(_) => KeyringError::Failed("more than one entry".into()),
        keyring::Error::NoEntry => KeyringError::Failed("no entry".into()),
        _ => KeyringError::Failed("unexpected keychain error".into()),
    }
}

impl Keyring for SystemKeyring {
    fn get(&self, account: &str) -> Result<Option<Zeroizing<String>>, KeyringError> {
        match Self::entry(account)?.get_password() {
            Ok(pw) => Ok(Some(Zeroizing::new(pw))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(keyring_error(e)),
        }
    }

    fn set(&self, account: &str, password: &str) -> Result<(), KeyringError> {
        Self::entry(account)?
            .set_password(password)
            .map_err(keyring_error)
    }

    fn delete(&self, account: &str) -> Result<bool, KeyringError> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(keyring_error(e)),
        }
    }
}

/// An in-memory keychain for tests (here, in the CLI and in the app).
#[derive(Default)]
pub struct MemoryKeyring {
    entries: Mutex<BTreeMap<String, Zeroizing<String>>>,
    unavailable: bool,
}

impl MemoryKeyring {
    /// A machine with no keychain at all.
    pub fn unavailable() -> Self {
        MemoryKeyring {
            unavailable: true,
            ..Default::default()
        }
    }

    fn check(&self) -> Result<(), KeyringError> {
        if self.unavailable {
            return Err(KeyringError::Unavailable("no Secret Service".into()));
        }
        Ok(())
    }
}

impl Keyring for MemoryKeyring {
    fn get(&self, account: &str) -> Result<Option<Zeroizing<String>>, KeyringError> {
        self.check()?;
        Ok(self.entries.lock().unwrap().get(account).cloned())
    }

    fn set(&self, account: &str, password: &str) -> Result<(), KeyringError> {
        self.check()?;
        self.entries
            .lock()
            .unwrap()
            .insert(account.to_owned(), Zeroizing::new(password.to_owned()));
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<bool, KeyringError> {
        self.check()?;
        Ok(self.entries.lock().unwrap().remove(account).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EmailConfig;

    const PW: &str = "hunter2-Secret!";

    fn cfg() -> EmailConfig {
        EmailConfig {
            username: "me@example.com".into(),
            smtp_host: "smtp.example.com".into(),
            ..EmailConfig::default()
        }
    }

    fn env_with(v: Option<&'static str>) -> impl Fn(&str) -> Option<String> {
        move |k: &str| (k == ENV).then_some(v).flatten().map(str::to_owned)
    }

    #[test]
    fn the_entry_is_per_username_and_host() {
        assert_eq!(SERVICE, "com.casavargas.deckle.smtp");
        assert_eq!(account(&cfg()), "me@example.com@smtp.example.com");
    }

    #[test]
    fn the_keychain_wins_over_the_environment() {
        let keys = MemoryKeyring::default();
        keys.set(&account(&cfg()), PW).unwrap();
        let (pw, src) = smtp_password(&cfg(), &keys, &env_with(Some("from-env"))).unwrap();
        assert_eq!(pw.as_str(), PW);
        assert_eq!(src, Source::Keychain);
    }

    #[test]
    fn a_keychain_without_the_entry_doesnt_fall_back() {
        let keys = MemoryKeyring::default();
        let e = smtp_password(&cfg(), &keys, &env_with(Some("from-env"))).unwrap_err();
        assert!(matches!(e, SecretError::NotSet { .. }), "{e}");
        assert!(
            e.to_string().contains("deckle digest email password"),
            "{e}"
        );
    }

    #[test]
    fn the_environment_stands_in_when_there_is_no_keychain() {
        let keys = MemoryKeyring::unavailable();
        let (pw, src) = smtp_password(&cfg(), &keys, &env_with(Some("from-env"))).unwrap();
        assert_eq!(pw.as_str(), "from-env");
        assert_eq!(src, Source::Env);
        let e = smtp_password(&cfg(), &keys, &env_with(None)).unwrap_err();
        assert!(matches!(e, SecretError::NoKeychain(_)), "{e}");
        assert!(e.to_string().contains(ENV), "{e}");
        let e = smtp_password(&cfg(), &keys, &env_with(Some(""))).unwrap_err();
        assert!(
            matches!(e, SecretError::NoKeychain(_)),
            "an empty variable is unset"
        );
    }

    #[test]
    fn set_and_forget() {
        let keys = MemoryKeyring::default();
        set(&cfg(), &keys, PW).unwrap();
        assert!(has_password(&cfg(), &keys, &env_with(None)));
        assert!(forget(&cfg(), &keys).unwrap());
        assert!(!forget(&cfg(), &keys).unwrap(), "nothing left to forget");
        assert!(!has_password(&cfg(), &keys, &env_with(None)));
        assert!(
            set(&cfg(), &keys, "").is_err(),
            "an empty password is a mistake"
        );
        let none = EmailConfig::default();
        assert!(
            set(&none, &keys, PW).is_err(),
            "no username or host: no entry"
        );
    }

    #[test]
    fn errors_never_carry_the_password() {
        let keys = MemoryKeyring::unavailable();
        let mut shown = Vec::new();
        for e in [
            smtp_password(&cfg(), &keys, &env_with(None)).unwrap_err(),
            smtp_password(&cfg(), &MemoryKeyring::default(), &env_with(Some(PW))).unwrap_err(),
            set(&cfg(), &keys, PW).unwrap_err(),
        ] {
            shown.push(format!("{e} {e:?}"));
        }
        let (pw, src) = smtp_password(&cfg(), &keys, &env_with(Some(PW))).unwrap();
        shown.push(format!("{src:?} {}", serde_json::to_string(&src).unwrap()));
        drop(pw);
        for s in shown {
            assert!(!s.contains(PW), "{s}");
        }
    }
}
