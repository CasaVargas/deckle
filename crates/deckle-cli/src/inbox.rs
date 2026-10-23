// SPDX-License-Identifier: GPL-3.0-or-later
//! Books in the Inbox (save-to-paper design §10): each one is prepared (or, for a KFX,
//! copied as it is) into the outbox, which `deckle digest push`/`watch` finishes for the Kindle
//! that takes it and delivers. `deckle-digest` finds and moves the files; this is the
//! conversion step it can't do.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use deckle_core::deliver::{Target, device_filename};
use deckle_core::pipeline::{RunOptions, XRayMode, prepare};
use deckle_core::queue::Queue;
use deckle_digest::inbox::{ScanReport, Skip, scan_with_books};
use deckle_digest::reading::ReadingQueue;

/// Puts a book (not a KFX) into the outbox; the real one is [`pipeline`].
pub(crate) type Convert<'a> = dyn FnMut(&Path, &Queue) -> Result<(), Skip> + 'a;

/// Scans `dir`: articles into `reading`, books into `outbox` through `convert`.
pub(crate) fn scan(
    dir: &Path,
    reading: &ReadingQueue,
    outbox: &Queue,
    now: SystemTime,
    convert: &mut Convert,
) -> ScanReport {
    scan_with_books(dir, reading, now, &mut |book| {
        send_later(book, outbox, convert)
    })
}

/// Adds one book to the outbox: a KFX as it is, anything else converted first.
fn send_later(book: &Path, outbox: &Queue, convert: &mut Convert) -> Result<(), Skip> {
    let retry = |e: std::io::Error| Skip::Retry(format!("outbox: {e}"));
    let is_kfx = book
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("kfx"));
    if is_kfx {
        // The outbox only holds `*.kfx`, so `Book.KFX` is queued as `Book.kfx`.
        let stem = book.file_stem().unwrap_or_default().to_string_lossy();
        outbox
            .add(book, &format!("{stem}.kfx"), None)
            .map_err(retry)?;
        return Ok(());
    }
    convert(book, outbox)
}

/// The `deckle run` pipeline up to the KPF, queued as a prepared book named like `deckle run`
/// names it. At push it gets the cover rule's choice for that Kindle, X-Ray and Word Wise at
/// the default level (device-aware build §4.5). Kindle Previewer is looked for once, at the
/// first book; without it books wait in the Inbox.
pub(crate) fn pipeline() -> impl FnMut(&Path, &Queue) -> Result<(), Skip> {
    pipeline_with(None)
}

/// [`pipeline`] with Kindle Previewer at `previewer` (else found as `deckle run` finds it).
pub(crate) fn pipeline_with(
    previewer: Option<PathBuf>,
) -> impl FnMut(&Path, &Queue) -> Result<(), Skip> {
    let mut located: Option<Result<PathBuf, String>> = None;
    move |book, outbox| {
        let pv = located
            .get_or_insert_with(|| {
                deckle_core::previewer::locate(previewer.as_deref())
                    .map(|l| l.path)
                    .map_err(|e| e.to_string())
            })
            .clone()
            .map_err(|e| Skip::Retry(format!("can't convert books yet: {e}")))?;
        let opts = RunOptions {
            cde_type: deckle_kfx::convert::CdeType::Pdoc,
            cde_override: None,
            amazon_match: false,
            generated_cover: Some(None),
            metadata_fill: false,
            typography: false,
            popup_notes: false,
            verified_real_asin: false,
            asin: None,
            pages: None,
            // `prepare` delivers nothing: the Kindle is known at push.
            target: Target::Folder(std::env::temp_dir()),
            previewer: Some(pv),
            keep_work_dir: false,
            xray: XRayMode::Auto,
            llm: None,
            wikipedia: false,
            word_wise: Some(Default::default()),
            replaces: Vec::new(),
            originals: None,
        };
        let prepared = prepare(book, &opts, &mut |_| {})
            .map_err(|r| Skip::Unreadable(r.error.unwrap_or_else(|| "not converted".into())))?;
        let name = device_filename(&prepared.report.info);
        outbox
            .add_prepared(&prepared, &name, None)
            .map_err(|e| match e.kind() {
                // the book converted but can't build even for a folder
                std::io::ErrorKind::InvalidData => Skip::Unreadable(e.to_string()),
                _ => Skip::Retry(format!("outbox: {e}")),
            })?;
        Ok(())
    }
}

/// `kindle` for each book, after a copy for a Kobo or a PocketBook when `kobo` is their outbox
/// (other readers design §8): imported and cleaned, which needs no Previewer. A book the
/// e-reader outbox took leaves the Inbox even when the Kindle's copy has to wait for Previewer.
pub(crate) fn with_readers(
    kobo: Option<Queue>,
    mut kindle: impl FnMut(&Path, &Queue) -> Result<(), Skip>,
) -> impl FnMut(&Path, &Queue) -> Result<(), Skip> {
    move |book, outbox| {
        let Some(k) = &kobo else {
            return kindle(book, outbox);
        };
        let bytes = deckle_core::reader::book_epub(book).map_err(Skip::Unreadable)?;
        let stem = book.file_stem().unwrap_or_default().to_string_lossy();
        let name = format!("{}.epub", deckle_core::deliver::sanitize(&stem));
        let write = || -> std::io::Result<()> {
            std::fs::create_dir_all(&k.dir)?;
            let tmp = k.dir.join(format!("{name}.part"));
            std::fs::write(&tmp, &bytes)?;
            std::fs::rename(&tmp, k.dir.join(&name))
        };
        write().map_err(|e| Skip::Retry(format!("e-reader outbox: {e}")))?;
        match kindle(book, outbox) {
            Err(Skip::Retry(_)) => Ok(()),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    struct Setup {
        _tmp: tempfile::TempDir,
        inbox: PathBuf,
        reading: ReadingQueue,
        outbox: Queue,
    }

    fn setup() -> Setup {
        let tmp = tempfile::tempdir().unwrap();
        let inbox = tmp.path().join("Inbox");
        std::fs::create_dir_all(&inbox).unwrap();
        Setup {
            reading: ReadingQueue::new(tmp.path().join("reading.jsonl")),
            outbox: Queue::new(tmp.path().join("outbox")),
            inbox,
            _tmp: tmp,
        }
    }

    fn later() -> SystemTime {
        SystemTime::now() + Duration::from_secs(60)
    }

    /// Stands in for Previewer: queues `<stem>.kfx` and counts its calls.
    fn fake(calls: &mut Vec<PathBuf>) -> impl FnMut(&Path, &Queue) -> Result<(), Skip> + '_ {
        |book, outbox| {
            calls.push(book.to_path_buf());
            let stem = book.file_stem().unwrap().to_string_lossy().into_owned();
            let work = tempfile::tempdir().unwrap();
            let kfx = work.path().join("book.kfx");
            std::fs::write(&kfx, b"converted").unwrap();
            outbox.add(&kfx, &format!("{stem}.kfx"), None).unwrap();
            Ok(())
        }
    }

    fn names(q: &Queue) -> Vec<String> {
        q.pending()
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn an_epub_is_converted_into_the_outbox_in_one_scan() {
        let s = setup();
        std::fs::write(s.inbox.join("Dune.epub"), b"epub").unwrap();
        std::fs::write(s.inbox.join("Notes.docx"), b"docx").unwrap();
        let mut calls = Vec::new();
        let r = scan(
            &s.inbox,
            &s.reading,
            &s.outbox,
            later(),
            &mut fake(&mut calls),
        );
        assert_eq!(calls.len(), 2);
        assert_eq!(r.sent.len(), 2, "{:?} {:?}", r.unreadable, r.retry);
        assert_eq!(names(&s.outbox), ["Dune.kfx", "Notes.kfx"]);
        assert!(s.inbox.join("Sent/Dune.epub").exists());
        assert!(s.inbox.join("Sent/Notes.docx").exists());
        assert!(!s.inbox.join("Dune.epub").exists());
    }

    #[test]
    fn a_kfx_is_queued_as_it_is() {
        let s = setup();
        std::fs::write(s.inbox.join("Ready.KFX"), b"KFX bytes").unwrap();
        let mut calls = Vec::new();
        let r = scan(
            &s.inbox,
            &s.reading,
            &s.outbox,
            later(),
            &mut fake(&mut calls),
        );
        assert!(calls.is_empty(), "no conversion for a KFX");
        assert_eq!(r.sent.len(), 1);
        assert_eq!(names(&s.outbox), ["Ready.kfx"]);
        assert_eq!(std::fs::read(&s.outbox.pending()[0]).unwrap(), b"KFX bytes");
        assert!(s.inbox.join("Sent/Ready.KFX").exists());
    }

    #[test]
    fn a_failed_conversion_goes_to_unreadable_with_the_reason() {
        let s = setup();
        std::fs::write(s.inbox.join("Broken.epub"), b"junk").unwrap();
        let mut failing = |_: &Path, _: &Queue| -> Result<(), Skip> {
            Err(Skip::Unreadable("inspect: not a zip".into()))
        };
        let r = scan(&s.inbox, &s.reading, &s.outbox, later(), &mut failing);
        assert!(r.sent.is_empty());
        assert!(s.outbox.pending().is_empty());
        assert!(s.inbox.join("Unreadable/Broken.epub").exists());
        let why = std::fs::read_to_string(s.inbox.join("Unreadable/Broken.epub.txt")).unwrap();
        assert_eq!(why, "inspect: not a zip\n");
    }

    #[test]
    fn without_previewer_a_book_waits_in_the_inbox() {
        let s = setup();
        std::fs::write(s.inbox.join("Later.epub"), b"epub").unwrap();
        let mut missing = |_: &Path, _: &Queue| -> Result<(), Skip> {
            Err(Skip::Retry("Kindle Previewer not found".into()))
        };
        let r = scan(&s.inbox, &s.reading, &s.outbox, later(), &mut missing);
        assert_eq!(r.retry.len(), 1);
        assert!(s.inbox.join("Later.epub").exists());
        assert!(s.outbox.pending().is_empty());
    }

    #[test]
    fn a_fresh_book_waits_for_the_next_scan() {
        let s = setup();
        std::fs::write(s.inbox.join("Copying.epub"), b"epub").unwrap();
        let mut calls = Vec::new();
        let r = scan(
            &s.inbox,
            &s.reading,
            &s.outbox,
            SystemTime::now(),
            &mut fake(&mut calls),
        );
        assert!(calls.is_empty());
        assert!(r.sent.is_empty());
        assert!(s.inbox.join("Copying.epub").exists());
        let r = scan(
            &s.inbox,
            &s.reading,
            &s.outbox,
            later(),
            &mut fake(&mut calls),
        );
        assert_eq!(r.sent.len(), 1);
    }

    #[test]
    fn articles_still_go_to_your_reading() {
        let s = setup();
        std::fs::write(
            s.inbox.join("link.url"),
            "[InternetShortcut]\nURL=https://ex.com/a\n",
        )
        .unwrap();
        std::fs::write(s.inbox.join("Book.epub"), b"epub").unwrap();
        let mut calls = Vec::new();
        let r = scan(
            &s.inbox,
            &s.reading,
            &s.outbox,
            later(),
            &mut fake(&mut calls),
        );
        assert_eq!(r.queued, 1);
        assert_eq!(r.sent.len(), 1);
        assert!(s.inbox.join("Saved/link.url").exists());
        assert_eq!(s.reading.load()[0].url.as_deref(), Some("https://ex.com/a"));
        assert_eq!(names(&s.outbox), ["Book.kfx"]);
    }

    /// Device-aware build §4.5: an Inbox book is queued up to the KPF, asking for X-Ray and
    /// Word Wise, and finished at push for the Kindle that takes it.
    #[test]
    #[cfg(unix)]
    fn a_book_is_queued_prepared_asking_for_xray_and_word_wise() {
        let s = setup();
        std::fs::copy(
            deckle_kfx::fixtures::path("deckle-mini.epub"),
            s.inbox.join("Mini.epub"),
        )
        .unwrap();
        let r = scan(
            &s.inbox,
            &s.reading,
            &s.outbox,
            later(),
            &mut pipeline_with(Some(crate::tests::fake_previewer())),
        );
        assert_eq!(r.sent.len(), 1, "{:?} {:?}", r.unreadable, r.retry);
        assert_eq!(
            names(&s.outbox),
            ["Testwright, Jane - Deckle Mini Fixture.book"]
        );
        let entry = &s.outbox.pending()[0];
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(entry.join("prepared.json")).unwrap()).unwrap();
        assert_eq!(json["wishes"]["xray"], "auto");
        assert_eq!(json["wishes"]["word_wise"], 5);
        assert_eq!(json["wishes"]["cde_override"], serde_json::Value::Null);

        let kindle = s._tmp.path().join("kindle");
        let flushed = s
            .outbox
            .flush(&Target::Folder(kindle.clone()), None)
            .unwrap();
        assert_eq!(
            flushed.delivered,
            [kindle.join("Testwright, Jane - Deckle Mini Fixture.kfx")]
        );
    }

    #[test]
    fn with_no_kindle_the_book_stays_queued_for_push() {
        // Scanning never delivers: the book waits in the outbox until a push or watch finds
        // a Kindle, which then flushes it like the edition.
        let s = setup();
        std::fs::write(s.inbox.join("Queued.epub"), b"epub").unwrap();
        let mut calls = Vec::new();
        scan(
            &s.inbox,
            &s.reading,
            &s.outbox,
            later(),
            &mut fake(&mut calls),
        );
        scan(
            &s.inbox,
            &s.reading,
            &s.outbox,
            later(),
            &mut fake(&mut calls),
        );
        assert_eq!(names(&s.outbox), ["Queued.kfx"]);
        let kindle = s._tmp.path().join("kindle");
        std::fs::create_dir_all(&kindle).unwrap();
        let flushed = s
            .outbox
            .flush(&Target::Folder(kindle.clone()), None)
            .unwrap();
        assert_eq!(flushed.delivered.len(), 1);
        assert!(kindle.join("Queued.kfx").exists());
        assert!(s.outbox.pending().is_empty());
    }
}
