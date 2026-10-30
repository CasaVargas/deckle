// SPDX-License-Identifier: GPL-3.0-or-later
//! Tomorrow's paper, so far, for the Daily tab (tomorrow's paper design §4–§7): the view from
//! local files, the polite headline refresh, and Stop the presses.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use deckle_digest::chrono::{DateTime, Utc};
use deckle_digest::chrono_tz::Tz;
use deckle_digest::next::plan::NextPaper;

/// Paper `paper`'s config (`None`: the main paper at `digest`), with its stop and reading queue
/// beside `digest` (as they are beside the real one).
fn config_at(digest: &Path, paper: Option<&str>) -> Result<deckle_digest::Config, String> {
    let mut cfg = match digest.exists() {
        true => {
            deckle_digest::papers::load_one(digest, paper.unwrap_or(deckle_digest::papers::MAIN))?
        }
        false => deckle_digest::Config::parse(deckle_digest::config::STARTER)?,
    };
    cfg.press_path = Some(digest.with_file_name("press.json"));
    if cfg.reading.queue_path.is_none() {
        cfg.reading.queue_path = Some(digest.with_file_name("reading.jsonl"));
    }
    Ok(cfg)
}

/// Tomorrow's paper from local files: `cache` is the digest cache folder (`next.json`, and the
/// EPUBs `digest run` leaves, in the paper's own work folder). `readership` is the main
/// paper's file; another paper keeps its own.
pub fn next_at(
    digest: &Path,
    paper: Option<&str>,
    readership: &Path,
    cache: &Path,
    now: DateTime<Tz>,
    at: Option<(u8, u8)>,
) -> Result<NextPaper, String> {
    let cfg = config_at(digest, paper)?;
    let readership = cfg
        .state_file("readership.json")
        .unwrap_or_else(|| readership.to_path_buf());
    let built = match cache.parent() {
        Some(base) => cfg.work_dir(base),
        None => cache.to_path_buf(),
    };
    Ok(deckle_readership::sizes::next_paper(
        &cfg,
        now,
        at,
        &readership,
        &cache.join("next.json"),
        &built,
    ))
}

/// Stops the next scheduled edition (`on`), or restarts the presses.
pub fn stop_at(
    digest: &Path,
    on: bool,
    at: Option<(u8, u8)>,
    now: DateTime<Tz>,
) -> Result<(), String> {
    let cfg = config_at(digest, None)?;
    let stop = match on {
        true => Some(
            deckle_digest::press::press(at, None, now)
                .next
                .map(|n| n.date_naive())
                .ok_or("nothing is scheduled; there's no press to stop")?,
        ),
        false => None,
    };
    deckle_digest::press::write_stop(&cfg.press_state(), stop)
}

fn digest_path() -> PathBuf {
    deckle_digest::config::default_path()
}

fn cache_dir() -> PathBuf {
    deckle_core::wordwise::default_cache_dir().join("digest")
}

/// The installed time of paper `paper`'s job (`None`: the main paper).
fn schedule_at(paper: Option<&str>) -> Option<(u8, u8)> {
    let slug = paper.unwrap_or(deckle_digest::papers::MAIN);
    deckle_digest::schedule::status()
        .papers
        .into_iter()
        .find(|p| p.slug == slug)
        .and_then(|p| p.at)
        .and_then(|a| deckle_digest::schedule::parse_at(&a).ok())
}

fn now() -> DateTime<Tz> {
    Utc::now().with_timezone(&deckle_digest::tz::local_tz())
}

pub fn daily_next(paper: Option<String>) -> Result<NextPaper, String> {
    next_at(
        &digest_path(),
        paper.as_deref(),
        &deckle_readership::state_path(),
        &cache_dir(),
        now(),
        schedule_at(paper.as_deref()),
    )
}

/// Runs `f` unless another call holding `flag` is still running; says whether it ran.
fn single_flight(flag: &AtomicBool, f: impl FnOnce()) -> bool {
    if flag.swap(true, Ordering::AcqRel) {
        return false;
    }
    f();
    flag.store(false, Ordering::Release);
    true
}

/// A refresh in progress: a focus while one runs doesn't start a second over the same feeds.
static REFRESHING: AtomicBool = AtomicBool::new(false);

/// Checks the feeds for headlines (each URL at most twice an hour), then the view.
pub fn daily_next_refresh(paper: Option<String>) -> Result<NextPaper, String> {
    let mut out = Ok(());
    single_flight(&REFRESHING, || {
        out = config_at(&digest_path(), paper.as_deref()).and_then(|cfg| {
            let path = cache_dir().join("next.json");
            let mut cache = deckle_digest::next::live::LiveCache::load(&path);
            deckle_digest::next::live::refresh(&cfg, &mut cache, Utc::now(), &deckle_digest::Http);
            cache.save(&path)
        });
    });
    out?;
    daily_next(paper)
}

/// Stop the presses is the household's (one `press.json`, as in the CLI): it stops the
/// selected paper's next edition date for every paper.
pub fn press_stop(on: bool, paper: Option<String>) -> Result<NextPaper, String> {
    stop_at(&digest_path(), on, schedule_at(paper.as_deref()), now())?;
    daily_next(paper)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deckle_digest::chrono::{Days, TimeZone};

    fn now() -> DateTime<Tz> {
        let ny: Tz = "America/New_York".parse().unwrap();
        ny.with_ymd_and_hms(2026, 10, 22, 15, 8, 0)
            .single()
            .unwrap()
    }

    fn files() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let digest = dir.path().join("digest.toml");
        std::fs::write(
            &digest,
            "sections = [\"reading\", \"weather\", \"horoscope\"]\n",
        )
        .unwrap();
        (dir, digest)
    }

    #[test]
    fn next_at_reads_files_only() {
        let (dir, digest) = files();
        let p = next_at(
            &digest,
            None,
            &dir.path().join("readership.json"),
            &dir.path().join("cache"),
            now(),
            Some((5, 30)),
        )
        .unwrap();
        assert_eq!(p.date, now().date_naive() + Days::new(1));
        assert_eq!(p.plan[0].key, "reading");
        assert_eq!(p.week.len(), 6);
    }

    #[test]
    fn press_stop_round_trip() {
        let (dir, digest) = files();
        let paper = |d: &std::path::Path| {
            next_at(
                d,
                None,
                &dir.path().join("r.json"),
                &dir.path().join("c"),
                now(),
                Some((5, 30)),
            )
            .unwrap()
        };
        stop_at(&digest, true, Some((5, 30)), now()).unwrap();
        let p = paper(&digest);
        assert_eq!(p.press.stopped, Some(now().date_naive() + Days::new(1)));
        assert_eq!(p.date, now().date_naive() + Days::new(2));
        assert!(dir.path().join("press.json").exists());
        stop_at(&digest, false, Some((5, 30)), now()).unwrap();
        assert_eq!(paper(&digest).press.stopped, None);
    }

    #[test]
    fn another_paper_has_its_own_plan_and_sent_days() {
        let (dir, digest) = files();
        deckle_digest::papers::create(&digest, "kid", "Kid Courier", "Maya").unwrap();
        // Yesterday's Kid Courier was built, in its own work folder; the main paper's wasn't.
        let yesterday = now().date_naive() - Days::new(1);
        let work = dir.path().join("digest/papers/kid");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(
            work.join(format!("deckle-daily-kid-{yesterday}.epub")),
            b"x",
        )
        .unwrap();
        let cache = dir.path().join("digest");
        let p = next_at(
            &digest,
            Some("kid"),
            &dir.path().join("r.json"),
            &cache,
            now(),
            Some((7, 0)),
        )
        .unwrap();
        assert_eq!(p.paper, "Kid Courier");
        let main = next_at(
            &digest,
            None,
            &dir.path().join("r.json"),
            &cache,
            now(),
            Some((5, 30)),
        )
        .unwrap();
        let sent = |p: &NextPaper| {
            p.week
                .iter()
                .filter(|d| matches!(d.state, deckle_digest::next::WeekState::Sent))
                .count()
        };
        assert_eq!((sent(&p), sent(&main)), (1, 0));
    }

    #[test]
    fn press_stop_without_schedule_errors() {
        let (_dir, digest) = files();
        assert_eq!(
            stop_at(&digest, true, None, now()),
            Err("nothing is scheduled; there's no press to stop".to_owned())
        );
    }
}

#[cfg(test)]
mod flight_tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn one_refresh_at_a_time() {
        let flag = AtomicBool::new(false);
        let mut ran = 0;
        // while one runs, a second asks and is turned away
        assert!(single_flight(&flag, || {
            assert!(!single_flight(&flag, || unreachable!("ran twice")));
            ran += 1;
        }));
        // and the flag is free again after
        assert!(single_flight(&flag, || ran += 1));
        assert_eq!(ran, 2);
    }
}
