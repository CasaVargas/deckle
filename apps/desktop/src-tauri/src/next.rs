// SPDX-License-Identifier: GPL-3.0-or-later
//! Tomorrow's paper, so far, for the Daily tab (tomorrow's paper design §4–§7): the view from
//! local files, the polite headline refresh, and Stop the presses.

use std::path::{Path, PathBuf};

use deckle_digest::chrono::{DateTime, Utc};
use deckle_digest::chrono_tz::Tz;
use deckle_digest::next::plan::NextPaper;

/// The digest config at `digest`, with its stop and reading queue beside it (as they are beside
/// the real one).
fn config_at(digest: &Path) -> Result<deckle_digest::Config, String> {
    let mut cfg = match digest.exists() {
        true => deckle_digest::Config::load(digest)?,
        false => deckle_digest::Config::parse(deckle_digest::config::STARTER)?,
    };
    cfg.press_path = Some(digest.with_file_name("press.json"));
    if cfg.reading.queue_path.is_none() {
        cfg.reading.queue_path = Some(digest.with_file_name("reading.jsonl"));
    }
    Ok(cfg)
}

/// Tomorrow's paper from local files: `cache` is the digest cache folder (`next.json`, and the
/// EPUBs `digest run` leaves).
pub fn next_at(
    digest: &Path,
    readership: &Path,
    cache: &Path,
    now: DateTime<Tz>,
    at: Option<(u8, u8)>,
) -> Result<NextPaper, String> {
    let cfg = config_at(digest)?;
    Ok(deckle_readership::sizes::next_paper(
        &cfg,
        now,
        at,
        readership,
        &cache.join("next.json"),
        cache,
    ))
}

/// Stops the next scheduled edition (`on`), or restarts the presses.
pub fn stop_at(
    digest: &Path,
    on: bool,
    at: Option<(u8, u8)>,
    now: DateTime<Tz>,
) -> Result<(), String> {
    let cfg = config_at(digest)?;
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

fn schedule_at() -> Option<(u8, u8)> {
    deckle_digest::schedule::status()
        .at
        .and_then(|a| deckle_digest::schedule::parse_at(&a).ok())
}

fn now() -> DateTime<Tz> {
    Utc::now().with_timezone(&deckle_digest::tz::local_tz())
}

#[tauri::command]
pub fn daily_next() -> Result<NextPaper, String> {
    next_at(
        &digest_path(),
        &deckle_readership::state_path(),
        &cache_dir(),
        now(),
        schedule_at(),
    )
}

/// Checks the feeds for headlines (each URL at most twice an hour), then the view.
#[tauri::command]
pub async fn daily_next_refresh() -> Result<NextPaper, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let cfg = config_at(&digest_path())?;
        let path = cache_dir().join("next.json");
        let mut cache = deckle_digest::next::live::LiveCache::load(&path);
        deckle_digest::next::live::refresh(&cfg, &mut cache, Utc::now(), &deckle_digest::Http);
        cache.save(&path)
    })
    .await
    .map_err(|e| e.to_string())??;
    daily_next()
}

#[tauri::command]
pub fn press_stop(on: bool) -> Result<NextPaper, String> {
    stop_at(&digest_path(), on, schedule_at(), now())?;
    daily_next()
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
    fn press_stop_without_schedule_errors() {
        let (_dir, digest) = files();
        assert_eq!(
            stop_at(&digest, true, None, now()),
            Err("nothing is scheduled; there's no press to stop".to_owned())
        );
    }
}
