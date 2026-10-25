// SPDX-License-Identifier: GPL-3.0-or-later
//! Send-to-Kindle email (docs/superpowers/specs/2026-10-21-send-to-kindle-email-design.md).
//!
//! The paper's EPUB goes to the owner's `@kindle.com` address through the owner's own SMTP
//! account, over TLS only. Sending sits behind [`Mailer`], so tests record messages and never
//! open a connection. A failure is a report line, never a lost paper: `digest run` queues the
//! edition for USB instead (design §7).

use std::path::Path;
use std::time::Duration;

use chrono::NaiveDate;
use serde::Serialize;
use zeroize::Zeroizing;

use crate::config::EmailConfig;
use crate::secret::{self, Keyring, Source};

/// Amazon takes 50 MB an email; Deckle stops short of it (design §4).
pub const MAX_BYTES: u64 = 45 * 1024 * 1024;

/// One email with one attachment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outgoing {
    pub to: String,
    pub from: String,
    pub subject: String,
    pub body: String,
    /// File name and bytes.
    pub attachment: (String, Vec<u8>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MailError {
    /// `[email]` can't be used as it is.
    Config(String),
    TooLarge {
        bytes: u64,
    },
    NoPassword(String),
    /// The server refused the login.
    Auth(String),
    /// No connection, TLS failed, or it timed out.
    Network(String),
    /// The server answered, but not with yes.
    Rejected(String),
    Io(String),
}

impl std::fmt::Display for MailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MailError::Config(e) => write!(f, "{e}"),
            MailError::TooLarge { bytes } => write!(
                f,
                "the paper is {:.1} MB; Send-to-Kindle email takes up to 45 MB",
                *bytes as f64 / 1024.0 / 1024.0
            ),
            MailError::NoPassword(e) => write!(f, "{e}"),
            MailError::Auth(e) => write!(
                f,
                "the SMTP server refused the login: {e} (Gmail and iCloud need an app password)"
            ),
            MailError::Network(e) => write!(f, "couldn't reach the SMTP server: {e}"),
            MailError::Rejected(e) => write!(f, "the SMTP server refused the email: {e}"),
            MailError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for MailError {}

/// Sends one email.
pub trait Mailer {
    fn send(&self, msg: &Outgoing) -> Result<(), MailError>;
}

/// What the run's report says about the email.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct EmailReport {
    /// The Kindle address the paper went to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emailed: Option<String>,
    /// Why it didn't: the paper is then queued for USB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_error: Option<String>,
    /// `env` says a headless machine used `DECKLE_SMTP_PASSWORD`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_from: Option<Source>,
}

impl EmailReport {
    /// Whether the edition still goes to the USB outbox: always after a failure, and after a
    /// success only with `usb_too`.
    pub fn to_usb(&self, cfg: &EmailConfig) -> bool {
        self.emailed.is_none() || cfg.usb_too
    }
}

/// `Deckle Daily — Wednesday, October 21, 2026`, with `deckle-daily-2026-10-21.epub`.
pub fn edition_message(cfg: &EmailConfig, date: NaiveDate, epub: Vec<u8>) -> Outgoing {
    paper_message(
        cfg,
        None,
        &format!("{}{date}", crate::EDITION_PREFIX),
        date,
        epub,
    )
}

/// One paper's edition (many papers §4): the main paper (`title` `None`) as
/// [`edition_message`]; another as `<title> — <date>`, with `<stem>.epub`.
pub fn paper_message(
    cfg: &EmailConfig,
    title: Option<&str>,
    stem: &str,
    date: NaiveDate,
    epub: Vec<u8>,
) -> Outgoing {
    Outgoing {
        to: cfg.to.trim().to_owned(),
        from: cfg.from.trim().to_owned(),
        subject: format!(
            "{} — {}",
            title.unwrap_or("Deckle Daily"),
            crate::long_date(date)
        ),
        body: "Today's Deckle Daily is attached; Send-to-Kindle puts it in your Kindle's Docs."
            .into(),
        attachment: (format!("{stem}.epub"), epub),
    }
}

/// Emails the paper at `epub` when `[email]` is on; `None` when it's off. `connect` makes the
/// mailer once the password is known (`SmtpMailer::new` outside tests).
pub fn email_edition<M: Mailer>(
    cfg: &EmailConfig,
    date: NaiveDate,
    epub: &Path,
    keys: &dyn Keyring,
    env: &dyn Fn(&str) -> Option<String>,
    connect: impl FnOnce(Zeroizing<String>) -> M,
) -> Option<EmailReport> {
    let stem = format!("{}{date}", crate::EDITION_PREFIX);
    email_paper(cfg, None, &stem, date, epub, keys, env, connect)
}

/// [`email_edition`] for one paper (many papers §4): its own `to`, on the account `cfg`
/// carries; `title` `None` for the main paper.
#[allow(clippy::too_many_arguments)]
pub fn email_paper<M: Mailer>(
    cfg: &EmailConfig,
    title: Option<&str>,
    stem: &str,
    date: NaiveDate,
    epub: &Path,
    keys: &dyn Keyring,
    env: &dyn Fn(&str) -> Option<String>,
    connect: impl FnOnce(Zeroizing<String>) -> M,
) -> Option<EmailReport> {
    if !cfg.enabled {
        return None;
    }
    let sent = send_file(cfg, epub, keys, env, connect, |bytes| {
        paper_message(cfg, title, stem, date, bytes)
    });
    Some(match sent {
        Ok(src) => EmailReport {
            emailed: Some(cfg.to.trim().to_owned()),
            email_error: None,
            password_from: Some(src),
        },
        Err(e) => EmailReport {
            emailed: None,
            email_error: Some(e.to_string()),
            password_from: None,
        },
    })
}

/// `digest email test`: a one-page EPUB, written in `dir` and sent whether or not `[email]`
/// is enabled.
pub fn send_test<M: Mailer>(
    cfg: &EmailConfig,
    dir: &Path,
    date: NaiveDate,
    keys: &dyn Keyring,
    env: &dyn Fn(&str) -> Option<String>,
    connect: impl FnOnce(Zeroizing<String>) -> M,
) -> Result<Source, MailError> {
    let name = format!("{}email-test.epub", crate::EDITION_PREFIX);
    let path = dir.join(&name);
    write_test_epub(&path, date).map_err(MailError::Io)?;
    send_file(cfg, &path, keys, env, connect, |bytes| Outgoing {
        subject: "Deckle Daily — email test".into(),
        body: "If this reached your Kindle, Deckle Daily email works.".into(),
        attachment: (name.clone(), bytes),
        ..edition_message(cfg, date, Vec::new())
    })
}

/// The checks every send makes, in order: the config, the size, the password, then the send.
fn send_file<M: Mailer>(
    cfg: &EmailConfig,
    path: &Path,
    keys: &dyn Keyring,
    env: &dyn Fn(&str) -> Option<String>,
    connect: impl FnOnce(Zeroizing<String>) -> M,
    message: impl FnOnce(Vec<u8>) -> Outgoing,
) -> Result<Source, MailError> {
    cfg.validate().map_err(MailError::Config)?;
    let missing = cfg.missing();
    if !missing.is_empty() {
        return Err(MailError::Config(format!(
            "[email] needs {} to send",
            missing.join(", ")
        )));
    }
    let bytes = std::fs::metadata(path)
        .map_err(|e| MailError::Io(format!("{}: {e}", path.display())))?
        .len();
    if bytes > MAX_BYTES {
        return Err(MailError::TooLarge { bytes });
    }
    let (password, src) =
        secret::smtp_password(cfg, keys, env).map_err(|e| MailError::NoPassword(e.to_string()))?;
    let epub =
        std::fs::read(path).map_err(|e| MailError::Io(format!("{}: {e}", path.display())))?;
    connect(password).send(&message(epub))?;
    Ok(src)
}

/// One page: "If this reached your Kindle, Deckle Daily email works."
pub fn write_test_epub(path: &Path, date: NaiveDate) -> Result<(), String> {
    let gathered = crate::Gathered {
        edition: crate::Edition {
            paper: "Deckle Daily email test".into(),
            date,
            place: String::new(),
            weather_line: None,
            masthead: Default::default(),
            sections: vec![crate::Section {
                weather: None,
                puzzle: None,
                brief: Vec::new(),
                key: String::new(),
                title: "Email test".into(),
                kicker: None,
                articles: vec![crate::Article {
                    title: "It works".into(),
                    body: "<p>If this reached your Kindle, Deckle Daily email works.</p>".into(),
                    ..Default::default()
                }],
                unavailable: None,
            }],
            assets: Vec::new(),
        },
        news: Vec::new(),
        reading: None,
        long_reads: Vec::new(),
        reading_outcomes: Vec::new(),
        notes: Vec::new(),
    };
    crate::write_edition(&gathered, path)
}

/// The owner's SMTP account, over rustls: 587 with STARTTLS or 465 with TLS from the start.
pub struct SmtpMailer {
    host: String,
    port: u16,
    username: String,
    password: Zeroizing<String>,
}

impl std::fmt::Debug for SmtpMailer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmtpMailer")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

impl SmtpMailer {
    pub fn new(cfg: &EmailConfig, password: Zeroizing<String>) -> Self {
        SmtpMailer {
            host: cfg.smtp_host.trim().to_owned(),
            port: cfg.smtp_port,
            username: cfg.username.trim().to_owned(),
            password,
        }
    }
}

impl Mailer for SmtpMailer {
    fn send(&self, msg: &Outgoing) -> Result<(), MailError> {
        use lettre::Transport;
        use lettre::message::{Attachment, MultiPart, SinglePart, header::ContentType};

        let relay = match self.port {
            465 => lettre::SmtpTransport::relay(&self.host),
            587 => lettre::SmtpTransport::starttls_relay(&self.host),
            p => {
                return Err(MailError::Config(format!(
                    "smtp_port {p}: only 587 (STARTTLS) and 465 (TLS)"
                )));
            }
        }
        .map_err(|e| MailError::Config(format!("smtp_host {:?}: {e}", self.host)))?;
        let address = |what: &str, a: &str| {
            a.parse::<lettre::message::Mailbox>()
                .map_err(|e| MailError::Config(format!("[email] {what} = {a:?}: {e}")))
        };
        let epub = ContentType::parse("application/epub+zip").expect("a valid MIME type");
        let email = lettre::Message::builder()
            .from(address("from", &msg.from)?)
            .to(address("to", &msg.to)?)
            .subject(msg.subject.clone())
            .multipart(
                MultiPart::mixed()
                    .singlepart(SinglePart::plain(msg.body.clone()))
                    .singlepart(
                        Attachment::new(msg.attachment.0.clone())
                            .body(msg.attachment.1.clone(), epub),
                    ),
            )
            .map_err(|e| MailError::Config(e.to_string()))?;
        // lettre keeps its own copy of the password for the session; ours is zeroed on drop.
        let creds = lettre::transport::smtp::authentication::Credentials::new(
            self.username.clone(),
            self.password.to_string(),
        );
        let transport = relay
            .port(self.port)
            .credentials(creds)
            .timeout(Some(Duration::from_secs(60)))
            .build();
        transport.send(&email).map(|_| ()).map_err(smtp_error)
    }
}

/// 530/534/535 are login refusals; any other answer is a refusal of the email; no answer at
/// all is the network.
fn smtp_error(e: lettre::transport::smtp::Error) -> MailError {
    match e.status().map(|c| c.to_string()) {
        Some(code) if matches!(code.as_str(), "530" | "534" | "535") => {
            MailError::Auth(e.to_string())
        }
        Some(_) => MailError::Rejected(e.to_string()),
        None => MailError::Network(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::{Keyring, MemoryKeyring, Source};
    use std::cell::RefCell;
    use std::path::PathBuf;

    const PW: &str = "hunter2-Secret!";

    fn cfg() -> EmailConfig {
        EmailConfig {
            enabled: true,
            to: "me_1@kindle.com".into(),
            from: "me@example.com".into(),
            smtp_host: "smtp.example.com".into(),
            smtp_port: 587,
            username: "me@example.com".into(),
            usb_too: false,
        }
    }

    #[test]
    fn each_paper_has_its_subject_and_attachment() {
        let m = edition_message(&cfg(), date(), Vec::new());
        assert_eq!(m.subject, "Deckle Daily — Wednesday, October 21, 2026");
        assert_eq!(m.attachment.0, "deckle-daily-2026-10-21.epub");
        let m = paper_message(
            &cfg(),
            Some("The Kid Courier"),
            "deckle-daily-kid-courier-2026-10-21",
            date(),
            Vec::new(),
        );
        assert_eq!(m.subject, "The Kid Courier — Wednesday, October 21, 2026");
        assert_eq!(m.attachment.0, "deckle-daily-kid-courier-2026-10-21.epub");
    }

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 21).unwrap()
    }

    fn keys() -> MemoryKeyring {
        let k = MemoryKeyring::default();
        k.set(&crate::secret::account(&cfg()), PW).unwrap();
        k
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    /// Records what it was asked to send; fails with `fail` when set.
    #[derive(Default)]
    struct Recording {
        sent: RefCell<Vec<Outgoing>>,
        fail: Option<MailError>,
    }

    impl Mailer for &Recording {
        fn send(&self, msg: &Outgoing) -> Result<(), MailError> {
            self.sent.borrow_mut().push(msg.clone());
            match &self.fail {
                Some(e) => Err(e.clone()),
                None => Ok(()),
            }
        }
    }

    fn paper(dir: &Path) -> PathBuf {
        let p = dir.join("deckle-daily-2026-10-21.epub");
        std::fs::write(&p, b"PK fake epub").unwrap();
        p
    }

    #[test]
    fn the_edition_goes_out_as_a_dated_epub() {
        let dir = tempfile::tempdir().unwrap();
        let rec = Recording::default();
        let got_pw = RefCell::new(String::new());
        let r = email_edition(&cfg(), date(), &paper(dir.path()), &keys(), &no_env, |pw| {
            *got_pw.borrow_mut() = pw.to_string();
            &rec
        })
        .unwrap();
        assert_eq!(r.emailed.as_deref(), Some("me_1@kindle.com"));
        assert_eq!(r.email_error, None);
        assert_eq!(r.password_from, Some(Source::Keychain));
        assert_eq!(
            *got_pw.borrow(),
            PW,
            "the mailer gets the keychain's password"
        );
        let sent = rec.sent.borrow();
        assert_eq!(sent.len(), 1);
        let m = &sent[0];
        assert_eq!(m.to, "me_1@kindle.com");
        assert_eq!(m.from, "me@example.com");
        assert_eq!(m.subject, "Deckle Daily — Wednesday, October 21, 2026");
        assert!(!m.body.is_empty() && !m.body.contains('\n'), "one line");
        assert_eq!(m.attachment.0, "deckle-daily-2026-10-21.epub");
        assert_eq!(m.attachment.1, b"PK fake epub");
        assert!(!r.to_usb(&cfg()), "emailed: no second copy over USB");
        assert!(r.to_usb(&EmailConfig {
            usb_too: true,
            ..cfg()
        }));
    }

    #[test]
    fn email_off_sends_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let rec = Recording::default();
        let off = EmailConfig {
            enabled: false,
            ..cfg()
        };
        assert!(
            email_edition(&off, date(), &paper(dir.path()), &keys(), &no_env, |_| &rec).is_none()
        );
        assert!(rec.sent.borrow().is_empty());
    }

    fn failed(r: &EmailReport, needle: &str) {
        assert_eq!(r.emailed, None);
        let e = r.email_error.as_deref().unwrap_or_default();
        assert!(e.contains(needle), "{needle:?} not in {e:?}");
        assert!(r.to_usb(&cfg()), "a failed email falls back to USB");
    }

    #[test]
    fn an_auth_error_falls_back_to_usb() {
        let dir = tempfile::tempdir().unwrap();
        let rec = Recording {
            fail: Some(MailError::Auth(
                "535 5.7.8 Username and Password not accepted".into(),
            )),
            ..Default::default()
        };
        let r = email_edition(&cfg(), date(), &paper(dir.path()), &keys(), &no_env, |_| {
            &rec
        })
        .unwrap();
        failed(&r, "not accepted");
        failed(&r, "app password");
    }

    #[test]
    fn a_network_error_falls_back_to_usb() {
        let dir = tempfile::tempdir().unwrap();
        let rec = Recording {
            fail: Some(MailError::Network("connection timed out".into())),
            ..Default::default()
        };
        let r = email_edition(&cfg(), date(), &paper(dir.path()), &keys(), &no_env, |_| {
            &rec
        })
        .unwrap();
        failed(&r, "timed out");
    }

    #[test]
    fn a_paper_over_45_mb_isnt_emailed() {
        let dir = tempfile::tempdir().unwrap();
        let p = paper(dir.path());
        // Sparse: no 45 MB written to disk.
        std::fs::File::options()
            .write(true)
            .open(&p)
            .unwrap()
            .set_len(MAX_BYTES + 1)
            .unwrap();
        let rec = Recording::default();
        let r = email_edition(&cfg(), date(), &p, &keys(), &no_env, |_| &rec).unwrap();
        failed(&r, "45 MB");
        assert!(rec.sent.borrow().is_empty(), "never sent");
    }

    #[test]
    fn no_password_falls_back_to_usb() {
        let dir = tempfile::tempdir().unwrap();
        let rec = Recording::default();
        let r = email_edition(
            &cfg(),
            date(),
            &paper(dir.path()),
            &MemoryKeyring::default(),
            &no_env,
            |_| &rec,
        )
        .unwrap();
        failed(&r, "deckle digest email password");
        assert!(rec.sent.borrow().is_empty());
    }

    #[test]
    fn an_unfinished_email_table_says_what_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let rec = Recording::default();
        let half = EmailConfig {
            from: String::new(),
            ..cfg()
        };
        let r = email_edition(&half, date(), &paper(dir.path()), &keys(), &no_env, |_| {
            &rec
        })
        .unwrap();
        failed(&r, "from");
        assert!(rec.sent.borrow().is_empty());
    }

    #[test]
    fn the_env_password_is_reported_as_such() {
        let dir = tempfile::tempdir().unwrap();
        let rec = Recording::default();
        let env = |k: &str| (k == crate::secret::ENV).then(|| PW.to_owned());
        let r = email_edition(
            &cfg(),
            date(),
            &paper(dir.path()),
            &MemoryKeyring::unavailable(),
            &env,
            |_| &rec,
        )
        .unwrap();
        assert_eq!(r.password_from, Some(Source::Env));
        assert!(r.emailed.is_some());
    }

    #[test]
    fn smtp_refuses_plain_text_before_connecting() {
        let m = SmtpMailer::new(
            &EmailConfig {
                smtp_port: 25,
                ..cfg()
            },
            Zeroizing::new(PW.to_owned()),
        );
        let msg = edition_message(&cfg(), date(), b"x".to_vec());
        let e = m.send(&msg).unwrap_err();
        assert!(matches!(e, MailError::Config(_)), "{e}");
    }

    #[test]
    fn the_password_never_shows() {
        let dir = tempfile::tempdir().unwrap();
        let mut shown = Vec::new();
        let errors = [
            MailError::Auth("535 bad credentials".into()),
            MailError::Network("refused".into()),
        ];
        for e in errors {
            let rec = Recording {
                fail: Some(e),
                ..Default::default()
            };
            let r = email_edition(&cfg(), date(), &paper(dir.path()), &keys(), &no_env, |_| {
                &rec
            })
            .unwrap();
            shown.push(format!("{r:?} {}", serde_json::to_string(&r).unwrap()));
        }
        let rec = Recording::default();
        let r = email_edition(&cfg(), date(), &paper(dir.path()), &keys(), &no_env, |_| {
            &rec
        })
        .unwrap();
        shown.push(format!("{r:?} {}", serde_json::to_string(&r).unwrap()));
        shown.push(format!("{:?}", rec.sent.borrow()));
        let m = SmtpMailer::new(&cfg(), Zeroizing::new(PW.to_owned()));
        shown.push(format!("{m:?}"));
        let e = SmtpMailer::new(
            &EmailConfig {
                smtp_port: 25,
                ..cfg()
            },
            Zeroizing::new(PW.to_owned()),
        )
        .send(&edition_message(&cfg(), date(), Vec::new()))
        .unwrap_err();
        shown.push(format!("{e} {e:?}"));
        for s in shown {
            assert!(!s.contains(PW), "{s}");
        }
    }

    #[test]
    fn the_test_epub_says_it_works() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.epub");
        write_test_epub(&p, date()).unwrap();
        let mut z = zip::ZipArchive::new(std::fs::File::open(&p).unwrap()).unwrap();
        assert_eq!(z.by_index(0).unwrap().name().unwrap(), "mimetype");
        let mut all = String::new();
        for i in 0..z.len() {
            let mut f = z.by_index(i).unwrap();
            if f.name().unwrap().ends_with(".xhtml") {
                std::io::Read::read_to_string(&mut f, &mut all).unwrap();
            }
        }
        assert!(
            all.contains("If this reached your Kindle, Deckle Daily email works."),
            "{all}"
        );
    }

    #[test]
    fn a_test_send_uses_the_test_epub() {
        let dir = tempfile::tempdir().unwrap();
        let rec = Recording::default();
        let off = EmailConfig {
            enabled: false,
            ..cfg()
        };
        let src = send_test(&off, dir.path(), date(), &keys(), &no_env, |_| &rec).unwrap();
        assert_eq!(src, Source::Keychain);
        let sent = rec.sent.borrow();
        assert_eq!(sent.len(), 1, "a test goes out even with email off");
        assert_eq!(sent[0].attachment.0, "deckle-daily-email-test.epub");
        assert!(sent[0].subject.contains("test"));
    }
}
