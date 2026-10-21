// SPDX-License-Identifier: GPL-3.0-or-later
//! Books in the Inbox (save-to-paper design §10): each one is converted (or, for a KFX,
//! copied as it is) into the outbox, which `deckle digest push`/`watch` delivers.
//! `deckle-digest` finds and moves the files; this is the conversion step it can't do.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use deckle_core::deliver::Target;
use deckle_core::pipeline::{RunOptions, XRayMode, run};
use deckle_core::queue::Queue;
use deckle_digest::inbox::{ScanReport, Skip, scan_with_books};
use deckle_digest::reading::ReadingQueue;

/// Turns a book into a `.kfx` inside the given folder; the real one is [`pipeline`].
pub(crate) type Convert<'a> = dyn FnMut(&Path, &Path) -> Result<PathBuf, Skip> + 'a;

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
    let work = tempfile::tempdir().map_err(retry)?;
    let kfx = convert(book, work.path())?;
    let name = kfx
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "book.kfx".into());
    outbox.add(&kfx, &name, None).map_err(retry)?;
    Ok(())
}

/// The `deckle run` pipeline with its defaults (the cover rule decides PDOC or EBOK). X-Ray
/// and Word Wise are off: the outbox carries only the `.kfx`, not sidecars. Kindle Previewer
/// is looked for once, at the first book; without it books wait in the Inbox.
pub(crate) fn pipeline() -> impl FnMut(&Path, &Path) -> Result<PathBuf, Skip> {
    let mut previewer: Option<Result<PathBuf, String>> = None;
    move |book, out| {
        let pv = previewer
            .get_or_insert_with(|| {
                deckle_core::previewer::locate(None)
                    .map(|l| l.path)
                    .map_err(|e| e.to_string())
            })
            .clone()
            .map_err(|e| Skip::Retry(format!("can't convert books yet: {e}")))?;
        let opts = RunOptions {
            cde_type: deckle_kfx::convert::CdeType::Pdoc,
            cde_override: None,
            amazon_match: false,
            verified_real_asin: false,
            asin: None,
            pages: None,
            target: Target::Folder(out.to_path_buf()),
            previewer: Some(pv),
            keep_work_dir: false,
            xray: XRayMode::Off,
            llm: None,
            wikipedia: false,
            word_wise: None,
        };
        let r = run(book, &opts);
        match (r.error, r.delivered) {
            (None, Some(kfx)) => Ok(kfx),
            (e, _) => Err(Skip::Unreadable(
                e.unwrap_or_else(|| "not converted".into()),
            )),
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

    /// Stands in for Previewer: writes `<stem>.kfx` and counts its calls.
    fn fake(calls: &mut Vec<PathBuf>) -> impl FnMut(&Path, &Path) -> Result<PathBuf, Skip> + '_ {
        |book, out| {
            calls.push(book.to_path_buf());
            let kfx = out.join(format!(
                "{}.kfx",
                book.file_stem().unwrap().to_string_lossy()
            ));
            std::fs::write(&kfx, b"converted").unwrap();
            Ok(kfx)
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
        let mut failing = |_: &Path, _: &Path| -> Result<PathBuf, Skip> {
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
        let mut missing = |_: &Path, _: &Path| -> Result<PathBuf, Skip> {
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
